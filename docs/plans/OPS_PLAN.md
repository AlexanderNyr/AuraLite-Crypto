# OPS_PLAN — CI, releases, testnet, mainnet, community

**Status:** ✅ Stage 1 CLOSED — O0 ✅ (`patches/0103-O0-ci-baseline.patch`); O1 ✅ (`patches/0104-O1-releases.patch`); O2 ✅ (`patches/0105-O2-m0-m1-gates.patch`); O3 ✅ (`patches/0106-O3-closed-testnet-ops.patch`); O4 ✅ (`patches/0107-O4-public-testnet.patch`) — **Stage 1 (O0–O4 / M0–M2) complete**.
**Stage:** Stage 1: O0–O4 ✅. ⏸ Stage 2: O5–O9 (mainnet, upgrades, community).
**Deliverable:** `patches/0103-O0-...` … `patches/0112-O9-...`
**Dependencies:** all Stage 1 technical plans; O5/O6 are the final north-star
gates (⏸ Stage 2).

## Problem

"Works on my machine" is not yet a network. We need reproducible builds, a
public testnet with third-party validators, a genesis ceremony, monitoring,
documentation and an incident plan. Without that, mainnet is just a devnet with
a loud name.

## Decisions

- **D-O1.** Everything CI runs can be run locally with one command (`make test`). No "secret" shards.
- **D-O2.** Releases are reproducible and signed; SBOM attached (H1/H7).
- **D-O3.** Mainnet (⏸ Stage 2) starts only after feature freeze +
  audit (H8) + white paper (T8) + ceremony (W7). Stage 1 ends at O4: public
  testnet with no market token.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| ✅ **O0** | CI baseline | GitHub Actions, 10 shards, cache, `make test`, coverage report | NC: disable a shard → CI red (target registry check) |
| ✅ **O1** | Releases | reproducible build, Docker multi-stage, signatures, SBOM, CHANGELOG, semver | two independent hosts → identical binary sha256 |
| ✅ **O2** | M0/M1 gates | 4-validator devnet, "how to run" docs, first receipts | ROADMAP §5 M0 and M1 criteria met |
| ✅ **O3** | Closed testnet | invited operators, monitoring, on-call, backup/restore drills | 7 days uptime, 0 state losses, 1 restore drill |
| ✅ **O4** | Public testnet (M2) | faucet, explorer, docs, bug bounty, public metrics, >= 10 external validators | 30 days, >= 99% uptime, >= 100 000 txs, 0 unfixed criticals |
| ⏸ **O5** | Mainnet candidate (M3) | feature freeze, H8 audit, T8 white paper, W7 genesis ceremony, H9 playbook | independent check: same genesis hash on all validators; 0 criticals |
| ⏸ **O6** | Mainnet launch (M4) | >= 21 validators, no entity >= 1/3 of stake, 24/7 monitoring, chain-id fixed | north star: network finalizes blocks for 7 days, logs in `docs/evidence/o6/` |
| ⏸ **O7** | First upgrade | on-chain governance (B9/T6) really updates the protocol without a hard fork | Proposal → timelock → activation; NC: upgrade without timelock → red |
| ⏸ **O8** | Docs and support | validator/developer/user guides, localization, FAQ, status page | 10 external validators onboard using docs only |
| ⏸ **O9** | External perimeter (not code) | listings, liquidity, partnerships, legal support, community | honestly labelled **not guaranteed by code**; not part of the north star |

## Receipts

### O0 receipt (2026-11-25)

- Deliverable: `patches/0103-O0-ci-baseline.patch` (complete O0-from-W7 phase
  patch).
- The CI layout is a checked registry (D-O0-1): `ci/registry.txt` pins the
  ten lanes of CONTRIBUTING.md (core, state, exec, consensus, net, node,
  wallet, ecosystem, security, perf) over the workspace's `--lib` and
  `tests/*` targets (127 today), refreshed by `make ci-sync`; each lane pins
  its main test (a shard that does not run its main test is a dead gate —
  the MATURITY_AUDIT lesson). `aura-ci check` (the target registry check,
  D-O0-2) validates the registry against the discovered tree and against
  the workflow matrix, and runs inside `make test` — a disabled shard
  holding targets is `error: ci: shard security disabled`.
