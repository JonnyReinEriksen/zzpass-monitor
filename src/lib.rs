//! ZZPass uptime monitor — Cloudflare Worker (Rust) that probes the escrow +
//! telemetry `/health` endpoints on a cron and emails on state CHANGES.
//!
//! SCOPE / limitation (be honest): this runs ON Cloudflare, so it detects a single
//! worker or its D1 being down while the platform is up — the common failure. It
//! CANNOT observe a full Cloudflare/zone outage from the inside; pair it with an
//! external prober (see README) for that.
//!
//! Endpoints:
//!   GET /health          liveness of the monitor itself
//!   GET /status?token=…  token-gated page: each target's current up/down + since
//!
//! Alerting: only on a transition (up→down, down→up), tracked per-target in KV — so
//! a sustained outage is ONE email, not one per probe. Recovery emails too.

use serde_json::json;
use worker::*;

/// (name, url) of every probed endpoint.
const TARGETS: &[(&str, &str)] = &[
    ("escrow", "https://escrow.example.com/health"),
    ("telemetry", "https://telemetry.example.com/health"),
];

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    Router::new()
        .get("/health", |_, _| Response::ok("ok"))
        .get_async("/status", |req, ctx| async move { status_page(req, ctx).await })
        .run(req, env)
        .await
        .or_else(|e| {
            console_error!("internal error: {e}");
            json_response(&json!({ "error": "internal_error" }), 500)
        })
}

#[event(scheduled)]
async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    for (name, url) in TARGETS {
        let up = probe(url).await;
        if let Err(e) = handle_transition(&env, name, up).await {
            console_error!("monitor[{name}]: state handling failed: {e}");
        }
    }
}

/// True when the target answers 2xx. Any transport error or non-2xx = down.
/// (Cloudflare bounds subrequest duration, so a hung origin surfaces as an error
/// on this or a subsequent 2-minute run rather than stalling the monitor.)
async fn probe(url: &str) -> bool {
    match Request::new(url, Method::Get) {
        Ok(request) => match Fetch::Request(request).send().await {
            Ok(resp) => (200..300).contains(&resp.status_code()),
            Err(e) => {
                console_error!("monitor: probe {url} transport error: {e}");
                false
            }
        },
        Err(e) => {
            console_error!("monitor: probe {url} bad request: {e}");
            false
        }
    }
}

/// Compare the fresh probe with the stored state; email + persist on a change.
async fn handle_transition(env: &Env, name: &str, up: bool) -> Result<()> {
    let kv = env.kv("STATE")?;
    let key = format!("state:{name}");
    let now = (Date::now().as_millis() / 1000) as i64;
    // Stored value is "up|<since>" / "down|<since>"; absent = unknown (first run).
    let previous = kv.get(&key).text().await?;
    let was_up = previous.as_deref().map(|v| v.starts_with("up"));

    // No email on the very first observation (unknown → known); just record it.
    let changed = matches!(was_up, Some(prior) if prior != up);
    if changed {
        send_alert(env, name, up, now).await;
    }
    // Write to KV ONLY on a real state change (or the first observation) — never on
    // every run. This keeps daily KV writes near-zero (the free tier caps writes at
    // 1,000/day) and makes the stored timestamp the true "since" (the last transition),
    // not the current time — which is what the /status page reports.
    if changed || was_up.is_none() {
        kv.put(&key, format!("{}|{}", if up { "up" } else { "down" }, now))?
            .execute()
            .await?;
    }
    Ok(())
}

async fn send_alert(env: &Env, name: &str, up: bool, at: i64) {
    let (Ok(api_key), Ok(from), Ok(to)) = (
        env.secret("EMAIL_API_KEY"),
        env.secret("EMAIL_FROM"),
        env.secret("ALERT_TO"),
    ) else {
        console_error!("monitor: alert email not configured; would have sent {name} up={up}");
        return;
    };
    let api_url = env
        .secret("EMAIL_API_URL")
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "https://api.resend.com/emails".to_string());
    let (state, emoji) = if up { ("recovered", "✅") } else { ("DOWN", "🔴") };
    let subject = format!("{emoji} ZZPass {name} {state}");
    let payload = json!({
        "from": from.to_string(),
        "to": [to.to_string()],
        "subject": subject,
        "html": format!(
            "<p><strong>{name}</strong> is {state}.</p><p>Observed at unix {at} (UTC).</p>\
             <p>Monitor: <code>monitor.example.com/status</code></p>"),
    });
    let headers = Headers::new();
    let _ = headers.set("Authorization", &format!("Bearer {}", api_key.to_string()));
    let _ = headers.set("Content-Type", "application/json");
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(worker::wasm_bindgen::JsValue::from_str(&payload.to_string())));
    if let Ok(request) = Request::new_with_init(&api_url, &init) {
        if let Ok(mut resp) = Fetch::Request(request).send().await {
            let status = resp.status_code();
            if !(200..300).contains(&status) {
                console_error!("monitor: alert send got {status}: {}",
                    resp.text().await.unwrap_or_default());
            }
        }
    }
}

