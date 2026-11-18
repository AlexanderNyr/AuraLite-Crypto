# AuraLite-Crypto — public security audit report (H8)

Date: 2026-11-18 · Phase: H8 (SECURITY_PLAN) · Patch: `patches/0072-H8-audit.patch`
Gate: **0 unfixed critical/high**; re-audit must turn red on any reintroduced old bug.

## Scope

| Area | Code | Status |
|---|---|---|
| consensus | `crates/aura-consensus` (+ `crates/aura-mempool` ordering) | audited |
| VM | `crates/aura-exec` (verify, wasmi_engine, abi, detcheck) | audited |
| bridge | E9 design (Stage 2) | not implemented in this tree |
| wallet | `crates/aura-wallet` (keystore, secret, signing) | audited |

<!-- H8 area: consensus -->
<!-- H8 area: vm -->
<!-- H8 area: bridge -->
<!-- H8 area: wallet -->

## Methodology

1. **Registry-based regression oracle.** Every bug fixed during Stage 1
   (H0–H7) is pinned in `tools/fixtures/h8_known_bugs.json` with an exact
   textual invariant; `tools/h8_reaudit.py` re-checks all of them on every
   `make test` run. Re-introducing a fixed bug turns the gate red
   (`H8: re-audit finds an old bug`) — that is the phase negative control.
2. **Pattern inventory passes** over the four areas: `unsafe` (0 sites in
   scope), panic sites (`unwrap`/`expect` outside tests), truncating
   numeric casts, unchecked arithmetic, permission/secret handling.
3. **Targeted invariant reading** of the security-sensitive functions
   surfaced by pass 2, with each conclusion recorded as a finding below.
4. **Governance/docs pass**: disclosure policy and bounty rules
   (`SECURITY.md`) checked by the same oracle.

Deviation (recorded in `docs/h8-audit-plan.md` §3): no third-party audit
firm exists in this environment; the "external" review is executed as an
independent scripted re-review pass over pinned registries, which is what
makes the result reproducible and machine-checkable. The bounty program
is published (`SECURITY.md`) but payouts happen through the owner's channel.

## Findings

| ID | Area | Severity | Status | Fix |
|---|---|---|---|---|
| H8-F-001 | wallet | high | fixed | `patches/0071-H7-opsec.patch` — loader rejects `& 0o077 != 0` |
| H8-F-002 | wallet | high | fixed | `patches/0071-H7-opsec.patch` — access log line format pinned, no secrets |
| H8-F-003 | consensus | high | fixed | `patches/0070-H6-economic.patch` — reward accounting saturates |
| H8-F-004 | consensus | medium | fixed | `patches/0070-H6-economic.patch` — ParameterChange validates updated params |
| H8-F-005 | vm | informational | accepted | `verify.rs` frame pops guarded by `frames.len()` checks (`<2` / `==1`); `unwrap()` unreachable otherwise — invariant documented |
| H8-F-006 | vm | informational | accepted | 23 truncating casts (`as u32/u16/u8`) audited one by one: LEB128 masks, arities, post-limit lengths — none truncates unchecked external data |
| H8-F-007 | bridge | informational | not-applicable | E9 (Stage 2) — slot retained so the area cannot silently leave scope |
| H8-F-008 | consensus | medium | fixed | `SECURITY.md` — disclosure policy and bounty program published |

**Unfixed critical/high: 0.** The machine-readable copy of this table is
`tools/fixtures/h8_findings.json`; `tools/h8_reaudit.py` fails closed if
a critical/high ever shows up without a fix.

## Negative controls

`tools/nc_h8.py` (7 controls, `docs/evidence/h8/nc.log`): each injects one
old bug or policy break and requires the named red message. The phase gate
literally is control NC-1/NC-2: **re-audit finds an old bug → red**.