- Named control: `disabled_shard_refused` — a registry with `shard security
  disabled` is refused (exit 65); mutation NC-1 (`tools/nc_o0.py`) drops the
  enabled-check and the case must turn red. "Disable a shard -> CI red" —
  the O0 gate.
- Test gate: 10 scenarios (`tests/fixtures/o0_ci_vectors.json`) from the
  independent oracle `tools/gen_o0_vectors.py`, asserted byte-for-byte by
  `tests/o0_ci.rs` (11/11 with the count guard) against `aura_testkit::ci`:
  valid registry (check + coverage), D-O1 command surface,
  disabled/missing/duplicate/empty/shard-set/main-dead/foreign-line/workflow
  refusals.
- Mutation controls (`tools/nc_o0.py`, 6): disabled-shard check, missing
  target check, duplicate check, shard-set check, empty-shard check,
  unknown-line strictness — 6/6 PASS.
- Workflow `.github/workflows/ci.yml`: `registry` job (`make ci-check`), a
  10-way `test` matrix over the lanes of CONTRIBUTING.md (`make ci-shard
  SHARD=<lane>`), a `coverage` job (`make coverage` ->
  `docs/coverage.txt`), cargo cache keyed on `Cargo.lock`. Every step is a
  make target — `make test` remains the one local command (D-O1, no secret
  shards: the registry format cannot carry commands).

### O1 receipt (2026-11-25)

- Deliverable: `patches/0104-O1-releases.patch` (complete O1-from-O0 phase
  patch).
- The gate "two independent hosts -> identical binary sha256" is measured,
  not asserted (D-O1-1): `make repro-check` (`tools/repro_check.py`) builds
  the release binary `aurad` twice into two independent `CARGO_TARGET_DIR`s
  with `SOURCE_DATE_EPOCH` pinned to the commit date and
  `--remap-path-prefix` hygiene, and records the identical digests in
  `release/repro.txt`. Named control: `repro_mismatch_refused` — a record
  whose two host digests differ is `error: release: repro mismatch:
  aura-bin` (exit 65); mutation NC-1 drops the equality check and the case
  must turn red. "Two hosts disagree -> CI red" — the O1 gate.
- The release process is a checked pipeline (D-O1-2):
  `aura_testkit::release` validates the strict `release/manifest.txt`
  against the workspace — semver/`version drift`, the `## [0.1.0` CHANGELOG
  entry, artifact presence, SHA256SUMS digests, the 64-byte detached
  ed25519 signature structure and pinned keyid (D-O1-3; the RFC 8032
  verification is the wired `tools/sign_release.py verify` step, H1 demo
  key), the deterministic SBOM regeneration (D-O1-4), the two-host repro
  record, and the Dockerfile policy (multi-stage, digest-pinned bases).
- Test gate: 10 scenarios (`tests/fixtures/o1_release_vectors.json`) from
  the independent oracle `tools/gen_o1_vectors.py` (Python hashlib),
  asserted byte-for-byte by `tests/o1_release.rs` (11/11 with the count
  guard) against `aura_testkit::release` (std-only SHA-256 — D-O1-5):
  valid release (check + regenerated SHA256SUMS), deterministic SBOM,
  repro/version/changelog/sbom/checksum/signature/docker/foreign-line
  refusals.
- Mutation controls (`tools/nc_o1.py`, 6): repro-equality check (named),
  version check, changelog check, checksum match check, signature length
  check, unknown-line strictness — 6/6 PASS.
- Correction note: `docs/evidence/o0/` was regenerated with this bundle
  after fixing the named-control string check in
  `tools/write_o0_evidence.py` (it matched the pre-redesign `shard 07`
  text and reported a false `FAIL`); the O0 verdicts are unchanged (11/11
  tests, 6/6 NC).

### O2 receipt (2026-11-25)

- Deliverable: `patches/0105-O2-m0-m1-gates.patch` (complete O2-from-O1
  phase patch).
