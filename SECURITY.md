# Security Policy

ZZPass is a password manager, and we take its security seriously. We welcome reports from
security researchers.

## Reporting a vulnerability

**Email [security@zzpass.com](mailto:security@zzpass.com).**
**Please do not open a public GitHub issue for a security vulnerability.**

For sensitive reports, encrypt with our PGP key (published in
[`security.txt`](https://www.zzpass.com/.well-known/security.txt)).

Please include the affected component and version, a description of the issue, steps to
reproduce (a proof-of-concept if you have one), and the impact you believe it has. For a
password manager we are *especially* interested in anything touching **cryptography, key
handling, the sharing or escrow protocols, or autofill**.

## What to expect

- We acknowledge every good-faith report within **3 business days**.
- We give you an initial assessment within **7 business days** and keep you updated as we
  work toward a fix.
- We aim to remediate confirmed vulnerabilities within **90 days** — faster for
  high-severity issues — and coordinate public-disclosure timing with you.
- With your permission, we credit you in the release notes and a security-acknowledgements
  page.

## Safe harbor

We will not pursue or support legal action against researchers who, in good faith:

- follow this policy,
- avoid privacy violations, data destruction, and degradation of service,
- access only their own accounts and data — never another user's, and
- give us reasonable time to remediate before disclosing publicly.

Activity conducted consistently with this policy is authorized; we consider it lawful,
helpful, and appreciated.

## Scope

**In scope:** the ZZPass iOS and macOS apps; the open-source server components
(`timelock-escrow`, `zzpass-telemetry`, `zzpass-monitor`); and `zzpass.com` and its
subdomains.

**Out of scope** — please don't report these without a concrete, demonstrated impact:
denial-of-service / volumetric attacks; social engineering, phishing, or physical attacks;
automated-scanner output without a working proof-of-concept; missing "best-practice"
hardening (headers, SPF/DMARC, TLS config) with no exploit; vulnerabilities in third-party
services we rely on (Apple, Cloudflare, our email provider — report those to the vendor);
self-XSS; and issues that require an already-compromised or jailbroken device.

---

The full policy, cryptographic design, and threat model are in the **ZZPass Security
Whitepaper**, and a machine-readable summary is at
<https://www.zzpass.com/.well-known/security.txt> (RFC 9116).
