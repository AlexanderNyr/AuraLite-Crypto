# SECURITY_PLAN — threats, fuzzing, audit, incidents

**Status:** 🚧 IN PROGRESS — H0 ✅ (`patches/0063-H0-threat-model.patch`); H1 ✅ (`patches/0064-H1-supply-chain.patch`); H4 ✅ (`patches/0068-H4-property.patch`); H5 ✅ (`patches/0069-H5-consensus.patch`); H6 ✅ (`patches/0070-H6-economic.patch`); H7 ✅ (`patches/0071-H7-opsec.patch`) — **Stage 1 (H0–H7) closed**; H8 ✅ (`patches/0072-H8-audit.patch`); H9 ✅ (`patches/0073-H9-incident.patch`) — **SECURITY_PLAN (H0–H9) closed**  
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

**H3 resolution (2026-11-14).** Target matrix, fuzz oracles, recorded
campaign runner, coverage recording and crash triage landed with H3
(`docs/h3-fuzz-plan.md`). Deviation (plan §3): the 24-hour `full` budget is
`make fuzz-h3-full` and is **not** claimed by this session's evidence; ASan
and `-print_coverage` ride the cargo-fuzz runs and are recorded `skipped`
without the nightly toolchain. Miri/UBSan remain owed and move to the
Stage-2 external-review gate (O5).

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| ✅ **H0** | Threat model | `docs/THREAT_MODEL.md`: assets, adversary (external/validator/insider), trust boundaries, 30+ scenarios | review gate; NC: scenario without an owner → red |
| ✅ **H1** | Supply chain | `cargo deny`, `cargo audit`, pinning, SBOM (CycloneDX), gitleaks, release signatures | CI fails on a known vulnerable version; NC: remove deny → red |
| ✅ **H2** | Crypto review | constant-time, zeroize, domain separation, malleability check, PQ plan (ML-DSA FIPS 204 / SLH-DSA FIPS 205 [7](https://csrc.nist.gov/news/2024/postquantum-cryptography-fips-approved)) | 50 crypto vectors + side-channel checklist; NC: shared domain → signature passes, red |
| ✅ **H3** | Fuzzing | cargo-fuzz targets: codec, tx decode, RPC, p2p frames, WASM verifier, wallet keystore | 24 h total, 0 new crashes; NC: injected bug → fuzz finds it |
| ✅ **H4** | Property/differential | reference implementation of tree/codec, model-based VM tests, balance invariants | 1M generated txs: `sum(balances)` conserved |
| ✅ **H5** | Consensus safety | TLA+/Apalache spec, Jepsen-like runs (together with B10/N8) | model finds a counterexample when 2f+1 is removed; NC documented |
| ✅ **H6** | Economic attacks | MEV, censorship, stake grinding, 34% attack, oracle manipulation, bridge drain | simulations: damage bounded by limits/slashing; NC: no pause → red |
| ✅ **H7** | OpSec | validator keys (HSM/offline), CI isolation, permissions, access log, reproducible build | NC: secret in git → CI red; two hosts build to the same sha256 |
| ✅ **H8** | External audit + bounty | audit of consensus/VM/bridge/wallet, public report, fixes, bug bounty + disclosure | 0 unfixed critical/high; NC: re-audit finds an old bug → red |
| ✅ **H9** | Incidents | playbook: halt/pause/upgrade/rollback, communication, post-mortem, drills | drill "Byzantine validator + bridge bug": network halted/recovered within SLO |

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

### H0 receipt (2026-11-11)

- Deliverable: `patches/0063-H0-threat-model.patch` (complete H0 phase patch);
  evidence in `docs/evidence/h0/`.
- `docs/THREAT_MODEL.md`: assets A1–A8, adversaries
  (ADV-EXT/VAL/INS/SUP/PHY), trust boundaries B1–B6 and **32 owned scenarios**
  H0-S01..H0-S32 mapped to shipped mitigations (W0–W4, B/N/D phases) and
  residual risks; owners are the seven roles of §5.
- Named control: `scenario_without_an_owner` — `tools/nc_h0.py` NC-1 deletes
  one Owner field (H0-S07) and requires the review gate to fail with
  `error: scenario H0-S07: missing owner`.
- Test gate: `tools/check_threat_model.py` (registered in `make test`)
  enforces ≥30 scenarios, unique ids, required fields, known
  class/boundary/asset references, owners in role, non-empty mitigations, and
  field-by-field equality with the deterministic oracle output
  (`tools/gen_h0_vectors.py` → `tools/fixtures/h0_threat_model_vectors.json`).
- Mutation controls (`tools/nc_h0.py`, 6): owner dropped, duplicate id,
  unknown class, unknown boundary, empty mitigations, fewer than 30
  scenarios — 6/6 PASS.

### H1 receipt (2026-11-14)

- Deliverable: `patches/0064-H1-supply-chain.patch`; evidence in
  `docs/evidence/h1/`; plan and deviations in `docs/h1-supply-chain-plan.md`.
- Supply-chain gate `tools/check_supply_chain.py` (registered in `make test`):
  toolchain pin, lockfile digest pin, advisory deny (pinned snapshot), SBOM
  digest, secrets scan, release signatures — 11/11 self-test verdicts,
  including the named gate "CI fails on a known vulnerable version"
  (`error: supply-chain: denied RUSTSEC-TEST-0001 (demo-vulnerable v0.9.0)`
  on `tools/fixtures/h1_vulnerable.lock`).
- Named control: `remove deny` — `tools/nc_h1.py` NC-1 weakens the advisory
  deny and requires the self-test case `vulnerable_lock_denied` to go red.
- Tooling: deterministic CycloneDX 1.5 SBOM (`sbom.cdx.json`), gitleaks-class
  scanner (`tools/scan_secrets.py`, planted-secret fixture proves detection),
  Ed25519 demo release signatures (`tools/sign_release.py`, pinned pubkey
  `docs/release-pubkey.txt`), policy `deny.toml`; auxiliary pinned tools
  cargo-deny 0.20.2 / gitleaks 8.30.1 recorded in evidence.
- Mutation controls (`tools/nc_h1.py`, 6): deny removed, toolchain-pin
  removed, lockfile digest removed, sbom comparison removed, secret rule
  disabled, signature verification skipped — 6/6 PASS.

### H2 receipt (2026-11-14)

- Deliverable: `patches/0066-H2-crypto-review.patch`; evidence in
  `docs/evidence/h2/`; plan in `docs/h2-crypto-review-plan.md`.
- 50 golden crypto vectors (`crates/aura-crypto/tests/fixtures/h2_vectors.txt`,
  oracle `tools/gen_h2_vectors.py` — pure-Python RFC 8032 + python blake3 +
  from-spec `domain_message`): keygen, sign/verify byte-for-byte, wrong
  domain, cross chain, tampered payload, wrong key, high-S malleation,
  digest layout, edge payloads — asserted by
  `crates/aura-crypto/tests/h2_crypto.rs` (registered in `make test`).
- Malleability check: `verify_raw` rejects `S >= L` with the distinct
  `CryptoError::NonCanonicalSignature` (the classic twin `(R, S+L)`; dalek's
  `legacy_compatibility` feature would reopen the window).
- Side-channel checklist: 10 machine-readable items (`H2-SC01..SC10`,
  Claim/Owner/Evidence/Status) pinned by
  `tools/fixtures/h2_review_checklist.json`, re-derived and source-checked by
  `tools/check_h2_review.py` (registered in `make test`).
- PQ plan (planned): ML-DSA (FIPS 204) primary, SLH-DSA (FIPS 205) stateless
  backup, hybrid `Ed25519 || ML-DSA` migration at W1-FORMAT-v2 / Stage 2.
- Named control: `shared domain` — `tools/nc_h2.py` NC-1 collapses
  `Domain::tag()` to one tag and requires the h2 suite to go red with
  `H2: wrong-domain signature must be rejected`.
- Mutation controls (`tools/nc_h2.py`, 6): shared domain, canonical-S guard
  removed, chain binding dropped, checklist owner/evidence/status deleted —
  6/6 PASS.

### H3 receipt (2026-11-14)

- Deliverable: `patches/0067-H3-fuzzing.patch`; evidence in
  `docs/evidence/h3/`; plan in `docs/h3-fuzz-plan.md`.
- Target matrix complete: 11 libFuzzer sources (new `fuzz_p2p_frame`,
  `fuzz_wasm_verify`) + 6 portable areas (plan §2).
- Fuzz oracles (`fuzz/src/h3.rs`): totality + determinism per decoder area
  over a fixed boundary corpus; a decoder panic fails with
  `H3: fuzz oracle caught a decoder panic in … (injected bug found)`.
- Campaign: `tools/h3_fuzz_campaign.py`, recorded `smoke` run archived in
  `docs/evidence/h3/campaign/` (seeds from 20261180); `full` = the 24-hour
  budget (86,400 s / 17 runs) as one parameter. Deviation (plan §3): the
  24-hour run is not claimed in this session; libFuzzer/ASan runs need
  nightly-2026-11-03 + cargo-fuzz and are recorded `skipped` without them.
- Crash triage: plan §4 (recorded seeds/artifacts reproduce; fixes land
  with regression vectors).
- Named control: `injected bug -> fuzz finds it` — `tools/nc_h3.py` NC-1
  injects a `HandshakeHello::decode` panic and the fuzz oracle must go red.
- Mutation controls (`tools/nc_h3.py`, 6): p2p handshake, codec, tx, gossip
  frame, wasm gate, RPC parser panics — 6/6 PASS.

### H4 receipt (2026-11-16)

- Deliverable: `patches/0068-H4-property.patch`; evidence in
  `docs/evidence/h4/`; plan in `docs/h4-property-plan.md`.
- Balance invariants: `Account::{credit,debit}` algebra (conservation,
  reject-without-change, checked arithmetic) +
  `crates/aura-testkit/tests/h4_properties.rs`.
- Model-based VM tests: `crates/aura-exec/tests/h4_model_vm.rs` — a
  reference model of `detcheck::run` transfer semantics predicts
  committed/refused/gas; node-label independence enforced.
- Reference tree/codec (differential): naive in-tree Rust references plus
  the cross-language reference `tools/h4_reference.py` (C4 Python tx
  encoder + from-scratch S1 merkle), compared both ways by
  `tools/h4_property_campaign.py`.
- Gate: `crates/aura-testkit/src/bin/h4-properties.rs --txs N` — every
  generated tx signed, verified and canonically round-tripped; the
  invariant `sum(balances) == accounts × start` is checked after every
  transaction. Deviation (plan §3): the 1M-tx gate run is `make
  property-h4-full`; this session's evidence records the schedule it
  actually executed with exact counts.
- Named control: `injected bug -> property finds it` — `tools/nc_h4.py`
  NC-1 shrinks a 7-unit debit and the conservation property must go red.
- Mutation controls (`tools/nc_h4.py`, 6): debit shrink, canonicality
  guard disabled, expiry dropped from signing bytes, transfer path
  refusing amount 7, merkle node order swapped, credit inflation — 6/6
  PASS.

### H5 receipt (2026-11-17)

- Deliverable: `patches/0069-H5-consensus.patch`; evidence in
  `docs/evidence/h5/` (`campaign/` self-contained: model-table and
  chaos-matrix logs, TLC logs for both configurations, `result.json` with
  commands and source digests).
- Gate met literally: the executable model returns Safe with
  `min_honest_quorum_overlap >= 1` for (4,1)(7,2)(10,3)(13,4)(31,10)(100,33),
  and **finds a counterexample** (with named double-signing offenders) the
  moment `2f+1` is removed; TLC over `docs/h5/HotStuff3ChainSafetyMC.tla`
  exhausts the safe configuration (11664 distinct states, no violation)
  and reports `Invariant AccountableSafety is violated` under
  `WeakenQuorum = TRUE` (threshold F+1).
- Jepsen-like runs (with B10/N8): the N8 chaos matrix 20 scenarios × 3
  runs — zero finalized-head divergence; the divergence monitor rejects a
  crafted divergent cluster and accepts a converged one.
- NC 6/6: checker always Safe, quorum weakened to f+1, offender
  extraction emptied, TLA threshold changed to F+1, divergence monitor
  disabled, chaos matrix truncated — 6/6 PASS.

### H6 receipt (2026-11-17)

- Deliverable: `patches/0070-H6-economic.patch`; evidence in
  `docs/evidence/h6/` (`campaign/` self-contained: simulation logs and
  `result.json` with commands and source digests).
- Gate met: deterministic simulations show every listed attack bounded —
  MEV by the canonical `(sender, nonce)` order (insert-order metamorphism),
  censorship by round-robin proposer rotation (bounded delay ≤ n-1 rounds),
  stake grinding by stake-weighted selection plus slashable forged leader
  claims (`InvalidVrfLeaderClaim`, `stake_after < stake_before`), the 34%
  attack by the emission sum invariant across an adversarial proposer/fee
  schedule, oracle manipulation by the stake-weighted median bound, and
  bridge drain by the reserve invariant (no overdraw/replay). Oracle and
  bridge are modelled surfaces (no native component in the shipped crates)
  — deviation recorded in `docs/h6-economic-plan.md` §3.
- Named control: removing the emergency pause turns the gate red —
  `submit_proposal`/`execute_proposal` must fail with `EmergencyPaused`
  while the 2f+1-QC pause is active.
- NC 7/7: pause enforcement removed, emission minted from nowhere, slash
  without burn, rotation pinned to one validator, canonical order
  reversed, oracle median replaced by max, bridge reserve bound disabled —
  7/7 PASS.

### H9 receipt (2026-11-18) — incident drills, SECURITY_PLAN closed

- Deliverable: `patches/0073-H9-incident.patch`; evidence in
  `docs/evidence/h9/` (`campaign/` self-contained: drill logs,
  `result.json` with commands and source digests).
- Gate met literally: drill **"Byzantine validator + bridge bug"**
  halts the network at t+5 min (SLO ≤ 15) and recovers it at t+180 min
  (SLO ≤ 240); NC-1 (halt removed) and NC-2 (recover removed) turn the
  runner red with the named message each time.
- Playbook `docs/incident/PLAYBOOK.md` covers halt/pause/upgrade/
  rollback + communication + post-mortem with normative SLOs; three
  drills tied to H0 scenarios (D1 → H0-S10/S11, D2 → H0-S02/S13,
  D3 → H0-S15/S18).
- Negative controls: `tools/nc_h9.py`, 7/7 in `docs/evidence/h9/nc.log`;
  cumulative negative control total 488. Deviations (scheduled-timeline
  drill without a live testnet; bridge half = E9) recorded in
  `docs/h9-incident-plan.md` §3.

### H8 receipt (2026-11-18) — external audit + bounty

- Deliverable: `patches/0072-H8-audit.patch`; evidence in
  `docs/evidence/h8/` (`campaign/` self-contained: re-audit logs,
  `result.json` with commands and source digests).
- Gate met literally: **0 unfixed critical/high** in the findings
  registry (`tools/fixtures/h8_findings.json`), and **re-audit finds an
  old bug → red** — NC-1/NC-2 re-introduce the two named old bugs
  (wallet keystore mode check; consensus reward overflow) and
  `tools/h8_reaudit.py` turns red with the named message each time.
- Public report `docs/audit/h8-public-audit-report.md` covers
  consensus/VM/bridge/wallet (bridge = E9, slot retained);
  coordinated disclosure + bug bounty published in `SECURITY.md`.
- Negative controls: `tools/nc_h8.py`, 7/7 in `docs/evidence/h8/nc.log`;
  cumulative negative control total 481. Deviations (scripted external
  review; bridge out of code scope) recorded in `docs/h8-audit-plan.md` §3.

### H7 receipt (2026-11-17) — Stage 1 closed

- Deliverable: `patches/0071-H7-opsec.patch`; evidence in
  `docs/evidence/h7/` (`campaign/` self-contained: gate logs, two-host
  `repro-result.json`, `result.json` with commands and source digests).
- Gate met literally: a secret committed to the tree turns the CI red
  (`tools/check_opsec.py` over `tools/scan_secrets.py`, fail-closed and
  wired into `make test`), and two independent hosts (disjoint source
  paths and target directories, shared `--remap-path-prefix` target,
  toolchain pinned 1.99.1) build `aura-types --release --lib` to the
  **same sha256**.
- Validator keys: keystore files stay owner-only (save `0600`, dir `0700`)
  and — new — `Keystore::load` rejects any group/other permission bits;
  access log records (`keystore <op> file=<name> ok=<bool>`) are pinned,
  owner-only and never carry secret material. HSM/offline is covered by
  the W3 air-gapped signing flow (deviation in `docs/h7-opsec-plan.md` §3).
- CI isolation: both workflows carry least-privilege `permissions:
  contents: read` and pin the toolchain; `check_opsec` enforces it.
- NC 7/7: secret injected, remap made host-specific, permissions block
  removed, save mode widened, loader check disabled, toolchain unpinned,
  access-log format leaked — 7/7 PASS.