async fn status_page(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if let Some(limited) = rate_limit_gate(&ctx.env, &req, "STATUS_LIMITER").await {
        return limited;
    }
    let Ok(secret) = ctx.env.secret("STATUS_TOKEN") else {
        return json_response(&json!({ "error": "unauthorized" }), 401);
    };
    let presented = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "token")
        .map(|(_, v)| v.to_string())
        .unwrap_or_default();
    if !constant_time_eq(presented.as_bytes(), secret.to_string().as_bytes()) {
        return json_response(&json!({ "error": "unauthorized" }), 401);
    }

    let kv = ctx.env.kv("STATE")?;
    let mut rows = String::new();
    for (name, url) in TARGETS {
        let stored = kv.get(&format!("state:{name}")).text().await?.unwrap_or_else(|| "unknown".into());
        let (state, since) = stored.split_once('|').unwrap_or((stored.as_str(), ""));
        let color = if state == "up" { "#34c759" } else if state == "down" { "#ff3b30" } else { "#8e8e93" };
        rows += &format!(
            "<tr><td>{}</td><td style=\"color:{color};font-weight:600\">{}</td>\
             <td>{}</td><td class=\"u\">{}</td></tr>",
            escape_html(name), escape_html(state), escape_html(since), escape_html(url));
    }
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"robots\" content=\"noindex\"><title>ZZPass status</title><style>\
         body{{font-family:-apple-system,system-ui,sans-serif;margin:2rem auto;max-width:640px;padding:0 1rem}}\
         table{{border-collapse:collapse;width:100%}}td,th{{text-align:left;padding:.4rem .6rem;\
         border-bottom:1px solid #e5e5ea;font-size:.9rem}}.u{{color:#8e8e93;font-size:.8rem}}\
         @media(prefers-color-scheme:dark){{body{{background:#1c1c1e;color:#f2f2f7}}td,th{{border-color:#3a3a3c}}}}\
         </style></head><body><h1>ZZPass status</h1>\
         <table><tr><th>service</th><th>state</th><th>since (unix)</th><th>endpoint</th></tr>{rows}</table>\
         <p class=\"u\">Probed every 30 min from Cloudflare. Does not detect a full Cloudflare outage — \
         pair with an external monitor.</p></body></html>"
    );
    let headers = Headers::new();
    headers.set("Content-Type", "text/html; charset=utf-8")?;
    headers.set("Cache-Control", "no-store")?;
    Ok(Response::ok(body)?.with_headers(headers))
}

// ---------------------------------------------------------------------------
// Shared helpers (same idioms as escrow / telemetry)
// ---------------------------------------------------------------------------

async fn rate_limit_gate(env: &Env, req: &Request, binding: &str) -> Option<Result<Response>> {
    let ip = req.headers().get("CF-Connecting-IP").ok().flatten()
        .unwrap_or_else(|| "unknown".to_string());
    let limiter = env.rate_limiter(binding).ok()?;
    match limiter.limit(ip).await {
        Ok(outcome) if outcome.success => None,
        Ok(_) => Some(json_response(&json!({ "error": "rate_limited" }), 429)),
        Err(_) => None,
    }
}

fn json_response(body: &serde_json::Value, status: u16) -> Result<Response> {
    Ok(Response::from_json(body)?.with_status(status))
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert!(constant_time_eq(b"tok", b"tok"));
        assert!(!constant_time_eq(b"tok", b"toX"));
        assert!(!constant_time_eq(b"tok", b"token"));
        assert_eq!(escape_html("<a>&\"b\""), "&lt;a&gt;&amp;&quot;b&quot;");
        assert_eq!(TARGETS.len(), 2);
        assert!(TARGETS.iter().all(|(_, url)| url.ends_with("/health")));
    }
}