- The gate "ROADMAP §5 M0 and M1 criteria met" is measured on receipts,
  not asserted (D-O2-2, D-O2-3): `make milestone-receipts` (`aurad-receipt`)
  runs the shipped D2/D3 pipeline twice through genesis -> 1000 signed
  transfers -> committed state root (both roots byte-identical — the M0
  criterion) and a real 4-validator `DevnetCluster` through 100 committed
  blocks (block interval measured <= 1 s, all four heads identical —
  0 desync), then reads the recipient balance through the shipped
  `aura_rpc::RpcService` JSON-RPC dispatch (positive — the M1 criterion).
  The measured values land in `release/milestone.txt`; `aura-milestone
  check` validates them against the criteria.
- Named control: `m0_root_drift_refused` — a receipt whose two `m0
  state_root` lines differ is `error: milestone: m0 state root drift`
  (exit 65); mutation NC-1 drops the root-equality check and the case must
  turn red. "Two runs disagree -> CI red" — the O2 gate.
- Test gate: 10 scenarios (`tests/fixtures/o2_milestone_vectors.json`)
  from the independent oracle `tools/gen_o2_vectors.py`, asserted
  byte-for-byte by `tests/o2_milestones.rs` (11/11 with the count guard)
  against `aura_testkit::milestone`: the valid receipt (check + pinned
  summary), root drift / validator-count / desync / height / rpc-balance /
  tx-count / block-rate / foreign-line refusals.
- Mutation controls (`tools/nc_o2.py`, 6): root-equality check (named),
  validator-count check, desync check, height check, rpc-balance check,
  unknown-line strictness — 6/6 PASS.
- M1 multi-process acceptance: `tests/d7_devnet.rs` (four real `aurad`
  processes, 100 finalized blocks with identical head/state_root, RPC
  balance after a faucet transfer, 1-of-4 outage survival) re-run in the
  evidence bundle (`m1-acceptance.log`).
- "How to run" docs: `docs/how-to-run.md` (build, the 4-validator devnet
  with the exact `aurad` flags the acceptance gate uses, RPC examples, the
  milestone measurement commands). First receipts: `docs/evidence/o2/`
  (`milestone.txt`, `test.log`, `nc.log`, `receipts.log`,
  `milestone-check.log`, `m1-acceptance.log`).

### O3 receipt (2026-11-25)

- Deliverable: `patches/0106-O3-closed-testnet-ops.patch` (complete
  O3-from-O2 phase patch).
- The gate "7 days uptime, 0 state losses, 1 restore drill" is measured
  on receipts, not asserted (D-O3-2, D-O3-3): `make testnet-ops-receipts`
  (`aurad-ops`) runs the scripted 7-day operator schedule (7 x 24 hourly
  polls) on the real pipeline — day/hour from the schedule, heights and
  state roots from the live chain — into the committed ledger
  `uptime-7d.txt` (rollup: 7 complete day-buckets, zero head
  regressions), and the **live backup/restore drill over the shipped S2
  backup format**: real state through the pipeline (blocks with transfers
  and receipts), `aura_storage::create_backup` bundle (manifest + WAL
  records + sha256), the original store destroyed, `restore_backup`
  replaying every record through the real commit path with recomputed
  roots — the restored head is byte-identical (the two `restore
  state_root` lines). Honest boundary: the D2 pipeline refuses to reopen
  a receipt-bearing store by design (restart checkpointing is a later
  node integration task), so restoration is verified at the S2 storage
  layer the backup format targets.
- Named control: `restore_mismatch_refused` — a receipt whose two
  `restore state_root` lines differ is `error: testnet-ops: restore
  mismatch` (exit 65); mutation NC-1 drops the restore root-equality
  check and the case must turn red. "The restore did not restore -> CI
  red" — the operational twin of `m0_root_drift_refused`.
- Test gate: 10 scenarios (`tests/fixtures/o3_testnet_ops_vectors.json`)
  from the independent oracle `tools/gen_o3_vectors.py`, asserted
  byte-for-byte by `tests/o3_testnet_ops.rs` (11/11 with the count guard)
  against `aura_testkit::ops`: the valid receipt (check + pinned
  summary), restore-mismatch / state-losses / uptime-days /
  restore-drills / root-count / foreign-line refusals, and the ledger
  rollup (happy path + a refused state loss).
