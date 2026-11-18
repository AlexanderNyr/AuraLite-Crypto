# Security — disclosure policy and bug bounty

Project: AuraLite-Crypto (W1-FORMAT-v1 testnet).
Scope baseline: Stage 1 (H0–H7) closed 2026-11-17; this policy ships with H8.

## 1. Coordinated disclosure

<!-- H8: disclosure -->

- Report privately first: open a **private** security advisory on the
  repository (GitHub Security Advisories) or email the addresses listed in
  `.github/SECURITY_CONTACTS` (published contacts; no public issue before
  a fix ships).
- You will get an acknowledgement within **3 working days** and a status
  update at least every **14 days**.
- A fix ships as a signed patch in `patches/` with an advisory entry in
  `docs/evidence/`; public disclosure happens **no earlier than 90 days**
  after the report, or immediately after the fix ships, whichever you
  prefer.
- Safe harbour: good-faith research that avoids privacy violations, data
  destruction and service disruption against systems you do not own is
  welcome and will not be pursued legally.

## 2. Bug bounty

<!-- H8: bounty -->

Rewards (paid after the fix is verified and merged; amounts scale with
impact and quality of the report):

| Severity | Examples | Bounty (USD) |
|---|---|---|
| Critical | consensus safety violation, double spend, key theft, inflation bug | 5 000 – 25 000 |
| High | forced chain halt, fund freeze, signature bypass, secret leak | 1 000 – 5 000 |
| Medium | DoS with amplification, governance griefing, recovery failure | 250 – 1 000 |
| Low | informational hardening gaps, docs/ops issues with security impact | 50 – 250 |

In scope: `crates/aura-consensus` (consensus), `crates/aura-exec` (VM),
`crates/aura-wallet` (wallet/keys), `crates/aura-mempool`,
`crates/aura-node`, the bridge design (E9, once implemented), CI/CD and
release signing (`tools/`).
Out of scope: social engineering, third-party service outages, findings
against test fixtures or `insecure-test-kdf` builds, reports that only
reproduce a finding already listed in `docs/audit/h8-public-audit-report.md`.

## 3. Severity table (normative)

The severity levels above are the same ones used by the audit findings
registry `tools/fixtures/h8_findings.json`; the gate
`tools/h8_reaudit.py` enforces **0 unfixed critical/high** findings.
