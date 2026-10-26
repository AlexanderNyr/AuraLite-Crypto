# SECURITY_PLAN — threats, fuzzing, audit, incidents

**Status:** 📋 PLAN (H0–H9)  
**Stage:** Stage 1: H0–H7. ⏸ Stage 2: H8–H9 (external audit, bug bounty, incident drills).  
**Deliverable:** `patches/0086-H0-...` … `patches/0095-H9-...`
**Dependencies:** runs in parallel with CORE (H0/H1 with C0). H0–H7 are mandatory
before O4 (M2); H8–H9 before O5/O6 (⏸ Stage 2).

## Problem

A network with money is not "just another open-source project". A bug in the
codec, state machine or bridge costs real funds. Meanwhile a green `make test`
describes the tested subset, not the whole attack surface (the lesson of
`MATURITY_AUDIT.md` in AuraLite-OS).

## Decisions

- **D-H1.** Threat model first, code second. Every defence has an attacking test and an NC.
- **D-H2.** All parsers (codec, RPC, p2p, WASM verifier) are fuzzed; all consensus invariants are property-based + model checked.
- **D-H3.** External audit is a mandatory gate O5 (⏸ Stage 2); internal "I checked everything" does
  not count. Stage 1 closes with H0–H7.
- **D-H4.** Responsibility: disclosure policy, bug bounty, playbook, post-mortem — these are phase artifacts too.


**C9 note (2026-10-26).** `CORE_PLAN` C9 shipped four `libfuzzer-sys` source
targets (`fuzz/`) and a portable 60-second fuzz gate (`make fuzz-smoke`).
H3 still owes sanitizers (Miri/ASan/UBSan), the 24-hour libFuzzer campaign,
coverage measurement and crash triage. C9 did not claim any of those.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| **H0** | Threat model | `docs/THREAT_MODEL.md`: assets, adversary (external/validator/insider), trust boundaries, 30+ scenarios | review gate; NC: scenario without an owner → red |
| **H1** | Supply chain | `cargo deny`, `cargo audit`, pinning, SBOM (CycloneDX), gitleaks, release signatures | CI fails on a known vulnerable version; NC: remove deny → red |
| **H2** | Crypto review | constant-time, zeroize, domain separation, malleability check, PQ plan (ML-DSA FIPS 204 / SLH-DSA FIPS 205 [7](https://csrc.nist.gov/news/2024/postquantum-cryptography-fips-approved)) | 50 crypto vectors + side-channel checklist; NC: shared domain → signature passes, red |
| **H3** | Fuzzing | cargo-fuzz targets: codec, tx decode, RPC, p2p frames, WASM verifier, wallet keystore | 24 h total, 0 new crashes; NC: injected bug → fuzz finds it |
| **H4** | Property/differential | reference implementation of tree/codec, model-based VM tests, balance invariants | 1M generated txs: `sum(balances)` conserved |
| **H5** | Consensus safety | TLA+/Apalache spec, Jepsen-like runs (together with B10/N8) | model finds a counterexample when 2f+1 is removed; NC documented |
| **H6** | Economic attacks | MEV, censorship, stake grinding, 34% attack, oracle manipulation, bridge drain | simulations: damage bounded by limits/slashing; NC: no pause → red |
| **H7** | OpSec | validator keys (HSM/offline), CI isolation, permissions, access log, reproducible build | NC: secret in git → CI red; two hosts build to the same sha256 |
| ⏸ **H8** | External audit + bounty | audit of consensus/VM/bridge/wallet, public report, fixes, bug bounty + disclosure | 0 unfixed critical/high; NC: re-audit finds an old bug → red |
| ⏸ **H9** | Incidents | playbook: halt/pause/upgrade/rollback, communication, post-mortem, drills | drill "Byzantine validator + bridge bug": network halted/recovered within SLO |

## Risks

- "Tests written but not registered in CI" — a separate check of the target list in every patch (rule from `MATURITY_AUDIT.md`).
- Audit before API freeze — wasted money: H8 only after the O5 feature freeze.
- A bridge bug is the most expensive: E9 is not enabled on mainnet without H8.

## Non-goals

Legal liability (T8), security of third-party exchanges/custodians, physical
security of validators (documented as a requirement for O6).

## Closure criteria

Stage 1: every critical component has a threat model, attacking test, NC,
fuzzing target and/or model check (H0–H7).
Stage 2 (⏸): external audit, bug bounty and incident drills (H8–H9).