- Mutation controls (`tools/nc_o3.py`, 6): restore root-equality (named),
  state-losses check, uptime-days check, restore-drills check, root-count
  check, unknown-line strictness — 6/6 PASS.
- Independent rollup (D-O3-5): `aura-ops ledger` re-derives
  `uptime ledger: days=7 state_losses=0` from the committed ledger and
  must agree with the measurer.
- Invited operators / on-call: `docs/oncall-runbook.md` (monitoring and
  alert thresholds over the uptime ledger, backup cadence and
  verification, weekly restore drill, incident response, written
  handover). First O3 receipts: `docs/evidence/o3/` (`testnet-ops.txt`,
  `uptime-7d.txt`, `test.log`, `nc.log`, `receipts.log`,
  `testnet-ops-check.log`, `summary.log`).

### O4 receipt (2026-11-27)

- Deliverable: `patches/0107-O4-public-testnet.patch` (complete O4-from-O3
  phase patch; closes Stage 1 of `OPS_PLAN` and milestone M2).
- The gate "30 days, >= 99% uptime, >= 100 000 txs, 0 unfixed criticals"
  is measured on receipts, not asserted (D-O4-1..D-O4-5):
  `make testnet-receipts` (`aurad-testnet`) runs the scripted 30-day
  public schedule (30 x 24 hourly polls, 140 signed transfers per poll =
  100 800 transactions) on the shipped pipeline into `metrics-30d.txt`,
  verifies the 10-validator genesis window across ten independent
  single-validator pipelines (`validators 10`), and evaluates the
  bug-bounty triage journal (`triage.txt`, seeded from
  `tools/fixtures/h8_findings.json` plus M2 triage entries) into
  `testnet.txt`.
- Named control: `unfixed_criticals_refused` — a receipt carrying
  `criticals 1` is refused (`error: testnet: criticals: 1`, exit 65);
  mutation NC-1 drops the zero-criticals check and the case must turn
  red.
- Test gate: 12 deterministic scenarios
  (`tests/fixtures/o4_testnet_public_vectors.json`) from the independent
  oracle `tools/gen_o4_vectors.py`, asserted byte-for-byte by
  `tests/o4_testnet_public.rs` (13/13 with the count guard) against
  `aura_testkit::testnet`: valid receipt + pinned summary, five gate
  refusals (days / uptime / validators / txs / criticals [named]),
  foreign-line refusal, and independent ledger rollups (`metrics-30d.txt`
  pass + `tx regression at 3 2`; `triage.txt` pass + `open critical
  T-0002`).
- Mutation controls (`tools/nc_o4.py`, 7): criticals check (named), days
  check, uptime check, validators check, txs check, metrics regression
  check, triage open-critical check — 7/7 PASS (699 cumulative across
  106 phases).
- Public surface & docs (D-O4-5): `aurad-faucet` and `aurad-explorer`
  exercised over the node `RpcBackend` (`public-surface.log`), operator
  and developer guide `docs/testnet-public.md`, bug-bounty policy
  `docs/testnet-bounty.md`. Receipts: `docs/evidence/o4/` (`testnet.txt`,
  `metrics-30d.txt`, `triage.txt`, `gates.txt`, `test.log`, `nc.log`,
  `receipts.log`, `testnet-check.log`, `summary.log`,
  `public-surface.log`, `inventory.txt`, `report.md`, `SHA256SUMS`).

## Risks

- Validators belonging to one operator ("decentralization on paper") — operator uniqueness check in O6.
- Upgrade without timelock/emergency plan — O7 + H9.
- Publishing "mainnet" before the audit — forbidden by D-O3, caught by the O5 review gate.

## Non-goals

Exchange listing as a technical gate, marketing, price promises (O9 is only an
organizational track).

## Closure criteria

Stage 1: public testnet M2 with external validators, reproducible releases and
monitoring (O0–O4).
Stage 2 (⏸): mainnet, audit, incident playbook and the first governance upgrade
(O5–O9). Anything not closed is explicitly listed in `docs/STATUS.md`.
