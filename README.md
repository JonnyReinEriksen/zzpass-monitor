# zzpass-monitor

A tiny Cloudflare Worker (Rust) that probes a list of service `/health` endpoints on a
cron and **emails only on state changes** (up→down, down→up) — so a sustained outage is
one email, not one every run. Up/down state lives in KV.

## Scope & limitation

It runs **on** Cloudflare, so it detects a single worker or its backing store being down
while the platform is up — the common failure. It **cannot** observe a full Cloudflare/
zone outage from the inside. Pair it with an external prober (UptimeRobot / Better Uptime,
free tier) pointed at the same `/health` URLs for that layer.

## Configure what it probes

Edit the `TARGETS` constant in `src/lib.rs` — a list of `(name, url)` pairs:

```rust
const TARGETS: &[(&str, &str)] = &[
    ("escrow",    "https://escrow.example.com/health"),
    ("telemetry", "https://telemetry.example.com/health"),
];
```

## Endpoints

| | |
|---|---|
| `GET /health` | the monitor's own liveness |
| `GET /status?token=…` | HTML page: each target's current up/down + since (unix). Token = the `STATUS_TOKEN` secret (constant-time compared). |

## First deploy

```sh
wrangler kv namespace create MONITOR_STATE     # paste the id into wrangler.toml (binding STATE)
wrangler secret put STATUS_TOKEN               # capability token for /status
# Alerting (dormant until all three are set):
wrangler secret put EMAIL_API_KEY              # transactional-email provider API key
wrangler secret put EMAIL_FROM                 # sender on a provider-VERIFIED domain
wrangler secret put ALERT_TO                   # where alerts go
wrangler deploy                                # provisions your route + the cron
```

`EMAIL_FROM` must be an address on a domain **verified with your email provider** — an
unverified sending domain is rejected by the provider (e.g. Resend returns `403`) and no
mail goes out. The mailbox itself need not exist; only the sending domain must be verified.
Targets the Resend JSON API by default; set the optional `EMAIL_API_URL` var for another
provider with a `/emails`-shaped endpoint.

Local: `wrangler dev --test-scheduled`, then `curl localhost:PORT/__scheduled?cron=…`.

## How it stays quiet

State is stored per target in KV. An email fires **only** on a transition, so:
- first observation (unknown→known) sends nothing,
- a service that stays down sends one email, not one per run,
- recovery (down→up) sends one email.

Down and up alerts share one code path, so verifying recovery verifies both directions.

## License

MIT — see `LICENSE`.
