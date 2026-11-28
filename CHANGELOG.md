# Changelog — AuraLite-Crypto

## [O4 — Public testnet (M2) phase closed (OPS Stage 1 complete): the 30-day public metrics ledger (30 x 24 hourly polls, 100 800 signed transfers on the real pipeline, D-O4-2), the bug-bounty triage journal seeded from H8 findings (`triage.txt`, 0 unfixed criticals, D-O4-3), the 10-validator genesis reproduction window (D-O4-4), the public faucet + explorer CLI services over `RpcBackend` (`aurad-faucet`, `aurad-explorer`) + `docs/testnet-public.md` + `docs/testnet-bounty.md` (D-O4-5), and the checked `rule testnet-v1` receipt (D-O4-1); gate 12 scenarios + NC: unfixed_criticals_refused (open critical -> red)] 2026-11-27

Stage 1 of `OPS_PLAN` (O0–O4) and milestone **M2 (Public testnet)** are
closed on measured receipts: `aurad-testnet` runs the 30-day public
schedule (100 800 transfers across 720 blocks, 100% uptime), verifies
byte-identical genesis state roots across 10 independent validator nodes,
checks the triage journal (`items=12 open=3 criticals=0`) and writes
`testnet.txt` + `metrics-30d.txt`; `aura-testnet` (`check`, `metrics`,
`triage`, `summary`) independently re-derives every rollup and refuses
any shortfall or regression with exit 65. 12 deterministic scenarios
(`tests/fixtures/o4_testnet_public_vectors.json`) from the independent
oracle `tools/gen_o4_vectors.py`, asserted byte-for-byte by
`tests/o4_testnet_public.rs` (13/13 with the count guard). Named control:
`unfixed_criticals_refused` (`error: testnet: criticals: 1`, exit 65).
NC: 7 mutations (699 cumulative across 106 closed phases). Also restores
regressed historical evidence (`c10`, `d0`, `e9`, `w2`, `x5` standards),
guards `e9_emulator_lane.py` behind `--log`, updates `fuzz/Cargo.lock`
and `docs/coverage.txt`, and cleans stray workspace artifacts.

## [O3 — Closed testnet ops phase closed (OPS Stage 1): the uptime ledger (7-day scripted operator schedule on the real pipeline, D-O3-3: day/hour from the schedule, heights and state roots from the live chain), the live backup/restore drill over the shipped S2 backup format (create_backup/restore_backup with replayed records and recomputed roots, D-O3-2), the checked ops receipt `rule testnet-ops-v1` (D-O3-1) + on-call runbook (D-O3-4) + independent ledger rollup (D-O3-5); gate 10 scenarios + NC: restore_mismatch_refused (the restore did not restore -> red)] 2026-11-25

The OPS_PLAN O3 gate ("7 days uptime, 0 state losses, 1 restore drill")
is a receipt, not an adjective: `aurad-ops` measures the 7-day operator
window and the backup/restore drill on the shipped pipeline and backup
format and writes `testnet-ops.txt` + `uptime-7d.txt`; `aura-ops check`
refuses any criterion miss (`restore mismatch` [named], `state losses`,
`restore drills`, `uptime days`, `restore state root count`, unknown
lines) with exit 65. The measured run records 7 uptime days, 0 state
losses and 1 drill whose restored head is byte-identical
(`cd2464aa…773da3` x2). Honest boundary, published as-is: the D2
pipeline refuses to reopen a receipt-bearing store by design (restart
checkpointing is a later node integration task), so restoration is
verified at the S2 storage layer the backup format targets. 10 scenarios
(`tests/fixtures/o3_testnet_ops_vectors.json`) from the independent
oracle `tools/gen_o3_vectors.py`, asserted byte-for-byte by
`tests/o3_testnet_ops.rs` (11/11 with the count guard). Named control:
`restore_mismatch_refused` (`error: testnet-ops: restore mismatch`, exit
65). NC: 6 mutations (692 cumulative). `docs/oncall-runbook.md` is the
invited-operators procedure.

## [O2 — M0/M1 gates phase closed (OPS Stage 1): the milestone receipts (ROADMAP §5 criteria measured, not asserted — D-O2-1 `rule milestones-v1`), M0 measured on the real pipeline twice (genesis -> 1000 tx -> byte-identical state root on 2 runs, D-O2-2), M1 measured on the real 4-validator cluster (100 committed blocks, block interval <= 1 s, 0 desync, positive RPC balance through the shipped `aura_rpc::RpcService` dispatch, D-O2-3) + `docs/how-to-run.md` + first receipts (D-O2-4), two independent implementations (D-O2-5); gate 10 scenarios + NC: m0_root_drift_refused (two runs disagree -> red)] 2026-11-25

The milestones are receipts, not adjectives: `aurad-receipt` measures M0
and M1 on the shipped pipeline/cluster and writes `milestone.txt`;
`aura-milestone check` refuses any criterion miss (`m0 state root drift`,
`m0 tx count`, `m1 validators`, `m1 height`, `m1 desync`, `m1 rpc
balance`, `m1 block rate`) with exit 65. The measured run records two
identical M0 replay roots and an M1 receipt of 4 validators / 33 ms block
interval / 100 committed blocks / 0 desync / positive RPC balance. The
multi-process acceptance `tests/d7_devnet.rs` (four real `aurad`
processes) stays the deep M1 gate inside `make test`. 10 scenarios
(`tests/fixtures/o2_milestone_vectors.json`) from the independent oracle
`tools/gen_o2_vectors.py`, asserted byte-for-byte by
`tests/o2_milestones.rs` (11/11 with the count guard). Named control:
`m0_root_drift_refused` (`error: milestone: m0 state root drift`, exit
65). NC: 6 mutations (686 cumulative).

## [O1 — Releases phase closed (OPS Stage 1): the two-host reproducibility gate (`tools/repro_check.py`: two independent builds -> identical binary sha256, D-O1-1), the checked release manifest `release/manifest.txt` (`aura-release check`, D-O1-2), structural signature binding + wired RFC 8032 verify via `tools/sign_release.py` (D-O1-3), deterministic SBOM + digest-pinned multi-stage `docker/Dockerfile` + semver/CHANGELOG binding (D-O1-4), std-only SHA-256 pinned against Python hashlib (D-O1-5); gate 10 scenarios + NC: repro_mismatch_refused (two hosts disagree -> red)] 2026-11-25

The release process is a checked pipeline, not folklore: `aura_testkit::release`
validates a release directory (manifest, SHA256SUMS digests, 64-byte
detached ed25519 signatures, the SBOM regeneration, the two-host
`repro.txt` record, and the Dockerfile policy) against the workspace;
`aura-release sync` regenerates the manifest records. 10 scenarios
(`tests/fixtures/o1_release_vectors.json`) from the independent oracle
`tools/gen_o1_vectors.py` (hashlib sha256), asserted byte-for-byte by
`tests/o1_release.rs` (11/11 with the count guard). Named control:
`repro_mismatch_refused` (`error: release: repro mismatch: aura-bin`,
exit 65). The live gate `make repro-check` builds `aurad` twice into
independent target directories with SOURCE_DATE_EPOCH + path remapping and
records identical digests in `release/repro.txt`. This bundle also
regenerates `docs/evidence/o0/` after correcting the named-control string
check in `tools/write_o0_evidence.py` (it matched the pre-redesign `shard
07` text); the O0 verdicts are unchanged. NC: 6 mutations (680 cumulative).

## [0.1.0 — first release line (semver baseline, O1)] 2026-11-25

Workspace version 0.1.0 (`[workspace.package]`, every crate on
`version.workspace = true`) covers the whole C0..O1 record below; the
release check binds this entry (`## [0.1.0`), the manifest `version` line
and the workspace version together (`version drift` -> red).

## [O0 — CI baseline phase closed (OPS Stage 1): the shard registry `ci/registry.txt` (10 shards over 127 `--lib`/`tests/*` targets, `make ci-sync`, D-O0-1), the target registry check `aura-ci check` wired into `make test` (D-O0-2), no secret shards — the format cannot carry commands, `aura-ci run --shard K` emits plain `cargo test` forms (D-O0-3), deterministic target-coverage report (D-O0-4), two independent implementations (D-O0-5); GitHub Actions matrix 0..9 + cargo cache + coverage job; gate 10 scenarios + NC: disabled_shard_refused (disable a shard -> CI red)] 2026-11-25

The CI layout is a checked registry, not YAML folklore: `aura_testkit::ci`
validates `ci/registry.txt` against the discovered workspace targets
(shard shape 10/ids 0..9, stale entries, duplicates, missing targets,
disabled shards holding targets, empty shards, foreign lines) and `make
test` runs that check. `aura-ci coverage` prints the pinned target-coverage
report (`docs/coverage.txt`). 10 scenarios
(`tests/fixtures/o0_ci_vectors.json`) from the independent oracle
`tools/gen_o0_vectors.py`, asserted byte-for-byte by `tests/o0_ci.rs`
(11/11 with the count guard); named control `disabled_shard_refused`.
Mutation controls `tools/nc_o0.py` 6/6 (disabled-shard, missing-target,
duplicate, shard-count, empty-shard, unknown-line strictness) + green
restore; `tools/write_o0_evidence.py`; audit: **102 phases, 674 NC**.

## [W7 — Genesis ceremony phase closed (WALLET Stage 2): offline deterministic key generation (BLAKE3(KEY_CONTEXT || entropy || i), D-W7-1), Shamir GF(2^8) sharing with deterministic coefficients + Lagrange-at-0 reconstruction (D-W7-2), hash-chained witness log (D-W7-3), destruction of temporary media before seal (D-W7-4), genesis commits to exactly the witnessed pubkey set (D-W7-5); gate 10 scenarios + NC: no_private_key_in_outputs (O5: not one private key in the repository/CI)] 2026-11-25

The ceremony (`crates/aura-genesis/src/ceremony.rs`, bin `aura-ceremony`) runs
open/generate/split/shred/seal/reconstruct/verify on an air-gapped host:
validator keys are derived deterministically from ceremony entropy, shares
are Shamir shards over GF(2^8), every step is witnessed by a hash-chained log
whose `generate` entries commit to the pubkey set, and `shred` destroys
`media/` (double overwrite) before `seal` builds the canonical `Genesis`
(refusals: `validator set mismatch` / `temporary media present` / `witness
chain broken` / `witness log incomplete` / `not enough shares` / `duplicate
share`). `verify` scans the ceremony tree for any `media/` file or `secret=`
line. 10 scenarios (`tests/fixtures/w7_ceremony_vectors.json`) from the
independent oracle `tools/gen_w7_vectors.py` (gen_c8 genesis primitives, gen_c4
Ed25519), asserted byte-for-byte by `tests/w7_ceremony.rs` (11/11 with the
count guard); named control `no_private_key_in_outputs`. Mutation controls
`tools/nc_w7.py` 6/6 (shredding skipped, threshold dropped, chain check
dropped, pubkey binding dropped, reconstruction corrupted, media check
dropped) + green restore; `tools/write_w7_evidence.py`; audit: **101
phases, 668 NC**.

## [W6 — Web wallet phase closed (WALLET Stage 2): real-tx-field confirmation (`aura_wallet::web`, CLI `web-sign`), confirmation binding with doctored-view refusal (D-W6-2), phishing policy https-only + ASCII-host homoglyph gate (D-W6-3), strict `auralite://sign?…` schema (D-W6-4), reproducible `aura-keys` WASM build (8 exports, 0 imports) + untrusted extension glue (D-W6-5); gate 10 scenarios + NC: recipient substitution in UI -> red] 2026-11-25

The confirmation view is derived exclusively from the transaction bytes; a
page/deep-link claim is compared against the bytes and a disagreement is
refused with the **real** fields still shown (`error: web: recipient
substitution` / `amount substitution`). Signing requires the rendered view;
blind requests are refused before any key use. The trusted core
`crates/aura-wallet/src/web.rs` is compiled verbatim into the standalone
`clients/aura-keys` crate (own lockfile, root `Cargo.lock` untouched), built
for `wasm32-unknown-unknown` twice with remapped paths — byte-identical,
pinned by `tools/build_aura_keys.py`; `clients/wallet-web/` is untrusted MV3
glue. 10 scenarios (`tests/fixtures/w6_web_wallet_vectors.json`) from the
independent oracle `tools/gen_w6_vectors.py`, asserted byte-for-byte by
`tests/w6_web.rs` (11/11 with the count guard); named control
`ui_recipient_substitution`. Mutation controls `tools/nc_w6.py` 6/6 (recipient
comparison, amount comparison, homoglyph check, deep-link whitelist, blind
acceptance, recipient render) + green restore; `tools/write_w6_evidence.py`;
audit: **100 phases, 662 NC**.

## [W5 — Hardware keys phase closed (WALLET Stage 2): HID/CCID-like device protocol with on-screen confirmation (`aura_wallet::hardware`), display derived exclusively from the signed bytes (D-W5-1), showable-payloads-only + host-claim binding (D-W5-2), `Armed -> Confirm` binding with deny-retry (D-W5-3), plain `tx` domain signatures (D-W5-4); gate 10 confirmed/denied scenarios + NC: blind confirmation -> red] 2026-11-24

The device is a deterministic secure-element simulation: `GetVersion` /
`BeginSign` / `Decide` / `BlindSign` frames in, `Version` / `Display` /
`Signed` / `Refused` out (`crates/aura-wallet/src/hardware.rs`, CLI
`hw-sign`). The screen shows the nine unsigned protocol fields plus the
decoded intent; `transfer` payloads must be `recipient||amount` (28 bytes),
`delegate`/`undelegate` the canonical stake payload — anything else is
refused as `undecodable payload` (a device never signs bytes it cannot
display). 10 human confirmed/denied scenarios
(`tests/fixtures/w5_hardware_vectors.json`) from the independent oracle
`tools/gen_w5_vectors.py`, asserted byte-for-byte by `tests/w5_hardware.rs`
(11/11 with the count guard); named control `blind_confirmation_rejected`.
Mutation controls `tools/nc_w5.py` 6/6 (blind acceptance, denial ignored,
display binding dropped, chain check dropped, amount render altered, payload
decode check dropped) + green restore; `tools/write_w5_evidence.py`; audit:
**99 phases, 656 NC**.

## [P6 — Published SLOs (PERF Stage 2, finale): north-star registry 6/6 (spread 0, verify 22, parallel 15 543, commit 12 300, traffic 86 495, finality 586), tamper-evident run log (hash chain, tip `ac0f9067…e52a`), 6 methodologies + 4 honest "did not work" items, dashboards; NC: rigged run → log check red + 5 revert mutations] 2026-11-24

**PERF PLAN COMPLETE (P0–P6).** The `aura-bench::p6` profile (D-P7):
a registry of six north-star SLOs (the P0–P5 DoD values, pinned), a tamper-
evident run log (hash chain `sha256(prev|"|"|line)`, tip
`ac0f9067…e52a`; rigged run → `P6: rigged run detected (key=value)`),
methodology (6 sections `pN-methodology`, D-P3), dashboards
(`docs/perf/slo-dashboard.md`), honest "did not work" (4 items:
`p2-measured-wall-clock`, `p3-measured-sandbox-fsync`,
`p1-bls-model-only`, `p2-lcg-fixture`). 11 tests (DoD: north star
6/6 confirmed; NC: rigged run → log check red); the twin
`tools/p6_slo_check.py` (example ↔ python IDENTICAL; the measured layer
D-P9: a 4096-entry chain — 7.8/6.9 ms); negative controls
`tools/nc_p6.py` 6/6 (northstar/chain/**rigged**/methodology/honest/
determinism) + green restore; `tools/write_p6_evidence.py`; audit:
**98 phases, 650 NC** — PERF PLAN COMPLETE.

## [P5 — Consensus phase closed (PERF Stage 2): pipelining (happy 3-chain 357→189 ms), QC cache (re-verify 24→2 ms), faster view change (stall 2119→397 ms, timeout 2000→300 ms); gate p99 finality 586 < 2000 ms at 100 nodes + 6 NC revert mutations] 2026-11-24

- **`aura-bench::p5`** — a deterministic millisecond-accurate model of the
  3-chain commit path (decision D-P7: a self-contained profiling artifact;
  moving the tuning into `aura-consensus`/`aura-node` — follow-up per D-P6/D-P7):
  100 validators (D-P1); proposal → vote round → BLS-QC (2f+1 = 67) →
  the 3-chain rule (D-B1/D-B2) → view-change stall. The baseline is the actual
  shipped path: per-signature QC re-verification (B2) + `DEFAULT_VIEW_TIMEOUT_MS
  = 2000` from `aura-consensus::finality`.
- **DoD (PERF_PLAN P5)** met: **p99 finality 586 ms < 2 000 ms at 100
  nodes** (the SLO is plan-fixed; the methodology is published — D-P3:
  `docs/perf/p5-consensus-report.md`); the baseline 2 476 ms honestly fails
  the SLO. Three optimizations with before/after pairs (D-P2): **pipelining** happy 3-chain
  357 → 189 ms (proposal overlapped with chain extension), **QC cache** 24 →
  2 ms/view (O(1) instead of per-signature), **faster view change** stall
  2 119 → 397 ms (timeout 2 000 → 300 ms). The pinned percentile rule:
  1% view change per view in a 3-view window → happy share 970 permille → p50 = p95 =
  189 ms, p99 = happy + one stall. The 2-view-window sensitivity (586 ms) also fits
  the SLO. Structural gates: the 3-chain rule (depth pinned at 3) and quorum
  (a QC at exactly 67 of 100 votes). **NC: revert → red** (`tools/nc_p5.py`,
  6/6 + green restore; messages `P5: …` ×6). 13 tests in `tests/p5_consensus.rs`;
  `tools/p5_consensus_check.py` — the model twin (writes `sim/p5-consensus.json`);
  example ↔ python — IDENTICAL; the D-P9 measured layer, non-gate;
  `make test-p5|check-p5|examples-p5|nc-p5|evidence-p5`.

## [P4 — Network phase closed (PERF Stage 2): vote batching, RLE compression, gossipsub parameter tuning (GossipFactor 25%→10%), mesh tuning (D 8→5); gate 100 nodes traffic/validator 86 495 ≤ 125 000 B/s budget (−74% vs baseline) + 6 NC revert mutations] 2026-11-24

- **`aura-bench::p4`** — a deterministic byte-accurate model of the consensus-gossip
  path (decision D-P7: a self-contained profiling artifact; moving
  the parameters into `aura-p2p` — follow-up per D-P6/D-P7): 100 validators × 1
  view/s; votes + proposal + QC per view; IHAVE/IWANT advertisement overhead
  (`GossipFactor` × 2 hops); validator egress = D × (messages + overhead).
  The vocabulary is gossipsub v1.0 (D/D_low/D_high/heartbeat) + N2 (`vote`,
  `proposal`, `QC`).
- **DoD (PERF_PLAN P4)** met: **traffic/validator 86 495 B/s ≤ the budget
  125 000 B/s (1 Mbit/s egress at 1 view/s)**; the budget is published with
  its methodology (D-P3: `docs/perf/p4-network-report.md`); the baseline 334 848 B/s
  honestly fails the budget ×2.7. Four optimizations with before/after pairs
  (D-P2): vote batching k=8 (19 200 → 13 632), RLE compression of the proposal
  (8 512 → 592, a lossless gate), gossipsub parameters (overhead 13 952 → 2 883,
  D_low=3 ≤ D=5 ≤ D_high=8, heartbeat 700 ms, mcache 5), mesh tuning
  (D 8 → 5, ring+chords connectivity on 100 nodes); the total −74%. The 2:1 sensitivity
  (108 860 B/s) also fits the budget. **NC: revert → red** (`tools/nc_p4.py`, 6/6 +
  green restore; messages `P4: …` ×6). 12 tests in `tests/p4_network.rs`;
  `tools/p4_network_check.py` — the model twin (writes `sim/p4-network.json`);
  example ↔ python — IDENTICAL; the D-P9 measured layer, non-gate;
  `make test-p4|check-p4|examples-p4|nc-p4|evidence-p4`.

## [P3 — Storage phase closed (PERF Stage 1): mini-LSM commit path with batch writes, bloom (10 bits/key, k=7), compaction; gate p99 12 300 < SLO 15 000 (+2428% vs fsync-per-write baseline) + NC fsync not in the commit path -> S7 red] 2026-11-24

- **`aura-bench::p3`** — the storage-subsystem commit-path profile (decision D-P7:
  a self-contained profiling artifact; tuning the `aura-storage` RocksDB backend
  — follow-up per D-P6): a mini-LSM engine (WAL with fsync **before publishing** — the contract
  S7 of `aura-storage`) + memtable + sorted runs + a Bloom filter (10 bits/key, k=7)
  + compaction (merge, "last write wins") + crash recovery (replaying only the
  synced tail — NC S7-6); a WriteBatch optimization of the commit path.
- **DoD (PERF_PLAN P3)** met: the deterministic model — **p99 block
  commit 12 300 units < SLO 15 000 units** (the fsync-per-write baseline: 311 000 —
  honestly fails the SLO), +2428% (D-P2 before/after); bloom cuts lookup-miss
  40 → 1 unit (fp 0/200 pinned). **NC: fsync in the wrong place → S7 red**
  (`tools/nc_p3.py`, 6/6 mutations red + green restore; named
  messages `P3: …` ×6, including `fsync missing in commit path (batch N)`). The SLO
  is published with its methodology (D-P3): `docs/perf/p3-storage-report.md` + the note
  `docs/evidence/p3/p3-storage-durability.txt` (model goldens + measured
  the D-P9 layer is honestly marked non-gate). 11 tests in `tests/p3_storage.rs`;
  `tools/p3_storage_check.py` — the model twin (writes `sim/p3-storage.json`);
  example ↔ python — IDENTICAL; `make test-p3|check-p3|examples-p3|nc-p3|evidence-p3`.

All notable changes. Dates are ISO 8601, Europe/Moscow. Historical entries are
never rewritten: corrections are made with a new entry or an explicit note.

## [P2 — Parallel execution phase closed (stretch): optimistic MVCC / Block-STM-like engine; gate 1000 txs parallel == sequential with honest rollback; +536% model, honest negative wall-clock] 2026-11-24

### Added

- `aura_bench::p2` — deterministic optimistic parallel executor
  (P2, D-P6): chunk speculation over a write overlay (Block-STM
  multi-version shape), read-set validation at commit, conflicts abort
  → discard speculative writes (honest rollback) → re-execute in
  serializable index order (Kung-Robinson OCC); pinned fixture of 1000
  txs / 2096 accounts (936 independent transfers + 32 structural
  cross-chunk conflicts); cost model seq 99 000 → par 15 543 units
  (+536%, retry costs a full re-execution); `P2Report` (pinned digest
  `96863c0b…65174`), `P2Error` (six named `P2: …` messages), gates
  (`gate_parallel` with leak diagnosis, `gate_commit_order`,
  `gate_fixture`), and the honest wall-clock layer `measure` with two
  parallel numbers (cold spawn / steady-state worker pool; evidence
  only — release: −49% / −5% on 1000-tiny-tx blocks, negative result
  published, D-P9).
- `crates/aura-bench/tests/p2_parallel.rs` (12 tests: DoD equality,
  honest rollback with leak diagnosis, serializable order, structural
  conflict accounting, engine determinism under real threads, 2000-tx
  disjoint scale-out, balance conservation), example
  `p2_parallel_example.rs`, `tools/p2_parallel_check.py` (independent
  Python engine — byte-identical twin), `tools/nc_p2.py` (6/6 revert
  mutations), evidence tier `docs/evidence/p2/` +
  `tools/write_p2_evidence.py`, RU report
  `docs/perf/p2-parallel-report.md`, `sim/p2-parallel.json`.
- Makefile: `parallel execution gate (P2)` in `test` + targets
  `test-p2`/`check-p2`/`examples-p2`/`nc-p2`/`evidence-p2`.

### Changed

- `docs/plans/PERF_PLAN.md`: P2 row met (DoD equality + honest
  rollback + published negative wall-clock), `### P2 receipt`.
  Cumulative negative controls: 626 (34 phases).

## [P1 — Codec/crypto phase closed: batch signature verification, zero-copy decode, BLS cost model; gate +72% verify/s with the same determinism + NC revert -> red] 2026-11-24

### Added

- `aura_crypto::verify_batch_raw` / `BatchItem` — batch Ed25519
  verification (one Merlin batch equation over `ed25519_dalek::
  verify_batch`, feature `batch`; lockfile gains merlin 3.0.0) with the
  strict single-verifier semantics kept: canonical-S pre-check per item
  (H2 malleability) and first-failing-index diagnostics.
- `aura_codec::transfer` — P1 zero-copy transfer wire block (canonical
  350-byte form: version, from, to, amount, nonce, memo_len + zero
  padding): `decode_zero_copy` (borrowed view, no allocations) and
  `decode_copy` (copying baseline) as independent code paths plus
  `encode`; `TransferError` case names drive the malformed-input gate.
- `crates/aura-bench/src/p1.rs` — P1 cost model (D-P2 before/after:
  verify 38 -> 22 units per signature at batch k=8 = **+72% verify/s**,
  decode 25 -> 12, BLS FastAggregateVerify 60 vs 352 — MODEL ONLY,
  decision D-P4), `P1Report` (pinned digest `340a884d…80241`),
  `P1Error` (six named `P1: …` messages), functional gates
  (`gate_batch` == sequential incl. S+L malleated twin, `gate_zero_copy`
  field-by-field equality, `gate_malformed` joint rejection), and the
  honest wall-clock layer `measure` (evidence only, D-P9: real
  ed25519-dalek batch +29% verify/s at k=8 on the CI host).
- `crates/aura-bench/tests/p1_crypto.rs` (12 tests incl. the literal
  DoD +72% and "revert -> red" states), example
  `p1_crypto_example.rs`, `tools/p1_crypto_check.py` (byte-identical
  twin), `tools/nc_p1.py` (6/6 revert mutations), evidence tier
  `docs/evidence/p1/` + `tools/write_p1_evidence.py`, RU report
  `docs/perf/p1-crypto-report.md`, `sim/p1-crypto.json`.
- Makefile: `codec/crypto gate (P1)` in `test` + targets
  `test-p1`/`check-p1`/`examples-p1`/`nc-p1`/`evidence-p1`.

### Changed

- `Cargo.toml`: `ed25519-dalek` gains the `batch` feature (merlin 3.0.0
  enters the lockfile; supply-chain lock pin updated accordingly).
- `docs/plans/PERF_PLAN.md`: P1 row met (+72% verify/s, NC revert ->
  red), `### P1 receipt`. Cumulative negative controls: 620 (33 phases).

## [P0 — Baseline bench phase closed: criterion + harness, fixed hardware, first honest numbers; gate 5 repeat runs spread 0% < 5% + substituted-workload functional NC] 2026-11-23

### Added

- `crates/aura-bench` — new workspace crate (P0 baseline bench,
  PERF_PLAN P0; decisions D-P1/D-P2/D-P3 + D-P9/D-P9b): `MachineProfile`
  (D-P1 fixed hardware: 8 vCPU / 32 GiB / NVMe / 100 validators /
  1 Gbit/s) and `Workload` (350-byte transfer, 1 input / 1 output) with
  canonical-form digests; `CostModel::BASELINE` (decode 4+bytes/16,
  verify 38/input, execute 10+15/output, commit 40 → 128 cost units per
  run); nearest-rank `percentile` (p50/p95/p99) and integer `spread_pct`;
  `BenchReport::build` pins the deterministic report (5 runs, spread 0,
  total 640, digest `cfc46920…20ec`); `validate` — the P0 criterion
  gate control in fixed order (repeat count → determinism → spread →
  workload → machine → published metrics). 6 named messages
  (`P0: …`). Honest measured layer `measure_pipeline_ns` (real 350-byte
  sha256 + ed25519 verify, wall-clock ns/tx) — evidence only, never a
  gate (D-P9).
- `crates/aura-bench/tests/p0_bench.rs` — 8 gate tests: pinned golden
  numbers (the literal DoD "5 repeat runs: spread < 5%"), the
  functional negative control "substituted workload → different
  numbers" (350B→128 vs 352B→129, digests differ, named message),
  exact NC literals, validate-suite, metamorphic controls (pure
  determinism, run scaling, percentiles, cost breakdown).
  `examples/p0_bench_example.rs` ↔ `tools/p0_bench_check.py`
  (independent Python model) — byte-identical `P0-RUNS-OK 5` /
  `P0-BENCH-OK {…}`; `sim/p0-bench.json`.
- `tools/nc_p0.py` (6 mutation controls), `tools/write_p0_evidence.py`,
  evidence tier `docs/evidence/p0/`, report
  `docs/perf/p0-baseline-report.md` (sources: criterion.rs measurement
  protocol (warm-up → sample → statistics), consensus-benchmark
  methodology (P50/P95/P99 inclusion latency), OpenChainBench
  methodology (identical inputs, honest aggregates, auditable runs, no
  cherry-picking)). Decisions D-P9/D-P9b.

## [E9 — Bridge + AuraLite OS phase closed: IBC-like light-client bridge, limits/pause; gate MITM forged header -> reject, OS client checks finalized checkpoint, emulator lane green] 2026-11-22

### Added

- `crates/aura-storage/src/bridge.rs` — E9 model (D-E4 protections):
  IBC/Tendermint-shaped light client (header hash chain, ed25519 commits
  via `aura_crypto` RFC 8032, strictly-more-than-2/3 quorum = 27/40,
  **every** commit signature verified — unknown signer, bit-flipped
  signature and duplicate votes are `E9: forged header signature (…)`,
  no early stop), bridge protections (per-transaction limit 1000 /
  daily velocity limit 1200 with day-window reset, guardian pause,
  one-time message ids — Nomad replay lesson, reserve invariant
  `locked == minted` with `withdraw exceeds reserve` — Wormhole lesson,
  state-root binding of every message) and `OsClient`, the
  finalized-checkpoint verification entry point for AuraLite OS
  (`verify_checkpoint` / `accept` / `check_root`). 12 named messages
  (`E9: …`).
- `crates/aura-storage/tests/e9_bridge.rs` — 8 gate tests: the three
  DoD criteria (4 MITM forgery shapes rejected; OS client finalized-
  checkpoint rule incl. forged checkpoint; emulator lane scenario with
  the pinned 25-step log), exact NC literals, remaining named messages,
  metamorphic commit-order / limit-boundary / determinism / quorum
  controls. `examples/e9_bridge_example.rs` ↔ `tools/e9_bridge_check.py`
  (independent model, RFC 8032 via `cryptography`) — identical
  `E9-STEPS-OK 25` / `E9-SIM-OK` output; `sim/e9-bridge.json`.
- `tools/e9_emulator_lane.py` (`make emulator-lane`, registered in
  CI `ci.yml`) — the `aurad` CI lane (E9-LANE-OK 4: `--version`,
  `report --fixture`, startup refusal without `--chain-id`, bounded node
  run + clean SIGTERM shutdown); in-repo stand-in for AuraLite-Emulator
  (deviation recorded). `tools/nc_e9.py` (6 mutation controls),
  `tools/write_e9_evidence.py`, evidence tier `docs/evidence/e9/`,
  report `docs/ecosystem/e9-bridge-report.md` (sources: tendermint spec
  verification_002_draft, chainscorelabs IBC light client,
  docs.tendermint.com accountability, ipsa-contracts VerifyCommitLight;
  nomoslabs Nomad/Wormhole post-mortems, dev.to/0xrivet, zealynx
  checklist, blockchaindose). Decisions D-E9..D-E9c.

## [E8 — NFT + names + DAO phase closed: ARC-721/1155, .aura name service, on-chain identity, governance UI; gate 50 standard vectors + first DAO vote through the UI] 2026-11-22

### Added

- `crates/aura-storage/src/nft.rs` — E8 model: ARC-721 (per the repo's
  X5 `standards/arc721`; transfers clear approvals, burned ids may be
  re-minted), ARC-1155 (per-id supply, operators via `set_operator`,
  all-or-nothing batch transfers, URIs), the ENS-shaped `.aura` name
  service (365-day term, 90-day grace, renewal by anyone, re-register
  after expiry+grace, primary/reverse resolution) and on-chain identity
  records (metadata digest bound to a live name). 25 named messages
  (`E8: …`).
- `crates/aura-storage/src/dao_ui.rs` — governance UI over the T6 DAO:
  propose/vote/finalize/execute/veto through the controller, per-vote
  `UiReceipt` action log, deterministic self-contained HTML page
  (`docs/evidence/e8/governance-ui.html`), pinned scenario
  `run_e8_scenario` (the first DAO vote cast through the UI).
- `crates/aura-storage/tests/fixtures/e8-standards.txt` +
  `tools/gen_e8_vectors.py` — **50 standard vectors** (golden fixture,
  block format, exact error literals and post-state reads);
  `crates/aura-storage/tests/e8_nft.rs` — 15 gate tests (vectors with
  exact literals, digests, DAO scenario, UI page, DAO error messages,
  metamorphic batch/order/determinism controls);
  `examples/e8_nft_example.rs` ↔ `tools/e8_nft_check.py` — identical
  `E8-VECTORS-OK 50` / `E8-UI-OK` / `E8-SIM-OK` output and byte-equal
  governance page; `tools/nc_e8.py` (6 mutation controls),
  `tools/write_e8_evidence.py`, evidence tier `docs/evidence/e8/`,
  report `docs/ecosystem/e8-nft-report.md` (sources: rareskills.io
  ERC-1155, eco.com, webopedia, metana; gate.com/chainscorelabs/
  cryptoslate/docs.ens.domains ENS). Decisions D-E6..D-E8.

## [E7 — Lending phase closed: markets, kinked interest rate model, collateral factor, liquidation engine; gate 30-day simulation collateral >= debt invariant] 2026-11-22

### Added

- `crates/aura-storage/src/lending.rs` — Aave/Compound-style lending
  market: LTV 75% borrow cap, liquidation at health < 12500 bps
  (LTV > 80%, buffer to LTV), 5% liquidation bonus, 50% close factor,
  kinked utilization rate model (base 2% + slope1 10%·U/kink, kink
  80%, slope2 30% above), daily floor interest accrual, 256-bit
  `mul_div`/`mul_div_ceil` math. 12 named messages (`E7: …`).
- `crates/aura-storage/tests/e7_lending.rs` — 14 gate tests: literal
  gate (30-day simulation, `collateral_value >= debt` never breaks per
  account across 5 price shocks and 16 liquidations); functional NC
  (same path without liquidations breaks the invariant on day 18 —
  "no liquidation -> red"); golden rate-model and capacity boundaries;
  metamorphic supply split, borrow/repay round trip, interest
  monotonicity, determinism.
- `crates/aura-storage/examples/e7_lending_example.rs` +
  `sim/e7-lending.json` — deterministic scenario (`E7-SIM-OK`),
  independent Python model `tools/e7_lending_check.py` (`E7-MODEL-OK`),
  aggregate digest f8fa28de…bea7dd.
- `tools/nc_e7.py` — 6 negative-control mutations (one per targeted
  named message; `bad-borrow-capacity` is the gate mutation),
  `tools/write_e7_evidence.py`, evidence tier `docs/evidence/e7/`,
  report `docs/ecosystem/e7-lending-report.md` (sources: blofin.com
  Aave vs Compound, academy.redduck.io lending lecture, Medium DeFi
  lending comparative, GitHub lending-protocol reference). Order D-E3
  kept: after the oracle (E5), the H8 audit, the AMM (E4) and the CDP
  (E6).

## [E6 — Stablecoin CDP phase closed: over-collateralized vaults, liquidations, peg stress; gate 200% collateral at -40% price -> liquidation with no bad debt] 2026-11-22

### Added

- `crates/aura-storage/src/cdp.rs` — MakerDAO-style CDP engine:
  min collateral ratio 150% (`E6_MIN_RATIO_BPS`, ETH-A liquidation
  ratio), liquidation penalty 5% (`E6_LIQ_PENALTY_BPS`, USDC-A/Raft
  precedent), 256-bit `mul_div` floor math, mint/withdraw keep the
  ratio, `liquidate` seizes `debt·(1+penalty)` at the oracle price and
  returns the excess to the owner, peg invariant
  `bad_debt == 0 && backed_value >= stable_supply`. 12 named messages
  (`E6: …`).
- `crates/aura-storage/tests/e6_cdp.rs` — 14 gate tests: literal gate
  (200% collateral at -40% price -> liquidation with no bad debt);
  property peg backing never breaks across the stress sequence;
  NC mint-ratio guard message weakened -> red; metamorphic deposit
  split, mint/repay round trip, liquidation-order commutativity,
  determinism; golden boundaries (max mint 133333333333333333333,
  max withdraw 50e18, seized 233333333333333333333 at price 0.45).
- `crates/aura-storage/examples/e6_cdp_example.rs` + `sim/e6-cdp.json` —
  deterministic scenario (`E6-SIM-OK`), independent Python model
  `tools/e6_cdp_check.py` (`E6-MODEL-OK`), aggregate digest
  9b64b653…a499c95.
- `tools/nc_e6.py` — 6 negative-control mutations (one per targeted
  named message; `bad-mint-ratio` is the gate mutation),
  `tools/write_e6_evidence.py`, evidence tier `docs/evidence/e6/`,
  report `docs/ecosystem/e6-cdp-report.md` (sources: MakerDAO vaults
  docs, chainscorelabs.com CDP engine architecture, MakerDAO
  whitepaper explainer). Order D-E3 kept: after the oracle (E5), the
  H8 audit and the AMM (E4).

## [E4 — AMM DEX phase closed: constant-product pools, LP tokens, swap + slippage guard, routing; gate invariant k never decreases] 2026-11-22

### Added

- `crates/aura-storage/src/amm.rs` — constant-product AMM (x·y = k):
  fee-on-input 30 bps (0.3%, Uniswap V2 model), floor rounding at every
  division, 256-bit `wide_mul`/`mul_div` arithmetic, invariant
  `k = reserve_a · reserve_b` tracked in 256-bit limbs, LP tokens
  (initial mint `a + b` — explained deviation from `sqrt`; later mints
  strictly proportional; burn pro-rata with exact round trip),
  `swap_exact_in` with slippage guard, `swap_route` multi-hop routing.
  9 named messages (`E4: …`).
- `crates/aura-storage/tests/e4_amm.rs` — 21 gate tests: invariant `k`
  never decreases across swaps and routes (the literal gate property);
  NC slippage guard off -> red; metamorphic split ≤ single, route =
  manual hops, LP burn round trip, determinism; golden values
  (1 A→B out of (100/200) = 1974316068794122597, route = 5791186053878244695).
- `crates/aura-storage/examples/e4_amm_example.rs` + `sim/e4-amm.json` —
  deterministic scenario (`E4-SIM-OK`), independent Python model
  `tools/e4_amm_check.py` (`E4-MODEL-OK`), aggregate digest
  c1286a35…283865.
- `tools/nc_e4.py` — 6 negative-control mutations (one per named
  message; the slippage-guard mutation is the gate mutation),
  `tools/write_e4_evidence.py`, evidence tier `docs/evidence/e4/`,
  report `docs/ecosystem/e4-amm-report.md` (sources: Uniswap V2
  whitepaper, dev.to/autheo/zealynx AMM articles). Order D-E3 kept:
  DeFi after the oracle (E5) and the H8 audit.

## [E5 — Oracle phase closed: signed feed network, median, quorum N/2+1, staleness 3600s; gate 5 providers / 2 malicious -> price unchanged] 2026-11-22

### Added

- `crates/aura-storage/src/oracle.rs` — decentralized price feed
  network: ed25519-signed provider reports (canon
  `feed16||price16BE||ts8BE`), strict admission with 7 named messages,
  quorum N/2+1 (min 3), lower-median aggregation, ±500 bps outlier
  band, staleness window 3600 s (constants `E5_MAX_STALENESS_SECS`,
  `E5_MIN_QUORUM`, `E5_DEVIATION_BPS`, `E5_ONE` = 10^18, D-T1 u128).
- `crates/aura-storage/tests/e5_oracle.rs` — 14 gate tests: 5 providers
  / 2 malicious -> price unchanged; NC one provider -> red; bad
  signature, stale, future, duplicate, foreign feed, unknown provider,
  empty round, outlier rejection; metamorphic permutation/scale and
  determinism.
- `crates/aura-storage/examples/e5_oracle_example.rs` +
  `sim/e5-oracle.json` — deterministic round simulation
  (`E5-SIM-OK`), independent Python model `tools/e5_oracle_check.py`
  (`E5-MODEL-OK`), aggregate digest aee332b6…dead24.
- `tools/nc_e5.py` — 6 negative-control mutations (one per named
  message), `tools/write_e5_evidence.py`, evidence tier
  `docs/evidence/e5/`, report `docs/ecosystem/e5-oracle-report.md`
  (sources: Chainlink DON/OCR, Pyth, UMA).

## [T8 — White paper and law phase closed: MiCA checklist Art. 4/6/8/9, notification >= 20 working days, publication; not legal advice] 2026-11-22

### Added

- `docs/whitepaper.md` — the crypto-asset white paper (project,
  technology, risks, holder rights; summary, table of contents,
  notification-date field per Art. 6) with every quantitative claim
  traceable to a pinned model constant; `docs/marketing.md` — marketing
  materials with the mandatory disclaimer and the after-publication
  rule (Art. 7); `docs/legal-review.md` — the EU/Latvia review
  perimeter: the 12-point MiCA checklist (Regulation (EU) 2023/1114
  Art. 4/6/8/9) with per-item status. **Not legal advice** (D-T4); no
  income promises (D-T2).
- `crates/aura-storage/src/whitepaper.rs` — the MiCA white paper
  machine: Art. 4 (offeror is a legal person; paper drawn up/notified/
  published), Art. 6 (four sections + claims pinned to `MAX_SUPPLY`,
  `GENESIS_SUPPLY`, `T3_INITIAL_BASE_FEE`, `T6_PROPOSAL_THRESHOLD`,
  `T7_SIGNER_THRESHOLD`), Art. 8 (`T8_NOTICE_WORKING_DAYS = 20` working
  days — boundary: exactly 20 passes, 19 is red), Art. 9 (publication
  before the offer starts; published version identical to the notified
  one), Art. 7 (marketing references the published white paper and
  follows it), mandatory `T8_NOT_LEGAL_ADVICE` disclaimer.
- `crates/aura-storage/tests/t8_whitepaper.rs` (10 gate tests: the
  four-article checklist walk, claim traceability, the 20-working-day
  boundary, publication/marketing/disclaimer gates, pinned digest;
  metamorphisms — claim reordering, section order), the example
  `t8_whitepaper_example` (`T8-SIM-OK`), `sim/t8-whitepaper.json` (8
  documents, 77 steps, outcomes `[0, 2, 3, 4, 7, 6, 1, 9]`) with golden
  digest `03a859433b58f14d6ad538faf2f3b504631c8b05707856a6ef1580694a89396f`
  and `docs/economics/t8-whitepaper.md`.
- `tools/t8_whitepaper_check.py` (the phase gate oracle: independent
  step-log model, same digest), `tools/nc_t8.py` (7 mutation controls:
  unverified claim accepted, legal-person/notice-window/publication/
  marketing/disclaimer gates disabled, parameter drift),
  `tools/write_t8_evidence.py` (`docs/evidence/t8/`, cumulative
  inventory `TOTAL: 572 negative controls over 25 phases`).
- Wiring: Makefile `white paper gate (T8)` + `test-t8/check-t8/
  examples-t8/nc-t8/evidence-t8`; `tools/audit_closeout.py`
  (`CURRENT = "T8"`, `SHIPPED` += T7, `ECONOMICS_PHASES` += T7+T8,
  `NC_PER_PHASE["T8"] = 7`, `PATCH_ORDER` += T7+T8); `### T8 receipt`
  in `docs/plans/ECONOMICS_PLAN.md`.

### Restored

- The workspace was reset mid-session (reset #10: `crates/` reverted to
  the github state, `.git` and `cargo` removed). The T0–T7 tree was
  restored from the delivered combined delta
  (`AuraLite-T7-delta/AuraLite-T0-T7-vs-github-e1a5b56.patch`) as
  commit `983196e` «T0 - T7 update» (byte-identical to the T7 canon
  `6e69f32` of the pre-reset chain) and Rust 1.99.1 was reinstalled.

## [T7 — Treasury and grants phase closed: every payout has proposal + tx + report, withdrawal without vote rejected, multisig 2-of-3 (W4)] 2026-11-21

### Added

- `crates/aura-storage/src/treasury.rs` — the T7 Treasury: an on-chain
  treasury with a signed `i128` balance (an overdraw is visible and
  rejected), a council of 5 (strict majority) voting payout proposals,
  a multisig of 3 with threshold 2 (decision W4) and the lifecycle
  propose -> vote -> finalize -> approve -> execute. Every payout has
  **proposal + tx + report**: a numbered tx and a transparent
  `Report {tx_id, proposal_id, amount, recipient, purpose, height}`;
  a withdrawal without a passed vote is rejected (`NoVote`); a payout
  cannot execute twice (`AlreadyExecuted`); a rejected proposal never
  pays (`Rejected`); the treasury cannot overdraw
  (`InsufficientFunds`).
- `crates/aura-storage/tests/t7_treasury.rs` (10 gate tests + 2
  metamorphisms), the example `t7_treasury_example` (`T7-SIM-OK`),
  `sim/t7-treasury.json` (10 proposals, 82 steps, 6 payouts; final
  balance 225 000 ALCRY, 475 000 ALCRY paid) with golden digest
  `719c93b244ff42ebf94538a5e9fb5b9e8f52eac07035abaf3c526e4c0a3231f7`
  and `docs/economics/t7-treasury.md`.
- `tools/t7_treasury_check.py` (independent oracle, same digest),
  `tools/nc_t7.py` (7 mutation controls incl. the literal
  withdrawal-without-vote rejection), `tools/write_t7_evidence.py`
  (`docs/evidence/t7/`, cumulative `TOTAL: 565 negative controls over
  24 phases`).
- Wiring: Makefile `treasury gate (T7)` + `test-t7/check-t7/examples-t7/
  nc-t7/evidence-t7`; `tools/audit_closeout.py` (`CURRENT = "T7"`,
  `SHIPPED` += T6, `NC_PER_PHASE["T7"] = 7`); `### T7 receipt` in
  `docs/plans/ECONOMICS_PLAN.md`.

### Restored

- The workspace was reset mid-session (reset #9: `crates/` reverted to
  the github state, `.git` and `cargo` removed; the plain combined
  delta was deleted while its `.xz` survived). The T0–T6 tree was
  restored from `AuraLite-T6-delta/AuraLite-T0-T6-vs-github-e1a5b56.patch`
  as commit `b0b6150` «T0 - T6 update» (byte-identical to the T6 canon
  `407fe8d`) and Rust 1.99.1 was reinstalled.

## [T6 — Governance phase closed: 20 proposals, valid ones pass, flash-stake attack rejected, 48 h timelock, veto sunset] 2026-11-21

### Added

- `crates/aura-storage/src/governance.rs` — governance model: proposal
  threshold 10 000 `ALCRY`; quorum 2000 bps of the power snapshot
  (strict `yes > no`, a tie is rejected; abstain counts for the quorum
  only); voting period 120 960 blocks (7 days at 5 s) as the half-open
  window `[created, created + period)`; **48 h timelock** (34 560
  blocks) before execution; emergency veto with a **sunset** at block
  1 521 000; votes carry the **snapshot** power of the proposal's
  creation block — a flash-stake after the snapshot has no power and is
  rejected (`NoVotingPower`).
- `crates/aura-storage/tests/t6_governance.rs` (10 gate tests: the
  literal 20-proposal gate, flash-stake rejection, threshold/quorum/
  voting-period/timelock/veto-sunset boundaries incl. exact edges,
  pinned digest; metamorphisms — vote reordering, late stake vs
  snapshot) and `crates/aura-storage/examples/t6_governance_example.rs`.
- `sim/t6-governance.json` — the pinned scenario (20 proposals, 113
  steps: 15 executed, 4 rejected, 1 vetoed) with golden digest
  `828b13cf7ca3abcaea65386f7c62e1e155f4d207cb000aceccd86397b5c729aa`
  and checkpoints; `docs/economics/t6-governance.md`.
- `tools/t6_governance_check.py` (the phase gate oracle: full model
  re-implementation, constants, samples, Rust example cross-run),
  `tools/nc_t6.py` (8 mutation controls: pass condition flipped,
  snapshot replaced by live stake, threshold/window/quorum/timelock/
  sunset disabled or drifted, parameter drift), `tools/write_t6_evidence.py`
  (`docs/evidence/t6/`, cumulative inventory
  `TOTAL: 558 negative controls over 23 phases`).
- Wiring: Makefile `governance gate (T6)` + `test-t6/check-t6/examples-t6/
  nc-t6/evidence-t6`; `tools/audit_closeout.py` (`CURRENT = "T6"`,
  `SHIPPED` += T5, `NC_PER_PHASE["T6"] = 8`, `PATCH_ORDER` += T6);
  `### T6 receipt` in `docs/plans/ECONOMICS_PLAN.md`.

### Restored

- The workspace was reset mid-session (reset #8: `crates/` reverted to
  the github state, `cargo` removed). The T0–T5 tree was restored from
  the delivered combined delta
  (`AuraLite-T5-delta/AuraLite-T0-T5-vs-github-e1a5b56.patch`) as
  commit `e559d94` «T0 - T5 update» (byte-identical to the T5 canon
  `9b648b6` of the pre-reset chain) and Rust 1.99.1 was reinstalled.

## [T5 — Slashing and insurance phase closed: 7 scenarios, penalty ≥ damage, insurance fund never negative, mass-slash cap with a deferred queue] 2026-11-21

### Added

- `crates/aura-storage/src/slashing.rs` — slashing and insurance model:
  penalty parameters (`T5_PENALTY_BPS = 15_000`: the offender pays 150 %
  of the measured damage, `penalty >= damage` at every incident);
  insurance fund (`T5_FUND_SHARE_BPS = 5_000` of every applied penalty
  replenishes the fund; victims are covered `min(damage, balance)`, the
  rest is recorded as `shortfall`; the balance is a signed `i128` and
  must **never go negative**); mass-slash protection
  (`T5_MASS_SLASH_CAP_BPS = 500` of the registered bonded base
  `T5_REGISTERED_BONDED` = 1 000 000 `ALCRY` per window; excess penalty
  is deferred — never dropped — and drains over windows until
  `total_applied == total_penalty`).
- `crates/aura-storage/tests/t5_slashing.rs` (10 gate tests: the literal
  7-scenario gate with both invariants at every step, penalty coverage,
  fund-never-negative, the mass-slash cap, deferred penalties, exact
  penalty margin incl. floor edges, exact fund share, pinned digest;
  metamorphisms — splitting a report, reordering independent reports)
  and `crates/aura-storage/examples/t5_slashing_example.rs`.
- `sim/t5-slashing.json` — the pinned scenario (7 scenarios, 87 steps,
  48 incidents) with golden digest
  `4cc9ea0319374c3c2c74bc88db6d7747b23f0e21ea637ae2270bc4c5707c9cef`
  and checkpoints; `docs/economics/t5-slashing.md`.
- `tools/t5_slashing_check.py` (the phase gate oracle: full model
  re-implementation, constants, samples, Rust example cross-run),
  `tools/nc_t5.py` (7 mutation controls: penalty below damage, coverage
  clamp removed, mass-slash cap raised, deferred penalty dropped, fund
  income off by one, penalty formula drift, pinned parameter drift),
  `tools/write_t5_evidence.py` (`docs/evidence/t5/`, cumulative
  inventory `TOTAL: 550 negative controls over 22 phases`).
- Wiring: Makefile `slashing gate (T5)` + `test-t5/check-t5/examples-t5/
  nc-t5/evidence-t5`; `tools/audit_closeout.py` (`CURRENT = "T5"`,
  `SHIPPED` += T4, `NC_PER_PHASE["T5"] = 7`, `PATCH_ORDER` += T5);
  `### T5 receipt` in `docs/plans/ECONOMICS_PLAN.md` (Stage 2 opened).

## [T4 — Staking phase closed: 1000 delegators, shares sum to exactly one, withdrawal without unbonding red, 21-day unbonding, proportional slashing; Stage 1 (T0–T4) complete] 2026-11-21

### Added

- `crates/aura-storage/src/staking.rs` — share-based staking model:
  min stake 1 000 `ALCRY`; delegation mints shares (rewards compound
  into the pool, share counts never move — `Σ shares == total_shares`
  exactly); validator commission 1000 bps **from rewards only**;
  unbonding 21 days (362 880 blocks at 5 s) with `NoUnbonding`/
  `StillUnbonding` rejections on early or unbacked withdrawals;
  slashing period = the unbonding window, `slash(rate_bps)` cuts bonded
  and unbonding stake proportionally.
- `crates/aura-storage/tests/t4_staking.rs` (10 gate tests: the
  1000-delegator shares-sum gate, withdrawal-without-unbonding
  rejection, the exact 21-day boundary, commission split, min stake,
  proportional slash incl. 0 %/100 %, slashability of unbonding stake,
  pinned digest; metamorphisms — split delegations, slash composition)
  and `crates/aura-storage/examples/t4_staking_example.rs`.
- `sim/t4-staking.json` — the pinned scenario (2004 steps) with golden
  digest
  `e80662c82c74e1d1a0a5b0624beb4cf08af88c24f8091751c277cc7ea6887f07`
  and checkpoints; `docs/economics/t4-staking.md`.
- `tools/t4_staking_check.py` (the phase gate oracle: full model
  re-implementation, constants, samples, Rust example cross-run),
  `tools/nc_t4.py` (7 mutation controls: share accounting,
  withdraw-without-unbonding, unbonding window, double commission,
  min stake, slash proportionality, parameter drift),
  `tools/write_t4_evidence.py` and the self-contained evidence in
  `docs/evidence/t4/` (`campaign/`).
- Makefile targets `test-t4 check-t4 examples-t4 nc-t4 evidence-t4`;
  `make test` gains the "staking gate (T4)"; `tools/audit_closeout.py`
  tracks `ECONOMICS_PHASES = [..., "T4"]` (`CURRENT = "T4"`, NC 7,
  patch order T4). Inventory: 81 phases, 543 negative controls
  cumulative. **Stage 1 of ECONOMICS_PLAN (T0–T4) is complete.**

### Fixed

- (none)

### Security

- T4 replaces no crypto. Rewards are variable parameters, never
  promised income (D-T2); parameters stay governance-locked (D-T1) and
  are simulated on change (D-T3). Every T4 control is mutation-based
  (`tools/nc_t4.py`, 7/7 — the standard's 6 plus slash
  proportionality, H6–H9 precedent).

## [T3 — Fees phase closed: base fee follows the EIP-1559 formula over 10 000 congested blocks, base burned, tips to validators, 6 mutation controls] 2026-11-21

### Added

- `crates/aura-storage/src/fees.rs` — EIP-1559-like fee market:
  `base_fee` + `tip`, integer formula `base ±
  max(base·|gas−target|/target/8, 1)` (unchanged at target), the base
  part **burned** (`paid == burned + tips`) and only the tip credited to
  the validator; elastic gas target 15 000 000 (block limit 30 000 000,
  elasticity 2) enforced by `check_gas`. Pinned scenario `t3_demand`
  (250-block segments +25 %/−25 %/+50 %/−50 %/target) over 10 000
  congested blocks.
- `crates/aura-storage/tests/t3_fees.rs` (9 gate tests: per-step
  formula against an independent re-derivation, direction semantics,
  burn, tip, elastic limit, pinned digest; metamorphisms — at-target
  flatness with the minimum one-unit delta, transaction splitting,
  demand monotonicity) and `crates/aura-storage/examples/t3_fees_example.rs`.
- `sim/t3-fees.json` — scenario with golden path digest
  `412f1f9e120ec829fb5d2ae65e4590da69771cd7b49bef83f29440d0d93bc124`
  and checkpoints (base after 249/499/9999, peak, burn/tip totals);
  `docs/economics/t3-fees.md`.
- `tools/t3_fees_check.py` (the phase gate oracle: 10 000-block
  re-derivation, constants, samples, Rust example cross-run),
  `tools/nc_t3.py` (6 mutation controls: formula denominator, burn,
  tip credit, directions, elastic limit, parameter drift),
  `tools/write_t3_evidence.py` and the self-contained evidence in
  `docs/evidence/t3/` (`campaign/`).
- Makefile targets `test-t3 check-t3 examples-t3 nc-t3 evidence-t3`;
  `make test` gains the "fees gate (T3)"; `tools/audit_closeout.py`
  tracks `ECONOMICS_PHASES = [..., "T3"]` (`CURRENT = "T3"`, NC 6, patch
  order T3). Inventory: 80 phases, 536 negative controls cumulative.

### Fixed

- (none)

### Security

- T3 replaces no crypto. The tip is payment for inclusion, not a
  promise of returns (D-T2); parameters stay governance-locked (D-T1).
  Every T3 control is mutation-based (`tools/nc_t3.py`, 6/6).

## [T2 — Allocation phase closed: genesis pots reconcile exactly, extra coins red, vesting with cliffs enforced, 6 mutation controls] 2026-11-20

### Added

- `crates/aura-storage/src/allocation.rs` — genesis allocation table
  (validators 1 200 000 / treasury 600 000 / community 750 000 / team
  450 000 `ALCRY` = 3 000 000 = the T1 genesis supply) with vesting
  schedules (cliffs 6/24 and 12/36 months), locked/liquid accounting and
  a strict reconciliation: unique pot names, `sum(pots) == genesis
  supply` exactly — the literal phase gate, "extra coins -> red".
- `crates/aura-storage/tests/t2_allocation.rs` (8 gate tests) and
  `crates/aura-storage/examples/t2_allocation_example.rs` (the
  reconciliation runner the independent oracle cross-checks).
- `sim/t2-allocation.json` — the table with golden digest
  `5fe7417a134011e7028f255cf7cf180335cc23c773ee9657a68de260fcc9ea41`
  and vesting samples; `docs/economics/t2-allocation.md`.
- `tools/t2_allocation_check.py` (the phase gate script: reconciles
  genesis balances with the table, re-derives the vesting rules, pins
  the Rust constants), `tools/nc_t2.py` (6 mutation controls: extra
  coins, vesting overflow, cliff removal, non-completing vesting,
  ignored lockup, table drift), `tools/write_t2_evidence.py` and the
  self-contained evidence in `docs/evidence/t2/` (`campaign/`).
- Makefile targets `test-t2 check-t2 examples-t2 nc-t2 evidence-t2`;
  `make test` gains the "allocation gate (T2)"; `tools/audit_closeout.py`
  tracks `ECONOMICS_PHASES = [..., "T2"]` (`CURRENT = "T2"`, NC 6, patch
  order T2). Inventory: 79 phases, 530 negative controls cumulative.

### Fixed

- (none)

### Security

- T2 replaces no crypto. Vesting/lockups are economic guarantees (D-T2:
  no promises of returns; legal perimeter stays T8). Every T2 control is
  mutation-based (`tools/nc_t2.py`, 6/6): the "extra coin" mutation turns
  the reconciliation red with the named message.

## [T1 — Issuance model phase closed: 10-year supply matches the formula within 1 unit, 6 mutation controls] 2026-11-20

### Added
- `crates/aura-storage/src/issuance.rs` (T1 issuance model: `ALCRY` u128
  18 decimals, genesis 3 000 000, annual emission 300 000 halving every 4
  years, hard cap 6 000 000 fixed, target staked ratio 6000 bps — all
  genesis-fixed per D-T1/D-T3), `sim/t1-issuance.json` (parameters,
  expected path, cap-binding scenario), gate
  `crates/aura-storage/tests/t1_issuance.rs` (7 tests: formula ≤ 1 unit
  every year, halving, cap, genesis, staked ratio, golden path digest
  c2a4669192ddde5d…c1a35, stepwise≡closed-form metamorphism), example
  `crates/aura-storage/examples/t1_issuance_example.rs` (`T1-SIM-OK`),
  independent oracle `tools/t1_issuance_check.py` (stepwise + closed form
  + Rust cross-run + const consistency), negative controls
  `tools/nc_t1.py` ×6 (formula mismatch literally red), evidence
  `docs/evidence/t1/` (`TOTAL: 524 negative controls cumulative`),
  model doc `docs/economics/t1-issuance.md`, builds `make test-t1` /
  `make check-t1` / `make examples-t1` / `make nc-t1` /
  `make evidence-t1` + gates wired into `make test`. Phase receipt +
  deviations: `docs/plans/ECONOMICS_PLAN.md` `### T1 receipt`;
  deliverable `patches/0079-T1-...patch`.

## [T0 — Economics baseline phase closed: emission from nowhere -> red, no token, 6 mutation controls] 2026-11-20

### Added
- `crates/aura-storage/src/econ.rs` (T0 economics baseline: deterministic
  supply simulation — xorshift64*, seed 20261180, cap-at-balance steps,
  invariants checked after every step, `DESIRED_PROPERTIES`
  scarcity/security/accessibility, `state_digest` via
  `aura_state::sha256_bytes`), `sim/t0-baseline.json` (scenario: 5 agents,
  3 000 000 units, 1000 steps), gate
  `crates/aura-storage/tests/t0_economics.rs` (8 tests incl. the literal
  gate: "emission from nowhere" -> red; golden digest
  aec12b64eada1b85…c7a51a), example `crates/aura-storage/examples/t0_sim.rs`
  (`T0-SIM-OK`), independent oracle `tools/t0_economics_check.py`
  (Python model + Rust example cross-run + scenario/const consistency),
  negative controls `tools/nc_t0.py` ×6 (emission-from-nowhere literally
  red), evidence `docs/evidence/t0/` (`TOTAL: 518 negative controls
  cumulative`), properties doc `docs/economics/t0-baseline.md`, builds
  `make test-t0` / `make check-t0` / `make examples-t0` / `make nc-t0` /
  `make evidence-t0` + gates wired into `make test`. Phase receipt +
  deviations: `docs/plans/ECONOMICS_PLAN.md` `### T0 receipt`;
  deliverable `patches/0078-T0-...patch`.

## [E3 — Faucet+SDK phase closed: 1000 requests 1 payout/address/day, 3/3 SDK examples in CI, 6 mutation controls] 2026-11-19

### Added
- `crates/aura-storage/src/faucet.rs` (E3 faucet: 1 payout per address
  per day, testnet only, deterministic policy + stdlib HTTP; ledger
  payouts per the W1 freeze — deviation in `### E3 receipt`),
  `crates/aura-storage/src/sdk.rs` (Rust client SDK), `sdk/ts/`
  (TypeScript SDK + example), `sdk/python/` (Python SDK + example),
  `docs/sdk/index.md` (docs site), gate
  `crates/aura-storage/tests/e3_faucet.rs` (1000-request script →
  750 payouts/250 denials, named messages), independent policy oracle
  `tools/e3_faucet_check.py` + `tools/fixtures/e3_faucet_script.json`,
  SDK examples runner `tools/e3_examples_check.py` (rust/ts/python 3/3
  against a live reference faucet), negative controls `tools/nc_e3.py` ×6
  (rate limit removed → red, literally), evidence `docs/evidence/e3/`
  (`TOTAL: 512 negative controls cumulative`), builds `make test-e3` /
  `make check-e3` / `make examples-e3` / `make nc-e3` /
  `make evidence-e3` + gates wired into `make test`. Phase receipt +
  deviations: `docs/plans/ECOSYSTEM_PLAN.md` `### E3 receipt`;
  deliverable `patches/0077-E3-...patch`.

## [E2 — Explorer phase closed: 10/10 user scenarios green, reproducible contract build, 6 mutation controls] 2026-11-19

### Added
- `crates/aura-storage/src/explorer.rs` (E2 explorer: blocks/tx/
  accounts/validators/contracts views over the E1 indexer (D-E1),
  deterministic ALVM-1 contract builder with verified source, stdlib
  HTTP web UI + JSON API — no new dependencies, `Cargo.lock` stays
  pinned at H1), gate `crates/aura-storage/tests/e2_explorer.rs`
  (10 user scenarios over real HTTP + reproducible build property,
  named messages), checklist `tools/fixtures/e2_explorer_scenarios.json`
  (10/10 ready) with oracle `tools/e2_explorer_check.py` (independent
  Python contract builder cross-checks
  `tools/fixtures/e2_contracts.json`), report
  `docs/ecosystem/e2-explorer-report.md`, negative controls
  `tools/nc_e2.py` ×6 (reproducible build without canonicalization →
  red, literally), evidence `docs/evidence/e2/` (`TOTAL: 506 negative
  controls cumulative`), builds `make test-e2` / `make nc-e2` /
  `make evidence-e2` + gates wired into `make test`. Phase receipt +
  deviations: `docs/plans/ECOSYSTEM_PLAN.md` `### E2 receipt`;
  deliverable `patches/0076-E2-...patch`.

## [E1 — Indexer phase closed: 10 000 blocks + reorg, 0 duplicates/losses, 6 mutation controls] 2026-11-18

### Added
- `crates/aura-storage/src/indexer.rs` (E1 chain indexer: idempotent
  ingest + reorg rollback over a local row store — the Postgres
  component is replaced by a recorded deviation, `Cargo.lock` stays
  pinned at H1), gate `crates/aura-storage/tests/e1_indexer.rs`
  (10 000 blocks + artificial 205-block reorg → 0 duplicates/losses,
  named messages), negative controls `tools/nc_e1.py` ×6 (without
  rollback → red, literally), evidence `docs/evidence/e1/`
  (`TOTAL: 500 negative controls cumulative`), builds `make test-e1` /
  `make nc-e1` / `make evidence-e1` + gate wired into `make test`.
  Phase receipt + deviation: `docs/plans/ECOSYSTEM_PLAN.md` `### E1
  receipt`; deliverable `patches/0075-E1-indexer.patch`.

## [E0 — Ecosystem baseline phase closed: 10 user scenarios checklist (0/10), 6 mutation controls] 2026-11-18

### Added
- `docs/ecosystem/e0-baseline-report.md` ("what a user must be able to
  do": 10 user scenarios US-01..US-10 with surfaces and traceability),
  `tools/fixtures/e0_user_scenarios.json` (machine-readable checklist),
  `tools/e0_baseline_check.py` (deterministic oracle: exactly 10 unique
  scenarios, need/status vocabulary, report coverage, honest readiness
  counter; wired into `make test`), `tools/nc_e0.py` (6 mutation
  controls), `tools/write_e0_evidence.py`.

## [H9 — Incident drills phase closed: named drill halted/recovered within SLO, playbook + 7 mutation controls] 2026-11-18

### Added
- `docs/incident/PLAYBOOK.md` (halt/pause/upgrade/rollback, detection,
  recovery, communication, post-mortem template; SLOs halt ≤ 15 min,
  recover ≤ 240 min, post-mortem ≤ 480 min), `tools/fixtures/h9_drills.json`
  (three deterministic drill timelines tied to H0 scenarios),
  `tools/h9_incident_drill.py` (deterministic drill runner: playbook
  coverage, incident state-machine order, SLO bounds; the named gate
  literally — drill "Byzantine validator + bridge bug" halted/recovered
  within SLO; wired into `make test`), `tools/nc_h9.py` (7 mutation
  controls), `tools/write_h9_evidence.py`, `docs/h9-incident-plan.md`.

## [H8 — External audit + bounty phase closed: 0 unfixed critical/high, re-audit oracle, 7 mutation controls] 2026-11-18

### Added
- `docs/audit/h8-public-audit-report.md` (public audit of
  consensus/VM/bridge/wallet: findings table, methodology),
  `SECURITY.md` (coordinated disclosure policy + bug bounty with a
  normative severity table), `tools/fixtures/h8_known_bugs.json` and
  `tools/fixtures/h8_findings.json` (pinned registries),
  `tools/h8_reaudit.py` (deterministic re-audit oracle, fail-closed,
  wired into `make test`), `tools/nc_h8.py` (7 mutation controls; the
  phase gate literally — re-introduced old bugs turn the gate red),
  `tools/write_h8_evidence.py`, `docs/h8-audit-plan.md`.

## [H7 — OpSec / reproducible builds phase closed: two-host sha256 gate, secret-in-git gate, 7 mutation controls] 2026-11-17

### Added
- `tools/h7_repro_build.py` (two-host reproducible build: disjoint source
  paths + shared remap target → identical sha256), `tools/check_opsec.py`
  (fail-closed secrets / CI-lint / toolchain-pin gates, wired into
  `make test`), `crates/aura-wallet/tests/h7_opsec.rs` (keystore
  owner-only save + permission-rejecting loader + access-log hygiene),
  `tools/nc_h7.py` (7 mutation controls), `docs/h7-opsec-plan.md`.

### Changed
- `Keystore::load` now rejects keystores with group/other permission
  bits (H7); `keystore::{access_log_line, append_access_log}` add an
  owner-only access log without secret material; both workflows carry
  least-privilege `permissions:` blocks.

## [H6 — economic attacks phase closed: bounded-damage simulations, no-pause gate, 7 mutation controls] 2026-11-17

### Added
- `crates/aura-consensus/tests/h6_economic.rs` (stake grinding bounds +
  forged-claim slashing, 34% emission conservation, oracle/bridge modelled
  surfaces, emergency-pause gate, censorship bound) and
  `crates/aura-mempool/tests/h6_mev.rs` (canonical-order determinism).
- `tools/h6_economic_campaign.py` (recorded campaign), `tools/nc_h6.py`
  (7 mutation controls incl. the named no-pause gate),
  `docs/h6-economic-plan.md`.

## [H5 — consensus safety phase closed: TLC counterexample gate, chaos matrix, 6 mutation controls] 2026-11-17

### Added
- `docs/h5/HotStuff3ChainSafetyMC.tla` + `h5-safe.cfg` / `h5-weakened.cfg`:
  model-checkable behaviour spec of the 3-chain accountable-safety essence
  (single honest vote per view + quorum intersection); TLC gate — safe
  configuration exhausts clean, `WeakenQuorum` (threshold F+1) must produce
  a counterexample.
- `crates/aura-consensus/tests/h5_safety_model.rs` (model table + the
  counterexample gate + TLA spec integrity), `crates/aura-p2p/tests/
  h5_jepsen.rs` (chaos matrix 20×3 zero divergence + divergence monitor
  sensitivity), `tools/h5_consensus_campaign.py` (recorded campaign:
  model, chaos, TLC both configs), `tools/nc_h5.py` (6 mutation controls),
  `docs/h5-consensus-plan.md`.

## [H4 — property/differential phase closed: 1M-tx conservation, 6 mutation controls] 2026-11-16

- **Balance invariants (D-H4-3).** `Account::{credit,debit}` algebra:
  conservation, reject-without-state-change, checked (never wrapping)
  arithmetic; every generated transfer conserves `sum(balances)`.
- **Model-based VM tests (D-H4-2).** `crates/aura-exec/tests/h4_model_vm.rs`:
  a reference model of `detcheck::run` transfer semantics predicts
  committed/refused/gas for generated scenarios; node labels must not
  influence execution.
- **Reference tree/codec (D-H4-1).** Naive in-tree Rust references plus the
  cross-language `tools/h4_reference.py` (existing self-checked C4 Python
  encoder for the Tx layout, from-scratch S1 merkle), compared both ways by
  the campaign runner.
- **Gate (D-H4-4).** `h4-properties --txs N` (Lcg seeds from 20261180):
  every tx fully signed + verified + canonically round-tripped;
  `sum(balances)` checked after **every** transaction. Deviation (plan §3):
  the 1M run is `make property-h4-full`; this session records the schedule
  named in `docs/evidence/h4/` with exact counts — no extrapolation.
- **Controls (D-H4-6).** `tools/nc_h4.py` 6/6; named control
  `injected bug -> property finds it` (NC-1). NC total 454.
- **Docs.** `docs/h4-property-plan.md`; `docs/evidence/h4/` (gates.txt/
  nc.log/campaign/report.md/inventory.txt/SHA256SUMS).

## [H3 — fuzzing phase closed: 11 targets, recorded campaign, 6 mutation controls] 2026-11-14

- **Targets (D-H3-1).** libFuzzer matrix complete: 11 sources in
  `fuzz/fuzz_targets/` (new: `fuzz_p2p_frame.rs` → `HandshakeHello::decode` +
  `GossipMessage::decode`; `fuzz_wasm_verify.rs` → `Sandbox::gate`, the static
  verifier half). Portable areas unchanged (6).
- **Fuzz oracles (D-H3-2).** `fuzz/src/h3.rs`: totality + determinism per
  decoder area over a fixed boundary corpus; a decoder panic fails with
  `H3: fuzz oracle caught a decoder panic in … (injected bug found)`.
- **Campaign (D-H3-3/-4).** `tools/h3_fuzz_campaign.py`: recorded, resumable,
  seeds from 20261180; the `smoke` run is archived in
  `docs/evidence/h3/campaign/`; `full` = the 24-hour budget (86,400 s over
  17 runs). Deviation (plan §3): the 24-hour run is not claimed in this
  session; libFuzzer/ASan runs need nightly-2026-11-03 + cargo-fuzz and are
  recorded as `skipped` without them, never as passes.
- **Controls (D-H3-8).** `tools/nc_h3.py` 6/6; named control
  `injected bug -> fuzz finds it` (NC-1: `HandshakeHello::decode` panic on
  empty input must be caught by the fuzz oracle). NC total 448.
- **Docs.** `docs/h3-fuzz-plan.md` (target matrix, triage, deviations);
  `docs/evidence/h3/` (gates.txt/nc.log/campaign/report.md/inventory.txt/
  SHA256SUMS).

## [H2 — crypto review phase closed: 50 vectors, 6 mutation controls] 2026-11-14

- H2 phase closed (SECURITY_PLAN). Deliverable
  `patches/0066-H2-crypto-review.patch`, receipt in `docs/plans/SECURITY_PLAN.md`,
  evidence in `docs/evidence/h2/`.
- 50 golden crypto vectors (independent oracle: pure-Python RFC 8032 +
  python blake3 + from-spec `domain_message`): byte-for-byte sign/verify,
  keygen, digest layout and edge payloads, plus negatives — wrong domain,
  cross chain, tampered payload, wrong key, high-S malleation.
- Malleability check: `verify_raw` rejects `S >= L` with the distinct
  `CryptoError::NonCanonicalSignature` (the `(R, S+L)` twin; dalek's
  `legacy_compatibility` feature would reopen the window).
- Side-channel checklist: 10 machine-readable items with source-claim checks
  (`tools/check_h2_review.py`); PQ plan recorded (ML-DSA FIPS 204 primary,
  SLH-DSA FIPS 205 stateless backup).
- Named control (shared domain -> red): `H2: wrong-domain signature must be
  rejected`; mutation controls 6/6 PASS (`tools/nc_h2.py`).

## [H0 — threat model phase closed: 32 owned scenarios, 6 mutation controls] 2026-11-11

- H0 phase closed (SECURITY_PLAN). Deliverable
  `patches/0063-H0-threat-model.patch`, receipt in `docs/plans/SECURITY_PLAN.md`,
  evidence in `docs/evidence/h0/`.
- `docs/THREAT_MODEL.md`: assets, adversaries (external/validator/insider/
  supply chain/physical), trust boundaries and 32 owned scenarios; every
  scenario has exactly one owner from the seven roles.
- Named control (scenario without an owner -> red):
  `error: scenario H0-S07: missing owner`; review gate registered in
  `make test` (`tools/check_threat_model.py`).
- Mutation controls: 6/6 PASS (`tools/nc_h0.py`).

## [H1 — supply chain phase closed: 11 vectors, 6 mutation controls] 2026-11-14

- H1 phase closed (SECURITY_PLAN). Deliverable
  `patches/0064-H1-supply-chain.patch`, receipt in `docs/plans/SECURITY_PLAN.md`,
  evidence in `docs/evidence/h1/`.
- Supply-chain gate: toolchain/lockfile pinning, advisory deny (pinned
  snapshot), CycloneDX SBOM, secrets scan, Ed25519 release signatures;
  registered in `make test`.
- Named control (remove deny -> red): `FAIL vulnerable_lock_denied`; the gate
  refuses a lockfile that resolves a denied crate version.
- Mutation controls: 6/6 PASS (`tools/nc_h1.py`).
- Post-closeout deviation (2026-11-14): the secret scanner now skips the
  immutable `patches/` review archive (its diffs embed demo-fixture lines);
  red/green guard `tools/test_patch_scope_guard.py` wired into `make test`.
  Recorded in `docs/h1-supply-chain-plan.md`.

## [W4 — multisig phase closed: 3-of-5 gate, 22 vectors, 6 mutation controls] 2026-11-11

- W4 phase closed. Deliverable `patches/0062-W4-multisig.patch`, receipt in
  `docs/plans/WALLET_PLAN.md`, evidence in `docs/evidence/w4/`.
- Wallet-level M-of-N multisig: `msig-init`/`msig-propose`/`msig-approve`/
  `msig-collect` over canonical contract/proposal/approval documents;
  member rotation via `rotate` proposals; deterministic `--height` timeouts.
- Gate: 3-of-5 — 3 collected -> success, 2 -> reject — and the named control
  (threshold bypass -> red): `error: multisig: threshold not met (2/3)`.
- Mutation controls: 6/6 PASS (`tools/nc_w4.py`).

## [W3 — offline signing phase closed: air-gapped e2e, 24 vectors, 6 mutation controls] 2026-11-11

- W3 phase closed. Deliverable `patches/0061-W3-offline-signing.patch`,
  receipt in `docs/plans/WALLET_PLAN.md`, evidence in `docs/evidence/w3/`.
- Air-gapped offline signing: deterministic `AURALITE-OFFLINE-V1` envelopes,
  `ALO1:ii/nn` QR frames, `tx-prepare`/`tx-sign`/`tx-verify`/`qr-split`/
  `qr-join`; `broadcast` accepts signed envelopes directly.
- Gate: air-gapped scenario end-to-end (24 vectors, byte-for-byte, from the
  independent oracle) and the named control (a signature with a foreign
  domain is rejected: `error: signature verification failed`, exit 65).
- Mutation controls: 6/6 PASS (`tools/nc_w3.py`).

## [W2 — aura-wallet CLI phase closed: 40 CLI vectors, 6 mutation controls] 2026-11-11

- W2 phase closed. Deliverable `patches/0060-W2-wallet-cli.patch` (complete
  W2-from-W1 phase patch), receipt in `docs/plans/WALLET_PLAN.md`,
  evidence in `docs/evidence/w2/`.
- `aura-wallet` CLI (`new/import/list/balance/transfer/sign/broadcast/stake/
  vote`) with deterministic outputs, fixed error messages and exit codes
  0/64/65/69; file-backed `Ledger` backend (D-W2-1); Ed25519 wallet ballots
  (D-W2-2); canonical `wallet-stake-v1` payloads (D-W2-3); secrets only via
  files (D-W2-4).
- Gate: 40 CLI vectors including the named control (a tx with a foreign
  chain-id is rejected: `error: foreign chain-id`, exit 65), generated by the
  independent Python oracle and asserted byte-for-byte (42 tests with the
  count guard and broadcast replay refusal).
- Mutation controls: 6/6 PASS (`tools/nc_w2.py`).

## [W1 — Crypto keystore phase closed: W1-FORMAT-v1 freeze and closeout campaign] 2026-11-08

- W1 phase closed. Deliverable `patches/0059-W1-crypto-keystore.patch`
  (complete W1-from-W0 phase patch), receipt in `docs/plans/WALLET_PLAN.md`,
  and `docs/evidence/w1/` (gates, NC log, checksums, report, inventory).
  `make nc-w1` passes 52/52 wallet plus 15/15 fuzz-oracle mutation controls
  (67/67, 406/406 cumulative negative controls).
- The interoperability format is frozen as **W1-FORMAT-v1**
  (`docs/spec/keystore-format.md`) by owner decision; the candidate draft
  `docs/w1-format-draft.md` is superseded and kept as the historical record.
  Any format change is a versioned compatibility decision.
- A closeout ASan campaign (`/home/user/w1-campaign-v2-closeout`, seed base
  `20261180`, 1,800 s per target over the four wallet fuzz targets — six
  times the earlier per-target budget) completed with zero artifacts and
  checkout-verified manifests; run details are recorded in
  `docs/evidence/w1/report.md` and the fuzz observation document.
- Scope and carries (owner decision, 2026-11-08): this closure is testnet-only;
  the independent external security review is deferred to the Stage-2 audit
  and is not claimed here; H3's 24-hour total fuzz campaign remains on the H3
  plan; mainnet curve/path/coin-type allocation and migration policy remain
  separate decisions.
- `tools/audit_closeout.py` now audits W0–W1 (`--strict` green with the W1
  patch and evidence present).

## [W1 — recovery-metadata and scrypt-differential fuzz oracles, 15 mutation controls, work in progress] 2026-11-08

- `fuzz/src/lib.rs::recovery_metadata` is now a shared, unit/mutation-tested
  driver for `fuzz_recovery_metadata`: every accepted marker must round-trip
  through canonical `{"normalization_profile":"<id>"}` output, its
  `normalization_profile()` accessor result must match a second, raw-text
  (JSON-escape aware) extraction of the member — not merely a second accessor
  call — and an escape-spelled duplicate member
  (`{"\u006eormalization_profile":…}`), one extra member or 4,097-byte
  oversized text must be rejected before any default can be substituted. The
  37-case `w1_recovery_metadata_reference.json` matrix plus 4,096/4,097-byte
  boundary and must-reject cases run deterministically in `make test-w1`.
- `fuzz/src/lib.rs::scrypt_differential` compares the pinned local scrypt fork
  (`third_party/scrypt-w1`, upstream `scrypt` 0.11.0) against the upstream
  `scrypt` crate on deterministic inputs each iteration, with a fixed-input
  unit test; a fork output divergence panics the fuzz target.
- `tools/nc_w1_fuzz.py` now carries 15 fuzz-oracle negative controls
  (`52/52` wallet + `15/15` fuzz-oracle): the previous eight parser/decrypt
  controls plus six recovery-oracle controls (size-limit, extra-member,
  escaped-duplicate, error-classification and profile-accessor/raw-text
  cross-check bypasses) and one scrypt-fork-divergence control, each with a
  named mutation anchor in `crates/aura-wallet/src/keystore.rs` or
  `third_party/scrypt-w1/src/lib.rs`.
- A sustained version-2 ASan campaign (`/home/user/w1-campaign-v2-oracles`,
  seed base `20261170`, 301 s per target) completed for all four targets with
  zero artifacts and passed `verify_w1_fuzz_evidence.py --check-checkout`
  (8.1M parser, 271k decrypt, 225k scrypt and 12.7M recovery-metadata
  iterations). `w1-campaign-v2-sustained` predates this oracle refactor and is
  now an internal-consistency-only historical run.
- Historical corpus loss, recorded without rewriting receipts: the workspace
  snapshot restore that preceded this increment substituted
  `w1-campaign-v2-reauth`'s evolved `fuzz_recovery_metadata` corpus (1,583
  unmatched files present, 1,557 recorded files absent) and dropped its empty
  `artifacts` directories; the run's `corpus-sha256.json` manifest remains and
  the corpus tree is unrecoverable. This is the same class of workspace
  snapshot loss already recorded for `w1-campaign-v2-verified`'s
  `fuzz_keystore_parse` corpus. Both historical bundles are admissible as
  run-log/manifest-form evidence only; `verify_w1_fuzz_evidence.py` correctly
  fails their corpus checks.
- W1 remains open: unkeyed local manifests and bounded runs are not a signed
  phase receipt, format freeze, 24-hour H3 campaign or independent audit.

## [W1 — parser fuzz oracle harness, mutation controls and sustained ASan run, work in progress] 2026-11-08

- The parser-only fuzz oracle is now a shared, unit-tested driver
  (`fuzz/src/lib.rs::keystore_parse`) used by `fuzz_keystore_parse`: every
  accepted document must serialize stably, report exactly the normalization
  profile carried by its raw text (not merely a second call to the same
  accessor), and reject one extra field, an escape-spelled duplicate key and
  `MAX_KEYSTORE_BYTES + 1` size padding. Nine public fixtures (v1/v2,
  profile-bearing, uppercase-hex and valid-tag inconsistent variants) plus
  malformed and oversized inputs run deterministically in `make test-w1`.
- `tools/nc_w1_fuzz.py` now carries 8 fuzz-oracle negative controls
  (`52/52` wallet + `8/8` fuzz-oracle): the four decrypt-oracle controls plus
  new controls for extra-field, duplicate-key, oversize and profile-accessor
  bypasses in the strict parser.
- A sustained version-2 ASan campaign (`/home/user/w1-campaign-v2-sustained`,
  seed base `20261160`, 301 s per target — five times the earlier budget)
  completed for all four targets with zero artifacts and passed
  `verify_w1_fuzz_evidence.py --check-checkout` (7.4M parser, 263k decrypt,
  254k scrypt and 13.5M recovery-metadata iterations).
- W1 remains open: unkeyed local manifests and bounded runs are not a signed
  phase receipt, format freeze, 24-hour H3 campaign or independent audit.

## [W1 — canonical account checks, hex-case AAD fuzz modes, Unicode fork guard, work in progress] 2026-11-08

- New Python-generated, independently GCM-authenticated fixtures: an
  uppercase-hex v2 header (`w1_interop_argon2id_uppercase_hex_header.json`) that
  both implementations decrypt (ciphertext hex case is inert; `id`/`salt`/
  `nonce` hex case is covered by AAD), plus three valid-tag but semantically
  inconsistent account documents (uppercase `public_key`, uppercase Bech32
  `address`, and a weak-scrypt legacy account). Rust rejects the inconsistent
  ones after authentication (`AuthenticationFailed`) and the migration-only
  legacy APIs fail closed. Two new wallet negative controls cover
  case-insensitive comparison bypasses at the post-GCM account check
  (`52/52` wallet + `4/4` fuzz-oracle controls).
- The decrypt fuzz oracle now has 14 deterministic mutation modes over a
  byte-pinned public base document, adding authenticated profile-marker
  mutation and case/nibble hex spelling mutations in AAD fields whose decoded
  id/salt/nonce bytes stay identical. Two new fuzz-oracle controls cover a
  missing `seed_derivation` and a missing `kdf` in the AAD builder. The
  parser-only target derives extra-field, escaped-duplicate (`\u0066ormat`) and
  `MAX_KEYSTORE_BYTES + 1` rejects from each accepted document and checks that
  `normalization_profile()` survives reserialization.
- Added the offline Unicode-fork provenance guard `tools/check_unicode_15_1_fork.py`
  with its mutation test: all 23 unmodified files in
  `third_party/unicode-normalization-15-1/` are byte-equal to crates.io
  `unicode-normalization` 0.1.23, the two adapted files reverse to the exact
  upstream digests, extra files/symlinks are rejected and workspace bindings
  stay pinned. It runs inside `make test-w1`.
- A new version-2 ASan campaign (`/home/user/w1-campaign-v2-reauth`, seed base
  `20261150`, 61 s per target) completed for all four targets with zero
  artifacts and passed `verify_w1_fuzz_evidence.py --check-checkout`.
- W1 remains open: unkeyed local manifests and bounded runs are not a signed
  phase receipt, format freeze, 24-hour H3 campaign or independent audit.

## [W1 — recovery-marker reference oracle & fuzz evidence verifier, work in progress] 2026-11-08

- Added `tools/w1_recovery_metadata_reference.py`, its fail-closed guard, and a
  37-case public fixture (`w1_recovery_metadata_reference.json`) checked by
  Rust for acceptance, error category (including multi-byte UTF-8 4096/4097-byte
  limits), profile ID and canonical serialization, plus a 50th wallet negative
  control (`50/50` wallet + `2/2` fuzz-oracle controls).
- Upgraded `tools/w1_fuzz_campaign.py` to version-2 unkeyed SHA-256 source and
  corpus manifests, added `tools/verify_w1_fuzz_evidence.py` (with optional live
  checkout comparison and 12 fail-closed unit tests), and added the fourth W1
  fuzz smoke target (`fuzz_recovery_metadata`) to `.github/workflows/w1.yml`.
- W1 remains open: unkeyed local manifests and bounded runs are not a signed
  phase receipt, format freeze, 24-hour H3 campaign or independent audit.

## [W1 — strict recovery marker checks, work in progress] 2026-11-08

- Added a 25-case public regression matrix for recovery profile marker parsing,
  plus exact 4096/4097-byte boundaries and canonical round-trips. Selected
  mutations demonstrate detection of duplicate, extra-field and limit bypasses.
- The recovery metadata fuzz target now derives duplicate-key, extra-field and
  oversized cases from each accepted marker. These checks are bounded and do
  not establish complete parser safety or authenticate recovery metadata.
- W1 remains open: no freeze, independent audit or phase receipt is claimed.

## [W1 — recovery metadata fuzz coverage, work in progress] 2026-11-08

- Added a parser-only recovery-metadata fuzz target and public seed corpus;
  the sequential ASan runner now covers four wallet targets. A 61-second-per-
  target follow-up completed without artifacts. Earlier, three targets each
  completed a 301-second run; see `docs/w1-fuzz-extended-observation.md`.
- These bounded tests are neither an independent security review nor the H3
  24-hour campaign. W1 remains open; the format is not frozen.

## [W1 — recovery comparison and fuzz campaign tooling, work in progress] 2026-11-08

- Recovery seed comparison uses the existing constant-time byte comparison
  primitive instead of ordinary array equality. This does not make the entire
  recovery flow constant-time. A bypass mutation is detected by the existing
  profile-bound recovery test; `make nc-w1` passed 46 wallet controls plus 2
  fuzz-oracle controls, and `make test-w1` passed locally.
- Added a sequential ASan fuzz campaign runner, fail-closed runner tests and
  an independent-review brief. A campaign is not an audit or phase receipt.
  W1 remains open and no format is frozen by this entry.

## [W1 — independent schema-conformance matrix, work in progress] 2026-11-08

- Added a 71-case public Python/Rust parse-only matrix covering strict v1/v2
  schema, duplicate keys, Unicode, exact fields and KDF resource limits, plus
  a guard against vector corruption and an accept-all Python oracle mutation.
- Three additional Rust mutation controls exercise the matrix. Local
  `make test-w1` and `make nc-w1` passed (45 wallet and 2 fuzz-oracle controls).
  Parsed metadata remains untrusted until authentication. No production
  keystore or cryptographic behavior changed.
- W1 remains open; this is not a frozen format, phase receipt, independent
  security review, or production-readiness claim.

## [W1 — candidate format, profile interop and bounded fuzzing, work in progress] 2026-11-08

- Added a review draft of the testnet keystore format; it is not frozen or
  approved. Added six independently generated, authenticated profile vectors
  across both KDFs, including version-sensitive Unicode 15.1/18.0 recovery.
  Python and Rust exchange files in both directions. Fixed vector secrets are
  public test data only.
- Local `make test-w1` and `make test-w1-interop` passed; `make nc-w1` passed
  42 wallet plus 2 fuzz-oracle controls. Three 180-second AddressSanitizer
  libFuzzer runs completed without a crash; see `docs/w1-fuzz-observation-2026-11-08.md`.
  This is not the H3 24-hour campaign, a phase receipt or an independent audit.
- W1 remains open. No production cryptographic or keystore behavior changed.

## [W1 — additional AAD reference coverage, work in progress] 2026-11-08

- Expanded the independent Python/Rust AAD vectors from three to six cases:
  legacy v1 without profile metadata, v2 with a null profile, and v2 with the
  Unicode 15.1 profile. The two altered v2 documents are AAD-only: their
  original ciphertext tags are invalid and they are not decryption fixtures.
- Added two mutation controls detecting omission of those profile metadata
  values. Local `make test-w1` and `make nc-w1` passed (41 wallet and 2
  fuzz-oracle controls). No production keystore or crypto behavior changed.
- W1 remains open; this entry is not a phase receipt, frozen format, security
  audit or production-readiness claim. Historical evidence is not rewritten.

## [W0 — Wallet baseline: key-exposure surface audit, threat notes, and tree/log secret scanner] 2026-11-02

**First `WALLET_PLAN` phase (`W0` complete).**
Deliverable: `patches/0058-W0-wallet-baseline.patch`. Adds the 19th workspace crate (`aura-wallet`) with zero new external crate dependencies.

- `aura_wallet::baseline`, `crates/aura-wallet/src/bin/aura-wallet.rs`, and `docs/evidence/w0/threat-notes.md` —
  adds the W0 capability survey across W1–W4 (`keystore`, `wallet-cli`,
  `offline-signing`, `multisig`: `0` implemented, `4` placeholders, `0` missing),
  explicit stub refusal (`EXIT_STUB = 2`), the 8-surface key-exposure audit
  (`EXPOSURE_SURFACES`), 6 threat model notes (`THREAT_NOTES`, `T-W0-1`..`T-W0-6`,
  locking `m/44'/ALCRY'/0'/0/i`), and the tree/log secret-material scanner
  (`scan_text_for_key_leaks`, `audit_tree_for_key_leaks`).
- `crates/aura-wallet/tests/w0_key_audit.rs`, `crates/aura-wallet/tests/fixtures/w0_baseline.txt`,
  and `crates/aura-wallet/tests/fixtures/w0_key_audit.txt` — verifies that the
  live repository tree contains zero leaked keys in any log or file, that
  injecting a key into a log file turns the tree-search gate red, and that all
  64 deterministic scanner vectors pass.
- `tools/nc_w0.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-wallet/src/baseline.rs` (`--lib -- --exact`), including
  `NC-1: key in a log -> tree search must fail`. Cumulative negative controls:
  **339/339**. Evidence: `docs/evidence/w0/`.

## [B10 — Safety evidence: TLA+/Apalache model, 100-node simulation, long-range analysis, and `CONSENSUS_PLAN` close-out] 2026-11-01

**Eleventh and final `CONSENSUS_PLAN` phase (`B0`–`B10` complete).**
Deliverable: `patches/0057-B10-safety-evidence.patch`. Zero new external crate dependencies added.

- `aura_consensus::safety_model`, `aura_consensus::baseline`, and `docs/evidence/b10/HotStuff3ChainSafety.tla` —
  ships the TLA+/Apalache formal specification of 3-chain HotStuff BFT finality,
  an executable bounded model checker (`BftSafetyModelChecker`) proving
  `AccountableSafetyInvariant` for `f < n/3` and generating a concrete 6-step
  `CounterexampleTrace` when the `2f+1` quorum check is removed (`NC-1`) or
  `f >= n/3`, a 100-node (`n = 100, f = 33, quorum = 67`) BFT simulation
  (`run_hundred_node_simulation`) with zero finalized-head divergence, and a
  long-range / weak-subjectivity attack analyzer (`analyze_long_range_attack`),
  advancing `aura_consensus::baseline` to `B10` and closing `CONSENSUS_PLAN`.
- `crates/aura-consensus/tests/b10_safety_evidence.rs`, `crates/aura-consensus/tests/fixtures/b10_baseline.txt`,
  and `crates/aura-consensus/tests/fixtures/b10_safety_model.txt` — verifies the
  TLA+ specification, `f < n/3` safety proof, counterexample extraction without
  the quorum check, 100-node simulation, and 64 deterministic safety vectors.
- `tools/nc_b10.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-consensus/src/safety_model.rs` (`--lib -- --exact`),
  including `NC-1: remove quorum check -> model finds a counterexample`.
  Cumulative negative controls: **333/333**. Evidence: `docs/evidence/b10/`.

## [B9 — On-chain governance: proposal, vote, timelock, WASM runtime upgrade, and emergency pause] 2026-11-01

**Tenth `CONSENSUS_PLAN` phase.**
Deliverable: `patches/0056-B9-on-chain-governance.patch`. Zero new external crate dependencies added.

- `aura_consensus::governance` and `aura_consensus::baseline` — implements
  on-chain governance proposal submission with deposit escrow and WASM bytecode
  validation (`WASM_MAGIC_AND_VERSION`, `BLAKE3` code hash, strictly increasing
  `to_version`), stake-weighted BLS12-381 validator voting (`Yes`, `No`,
  `Abstain`, `NoWithVeto`), quorum/supermajority/veto tallying, mandatory
  timelock queue (`ProposalStatus::Timelocked`), `2f+1` QC-authorized emergency
  pause/resume, and hot WASM runtime upgrades across a 4-validator testnet
  without a manual validator restart (`HotUpgradableTestnet`), advancing
  `aura_consensus::baseline` to `B9`.
- `crates/aura-consensus/tests/b9_governance.rs`, `crates/aura-consensus/tests/fixtures/b9_baseline.txt`,
  and `crates/aura-consensus/tests/fixtures/b9_governance.txt` — verifies the
  first protocol upgrade in a 4-validator testnet without a manual validator
  restart (`restart_count == 0`) and checks 64 deterministic governance vectors.
- `tools/nc_b9.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-consensus/src/governance.rs` (`--lib -- --exact`).
  Cumulative negative controls: **327/327**. Evidence: `docs/evidence/b9/`.

## [B8 — Finality: fork choice, finalized checkpoint, light-client proof, and deep-reorg defence] 2026-11-01

**Ninth `CONSENSUS_PLAN` phase.**
Deliverable: `patches/0055-B8-finality-fork-choice.patch`. Zero new external crate dependencies added.

- `aura_consensus::fork_choice` and `aura_consensus::baseline` — implements
  fork-choice head selection (`ForkChoiceTree::select_canonical_head`),
  weak-subjectivity-bounded finalized checkpoints (`FinalizedCheckpoint`,
  `DEFAULT_WEAK_SUBJECTIVITY_WINDOW`), 3-chain consensus light-client proofs
  (`ConsensusLightClientProof`), and deep-reorg rejection with cryptographic
  `SlashingEvidence` emission (`ReorgRejection`, `ForkChoiceError::DeepReorgPastFinalized`),
  advancing `aura_consensus::baseline` to `B8` (`D-B3`, `D-B4`).
- `crates/aura-consensus/tests/b8_fork_choice.rs`, `crates/aura-consensus/tests/fixtures/b8_baseline.txt`,
  and `crates/aura-consensus/tests/fixtures/b8_fork_choice.txt` — verifies
  deep-reorg rejection + `SlashingEngine` tombstoning of the offending validator
  and checks 64 deterministic fork-choice / reorg vectors.
- `tools/nc_b8.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-consensus/src/fork_choice.rs` (`--lib -- --exact`).
  Cumulative negative controls: **321/321**. Evidence: `docs/evidence/b8/`.

## [B7 — Rewards: inflation/reward, fee burn, delegation, commission, and 100,000-block sum invariant] 2026-11-01

**Eighth `CONSENSUS_PLAN` phase.**
Deliverable: `patches/0054-B7-rewards.patch`. Zero new external crate dependencies added.

- `aura_consensus::rewards` and `aura_consensus::baseline` — implements block
  inflation/reward emission (`RewardParams`, `RewardsEngine`, `BlockRewardReceipt`),
  transaction fee burn (`fee_burn_bps`), proposer bonus (`proposer_bonus_bps`),
  stake delegation (`ValidatorRewardConfig`, `RewardsEngine::delegate`), validator
  commission (`commission_bps`), and the conservation invariant
  `sum(balances) + burned == issued` (`verify_sum_invariant`), advancing
  `aura_consensus::baseline` to `B7`.
- `crates/aura-consensus/tests/b7_rewards.rs`, `crates/aura-consensus/tests/fixtures/b7_baseline.txt`,
  and `crates/aura-consensus/tests/fixtures/b7_rewards.txt` — verifies
  `sum(balances) + burned == issued` within 1 unit (`0` exact drift) over
  100,000 blocks and checks 64 deterministic reward/fee-burn/commission vectors.
- `tools/nc_b7.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-consensus/src/rewards.rs` (`--lib -- --exact`).
  Cumulative negative controls: **315/315**. Evidence: `docs/evidence/b7/`.

## [B6 — Slashing / evidence: 6 attack types, penalties, jailing/tombstoning, and distribution] 2026-11-01

**Seventh `CONSENSUS_PLAN` phase.**
Deliverable: `patches/0053-B6-slashing-evidence.patch`. Zero new external crate dependencies added.

- `aura_consensus::slashing`, `aura_consensus::validator_set`, and `aura_consensus::baseline`
  — implements cryptographic evidence verification (`SlashingEvidence`, `SlashingEngine`,
  `CheckpointAttestation`) for all 6 slashable attack types (`DoubleSignProposal`,
  `DoubleSignVote`, `SurroundVote`, `AmnesiaVote`, `InvalidVrfLeaderClaim`,
  `ExtendedDowntime`), stake slashing and jailing/tombstoning (`ValidatorSet::slash_validator`),
  and deterministic penalty distribution (`PenaltyDistribution`: 10% reporter reward,
  40% protocol treasury share, 50% burn), advancing `aura_consensus::baseline` to `B6` (`D-B4`).
- `crates/aura-consensus/tests/b6_slashing.rs`, `crates/aura-consensus/tests/fixtures/b6_baseline.txt`,
  and `crates/aura-consensus/tests/fixtures/b6_slashing.txt` — verifies all 6 attack
  types against `ValidatorSet` and 64 deterministic slashing vectors.
- `tools/nc_b6.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-consensus/src/slashing.rs` (`--lib -- --exact`),
  including `NC-1: slashing off -> test red`. Cumulative negative controls:
  **309/309**. Evidence: `docs/evidence/b6/`.

## [N8 — Chaos harness: latency/loss/partition/eclipse simulation and 100-node stand (`NET_PLAN` complete)] 2026-11-01

**Ninth and final `NET_PLAN` phase.**
Deliverable: `patches/0052-N8-chaos-harness.patch`. Zero new external crate dependencies added.

- `aura_p2p::chaos` and `aura_p2p::baseline` — implements the 100-node P2P chaos
  stand (`ChaosStand`, `ChaosScenarioConfig`, `canonical_20_scenarios`,
  `run_chaos_matrix`, `verify_cluster_convergence`) with deterministic latency +
  jitter, packet loss up to 40%, network partitions + post-heal header-first
  sync, Sybil eclipse defence via `2f+1` QC verification, Byzantine equivocation
  isolation via `DosDefenceEngine`, and N7 `LightClientVerifier` checkpoint
  verification, completing all 8 `NET_PLAN` components (`8 implemented, 0 placeholders`).
- `crates/aura-p2p/tests/n8_chaos.rs` and `crates/aura-p2p/tests/fixtures/n8_baseline.txt`
  — runs 20 scenarios × 3 runs (= 60 runs) across the 100-node stand with zero
  finalized-head divergence (`divergent_runs == 0`) and verifies the `NET_PLAN`
  closure criteria (100 nodes, 20% packet loss, 1 Byzantine peer -> identical
  finalized `state_root` verified by a light client without a full node).
- `tools/nc_n8.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-p2p/src/chaos.rs` (`--lib -- --exact`). Cumulative
  negative controls: **303/303**. Evidence: `docs/evidence/n8/`.

## [N7 — Light client: header+QC proof, finalized checkpoint check, and simplified DAS] 2026-11-01

**Eighth `NET_PLAN` phase.**
Deliverable: `patches/0051-N7-light-client.patch`. Zero new external crate dependencies added.

- `aura_p2p::light_client` and `aura_p2p::baseline` — implements light-client
  header+QC proof verification (`LightClientProof`, `LightClientVerifier`),
  trusted finalized checkpoint progression (`FinalizedCheckpoint`), simplified
  Data-Availability Sampling (`DasChunkSample`, `build_das_tree`,
  `sample_das_chunks`, `LIGHT_CLIENT_LEAF_DOMAIN`, `LIGHT_CLIENT_BRANCH_DOMAIN`),
  and pre-crypto proof byte-size (`<= 16 KiB`) and verification-gas
  (`<= 150,000`) bounds (`LightClientBounds`), advancing `aura_p2p::baseline`
  to `N7` (7 implemented, 1 placeholder).
- `crates/aura-p2p/tests/n7_light_client.rs` and `crates/aura-p2p/tests/fixtures/n7_baseline.txt`
  — a light client rejects forged headers, mismatched QCs, and corrupted DAS
  samples (banning the offending peers) and advances its finalized checkpoint
  across adjacent and multi-block jumps with bounded proof size and gas.
- `tools/nc_n7.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-p2p/src/light_client.rs` (`--lib -- --exact`).
  Cumulative negative controls: **297/297**. Evidence: `docs/evidence/n7/`.

## [N6 — DoS defence: size/rate limits, signature rate limit, and equivocation detection] 2026-11-01

**Seventh `NET_PLAN` phase.**
Deliverable: `patches/0050-N6-dos-defence.patch`. Zero new external crate dependencies added.

- `aura_p2p::dos_defence` and `aura_p2p::baseline` — implements pre-crypto
  `BanList` filtering, frame size caps (`max_frame_bytes = 256 KiB`), per-peer
  sliding-window message rate limits (`128 msg/s`), per-peer Ed25519 signature
  verification CPU budgets (`64 sig/s`), invalid-signature strike escalation,
  and consensus double-proposal / double-vote equivocation detection with
  self-contained verifiable `EquivocationEvidence`, advancing
  `aura_p2p::baseline` to `N6` (6 implemented, 2 placeholders).
- `crates/aura-p2p/tests/n6_dos_defence.rs` and `crates/aura-p2p/tests/fixtures/n6_baseline.txt`
  — 1 malicious peer sends 10,000 msg/s (flood, corrupted signatures, and
  oversized frames) while 4 honest validators keep finalizing blocks `1..=5`
  uninterrupted; the malicious peer is banned and `>= 9,936` messages are
  dropped before Ed25519 verification.
- `tools/nc_n6.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-p2p/src/dos_defence.rs` (`--lib -- --exact`). Cumulative
  negative controls: **291/291**. Evidence: `docs/evidence/n6/`.

## [N5 — State sync: S4 chunks, manifest, S1 root verification, and corrupt-chunk rollback] 2026-11-01

**Sixth `NET_PLAN` phase.**
Deliverable: `patches/0049-N5-state-sync.patch`. Zero new external crate dependencies added.

- `aura_p2p::state_sync` and `aura_p2p::baseline` — integrates P2P state sync
  with S4 (`aura-state` `SnapshotManifest`, `chunk_hash`, `decode_chunk`,
  `export_snapshot`, `import_snapshot`, and S1 `state_root`), anchoring on a
  QC-certified `SyncedHeader`, verifying manifest checksum/`chunk_root`/`state_root`,
  rolling back and banning peers that serve corrupt chunks while re-requesting
  from honest peers, verifying the recomputed S1 sparse Merkle `state_root`,
  and advancing `aura_p2p::baseline` to `N5` (5 implemented, 3 placeholders).
- `crates/aura-p2p/tests/n5_state_sync.rs` and `crates/aura-p2p/tests/fixtures/n5_baseline.txt`
  — a new node syncs a 1,000-account S4 state snapshot (10 chunks) within
  measured time, rolls back a corrupted chunk, bans the offending peer,
  re-requests the chunk from an honest fallback peer, and verifies the S1
  `state_root`.
- `tools/nc_n5.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-p2p/src/state_sync.rs` (`--lib -- --exact`). Cumulative
  negative controls: **285/285**. Evidence: `docs/evidence/n5/`.

## [N4 — Block sync: header-first verification, QC validation, and parallel body download] 2026-11-01

**Fifth `NET_PLAN` phase.**
Deliverable: `patches/0048-N4-block-sync.patch`. Zero new external crate dependencies added.

- `aura_p2p::block_sync` and `aura_p2p::baseline` — implements header-first
  chain synchronization (`SyncedHeader`, `BLOCK_SYNC_HEADER_DOMAIN`), Quorum
  Certificate verification (`SyncQc`, `SyncValidatorSet`, `BLOCK_SYNC_QC_DOMAIN`,
  `>= 2/3 + 1` quorum stake), parallel block body download and `tx_root`
  verification (`SyncedBlockBody`, `BlockSyncEngine::download_bodies_parallel`,
  `BLOCK_SYNC_BODY_DOMAIN`) with automatic `BanList` escalation on corrupt peers,
  and advances `aura_p2p::baseline` to `N4` (4 implemented, 4 placeholders).
- `crates/aura-p2p/tests/n4_block_sync.rs` and `crates/aura-p2p/tests/fixtures/n4_baseline.txt`
  — a lagging node catches up 10,000 blocks via header-first verification and
  parallel body download across 4 peers (with out-of-order chunk completion) and
  reaches the exact canonical `block_hash` and `state_root`; corrupt body peers
  are banned and recovered from honest peers.
- `tools/nc_n4.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-p2p/src/block_sync.rs` (`--lib -- --exact`). Cumulative
  negative controls: **279/279**. Evidence: `docs/evidence/n4/`.

## [N3 — Peer discovery: Kademlia DHT, bootstrap list, NAT/UPnP, and Circuit Relay v2] 2026-11-01

**Fourth `NET_PLAN` phase.**
Deliverable: `patches/0047-N3-discovery.patch`. Zero new external crate dependencies added.

- `aura_p2p::discovery` and `aura_p2p::baseline` — implements the 256-bit
  Kademlia XOR metric (`XorDistance`), 256-bucket `KademliaTable` (`k = 20`,
  `alpha = 3`, `closest_peers`, stale `seq` rejection, `BanList` check),
  domain-separated Ed25519 `SignedPeerRecord` (`PeerId == BLAKE3(public_key)`
  and `chain_id` binding), `BootstrapList`, `UpnpMapper` (`NatStatus::UpnpMapped`),
  `RelayServer` / `RelayReservation` (`NatStatus::BehindSymmetricNat`), and
  advances `aura_p2p::baseline` to `N3` (3 implemented, 5 placeholders).
- `crates/aura-p2p/tests/n3_discovery.rs` and `crates/aura-p2p/tests/fixtures/n3_baseline.txt`
  — 30 nodes behind NAT (cone NAT with UPnP + symmetric NAT with Circuit Relay v2)
  find all 29 other peers in `<= 4` iterative Kademlia lookup rounds with no
  central server and exchange bounded frames over relay and UPnP endpoints.
- `tools/nc_n3.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-p2p/src/discovery.rs` (`--lib -- --exact`). Cumulative
  negative controls: **273/273**. Evidence: `docs/evidence/n3/`.

## [D8 — `aura` CLI: node ops, query, tx, keys, genesis, snapshot, and upgrade] 2026-10-31

**Ninth and final `NODE_PLAN` phase (`NODE_PLAN` D0–D8 complete).**
Deliverable: `patches/0046-D8-cli-aura.patch`. Zero new external crate dependencies added.

- `aura_node::cli` and `crates/aura-genesis/src/bin/aura.rs` — implements the
  complete `aura` command-line interface across all seven subcommand families:
  `genesis` (`hash`, `verify`, `show`), `keys` (`derive`, `inspect`, `sign`,
  `verify`), `tx` (`transfer`, `decode`, `hash`), `query` (`balance`, `block`,
  `tx`, `health`), `node` (`config-check`, `status`, `health`), `snapshot`
  (`create`, `verify`, `info`), and `upgrade` (`plan`, `verify`), plus complete
  `--help` and `--version` documentation.
- `crates/aura-node/tests/fixtures/d8_golden.txt` and `crates/aura-node/tests/d8_cli.rs`
  — 30 golden CLI vectors verified end-to-end plus live `aurad` devnet transaction
  submission (`aura tx transfer --rpc`) and RPC/health queries (`aura query`,
  `aura node`).
- `tools/nc_d8.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-node/src/cli.rs` (`--lib -- --exact`). Cumulative
  negative controls: **267/267**. Evidence: `docs/evidence/d8/`.

## [D7 — Four-validator devnet (M1), `make devnet` orchestration, and 3-of-4 quorum resilience] 2026-10-31

**Eighth `NODE_PLAN` phase (Step 4 of `D7_READINESS.md`, Milestone M1).**
Deliverable: `patches/0045-D7-devnet.patch`. Zero new external crate dependencies added.

- `aura_node::devnet` — implements deterministic 4-validator devnet composition
  (`DEVNET_CHAIN_ID = "auralite-devnet-1"`, `DEVNET_VALIDATOR_COUNT = 4`,
  `DEVNET_QUORUM_THRESHOLD = 3`, funded development-only faucet key),
  constant-time peer identity verification (`verify_devnet_peer_identity`),
  signed and deduplicated N2 gossip frame ingestion (`ingest_devnet_gossip`),
  3-of-4 (`2f + 1`) BLS vote verification and QC aggregation
  (`devnet_quorum_threshold`, `try_commit_round_votes`), in-process
  `DevnetCluster`, and the multi-process `aurad` devnet worker loop
  (`run_devnet_worker`, `trigger_and_wait_devnet_round`).
- `crates/aura-node/tests/d7_devnet.rs`, `tools/run_devnet.py` (`make devnet`),
  and `docker-compose.yml` — launch 4 independent `aurad` validator processes
  with separate storage and ports, verify `/health` and `/metrics`, execute a
  signed faucet transfer via JSON-RPC (`submit_tx`), finalize >= 100 blocks
  across all 4 nodes with identical `height`, `block_hash`, `state_root`, and
  account balances (`get_block`, `get_balance`), and verify continued
  finalization across the remaining 3 validators after stopping `validator-3`
  (`SIGTERM`, `f = 1` quorum of 3).
- `tools/nc_d7.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-node/src/devnet.rs` (`--lib -- --exact`). Cumulative
  negative controls: **261/261**. Evidence: `docs/evidence/d7/`.

## [B5 — Stake-weighted VRF leader election, grinding protection, and N1/N2 consensus wiring] 2026-10-31

**Sixth `CONSENSUS_PLAN` phase (Step 3 of `D7_READINESS.md`).** Deliverable:
`patches/0044-B5-vrf-leader.patch`. Zero new external crate dependencies added.

- `aura_consensus::vrf_leader` — implements cryptographic BLS12-381 VRF proving
  and public verification (`VrfInput`, `VrfProof`, `prove_vrf`, `verify_vrf`),
  grinding protection (`VrfError::ZeroEpochRandomness` after epoch 0 and
  `derive_epoch_randomness` over verified boundary `QuorumCertificate`s with
  canonically sorted signer keys/signatures), and stake-weighted VRF leader
  election (`VrfValidatorEntry`, `VrfLeaderElection`, `LeaderClaim`) mapping the
  64-bit VRF ticket onto `[0, total_stake)` via 128-bit fixed-point arithmetic.
- `aura_consensus::vrf_leader` N1/N2 wiring — connects `aura_p2p::identity::PeerIdentity`
  and `aura_p2p::gossipsub::GossipEngine` (`GossipTopic::Proposal`,
  `GossipTopic::Vote`, `ProposalEnvelope`, `VoteEnvelope`) to `FinalityEngine`,
  `QcAggregator`, `ValidatorSet`, and `VrfLeaderElection` via
  `NetworkedValidatorNode` and `FourValidatorNetwork` without bypassing BLS vote
  or QC verification.
- `crates/aura-consensus/tests/b5_vrf_leader.rs` — 10,000-round stake-weighted
  VRF leader distribution within statistical confidence interval (`+/- 200`
  rounds across `10%/20%/30%/40%` stakes), 4-validator 100-round N1/N2 BFT
  finality surviving 1 validator offline (`f = 1`, quorum = 3), and forged
  proposal/vote rejection.
- `aura_consensus::baseline` — updated to `PHASE = "B5"` (all 6 B1–B5 layers
  implemented, 0 placeholders).
- `tools/nc_b5.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-consensus/src/vrf_leader.rs` (`--lib -- --exact`).
  Cumulative negative controls: **255/255**. Evidence: `docs/evidence/b5/`.

## [B4 — Validator set: staking contract, epoch, enter/exit queue, and QC-verified set update] 2026-10-31

**Fifth `CONSENSUS_PLAN` phase.** Deliverable:
`patches/0043-B4-validator-set.patch`. Zero new external crate dependencies added.

- `aura_consensus::validator_set` — implements `StakingContract` (validator
  self-stake, delegations, `stake_and_enter`), deterministic epoch transitions
  (`EpochInfo`, `EnterRequest`, `ExitRequest`, `ValidatorSet::advance_block`),
  and QC-verified validator set updates (`SetUpdate`, `apply_set_update`,
  `apply_verified_set_update`) enforcing `SetUpdateError::WrongEpoch`,
  `SetUpdateError::InsufficientStake`, and `SetUpdateError::InvalidQc` (2f+1
  BLS quorum certificate bound to `current_tip_hash`).
- `crates/aura-consensus/tests/b4_validator_set.rs` — 7-node consensus test
  transitioning from 4 to 7 active validators at height `H = 50` across 100
  continuous blocks without stopping finality (quorum shifts from 3-of-4 to
  5-of-7), plus QC-authorized set update verification.
- `aura_consensus::baseline` — updated to `PHASE = "B4"` (5 implemented:
  `proposer`, `block-signature`, `qc`, `finality`, `validator-set`; 1
  placeholder: `vrf-leader` B5).
- `tools/nc_b4.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-consensus/src/validator_set.rs` (`--lib -- --exact`).
  Cumulative negative controls: **249/249**. Evidence: `docs/evidence/b4/`.

## [N2 — Gossipsub topics, message signatures, dedup, and peer scoring] 2026-10-31

**Third `NET_PLAN` phase (Step 2 of `D7_READINESS.md`).** Deliverable:
`patches/0042-N2-gossipsub.patch`. Zero new external crate dependencies added.

- `aura_p2p::gossipsub` — implements the 5 canonical `D-N2` gossipsub topics
  (`tx`, `proposal`, `vote`, `evidence`, `snapshot`), domain-separated Ed25519
  message signatures (`AURA_P2P_GOSSIP_SIG_V1\0`) with constant-time
  `PeerId == BLAKE3(pubkey)` verification (`D-N3`), bounded `SeenCache`
  deduplication over deterministic `MessageId = BLAKE3(AURA_P2P_GOSSIP_ID_V1\0 || ...)`,
  per-peer scoring (`PeerScoreParams`, `PeerScoreState`, `GossipEngine`) with
  sliding-window flood rate limiting and automatic `BanList` escalation at
  `ban_threshold`, and multi-node mesh propagation (`GossipMesh`).
- `crates/aura-p2p/tests/n2_gossipsub.rs` — 1,000 signed transactions broadcast
  across a 10-node mesh (ring + chords) in `< 1 s` with deduplication across all
  10 nodes; flooding and signature-forging peers lose score and are banned.
- `aura_p2p::baseline` — updated to `PHASE = "N2"` (2 implemented: `transport`,
  `gossipsub`; 6 placeholders `N3..N8`).
- `tools/nc_n2.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-p2p/src/gossipsub.rs` (`--lib -- --exact`). Cumulative
  negative controls: **243/243**. Evidence: `docs/evidence/n2/`.

## [N1 — P2P transport, identity, handshake, and ban list] 2026-10-31

**Second `NET_PLAN` phase (Step 1 of `D7_READINESS.md`).** Deliverable:
`patches/0041-N1-transport.patch`. Zero new external crate dependencies added.

- `aura_p2p::identity` — cryptographic peer identity (`PeerId = BLAKE3(ed25519_pubkey)`,
  `D-N1`) with constant-time verification (`PeerId::verify_public_key`) and
  domain-separated Ed25519 handshake transcript signing/verification
  (`AURA_P2P_HANDSHAKE_V1\0` binding `protocol_version`, `chain_id`,
  initiator/responder role, local & remote challenges, and public key).
- `aura_p2p::ban` — deterministic `BanList` tracking banned `PeerId`s and
  `IpAddr`s with permanent or expiring bans (`BanEntry`, `BanReason`).
- `aura_p2p::transport` — Noise-inspired `HandshakeHello` challenge-response
  exchange (`PROTOCOL_VERSION = 1`, `TransportKind`), directional BLAKE3-keyed
  stream + MAC session cipher (`SessionCipher`) with monotonic sequence-number
  anti-replay and `MAX_FRAME_BYTES` cap, and live TCP `TransportSwarm`.
- `crates/aura-p2p/tests/n1_transport.rs` — 10 live `TransportSwarm` nodes
  connect over TCP (`127.0.0.1:0`), authenticate each other, reject identity
  substitution (`P2pError::IdentitySubstitution`) and automatically ban the
  impostor `PeerId`, reject self-connections, and enforce peer/IP bans.
- `aura_p2p::baseline` — updated to `PHASE = "N1"` (1 implemented `transport`,
  7 placeholders `N2..N8`).
- `tools/nc_n1.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-p2p` (`--lib -- --exact`). Cumulative negative controls:
  **237/237**. Evidence: `docs/evidence/n1/`.

## [N0 — P2P network baseline] 2026-10-31

**First `NET_PLAN` phase, and a measurement rather than a feature.** Deliverable:
`patches/0040-N0-net-baseline.patch`. The claim: *there is no P2P network yet,
and Node B cannot observe Node A's transactions over RPC without N1 transport
and N2 gossipsub*. Zero external dependencies added.

- `aura_p2p::baseline` — surveys the eight `NET_PLAN` layers (`transport` N1,
  `gossipsub` N2, `discovery` N3, `block-sync` N4, `state-sync` N5,
  `dos-defence` N6, `light-client` N7, `chaos-harness` N8), classifying all
  eight as `placeholder` at N0 (0 implemented, 8 placeholders, 0 missing), and
  records the two-node isolation probe (`node_a_knows_tx=true`,
  `node_b_knows_tx=false`, `peer_count=0`,
  `documented_gap=N1-transport+N2-gossipsub-absent:node_b_cannot_see_node_a_tx`).
- The `aura-p2p` binary: no arguments → exit **2** (`EXIT_STUB`) naming every
  missing N1–N8 capability; `report` and `report --fixture` measure the tree and
  exit **3** on a non-workspace; unknown command → exit **64**.
- `crates/aura-node/tests/n0_two_nodes_no_p2p.rs` — starts two independent live
  `aurad` HTTP/JSON-RPC nodes (`node-a` and `node-b`), submits and commits a
  signed transfer on Node A, and proves over HTTP JSON-RPC that Node B cannot
  observe Node A's transaction (`get_tx` returns `null`, recipient balance stays
  `0`, height stays `0`, `/health` reports `peerCount=0` and
  `p2p="not_implemented"`).
- `tools/nc_n0.py` — **6/6** negative controls targeting exact library unit
  tests in `crates/aura-p2p/src/baseline.rs` (`--lib -- --exact`). Cumulative
  negative controls: **231/231**. Evidence: `docs/evidence/n0/`.

## [D6 — observability] 2026-10-30

**Deliverable:** `patches/0039-D6-observability.patch`. The node now emits JSON
structured logs, optionally exports OpenTelemetry spans via OTLP/gRPC, and serves
Prometheus `/metrics` plus readiness `/health` through the existing shared HTTP
listener. Pipeline height, finality lag and mempool gauges refresh from live state;
committed blocks update gas/transaction counters and a rolling gas-per-second gauge.

- Added a Grafana dashboard for committed height, finality lag, mempool size,
  peer count and committed gas/s, with setup and metric semantics in
  [`docs/observability/`](docs/observability/).
- P2P is not implemented. `aurad_peer_count` and `aurad_p2p_available` are
  explicitly zero; D6 does not claim network peers or multi-validator behavior.
- `make test-d6`: four observability unit tests, JSON log test, in-memory OTLP
  span-export test and two live HTTP contract tests passed. `make nc-d6`:
  **6/6** exact library-unit-test mutations reddened; restored library tests pass.
- Evidence and clean D5-baseline apply/revalidation: `docs/evidence/d6/`.

## [D5 — gRPC + REST] 2026-10-30

**Deliverable:** `patches/0038-D5-grpc-rest.patch`. Added `aura-grpc-rest` with
protobuf schema/codegen, an HTTP REST gateway and served OpenAPI 3.1 documents.
Both adapters use the existing D4 RPC service and preserve the D2/D3 execution
and validation boundaries.

- REST `/api/v1` and `/api/v1.1` share the same result contract; v1.1 adds
  `apiVersion` without changing the payload. Closed JSON request schemas reject
  unknown fields; unsupported versions and operations return explicit errors.
- gRPC uses a separate configurable listener (`--grpc-addr`, `AURA_GRPC_ADDR`,
  or TOML `grpc_addr`, default `127.0.0.1:9090`). Its additive v1.1 response
  metadata leaves prior protobuf field numbers and method paths unchanged.
- Only native transfers execute. Contract calls and unsupported transaction
  kinds remain explicit errors; D5 adds no P2P, multi-validator network or
  contract execution.
- `make test-d5`: seven REST/gRPC unit tests, one live HTTP + HTTP/2 contract,
  and four daemon CLI tests. `make nc-d5`: **6/6** exact-library-unit-test
  negative controls. Clean `make test`: workspace formatting, clippy, all tests,
  docs/dependency/vector/rig guards and close-out audit passed. Cumulative
  negative controls: **219/219**.
- Evidence: `docs/evidence/d5/`; complete contract:
  [`docs/spec/grpc-rest.md`](docs/spec/grpc-rest.md) and
  [`docs/api/openapi-d5.yaml`](docs/api/openapi-d5.yaml).

## [D0 — node baseline] 2026-10-29

**First `NODE_PLAN` phase, and a measurement rather than a feature.** Deliverable:
`patches/0029-D0-node-baseline.patch`. The claim: *the libraries exist, the node does
not* — and `aurad` refuses to pretend otherwise. No new crate, no new dependency
(`aura-node` was an empty placeholder since C0).

- `aura_node::baseline` — a survey of the thirteen library layers (files, lines, public
  item declarations) classifying each as `implemented`, `placeholder` or `missing`, plus
  the nine node capabilities that do not exist and the phase (`D1`–`D8`) that owes each.
- The `aurad` binary: no arguments → exit **2** with every missing capability and its
  phase; `--config`, `--chain-id`, `--data-dir`, `--rpc-addr`, `--validator-key` →
  recognised, named and refused; unknown command → exit **64**; `report` measures the
  tree and exits **3** when there is nothing to measure; `--version` prints
  `(D0 baseline — the node does not exist)`.
- **Measured now:** 9 layers implemented, 4 placeholders (`aura-consensus` B1,
  `aura-p2p` N1, `aura-mempool` D3, `aura-rpc` D4 — zero public items each), 0 missing.
- `tools/gen_d0_vectors.py` writes `crates/aura-node/tests/fixtures/d0_baseline.txt`
  from the binary's own `report --fixture` output; the gate compares live survey to pin,
  so landing consensus code without updating the plan fails the build.
- `tools/nc_d0.py` — 6 negative controls, each reddening a **named** test after a green
  baseline: the stub exits 0, every layer classified `implemented`, `report` measuring a
  non-workspace, an unknown command falling through to exit 0, the refusal dropping the
  capability list, and a node flag no longer recognised.
- Gates: 15 tests in the D0 gate, 374 total; 165 negative controls (6 new); 4 857
  vectors across 27 fixtures; 239 external crates unchanged.
- Documented: [`docs/NODE.md`](docs/NODE.md).
- Honest scope: a static count is not an audit; nothing starts a process, binds a port
  or parses a config file, on purpose; and **M0 still needs B (consensus), N (network)
  and the rest of D** — those crates are empty.

## [D1 — aurad skeleton] 2026-10-29

**First process phase of `NODE_PLAN`.** Deliverable:
`patches/0030-D1-aurad-skeleton.patch`. The `aurad` binary starts: it loads
configuration (TOML + env + CLI, `chain-id` mandatory, parsed with `toml::Value`
— `serde` is a D4 dependency), initialises tracing, writes a PID file, installs
signal handlers and shuts down gracefully on SIGTERM/SIGINT. No new crate, no new
dependency beyond `toml`, `tracing` and `tracing-subscriber`.

- `aura_node::config` — `Config` (chain-id, data-dir, rpc-addr, validator-key)
  loaded from CLI flags, environment variables (`AURA_*`) and `aurad.toml`, with
  CLI overriding env overriding TOML. `chain-id` is mandatory; `aurad` refuses
  with exit 2 and names the `config` capability (D1) without it.
- `aura_node::lifecycle` — `init_tracing()` (stderr, env-filtered), `write_pid_file`,
  `remove_pid_file`, `health_probe`, `install_signal_handlers` (OS default at D1,
  no `unsafe` FFI), `graceful_shutdown`.
- The `aurad` binary: `--chain-id <id>` starts the skeleton; `--help`/`--version`
  succeed; `report` measures the tree; unknown command → exit **64**.
- **Measured now:** 11 layers implemented, 2 placeholders (`aura-consensus` B1,
  `aura-p2p` N1 — zero public items each), 0 missing. `MISSING_CAPABILITIES`
  shrinks from 9 to 7 (config and lifecycle are no longer missing).
- `tools/gen_d1_vectors.py` writes `crates/aura-node/tests/fixtures/d1_skeleton.txt`;
  `tools/nc_d1.py` — 6 negative controls (chain-id mandatory, unknown TOML fields,
  report on non-workspace, unknown command, refusal names D1, config without chain-id).
- Gates: 12 integration tests in the D1 gate; D0 gate still green (13 tests);
  374 total workspace tests; 171 negative controls (6 new); 4 857 vectors / 27
  fixtures unchanged.

## [D2 — transaction pipeline] 2026-10-29

**D2 transaction path.** Deliverable: `patches/0035-D2-transaction-pipeline.patch`.
`aura_node::pipeline` now validates signed native transfers, executes them in
canonical `(signer, nonce)` order, constructs C5 blocks, obtains BLS precommit
QCs through the B3 three-chain engine, commits account state and transaction
receipts to S2 storage, and publishes events only after durability succeeds.

- Transfer payload: fixed `recipient[20] || amount_be[16]`; fixed gas 21,000,
  base fee 1, tip paid to proposer and base fee burned.
- Nine integration tests include a 1,000-transfer end-to-end run, receipt/root
  checks, finality delay, replay/nonce/balance/gas/fee rejection and fail-closed
  reopen behavior.
- `tools/nc_d2.py`: six controls that require named D2 tests to go red; the
  full close-out inventory now covers 201 negative controls.
- Honest boundary: the in-process composition is one-validator/local, only
  `Transfer` is executable, and committed-state recovery requires a finality
  checkpoint not yet persisted. Multi-validator transport, richer execution,
  RPC and checkpoint recovery remain later phases.

## [D3 — fee-priority mempool] 2026-10-30

**Bounded transaction pool for the D2 native-transfer path.** Deliverable:
`patches/0036-D3-fee-priority-mempool.patch`. `aura-mempool` now exposes a
16,384-entry sender/nonce pool with tip-priority selection over contiguous nonce
lanes, a checked 10% replacement bump, deterministic strict-higher-tip eviction,
and a reorg reinsertion boundary. `aura-node::pipeline` validates projected
nonce, balance, gas and fee state before admission, and removes selected
transactions only after a proposal is formed.

- Six unit-test negative controls cover nonce validation, fee priority,
  replacement, eviction, reorg reinsertion and transfer gas floor.
- The D3 acceptance gate submits 10,000 signed sequential transfers and checks
  exact nonce/balance accounting, unique durable receipts, gas, state root and
  commit-event ordering under three-chain finality.
- Scope limit: reorged transactions must be revalidated against the caller's new
  account view before reinsertion; D3 does not add a distributed reorg engine or
  finality checkpoint persistence.
- Evidence and cumulative NC inventory: `docs/evidence/d3/` (207 controls).

## [D4 — JSON-RPC 2.0] 2026-10-30

**Bounded HTTP and WebSocket RPC over the validated local pipeline.** Deliverable:
`patches/0037-D4-json-rpc.patch`. The new `aura-rpc` service implements JSON-RPC
2.0 request/error/batch/notification semantics, an HTTP transport and RFC 6455
WebSocket commit subscriptions. The node adapter exposes transaction submission,
account and transaction queries, bounded recent committed-block lookup, gas
estimation, and explicit unsupported-contract errors.

- Body/message, batch, connection and incomplete-request time limits are bounded;
  WebSocket client frames must be masked. Node locks are released before response
  serialization or network writes. Drop-aware subscriptions unregister even when
  no later commit arrives.
- Contract tests cover the pipeline-backed service and transport. The 50 golden
  wire vectors pass; six unit-test-based negative controls pass.
- `fuzz_rpc` exercises the HTTP and WebSocket JSON parsers. A 60-second H3
  parser shard completed 733,119 executions with no crashes; the full 24-hour H3
  campaign and remaining parser targets are explicitly not claimed.
- Boundary: only native transfers execute. Contract calls and unsupported
  transaction execution return errors; D4 does not add P2P or multiple validators.
- Evidence and cumulative inventory: `docs/evidence/d4/` (213 controls).

## [B3 — BFT finality] 2026-10-29

**BFT finality of `CONSENSUS_PLAN`.** Deliverable:
`patches/0034-B3-bft-pipeline.patch`. B3 lands `aura_consensus::finality`
(`FinalityBlock`, `FinalizedBlock`, `ThreeChainCommit`, `ViewChange`,
`Timeout`, `FinalityEngine`). The crate now measures as
**4 implemented, 2 placeholders, 0 missing**. No new crate, no new
dependency — builds on `aura-types`, `aura-crypto`, `aura-consensus::bls`,
and `aura-consensus::qc`.

- `aura_consensus::finality` — `FinalityBlock` (view, round, parent hash,
  consensus timestamp, QC, transactions); `FinalizedBlock` (height + block);
  `ThreeChainCommit` (3-chain commit rule: block + child + grandchild QCs
  → commit); `ViewChange` (view, round, high QC, timeout); `Timeout`
  (duration + deadline); `FinalityEngine` (propose, vote, aggregate, commit,
  change_view, change_round).
- `aura_consensus::baseline` — updated for B3: survey classifies proposer,
  block-signature, qc, and finality as `implemented`; validator-set and
  vrf-leader remain `placeholder`. Refusal names the 2 remaining missing
  capabilities with their phases (B4–B5).
- `tools/gen_b3_vectors.py` writes `b3_baseline.txt` from the binary's own
  `report --fixture` output; `tools/nc_b3.py` — 6 negative controls (baseline
  exits 0, all layers implemented, report on non-workspace, unknown command,
  refusal stops naming capabilities, logs treated as equivalent).
- Gates: 11 unit tests in the finality module; 25 baseline tests; 36 total
  in the B3 gate; B2 gate still green (36 tests); B1 gate still green
  (22 tests); D0 gate still green (13 tests); D1 gate still green (12
  tests); 4925 total workspace vectors; 189 negative controls shipped
  (incl. B2); 195 expected (incl. B3).

## [B2 — QC / BLS aggregation] 2026-10-29

**QC / BLS aggregation of `CONSENSUS_PLAN`.** Deliverable:
`patches/0033-B2-qc-bls.patch`. B2 lands `aura_consensus::bls`
(BLS12-381 secret key, public key, signature, domain-separated signing and
verification, address derivation) and `aura_consensus::qc` (Vote, VoteType,
QuorumCertificate, QcAggregator, quorum_threshold). The crate now measures as
**3 implemented, 3 placeholders, 0 missing**. New dependency: `blst = "=0.3.14"`.

- `aura_consensus::bls` — `BlsSecretKey::from_seed([u8; 32])` derives a key;
  `BlsSecretKey::sign(chain_id, domain, message)` signs with domain separation
  via `aura_crypto::hash_domain`; `verify` checks a BLS signature against a
  public key; `address_from_public_key` derives a 20-byte validator address.
- `aura_consensus::qc` — `Vote { view, round, block_hash, validator,
  vote_type, signature }`; `QuorumCertificate` holds public keys and
  signatures; `QcAggregator` collects votes keyed by validator address and
  emits a QC once `quorum_threshold(n)` signatures are present; QC verification
  checks every signature individually.
- `aura_consensus::baseline` — updated for B2: survey classifies proposer,
  block-signature, and qc as `implemented`; finality, validator-set, vrf-leader
  remain `placeholder`. Refusal names the 3 remaining missing capabilities with
  their phases (B3–B5).
- `tools/gen_b2_vectors.py` writes `b2_baseline.txt` from the binary's own
  `report --fixture` output; `tools/nc_b2.py` — 6 negative controls (baseline
  exits 0, all layers implemented, report on non-workspace, unknown command,
  refusal stops naming capabilities, logs treated as equivalent).
- Gates: 36 unit tests in the B2 gate; B1 gate still green (22 tests); D0 gate
  still green (13 tests); D1 gate still green (12 tests); 4925 total workspace
  vectors; 183 negative controls shipped (incl. B1); 189 expected (incl. B2).

## [B1 — PoA proposer + block signature] 2026-10-29

**First real consensus code of `CONSENSUS_PLAN`.** Deliverable:
`patches/0032-B1-poa-proposer.patch`. B1 lands `aura_consensus::proposer`
(deterministic round-robin over N validators) and `aura_consensus::signature`
(simple Ed25519 block signature with domain separation). The crate now measures
as **2 implemented, 4 placeholders, 0 missing**. No new crate, no new
dependency — builds on `aura-types`, `aura-crypto`, `aura-block`, `aura-tx`.

- `aura_consensus::proposer` — `Proposer::new(validators)` selects
  `validators[round % len]` for each round. Panics on empty validator set.
  `is_proposer(address, round)` checks membership. Deterministic PoA rotation.
- `aura_consensus::signature` — `BlockSignature::sign_block(seed, validator,
  block_hash, chain_id)` produces a 64-byte Ed25519 signature over
  `hash_domain(chain_id, Domain::Block, block_hash)`. `SignedBlock::sign(...)`
  builds and signs a block in one call. `verify(chain_id)` checks it.
- `aura_consensus::baseline` — updated for B1: survey classifies proposer and
  block-signature as `implemented`; qc, finality, validator-set, vrf-leader
  remain `placeholder`. Refusal names the 4 remaining missing capabilities with
  their phases (B2–B5).
- `tools/gen_b1_vectors.py` writes `b1_baseline.txt` from the binary's own
  `report --fixture` output; `tools/nc_b1.py` — 6 negative controls (baseline
  exits 0, all layers implemented, report on non-workspace, unknown command,
  refusal stops naming capabilities, logs treated as equivalent).
- Gates: 22 unit tests in the B1 gate; D0 gate still green (13 tests); D1 gate
  still green (12 tests); 384 total workspace tests; 177 negative controls
  shipped (incl. B0); 183 expected (incl. B1).

## [B0 — consensus baseline] 2026-10-29

**First `CONSENSUS_PLAN` phase, and a measurement rather than a feature.** Deliverable:
`patches/0031-B0-consensus-baseline.patch`. The claim: *there is no consensus, and this module measures that claim*. No new crate, no new dependency (`aura-consensus` was an empty placeholder with a `[[bin]]` since C0).

- `aura_consensus::baseline` — a survey of the six consensus layers (proposer, block-signature, QC, finality, validator set, VRF leader), classifying each as `implemented`, `placeholder` or `missing`, plus the six missing capabilities and the phase (`B1`–`B5`) that owes each. All six are placeholders at B0; 0 implemented, 0 missing.
- The `aura-consensus` binary: no arguments → exit **2** with every missing capability and its phase; `--help`/`--version` succeed; `report` measures the tree and exits **3** when there is nothing to measure; unknown command → exit **64**.
- `crates/aura-consensus/tests/fixtures/b0_baseline.txt` pins the survey so it cannot drift silently; `tools/gen_b0_vectors.py` writes it from the binary's own `report --fixture` output.
- `tools/nc_b0.py` — 6 negative controls, each reddening a **named** test after a green baseline: the baseline exits 0, every layer classified `implemented`, `report` measuring a non-workspace, an unknown command falling through to exit 0, the refusal dropping the capability list, and two different logs treated as equivalent.
- Gates: 10 unit tests in the B0 gate; D0 gate still green (13 tests); D1 gate still green (12 tests); 374 + 10 = 384 total workspace tests; 177 negative controls (6 new).

## [X8 — static verifier] 2026-10-29

**Eighth and last Stage 1 `EXEC_PLAN` phase.** Deliverable:
`patches/0028-X8-static-verifier.patch`. A **pure function over bytes**, not an
engine: `aura_exec::verify` answers with a `Report` — verdict, size, functions,
instructions, peak stack depth, peak body — and one `Problem { code, offset,
detail }` per defect, the offset being a byte offset into the module.

- **22 stable refusal codes**, each with a witness: `bad_magic`, `bad_version`,
  `module_too_large`, `section_order`, `section_unknown`, `section_truncated`,
  `float_type`, `import_not_allowed`, `global_type`, `memory_limits`,
  `function_code_mismatch`, `body_truncated`, `opcode_banned`, `stack_underflow`,
  `stack_too_deep`, `label_depth`, `local_index`, `global_index`, `call_index`,
  `opcode_budget`, `locals_budget`, `export_shape`.
- Checks: section order / uniqueness / id range / declared lengths, `function`
  against `code` counts, every value type, every local, the import whitelist (three
  ABI surfaces, function imports only) plus `VerifyLimits::restrictive()` that
  refuses all imports, integer-only globals, a declared memory maximum against the
  census ceiling, the `run` export, and — per body — instruction and local budgets,
  operand-stack depth with **callee signatures resolved from the type section**,
  label depth, and the `local` / `global` / `call` / `call_indirect` index spaces.
- `tools/gen_x8_mutants.py` — an **independent** Python section walker that damages
  one thing per module and states the verdict it expects: 15 kinds × 6 mutants over
  the six shipped artifacts, with a second round for the five kinds whose first six
  do not cover every artifact — **100 exactly**, plus the 6 unmodified artifacts.
  `make gen-x8-mutants`, `make test-x8`, `make nc-x8`, `make evidence-x8`.
- `x8-verify` — the binary the gate and the evidence script both run, so the report
  a reader sees is the report the gate asserts on: `x8-verify <file>...` and
  `x8-verify --fixture crates/aura-exec/tests/fixtures/x8_mutants.txt`.
- **Measured:** 100/100 mutants refused with the expected code (zero mismatches);
  6/6 shipped artifacts accepted — `kv` 33 functions / 1 905 instructions, `store`
  29 / 1 921, `events` 25 / 1 871, `vault` 53 / 3 692, `evil` 61 / 4 235, `recurse`
  51 / 3 331 — peak stack depth 7, peak body 630 instructions. Pinned in the gate.
- `tools/nc_x8.py` — 6 negative controls, each reddening a **named** test after a
  green baseline: `call` ignoring the callee signature, a stack underflow that never
  reaches the report, float value types read as `i32`, any export name counting as
  `run`, the `function`/`code` counts allowed to disagree, and the size ceiling
  unenforced.
- Gates: 37 tests in `x8_verify`, 359 total; 159 negative controls (6 new); 4 826
  vectors across 26 fixtures; 239 external crates unchanged — X8 adds **no**
  dependency and no crate.
- Fixed: `call` originally popped nothing and pushed nothing, so `store.wasm` was
  refused with `stack_underflow` and every stack number was arithmetic on a fiction;
  body offsets were relative to the body slice rather than the module.
- Honest scope: **depth, not types** — the stack is tracked as a length, so a body
  that confuses two integer widths can pass and still be refused by the engine; the
  engine stays the authority on instantiation. No phase calls the verifier yet
  (deploy-path wiring is D-phase), the budget constants are policy rather than
  measurement, and there is no differential run against `wasmparser` yet.

## [X7 — determinism gate] 2026-10-29

**Eighth `EXEC_PLAN` phase.** Deliverable: `patches/0027-X7-determinism-gate.patch`.
A gate, not a subsystem: **the same ordered input, replayed by independent nodes,
produces the same `state_root`, gas bill and events** — measured over 1 004 ordered
transactions in eight separate processes and three build configurations.

- `aura_exec::detcheck` — a scenario that is a pure function of one constant
  (SplitMix64 from `X7_SEED`), a replay driver over the X3 `Lifecycle` and the S3
  overlay, and a rolling digest that folds every step's class, status, gas and
  resulting root, so a divergence is localised rather than reported as "the roots
  differ".
- `x7-run` — one process, one node: `--node N` (a label, never an input),
  `--seed S` (the sensitivity check) and `--fixture` (the pinned lines).
- The workload mixes ABI v1 calls (40% kv, 18% store, 15% events), native transfers
  (10%), ABI v3 calls with a nested frame (12%), calls to an address with no code
  (3%) and out-of-gas calls (2%) — so the gate also re-runs X2, X3 and X6 1 000 times.
- Pins the shipped artifacts (`kv`, `store`, `events`, `vault`) by size and BLAKE3 in
  `crates/aura-exec/tests/fixtures/x7_determinism.txt`, generated by
  `tools/gen_x7_vectors.py` from `x7-run --fixture`.
- `tools/x7_matrix.py` (`make x7-matrix`) — `debug`, `release` and
  `release-cgu16`, two processes each: three *build configurations* on one machine.
  The three-OS dimension is the new CI job `.github/workflows/ci.yml` →
  `determinism` and is reported as **unproven** until it runs.
- `tools/nc_x7.py` — 6 negative controls, each reddening a **named** test after a
  green baseline: version stamp dropped from the root, empty diff counted as a
  commit, digest ignoring the resulting root, a refusal applying its diff, a starved
  call billed nothing, the generator ignoring its seed.
- Gates: 11 tests in `x7_determinism`, 322 total; 153 negative controls (6 new);
  4 692 vectors across 24 fixtures; 239 external crates unchanged — X7 adds **no**
  dependency and no crate.
- Documented: `docs/DETERMINISM.md`.
- Fixed: the deployer constant was not a funded account (one byte); an argument was
  passed to `run()` of contracts that take none, so a third of the workload never
  ran; the vault's `withdraw` was sent an address in front of its amount, so every
  withdrawal refused `-21`; and the first NC was green until it targeted the digest's
  root fold instead of its index.

## [X6 — call isolation] 2026-10-29

**Seventh `EXEC_PLAN` phase.** Deliverable: `patches/0026-X6-call-isolation.patch`.
A contract can now call another contract — and the two rules that make that safe
live in the host, not in the SDK: **a frame may not call an address already on the
stack** (`-14`) and **a stack may not exceed 8 frames** (`-15`). Stricter than a
per-account lock and simpler: the withdraw-then-reenter attack becomes
*unrepresentable*, not merely detected.

- `aura-exec::abi_v3` — `call` (100 + 3/byte), `returndata_len` (20),
  `returndata_read` (20 + 3/byte), `depth` (20), in the import module **`env3`**;
  `env` and `env2` untouched, so a v1/v2 contract is bit-for-bit unaffected.
- `abi_v3::MAX_CALL_DEPTH = 8`, `admit` (reentrancy → depth) before a child store
  exists, `nested_call` (bounds → calldata ≤ 1 KiB → charge → value → admit →
  code check `-16` → child store), `merge_child`, `merge_events`, `undo_value`.
- New codes: `-14 reentrancy`, `-15 call_depth`, `-16 no_code`,
  `-17 callee_failed` (a callee that traps maps to `-17`; `OutOfGas` stays `-8`).
- `aura-sdk::abi3` — `call_contract`, `returndata`, `returndata_len`, `depth`,
  plus a mock backend for host tests.
- `examples/vault` (pays **before** it debits, the bug X6 exists to survive; money
  moves with `env.transfer`, control with `env3.call`), `examples/evil` (re-enters
  from the payout callback, probes, sends value, records statuses instead of
  trapping), `examples/recurse` (nine-copy chain).
- `tools/build_x6_contracts.py` (builds twice, pins size/sha256 in
  `fixtures/x6_calls.txt`, then runs `x6-record` and appends the measured block —
  including the node's own BLAKE3 over each shipped `.wasm`), `tools/nc_x6.py`
  (6 NCs), `tools/record_x6_deps.py`, `tools/write_x6_evidence.py`.
- Gates: 15 tests in `x6_calls`, 311 total; 147 negative controls (6 new); 4 692
  vectors across 24 fixtures; 239 external crates unchanged — X6 adds **no**
  dependency, external or workspace.
- Documented: `docs/ABI_V3.md`, `docs/CALLS.md`.
- Fixed: the `wasmi` backend's `check_imports` kept its own copy of the allowed
  modules and refused `env3`; a child's status was dropped, so a callee's `-14`
  reached its caller as `0`; the vault's first draft paid through `env3.call`,
  which cannot pay an EOA; the depth fixture called itself, which the guard
  correctly refuses.

## [X5 — reference standards and ABI v2] 2026-10-29

**Sixth `EXEC_PLAN` phase.** Deliverable: `patches/0025-X5-standards.patch`.
Three standards in Rust — ARC-20, ARC-721, ARC-M1 multisig — and the ABI they
cannot exist without: **v2 in the module `env2`**, so `env` and its 23 frozen
vectors stay untouched.

- `aura-exec::abi_v2` — `calldata_len`, `calldata_read`, `return_write`, `caller`,
  `self_address`, `abi_version`. 20 gas base, 3 gas/byte for the copying calls, one
  counter shared with instructions and v1. Calldata 1 KiB and return data 1 KiB,
  both refused **before** the engine runs.
- `Session::set_calldata`, `Executor::call_data`, `Lifecycle::call_data`,
  `ContractReceipt::return_data` (empty unless the call committed),
  `LifecycleReceipt::ret`.
- `aura-sdk::abi2` (typed wrappers + mock backend), `codec::Reader`, and the
  `dispatch!` macro: 4-byte selector, arguments decoded in order, unknown selector
  `-20`, malformed arguments or a trailing byte `-21`.
- `standards/arc20` (10 methods, `caller`-derived `from`, hashed allowance keys),
  `standards/arc721` (10 methods, per-token owner slot, every transfer clears the
  approval), `standards/multisig` (8 methods, immutable 3-signer policy, executed
  flag written before the transfer and cleared on failure).
- `tools/build_standards.py` (builds twice into independent target dirs, pins
  size/sha256/blake3 in `crates/aura-exec/tests/fixtures/x5_standards.txt`, writes
  `abi.json` per standard), `x5-record` (measured gas), `tools/nc_x5.py` (6 NCs),
  `tools/write_x5_evidence.py`.
- Gates: 43 tests over the shipped artifacts (11 + 10 + 9 + 8 executable, 5 pins),
  296 total; 141 negative
  controls (6 new); 239 external crates unchanged — the standards add no lock entry.
- Documented: `docs/ABI_V2.md`, `docs/STANDARDS.md`.
- Fixed: an ARC-721 `approve` that trapped on every call (event buffer width), the
  missing `self_address` that made a balance check impossible, `RUSTFLAGS` used for
  a wasm-only linker flag, and a test harness that read a stale tree between calls.

## [X4 — contract SDK] 2026-10-29

**Fifth `EXEC_PLAN` phase.** Deliverable: `patches/0024-X4-sdk.patch`.
A contract is now **Rust**: two new crates (`aura-sdk`, `aura-sdk-macros`), three
reference contracts, ABI-JSON per artifact — and the three X0 rules a compiler
output cannot live without, relaxed under the hosted policy only. No new
external crate (`syn`/`quote`/`proc-macro2` were already in the lock).

- `aura-sdk` — `no_std` guest library: the seven ABI v1 functions as typed Rust,
  a fixed-layout codec (`u8`…`u128`, `i32`/`i64`, `bool`, `[u8; N]`, little
  endian, **length-exact**), typed storage (`Slot<T>`, composed `Key`), events
  with the published limits checked before the call. On the host the same source
  compiles against a mock backend that speaks the node's `extern "C"` contract,
  so the wrappers are tested rather than trusted.
- `aura-sdk-macros` — `#[entrypoint]`, `#[constructor]`, `#[migrate]`: rename the
  function and emit the `extern "C"` export X3 calls (`run`, `init`, `migrate`);
  a wrong arity or a non-`u32` return is a compile error with a real message.
- **`tools/build_contracts.py`** builds `examples/{store,kv,events}` for
  `wasm32-unknown-unknown` (release + LTO + `opt-level=z` + `panic=abort` +
  `strip` + `--max-memory=16MiB`), commits the artifacts, pins their sha256 in
  `tests/fixtures/x4_contracts.txt` and writes `examples/<name>/abi.json`.
- **DoD gate: 3 contracts — 5 187 / 5 290 / 5 033 bytes**, byte-identical across
  two independent target dirs; measured gas pinned by `x4-record`
  (deploy 58 495, call 13 115 for `store`; deploy 62 353, call 8 506, upgrade
  62 261 for `kv`; deploy 51 330, call 12 940 for `events`).
- **Four defects found by the gates and fixed:** the X0 census rejected every
  Rust contract (mutable `__stack_pointer`, `memory.copy`/`memory.fill`,
  `call_indirect` from `core::fmt`) — now allowed under `hosted()` with the
  bounds that matter (one mutable global; only the two deterministic byte ops;
  an element table that may never contain an import); `rustc` memory has no
  declared maximum, so the build passes `--max-memory`; `wasmi`'s `Config` was
  stricter than the census and refused accepted modules; the codec could
  silently truncate a wider value.
- NC: **6/6**; total **135** across C0–C10 + S0–S7 + X0–X4.
- **Honest scope:** wasmi-only hosted execution, no allocator in a contract, no
  serde, `call_indirect` bounded but not verified (X8), no cross-contract calls
  (X6), no `caller` checks (T-phase), and reproducibility is path-scoped and
  measured.

## [X3 — contract lifecycle] 2026-10-28

**Fourth `EXEC_PLAN` phase.** Deliverable: `patches/0023-X3-lifecycle.patch`.
A contract is now a **record in state** instead of a byte string handed to the
executor: it can be deployed, called by address, upgraded and migrated. No new
dependency at all — not external, not workspace.

- `aura_exec::lifecycle` — `CodeRecord`, `Lifecycle::{new, deploy, upgrade,
  call_stored}`, `LifecycleReceipt`, `LifecycleStatus` (`Deployed`, `Upgraded`,
  `Reverted`) and `LifecycleError` (eight typed refusals, each with a stable
  `code()` string).
- **Code record:** `magic(8) "ALCRYCD1" || format || abi || version || len ||
  code`, decoded strictly (bad magic / bad format / bad length / trailing bytes
  are four errors). `Account::code_hash = BLAKE3(code)` — identity of the logic,
  so "is this a contract?" needs no side table.
- **Deploy is atomic with instantiation:** one session stores the code, funds
  the account and runs the optional `init`; a trapping or out-of-gas `init`
  leaves no account, no code and no moved value.
- **Upgrade is optimistic and explicit:** it refuses a stale
  `expected_version`, rewrites only the code record and `code_hash`, and never
  touches balance or storage. Migration is contract logic
  (`migrate(from, to)`), not a chain operation.
- **Code is priced, not capped:** `1 000 + 10 × bytes`, i.e. ≤ ~100 KiB per
  transaction at the Stage 1 budget.
- `ModuleLimits::hosted()` enforces the X0 census at deploy while permitting
  exactly the seven X2 ABI imports; `Engine::entry_arity` resolves
  `init`/`migrate` presence and arity *before* any state is touched.
- **DoD gate: 9 programs, 12 tests**, measured gas `deploy(v1) = 3 560`,
  `call(v1) = 30`, `upgrade(v1→v2) = 4 527`, `call(v2) = 30`,
  `deploy(run_only) = 2 860`.
- **Four defects found and fixed:** the X0 import ban made the X2 ABI
  unloadable (now `allow_abi_imports`, X0 verdicts unchanged); `entry_arity`
  could not go through `prepare` (it requires `run`); a deploy without `init`
  was reported as a trap; `code()` collapsed every refusal into
  `refused:censored` instead of naming the cause.
- Gate: 221 tests, 4 595 vectors across 21 fixtures, 129 negative controls,
  239 external crates, `make audit-closeout` green.
- **Honest scope:** hosted execution is still wasmi-only, `deploy`/`upgrade` are
  explicit but **not permissioned** (no `caller == owner` — signatures are
  T-phase), no code archive, no factory/proxy, no cross-contract calls (X6), no
  SDK (X4), no fee market (T-phase).
- NC: **6/6** (version check removed, balance not preserved, `migrate` never
  runs, code hash not stored, `init` never runs, census skipped at deploy);
  total **129** across C0–C10 + S0–S7 + X0–X3.

## [X2 — gas + ABI v1] 2026-10-28

**Third `EXEC_PLAN` phase.** Deliverable: `patches/0022-X2-gas-abi.patch`.
A contract can now read and write state, hold and move value, emit events — and
pay for all of it. No new external dependency (three *workspace* dependencies).

- `aura_exec::abi` — **ABI v1**: module `env`, version 1, seven functions
  (`storage_read`, `storage_write`, `balance`, `transfer`, `emit`, `hash`,
  `block_ctx`), one dispatcher, closed status codes (`0`, `-1`…`-8`, `-9`
  absent key, `-10` unknown block field, `-11` too many events) and published
  limits (key 32 B, value 4 KiB, topic 64 B, 64 KiB buffer cap).
- `aura_exec::contract` — `HostState`, `Executor::call`, `ContractReceipt`,
  `ExecStatus`. Gas table published per function (base + per byte) and checked
  row by row against an independent Python copy.
- **Gas is one counter**: 1 fuel = 1 gas, and the instruction meter is reset to
  `gas_left` at every host-call boundary. This closes the X1 defect where
  `fuel_used` under-reported a call that ran out of budget at entry.
- **Commit rule:** `Returned` commits a canonical `StateDiff`; `OutOfGas`, a
  trap and a refusal all roll back, leaving the state root byte-identical, the
  events empty and the writes uncommitted. A contract that catches `-8` and
  returns is still out of gas.
- **DoD gate: 23 vectors — 13 returned, 5 out of gas, 3 trapped, 3 refused**
  (`out_of_gas_first_call`, `oog_between_two_calls`, `write_then_spin`,
  `write_then_trap`, `emit_then_trap`, `import_bad_module`,
  `import_unknown_env`, `import_memory`, `no_memory_export` → `-1`, …).
- **Three defects found by the gates and fixed in this patch:** the rollback was
  not load-bearing (the failure receipt used a hard-coded empty event vector, so
  deleting `HostState::rollback` left every test green — NC-4 caught it); a fuel
  trap was reported as a plain trap although 1 fuel = 1 gas; `slot_of` zero-pads
  keys, so `b"a"` and `b"a\0"` collide — now documented and asserted.
- **Honest scope:** hosted execution is **wasmi-only**; the `wasmtime` backend
  returns `EngineUnavailable` for it rather than pretending. No fee market
  (T-phase), no cross-contract calls (X3/X4), no lifecycle (X3), no SDK (X4).
- Gate: `make test` -> **208 passed / 0 failed**; `make nc-x2` -> **7/7 PASS**;
  123 negative controls overall; 4,555 vectors across 20 fixtures; 239 external
  crates (unchanged); `make audit-closeout` green. Report:
  `docs/evidence/x2/report.md`.

## [X1 — sandbox] 2026-10-28

**Second `EXEC_PLAN` phase.** Deliverable: `patches/0021-X1-sandbox.patch`.
`aura-exec` gets its first external dependency: the engine.

- `aura_exec::sandbox`: size check → X0 census → engine compile → zero-import
  check → instantiate with store limits → call `run` → closed trap set. Every
  step can only reject; no step can widen what the census allowed.
- `SandboxLimits::restrictive()`: fuel 1,000,000; memory 4096 pages; module
  512 KiB; stack 512 KiB; trace 4096 events. `validate()` refuses any value
  looser than the published census ceiling.
- `Engine` trait with two backends from one configuration: `wasmi =2.0.0`
  (default: deterministic interpreter, `portable-dispatch`) and
  `wasmtime =50.0.0` (feature `wasmtime-engine`). `Execution { outcome,
  fuel_used, trace, exports }`, `Outcome`, `Event`, `SandboxError` (7),
  `TrapKind` (6).
- **DoD gate: 19 programs — 6 executed, 5 trapped, 8 refused.** `spin` traps
  `trap_out_of_fuel` after exactly the published budget; `sum_to_n(1000)`
  returns 500 500 after 13 246 fuel; WASI clock, `env::random_get`, `f32.add`,
  start section and mutable global are refused by the census before the engine
  sees them; no `run` export → `instantiate`; two-parameter `run` →
  `limit:run_arity`.
- **Fuel is pinned per program** by `cargo run -p aura-exec --bin x1-record`,
  so an engine bump that changes metering turns the gate red.
- **Cross-engine (measured):** `wasmtime` agrees with `wasmi` on the outcome of
  all 19 programs. Fuel is engine-specific; shared gas is X2.
- ⚠️ AUDIT — three defects found by the new gates and fixed here:
  (1) `wasmi` without a dispatch feature overflows the native stack on
  `loop { br 0 }` instead of consuming fuel → workspace pins
  `portable-dispatch` + `deterministic`; (2) the first trap classifier missed
  `wasmtime`'s real messages and reported traps as instantiation errors;
  (3) disabling `reference-types` with the component model on makes `wasmtime`
  reject every module.
- Dependencies: **+144 external crates in the lock** (95 → 239): 10 for the
  `wasmi` subtree, 134 for the optional `wasmtime`/Cranelift subtree. The
  default build compiles the 10; `docs/evidence/x1-dependency-addendum.txt`
  records both. NC: 6 new → **116 total**. Tests: 184 → **193**. Vectors:
  4 415 → **4 532** across 19 fixtures. Report: `docs/evidence/x1/report.md`.

## [X0 — determinism census] 2026-10-28

**First `EXEC_PLAN` phase.** Deliverable:
`patches/0020-X0-determinism-census.patch`. No new crate and, on purpose, no
VM engine.

- `aura-exec` stops being an empty placeholder and lands
  `aura_exec::census`: `CENSUS` (16 published rows: source, feature, policy,
  enforcement layer, why), `ModuleLimits::restrictive()`, `Ban` (18 variants),
  `Verdict`, `CensusReport`, `census(bytes, &limits)`.
- **Static, fail-closed gate.** It parses magic/version, section framing and
  order, imports, tables, memories, globals and **every instruction of every
  function body** against a whitelist. An opcode nobody reviewed is rejected as
  `UnknownOpcode`: the scanner's risk is false rejection, never false
  acceptance.
- **Restrictive configuration:** module <= 512 KiB; linear memory must declare
  a maximum <= 4096 pages (256 MiB); tables must declare a maximum <= 1024
  entries; zero imports; no start function; no mutable globals; no floats; no
  SIMD/atomics/bulk memory; no indirect calls.
- **DoD gate: 21 programs, each banned or deterministic** — 5 allowed (empty,
  i32 arithmetic, i64 memory round-trip, locals + direct calls,
  blocks/loops/branches) and 16 banned with an exact reason (f32/f64
  arithmetic, `f64.const` NaN, `f64` conversion, WASI `clock_time_get`,
  `env::random_get`, shared memory, `memory.atomic.wait`, v128 SIMD, bulk
  `memory.copy`, `call_indirect`, start section, mutable global, memory without
  a maximum, memory above the cap, unlisted opcode). Every program is built by
  an **independent Python assembler** (`tools/gen_x0_vectors.py`), never by the
  crate under test.
- Golden vectors: 72 fixture lines (21 programs x hex/verdict/size plus the
  published limits). Offline and byte-stable.
- ⚠️ AUDIT (found by the gate): `f32.const`/`f64.const` (0x43/0x44) were not
  classified as float instructions, so their immediates were read as opcodes
  and the programs came out as `unknown_opcode`. The ban is now explicit and
  the whole float policy lives in exactly one function (`float_ban`), which is
  what NC-2 tampers with.
- Gate: `make test` -> **184 passed / 0 failed** (177 S7 + 3 unit + 4 gate),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate 95
  external crates, vector inventory **4 415** across 18 fixtures,
  `make audit-closeout` green. 6 X0 negative controls, total **110** across
  C0–C10 + S0–S7 + X0.
- Evidence: `docs/evidence/x0/gates.txt`, `nc.log`, `report.md` (census table +
  21-program verdict table), `inventory.txt`, `SHA256SUMS`,
  `docs/evidence/x0-dependency-addendum.txt`.
- **Honest scope:** X0 executes nothing — no fuel, no memory isolation, no
  engine-level determinism claim. `wasmtime` is pinned in X1, because fuel
  semantics are version sensitive and must be a protocol decision.
  Validators / blocks / tx remain **0 / 0 / 0**.

## [S7 — recovery and backup] 2026-10-28

**Eighth and final `STATE_PLAN` phase — STATE_PLAN is now COMPLETE.**
Deliverable: `patches/0019-S7-recovery-backup.patch`. No new crate, no new
external dependency; one new binary target (`s7-chaos`).

- Landed `aura-storage::backup`: `BackupManifest`, `BackupBundle`,
  `create_backup`, `restore_backup`, `RestoreReport`, `BackupError`, and
  `Storage::wal_records()`.
- **Bundle:** `u32le(manifest_len) || manifest_bytes || blob`;
  `manifest_bytes = payload || BLAKE3(S7_MANIFEST_CONTEXT || payload)` with
  `payload = ALM7 || u32 version || chain_id || head_version || head_root ||
  record_count || blob_len || blob_sha256`; `blob` = the S2 framed,
  BLAKE3-checksummed WAL records 1..=head; `blob_sha256 = SHA-256(blob)`.
- **Restore replays through the real commit path**, so declared roots are
  recomputed rather than trusted: a perfectly checksummed manifest that lies
  about a root fails with `RootMismatch`. Restore also rejects a non-empty
  target, a wrong chain-id and a broken parent chain.
- **DoD gate: 50 `kill -9` + restore cycles — 0 root mismatches.** Commit
  counts 1..=12; 17 cycles crash inside the WAL/head window. Each cycle
  reopens, compares head/root with the independent Python fixture, backs up,
  restores into a fresh directory, compares the whole WAL record-for-record,
  then commits the next batch in both stores and requires the same receipt.
  338 records replayed.
- Golden vectors: `tools/gen_s7_vectors.py` — reuses the independent S2
  reference and adds the S7 bundle; 28 fixture lines including the canonical
  1 261-byte bundle hex, manifest hex and blob SHA-256. Offline, byte-stable.
- Gate: `make test` -> **177 passed / 0 failed** (169 S6 + 7 S7 + 1 unit),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate 95
  external crates, vector inventory **4 349** across 17 fixtures,
  `make audit-closeout` green. 6 S7 negative controls, total **104** across
  C0–C10 + S0–S7.
- Evidence: `docs/evidence/s7/gates.txt`, `nc.log`, `report.md`,
  `inventory.txt`, `SHA256SUMS`,
  `docs/evidence/s7-dependency-addendum.txt`.
- ⚠️ AUDIT: closing `STATE_PLAN` broke the assumption behind C10 NC-2, which
  tampered with "the first not-yet-done S phase". `tools/nc_c10.py` now
  exercises the same rule from the other side once every S phase is shipped
  (remove the newest `### Sx receipt` heading) and passes 6/6 again. The
  frozen `docs/evidence/c10/nc.log` is **not** rewritten — it remains the
  historical record of the C10 run and its SHA256SUMS still verify.
- **STATE_PLAN closure criterion met and measured:** recover from zero to the
  current `state_root`, prove any balance with a Merkle proof, survive a
  sudden `kill -9`. Next plan in dependency order: `EXEC_PLAN` X0.
- Deferred: distributed/power-loss chaos, incremental and remote backups, key
  management (OPS_PLAN), VM (X1), `cargo deny` (H1).
  Validators / blocks / tx remain **0 / 0 / 0**.

## [S6 — pruning + archive] 2026-10-28

**Seventh `STATE_PLAN` phase.** Deliverable:
`patches/0018-S6-pruning-archive.patch`. No new crate, no new external
dependency.

- **Measured problem first.** S1 `MerkleTree` rebuilds the entire node arena
  on every `commit()`, measured at **16.4 ms per commit** with 1 024 leaves
  (5 000 commits timed) — ~27 minutes for the S6 DoD of 100 000 blocks. The
  phase therefore starts with a measurement, not with code.
- Landed `aura-state::HistoryTree`: a compressed sparse Merkle tree in a
  mutable arena, updating only the changed path (~12 hashes per write).
  Roots are bit-for-bit S1 and are verified against `MerkleTree` rebuilds at
  every checkpoint plus an independent Python rebuild.
- Landed `aura-storage::prune`: `RetentionMode`, `RetentionPolicy`
  (`prune(keep)` / `archive()`), `VersionRecord`, `RetentionReceipt`,
  `CompactionReport`, `RetentionStore` (`commit`, `root_at`, `state_at`,
  `proof_at`, `verify_root_at`, `compact`), exact byte accounting and
  `PruneError`.
- **Wire formats:** version records and base snapshots are canonically
  encoded and domain-separated —
  `record bytes = payload || BLAKE3(S6_RECORD_CONTEXT || payload)`,
  `base bytes = payload || BLAKE3(S6_BASE_CONTEXT || payload)`. Strict
  decode enforces checksum, magic, caps, no trailing bytes and
  byte-identical re-encoding.
- **Retention rule:** `base_version = max(base_version, head - keep_versions)`
  after every commit. Prune mode keeps a materialised base plus the last
  `keep_versions` deltas; older versions return `PruneError::Pruned`, never a
  plausible wrong state. Archive mode never prunes and refuses to compact.
- **DoD gate:** 100 000 blocks, 4 writes per block, 1 024 live keys.
  Archive: **36 500 045 bytes** retained and every historical root/proof
  served. Prune (`keep_versions = 128`): peak **113 326 bytes**, under the
  analytic bound `base_bytes + keep * max_record_bytes` —
  **322.1x smaller** than the archive. Compaction folds retained deltas into
  the base without moving the root.
- Golden vectors: `tools/gen_s6_vectors.py` — pure-Python BLAKE3, C7 key
  digest, S2 key path, a from-scratch S1 rebuild, full 100 000-block byte
  accounting and canonical hex for two records and one base snapshot.
  35 fixture lines, offline and byte-stable.
- Build-profile decision (measured, no version changes):
  `[profile.dev.package.blake3]` and `[profile.dev.package.sha2]` are
  `opt-level = 3`; `blake3_hash(65 B)` 5.785 us -> 0.178 us in a debug build.
- Gate: `make test` -> **169 passed / 0 failed** (154 S5 + 9 S6 storage +
  6 S6 tree), fmt/clippy clean, drift guard 12 plans / 112 rows, dependency
  gate 95 external crates, vector inventory **4 326** across 16 fixtures,
  `make audit-closeout` green. 6 S6 negative controls, total **98** across
  C0–C10 + S0–S6.
- Evidence: `docs/evidence/s6/gates.txt`, `nc.log`, `report.md`,
  `inventory.txt`, `SHA256SUMS`,
  `docs/evidence/s6-dependency-addendum.txt`.
- ⚠️ AUDIT: two tree defects were found by the cross-check gates and fixed in
  the code, not papered over — a branch prefix derived from the parent instead
  of the child (sibling slots collided, so later writes navigated into the
  wrong subtree) and a `split` that returned the deepest branch hash instead
  of the root. Both are now covered by the S1 rebuild comparison.
- Deferred: backup/restore + chaos gate (S7), VM (X1), `cargo deny` (H1).
  Validators / blocks / tx remain **0 / 0 / 0**.

## [S5 — state sync] 2026-10-27

**Sixth `STATE_PLAN` phase.** Deliverable:
`patches/0017-S5-state-sync.patch`. No new crate, no new external dependency.

- Landed `aura-state::sync`: `SyncHead`, `SyncDelta`, `SyncPackage`,
  `SyncResult`, `SyncError`, and strict canonical decoders for heads,
  deltas and packages.
- **Protocol:** a fresh node downloads ONE package — a quorum of three
  independent heads, an S4 snapshot and a compact delta of net changes —
  instead of the full event history. `HEAD_MAGIC`/`DELTA_MAGIC`/
  `PACKAGE_MAGIC` plus domain-separated SHA-256 checksums on every blob.
  Verification chain: package -> 3 head checksums -> unanimous quorum ->
  S4 manifest/chunk digests -> `chunk_root` binding -> apply sorted delta
  -> recompute S1 `state_root` -> compare with quorum. Substituted diff
  chunk rejected (NC-6). Wrong chain-id, wrong height, bad magic,
  checksum, truncation and trailing bytes all rejected.
- **DoD gate:** 256-key state, snapshot height 90 -> target height 100;
  package **16,992 bytes** vs full history **998,400 bytes**
  (**58.8x smaller**). New node reaches the same `state_root` as 3 old
  nodes within the measured 60 s budget. NC-6 disables the final root
  comparison and requires the substituted-diff rejection test to go red.
- Golden vectors: `tools/gen_s5_vectors.py` — pure-Python BLAKE3, hashlib
  SHA-256, C7 state-key digest, S1 sparse Merkle tree and S4 snapshot
  format. 48 fixture lines, 11 rejection vectors. Offline and byte-stable.
- Gate: `make test` -> **154 passed / 0 failed** (146 S4 + 8 S5), fmt/clippy
  clean, drift guard 12 plans / 112 rows, dependency gate 95 external
  crates, `make audit-closeout` green. 6 S5 negative controls, total 92
  across C0–C10 + S0–S5.
- Evidence: `docs/evidence/s5/gates.txt`, `nc.log`, `report.md`,
  `inventory.txt`, `SHA256SUMS`. The S5 format report lives in
  `docs/evidence/s5/report.md`, not `docs/spec/`, because
  `docs/spec/FORMAT_INDEX.md` is frozen checksummed C10 evidence.
- Tooling / CI: `make nc-all` now covers 17 NC drivers (92 NC); new targets
  `make gen-s5-vectors`, `make nc-s5`, `make evidence-s5`,
  `make record-s5-deps`.
- **Not claimed:** pruning/archive (S6), backup/chaos (S7), real VM
  (X1), `cargo deny` (H1). Validators/blocks/tx remain **0/0/0**.

## [S4 — deterministic state snapshots] 2026-10-27

**Fifth `STATE_PLAN` phase.** Deliverable:
`patches/0016-S4-snapshots.patch`. No new crate and no new external
dependency: `sha2 = 0.10.9` was already in `Cargo.lock` via `k256` and in
the frozen C3 baseline; S4 only makes it a DIRECT dependency of
`aura-state`. Recorded in `docs/evidence/s4-dependency-addendum.txt`.

- Landed `aura-state::snapshot`: `SnapshotManifest`, `ChunkDescriptor`,
  `Snapshot`, `SnapshotError`, `export_snapshot`, `import_snapshot`,
  `encode_chunk`, `decode_chunk`, `chunk_hash`, `chunk_root`,
  `state_root`, `sha256_bytes`, `parallel_chunk_hashes`
  and `manifest_bytes`.
- **Format:** `manifest_bytes || data_blob`. The blob is a concatenation
  of canonical chunk payloads; each chunk has its own
  `sha256(SNAPSHOT_CHUNK_CONTEXT || payload)`. The manifest carries
  descriptors `(offset, length, sha256)`, the S1 `state_root`, a BLAKE3
  `chunk_root` over the ordered digests and a SHA-256 checksum of the
  payload. Strict decode checks magic, version, minimal LEB128, contiguous
  offsets, caps, no trailing bytes, byte-identical re-encode and a
  recomputed `chunk_root`.
- **Parallel export/import.** `std::thread::scope` with an atomic work
  index and a bounded worker pool (1..8); deterministic descriptor order.
  Verified identical digests and `chunk_root` for 1/2/4/8 workers.
- **DoD gate:** 1,000,000 keys exported to 16 chunks (31,000,048 data
  bytes) and reimported **byte-identically**. Data SHA-256 and chunk_root
  match the independent Python reference, which computes the digest without
  writing 30 MB of hex into the fixture. NC-6 disables the digest
  comparison and requires the substituted-chunk rejection test to go red.
- Golden vectors: `tools/gen_s4_vectors.py` — hashlib SHA-256, pure-Python
  BLAKE3 (self-checked against 7 official vectors), the C7 state-key digest
  and the S1 sparse Merkle tree. 7-key canonical snapshot (3 chunks),
  1024-key parallel reference (8 chunks), 1M-key digest, 11 rejection
  vectors; 58 fixture lines. Offline and byte-stable.
- Gate: `make test` → **146 passed / 0 failed** (138 S3 + 8 S4),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate 95
  external crates, `make audit-closeout` green. 6 S4 negative controls,
  total 86 across C0–C10 + S0–S4.
- Evidence: `docs/evidence/s4/gates.txt`, `nc.log`, `report.md`,
  `inventory.txt`, `SHA256SUMS`. The S4 format report lives in
  `docs/evidence/s4/report.md`, not `docs/spec/`, because
  `docs/spec/FORMAT_INDEX.md` is frozen checksummed C10 evidence.
- Tooling / CI: `make nc-all` now covers 16 NC drivers (86 NC); new
  targets `make gen-s4-vectors`, `make nc-s4`, `make evidence-s4`,
  `make record-s4-deps`.
- **Not claimed:** state sync (S5), pruning (S6), backup/chaos (S7),
  real VM (X1), `cargo deny` (H1). Validators/blocks/tx remain **0/0/0**.

## [S3 — execution overlay] 2026-10-27

**Fourth `STATE_PLAN` phase.** Deliverable:
`patches/0015-S3-execution-overlay.patch`. No new crate and no new
external dependency — the overlay lives in `aura-state`, which already owns
`MerkleTree`, `Account` and `StateKey`.

- Landed: `StateView` (copy-on-write overlay over an immutable S1 tree),
  `StateDiff` (canonical wire format + strict decoder), `events_root()`,
  `execute()` and `apply_diff()`. Reads fall through to the base tree;
  writes/deletes are keyed by the C7 state-key digest in a `BTreeMap`.
- **Wire format:** `payload || BLAKE3(S3_DIFF_CONTEXT || payload)` where
  `payload = ALD3 || version || base_root || result_root || gas_used ||
  events || changes`. Strict decode enforces the checksum, magic, minimal
  LEB128 varints, strictly ascending change paths, caps, no trailing bytes and
  byte-identical re-encoding. `apply()` re-verifies base and result roots
  before materialising a new tree.

- **DoD gate:** `revert` and `OutOfGas` both leave `base_root` unchanged and
  the touched account balance byte-identical; `execute()` rolls back all
  changes, events and gas. NC-6 removes `rollback()` and requires the revert
  gate to go red (balance changes) — verified. 6 S3 negative controls,
  total 80 across C0–C10 + S0–S3.
- Golden vectors: `tools/gen_s3_vectors.py` — pure-Python BLAKE3
  (self-checked against 7 official vectors), the C7 state-key digest and the
  S1 sparse Merkle tree. 3 canonical diffs, 51 fixture lines, 10 rejection
  vectors. Offline and byte-stable.

- Gate: `make test` -> **138 passed / 0 failed** (132 S2 + 6 S3), fmt/clippy
  clean, drift guard 12 plans / 112 rows, dependency gate 95 external crates,
  `make audit-closeout` green.
- Evidence: `docs/evidence/s3/gates.txt`, `nc.log`, `report.md`,
  `inventory.txt`, `SHA256SUMS`. The S3 spec/report lives in
  `docs/evidence/s3/report.md` instead of `docs/spec/` because
  `docs/spec/FORMAT_INDEX.md` is frozen, checksummed C10 evidence and cannot
  gain a new indexed item without breaking the C10 checksum and the
  close-out audit. This mirrors the structure used in S1 and S2.
- Tooling / CI: `make nc-all` now covers 15 NC drivers (80 NC); new targets
  `make gen-s3-vectors`, `make nc-s3`, `make evidence-s3`.

- **AUDIT (docs correctness).** `ARCHITECTURE.md` section 3 claimed
  `aura-state <- aura-block, aura-tx, aura-codec, aura-crypto, aura-storage`,
  which is false and would be a cycle: `aura-storage` depends on
  `aura-state`. The graph now reflects the real `Cargo.toml` edges:
  `aura-state <- aura-types, aura-codec, aura-crypto`; `aura-storage <-
  aura-types, aura-codec, aura-crypto, aura-state`. No code change.

- **Not claimed:** real VM/WASM (X1), snapshots (S4), state sync (S5),
  pruning (S6), backup/chaos (S7), `cargo deny` (H1). Validators/blocks/tx
  remain **0/0/0**.

## [S2 — RocksDB + WAL durable state] 2026-10-26

**Third `STATE_PLAN` phase.** Deliverable:
`patches/0014-S2-rocksdb-wal.patch`. `aura-storage` gains a real API (it was
an honest placeholder since C0). The first OPTIONAL native dependency enters
the workspace: `rocksdb = 0.23.0` + `librocksdb-sys = 0.17.1+9.9.3`, exact
pinned. Default builds need **no** native library.

- **Architecture.** A `StateBackend` trait with two implementations. The
  default is a pure-Rust WAL + `BTreeMap` backend so `make test` is
  reproducible in any CI. The `rocksdb` Cargo feature enables the real
  RocksDB backend with four column families (`state`, `nonce`, `meta`,
  `wal_index`) and ONE atomic `sync = true` `WriteBatch` per commit.
- **WAL-before-head protocol.** A framed record
  `varint(payload_len) || payload || BLAKE3(S2_WAL_CONTEXT || payload)`,
  payload = magic `ALW1`, version, parent root, state root, and ordered
  changes with per-key nonces. The record is appended and `fsync`ed BEFORE
  head publication. A checksummed `head.bin` is written atomically
  (tmp + rename + directory fsync). `Storage::open` verifies every WAL
  checksum and the version/parent-root chain, then replays records newer
  than the published head. A head without a backing WAL record is refused.
- **State root** = `BLAKE3(S2_ROOT_CONTEXT || version || tree_root)` where
  `tree_root` is the S1 sparse Merkle root over
  `BLAKE3(S2_KEY_CONTEXT || len || key)` paths.
- **DoD gates:** `kill -9` at 10 commit points AND at 10 WAL/head
  crash-window points → after restart head and root match the independent
  golden roots. Clean reopen, recovery replay, missing-record detection and
  8 independent rejection vectors (bad checksum, bad magic, truncated frame,
  trailing bytes, non-sequential version, wrong parent root, zeroed root,
  mutated value). Both backends tested.
- **Golden vectors:** `tools/gen_s2_vectors.py` — pure-Python BLAKE3
  (self-checked against 7 official vectors), the S1 sparse Merkle tree and
  the S2 WAL framing. 11 golden commits, 110 fixture lines; offline and
  byte-stable. Vector inventory rises to **4,164** across 12 fixtures.
- **Native dependency governance.** 24 new external crates (rocksdb,
  librocksdb-sys, bindgen, clang-sys, …) recorded in
  `docs/evidence/s2-dependency-addendum.txt`; `tools/check_deps.py` now
  reads addenda from `docs/evidence/` as well as `docs/spec/` because the
  C10 `FORMAT_INDEX.md` is frozen checksummed evidence and must not gain
  new files. `tools/record_s2_deps.py` is the reviewed re-record action.
  ⚠️ AUDIT: Ubuntu 24.04 ships librocksdb 8.9.1, whose C API is
  incompatible with `rocksdb 0.23` (missing `rocksdb_writebatch_create_with_params`,
  `rocksdb_get_db_identity`, …). CI therefore runs the native backend in a
  dedicated **Debian trixie** job (librocksdb 9.10). `tools/s2_native_env.sh`
  detects librocksdb/libclang and fails loudly rather than silently building
  the bundled C++ library (which needs GB of RAM). `make test-s2-native`
  runs native clippy + the S2 store tests against real RocksDB.

- **Gate:** `make test` → **132 passed / 0 failed** (123 S1 + 9 S2),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate 95
  external crates, `make audit-closeout` green.
- **Negative controls:** `tools/nc_s2.py` → `make nc-s2` →
  `docs/evidence/s2/nc.log`, **6/6 PASS**: WAL context, key context, root
  context, independent golden WAL record, checksum verification and the
  WAL-before-head invariant. Total C0–C10 + S0–S2: **74 NC**.
- **Evidence:** `docs/evidence/s2/gates.txt`, `nc.log`, `report.md`,
  `inventory.txt`, `native.txt`, `SHA256SUMS`.
- **Tooling / CI:** `make nc-all` now covers 14 NC drivers (74 NC); new
  targets `make gen-s2-vectors`, `make nc-s2`, `make evidence-s2`,
  `make record-s2-deps`, `make test-s2-native`.
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (S2 block), `CHANGELOG.md`, `ROADMAP.md` (ledger,
  §4, §8 → next S3), `CONTRIBUTING.md`, `STATE_PLAN.md` (S2 ✅ + receipt).
- **Not claimed:** execution overlay / rollback (S3), chunked snapshots
  (S4), state sync (S5), pruning (S6), backup/chaos (S7), VM (X1),
  consensus/P2P/node/RPC, `cargo deny` (H1). Validators/blocks/tx remain
  **0/0/0**.

## [S1 — versioned Merkle tree] 2026-10-26

**Second `STATE_PLAN` phase.** Deliverable:
`patches/0013-S1-merkle-tree.patch`. No new crate, no new external
dependency — `aura-state::tree` is a new module.

- **Tree:** deterministic sparse binary Merkle tree over the 256 bits of
  `StateKey::digest` (C7), MSB first, with single-leaf subtree compression
  and depth-dependent `empty_hash(depth)`. `leaf_hash`, `node_hash`,
  `versioned_root`; `state_root = BLAKE3(S1_ROOT_CONTEXT ‖ version ‖
  tree_root)` binds the root to a block height. `MerkleTree` supports
  insert/remove/get, `commit()`, `root_at()`, `prove`, `prove_at`, and
  `prove_many_at` (one node arena for many proofs).
- **Proofs:** `MerkleProof` carries the version, path, value, leaf depth and
  bottom-up sibling hashes; `verify_proof` reconstructs the versioned root,
  and `verify_inclusion` also checks key/value. A proof cannot be replayed
  against a different height.
- **DoD gates:** 10,000 deterministic operations with 12 versioned
  checkpoint roots matching an independent Python reference; 1,000
  inclusion proofs byte-compared and verified. Tampered sibling, value, key
  and truncated siblings are rejected. The S0 insertion-order defect is
  fixed: order no longer changes the root; delete changes it and re-add
  restores it.
- **Golden vectors:** `tools/gen_s1_vectors.py` — pure-Python BLAKE3
  (self-checked against 7 official vectors), the C7 state-key digest and an
  independent sparse-tree implementation. 3,066 fixture lines; offline and
  byte-stable. Vector inventory rises to **4,061** across 11 fixtures.
- **Defects found and fixed by the gate:** proofs initially reconstructed
  the raw tree root while the published commitment is `versioned_root(...)`;
  the proof now carries the version. Rebuilding the node arena for each of
  1,000 proofs took 42 s; `prove_many_at` builds it once and the S1 suite
  runs in ~1 s in debug mode. Fixed in code, not suppressed.
- **Gate:** `make test` → **123 passed / 0 failed** (117 CORE+S0 + 6 S1),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate 71
  external crates, `make audit-closeout` green.
- **Negative controls:** `tools/nc_s1.py` → `make nc-s1` →
  `docs/evidence/s1/nc.log`, **6/6 PASS**: leaf context, node context,
  independent checkpoint root, proof root comparison, MSB-first path bit
  ordering, depth-independent empty subtrees. Total C0–C10 + S0 + S1:
  **68 NC**.
- **Audit update:** `tools/audit_closeout.py` now covers 13 phases, expects
  68 NCs, treats `s0/inventory.txt` as a historical 62-NC snapshot and
  `closeout.txt` as a historical 56-NC CORE snapshot. Frozen C10 artifacts
  (`FORMAT_INDEX.md`, `CLOSEOUT_AUDIT.md`) remain untouched; the S1 report
  lives in `docs/evidence/s1/`.
- **Tooling / CI:** `make nc-all` covers 13 NC drivers (68 NC); CI adds
  `make gen-s1-vectors` and regenerates only `make evidence-s1`. New
  targets: `make gen-s1-vectors`, `make nc-s1`, `make evidence-s1`.
- **Evidence:** `docs/evidence/s1/gates.txt`, `nc.log`, `report.md`,
  `inventory.txt`, `SHA256SUMS`.
- **Rig maintenance:** `tools/nc_c10.py` NC-2 hard-coded `S1` as its
  "deferred phase" target; once S1 shipped, `make nc-all` went red with
  "tamper target not found". It now picks the first not-yet-done S phase
  dynamically with a regex, so the historical C10 NC keeps working as
  `STATE_PLAN` advances. Found by `make nc-all` during this phase.
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (S1 block), `CHANGELOG.md`, `ROADMAP.md` (ledger,
  §4, §8 → next S2), `CONTRIBUTING.md`, `STATE_PLAN.md` (S1 ✅, decision
  D-S1a + receipt).
- **Not claimed:** persistence / WAL / crash recovery (S2), execution overlay
  (S3), snapshots / state sync / pruning / backup (S4–S7), VM (X1),
  consensus/P2P/node/RPC, `cargo deny` (H1). Validators/blocks/tx remain
  **0/0/0**.

## [S0 — state baseline] 2026-10-26

**First `STATE_PLAN` phase.** Deliverable:
`patches/0012-S0-state-baseline.patch`. No new crate, no new external
dependency — the baseline is a module in the existing `aura-state` crate.

- **This is a measurement, not a design.** `aura-state::baseline` provides
  `BaselineState` (append-only `Vec<BaselineWrite>` with last-write-wins
  lookup), `BaselineWrite`, `BaselineError`, `BaselineReport` and
  `FINAL_VIEW_CONTEXT`. `state_root()` returns `Err(NoRoot)`, `persist()` and
  `load()` return `Err(NoPersistence)`, and `report().deterministic` is
  `false`. The module is documented as something S1 must replace, never as a
  state tree to build consensus on.
- **Measured defects:** `docs/evidence/s0/baseline-report.md` records no
  state root, no persistence, and two "nodes" applying the same writes in
  opposite order ending with different balances (scenario 1: 300 vs 100
  aalcry). A disjoint-key scenario is order-independent, proving the defect
  is last-write-wins on conflicting keys rather than a false positive.
- **Independent golden vectors:** `tools/gen_s0_vectors.py` — a Python model
  of the write log and final-view serialization. 57 fixture lines for 4
  scenarios (including `u64`/`u128` boundary values); self-checks that
  conflicting writes diverge and disjoint writes do not. Offline and
  byte-stable. Vector inventory rises to **1006** across 10 fixtures.
- **Gate:** `make test` → **117 passed / 0 failed** (109 CORE + 8 S0),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate 71
  external crates, `make audit-closeout` green.
- **Negative controls:** `tools/nc_s0.py` → `make nc-s0` →
  `docs/evidence/s0/nc.log`, **6/6 PASS**: final-view context, golden final
  view, fake state root, fake persistence, fake determinism, falsified
  no-root measurement. Total C0–C10 + S0: **62 NC**.
- **Audit update:** `tools/audit_closeout.py` now accepts ✅ rows in
  `STATE_PLAN.md` (plan ownership keyed by phase prefix), requires S0's
  receipt/driver/log/patch/evidence and expects 62 NCs. The frozen CORE
  `docs/evidence/c10/closeout.txt` still reports its own 56.
  `docs/spec/FORMAT_INDEX.md` and `docs/CLOSEOUT_AUDIT.md` are checksummed
  C10 artifacts and were deliberately NOT modified; S0's report lives in
  `docs/evidence/s0/` instead.
- **Tooling / CI:** `make nc-all` now covers C0–C10 + S0 (62 NC) via an
  explicit phase list; CI regenerates only `make evidence-s0`. New targets:
  `make gen-s0-vectors`, `make nc-s0`, `make evidence-s0`.
- **Evidence:** `docs/evidence/s0/gates.txt`, `nc.log`,
  `baseline-report.md`, `inventory.txt`, `SHA256SUMS`.
- **⚠️ AUDIT — remote CI #1 failure (fixed in this patch).** GitHub Actions
  run `40379115764` failed only at step 12 (`make evidence-c10`, exit 2);
  steps 1–11 — `make test`, `make test-core`, 60 s fuzz, close-out audit and
  all 56 NCs — passed. Root cause: `actions/checkout@v4` defaults to a
  shallow clone while `tools/write_c10_evidence.py` called `git log HEAD~1`.
  Fixed with a `|| git log -1 --oneline` fallback, `fetch-depth: 0` in CI,
  and a new `tools/check_rig.py` rule that rejects any shallow-unsafe
  history lookup in `write_*_evidence.py`. Historical C10 evidence files
  were NOT rewritten (their checksums stay valid).
- **⚠️ AUDIT — evidence regeneration was not idempotent (fixed).** Re-running
  `make evidence-s0` (or `make evidence-c10`) rewrote that phase's own
  `nc.log` while its previous `SHA256SUMS` was still present, so the audit
  and NC restore checks saw a transient checksum mismatch and failed
  mid-run. Both evidence writers now clear their own phase's receipts before
  regenerating and re-checksum everything at the end. `make evidence-s0`
  is now repeatable and all commands in `gates.txt` report `[rc=0]`.
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (S0 block), `CHANGELOG.md`, `ROADMAP.md` (ledger,
  §4, §8 → next S1), `CONTRIBUTING.md`, `STATE_PLAN.md` (status, S0 ✅,
  decision D-S0 + receipt).
- **Not claimed:** real `state_root` / Merkle tree / proofs (S1), persistence
  and WAL (S2), execution overlay (S3), VM (X1), consensus/P2P/node/RPC,
  `cargo deny` (H1). Validators/blocks/tx remain **0/0/0**.

## [C10 — close-out] 2026-10-26

**Eleventh code phase and closure of `CORE_PLAN`.** Deliverable:
`patches/0011-C10-close-out.patch`. No new crate, no new external dependency.

- **Plan↔tree audit:** `tools/audit_closeout.py` runs inside `make test` and
  CI. For **every** ✅ phase row across all 12 plans it requires a numbered
  patch, a `### Cx receipt` in `CORE_PLAN.md`, a `## [Cx` CHANGELOG entry,
  `tools/nc_cX.py`, a valid `docs/evidence/cX/nc.log` and evidence receipts;
  it verifies all evidence SHA256SUMS, the **56-NC** total, that every
  `docs/spec/` file is indexed, and that no deferred (non-CORE) phase is
  marked done. The C10 patch itself is cut at commit time, so the audit
  validates the ten shipped patches and reports C10 once present
  (`--strict` requires it).
- **Format index:** `docs/spec/FORMAT_INDEX.md` indexes the six byte-level
  specs (tx, block, proof, account, genesis, test rig), the genesis config,
  the frozen C3 dependency baseline and the C8/C9 addenda, plus a
  phase→patch→NC→evidence ledger.
- **Maturity audit:** `docs/CLOSEOUT_AUDIT.md` — what is real (table with
  evidence), measured numbers, what is explicitly not real (no network, no
  state tree, no VM, no audit, no mainnet), six known weaknesses and the
  verdict **L1 — core libraries + test rig, no network**. M0 is not closed:
  it still needs `STATE_PLAN` S0–S2 and `EXEC_PLAN` X0–X2.
- **NC inventory:** `docs/evidence/c10/closeout.txt` is generated from the
  11 committed NC logs and lists all **56** controls with their verdicts.
- **Gate:** `make test` → **109 passed / 0 failed**, fmt/clippy clean, drift
  guard 12 plans / 112 phase rows, dependency gate 71 external crates,
  vector inventory 956, `make audit-closeout` green.
- **Negative controls:** `tools/nc_c10.py` → `make nc-c10` →
  `docs/evidence/c10/nc.log`, **6/6 PASS**: deleted C7 receipt, `S1` marked
  ✅ in a deferred plan, forged C9 NC result, unindexed format spec, forged
  patch subject, corrupted evidence checksum. All restore historical
  evidence byte-for-byte. Total C0–C10: **56 NC**.
- **Tooling / CI:** `make nc-all` covers C0–C10 (56 NC) without rewriting
  historical logs; CI adds a `close-out audit` step and regenerates only
  `make evidence-c10`. New targets: `make audit-closeout`, `make nc-c10`,
  `make evidence-c10`.
- **Evidence:** `docs/evidence/c10/gates.txt`, `docs/evidence/c10/nc.log`,
  `docs/evidence/c10/closeout.txt`, `docs/evidence/c10/SHA256SUMS` (also
  pins `FORMAT_INDEX.md` and `CLOSEOUT_AUDIT.md`).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md` (maturity L1),
  `docs/ARCHITECTURE.md` (C10 block), `CHANGELOG.md`, `ROADMAP.md` (ledger
  CORE ✅ COMPLETE, §4, §8 → next S0), `CONTRIBUTING.md`, `CORE_PLAN.md`
  (status ✅ COMPLETE, C10 ✅, decision D-C12 + receipt, corrected closure
  criteria), plus `docs/CLOSEOUT_AUDIT.md` and `docs/spec/FORMAT_INDEX.md`.
- **Not claimed:** state tree / `state_root` (S1), VM (X1), consensus/P2P/
  node/RPC, Miri/ASan/UBSan or 24 h fuzzing (H3), state-root determinism
  (X7), external audit / white paper / mainnet (⏸ Stage 2), `cargo deny`
  (H1). Validators/blocks/tx remain **0/0/0**.

## [C9 — test rig] 2026-10-26

**Tenth code phase.** Deliverable: `patches/0010-C9-test-rig.patch`.
Adds the **15th crate**, `aura-testkit`, and `proptest =1.12.0` as a **dev
dependency** (`default-features = false, features = ["std"]`); +13
transitive dev-only crates recorded in a reviewed C9 addendum.

- **Golden vectors:** `tools/gen_c9_vectors.py` — independent fixed-seed
  Python LCG with a C2 minimal-LEB128 transcription. 512 new vectors in
  `c9_vectors.txt`: 128 valid varints (boundaries + pseudo-random), 128
  non-minimal/overflow varints, 128 trailing-byte encodings, 128 distinct
  truncated encodings. Generator self-checks every case before writing;
  offline and byte-stable.
- **Vector inventory:** `tools/count_vectors.py` counts vectors across all 9
  fixtures with an explicit constant/vector classification → **956 vectors**
  (>= 200 required by the C9 DoD). It runs inside `make test`.
- **Wiring check:** `tools/check_rig.py` runs inside `make test` and fails if
  `test-core`, `fuzz-smoke`, the vector inventory, `proptest`, `nc-all` or a
  libFuzzer target disappears from the Makefile/CI. A rig that is never run
  protects nothing.
- **Property tests:** 5 `proptest` properties (256 cases each) — varint
  minimality/round-trip, trailing-byte rejection, `CanonicalRecord`
  round-trip, decoder totality on arbitrary bytes, C7 key-digest purity.
  Plus 3 harness tests and 5 vector tests (13 new tests total).
- **Portable fuzz harness:** `fuzz_one(target, bytes)` feeds arbitrary input
  to six decoders (`codec`, `tx`, `state-key`, `genesis-json`, `block`,
  `merkle-proof`) under `catch_unwind`; `run_portable_fuzz` uses a fixed
  xorshift64* PRNG with separate `work`/`iterations` counters. `aura-fuzz`
  CLI: `--seconds` is the TOTAL budget split across targets. `make
  fuzz-smoke` runs the measured **60 s campaign with 0 crashes**;
  `make test-core` uses a short iteration-bounded pass.
- **libFuzzer targets:** `fuzz/` (separate workspace, `exclude = ["fuzz"]`)
  with `fuzz_codec`, `fuzz_tx`, `fuzz_state_key`, `fuzz_genesis_json`
  calling the same `fuzz_one`. `libfuzzer-sys` never enters the production
  `Cargo.lock`. These targets were **not executed** in this sandbox (stable
  toolchain, no cargo-fuzz/nightly); H3 runs the 24-hour campaign. Stated,
  not claimed.
- **Real gates replace the C0 stubs:** `make test-core` (vector inventory +
  rig wiring + all crate tests + short fuzz + drift/deps guards) and
  `make fuzz-smoke` (60 s). CI has explicit `vector-inventory`, `CORE shard`
  and `fuzz smoke (60 s)` steps.
- **Spec:** `docs/spec/test-rig.md`.
- **Gate:** `make test` → **109 passed / 0 failed** (96 C0–C8 + 13 C9),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 26 markdown files,
  dependency gate 71 external crates allowlisted, 956 vectors, 60 s fuzz
  with zero panics.
- **Negative controls:** `tools/nc_c9.py` → `make nc-c9` →
  `docs/evidence/c9/nc.log`, **6/6 PASS**: corrupted golden vector,
  impossible `MIN_VECTORS`, injected non-minimal-varint bug, injected
  trailing-byte-guard bug, zeroed fuzz iteration counter, unwired Makefile.
  The first four inject **real production bugs** and prove the properties
  catch them. Total C0–C9: **50 NC**.
- **Fixes found by the gate itself:** clippy rejected the PRNG method name
  `next` (renamed `next_u64`); a `prop_assert_eq!` moved its value (cloned);
  the fuzz CLI initially spent 6×60 s instead of 60 s total; the truncated
  vector generator produced duplicate lengths (payload lengthened to 256
  bytes). All fixed in code, not suppressed.
- **NC hygiene:** because C9 NCs deliberately inject real bugs, `proptest`
  was writing `properties.proptest-regressions` seeds for failures that do
  not exist in correct code. `failure_persistence: None` is now set in the
  proptest config and `*.proptest-regressions` is gitignored, so artificial
  NC failures can never leak into the repository.
- **Tooling / CI:** `make nc-all` covers C0–C9 (50 NC) without rewriting
  historical logs; CI regenerates only `make evidence-c9`. New targets:
  `make gen-c9-vectors`, `make nc-c9`, `make evidence-c9`,
  `make vector-inventory`, `make record-c9-deps`.
- **Evidence:** `docs/evidence/c9/gates.txt` (pre-C9 counts from the C8
  commit, inventory, wiring, 60 s fuzz, raw gates), `docs/evidence/c9/nc.log`,
  `SHA256SUMS` (also pins the fixture, spec and C9 addendum).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (C9 block + crate graph), `CHANGELOG.md`,
  `ROADMAP.md` (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C9 ✅,
  decision D-C11 + receipt), `SECURITY_PLAN.md` (C9 note), and the new
  `docs/spec/test-rig.md`.
- **Not claimed:** Miri/ASan/UBSan, 24-hour libFuzzer campaign, coverage
  thresholds or crash triage (H3/O0); state-root determinism (X7);
  genesis block / execution; `cargo deny` (H1). Validators/blocks/tx remain
  **0/0/0**.

## [C8 — genesis] 2026-10-26

**Ninth code phase.** Deliverable: `patches/0009-C8-genesis.patch`.
Adds the **14th crate**, `aura-genesis`, and the **first new external
dependency since C3**: `serde_json =1.0.154` (config parsing only).

- **Genesis document:** `Genesis { chain_id, protocol_version,
  genesis_time_ms, validators, allocations }` with `Validator`
  (`address`, `power`, `public_key[32]`) and `Allocation` (`address`,
  `balance`). A strict JSON schema with exactly five top-level keys; unknown
  or missing keys, wrong types, invalid bech32/hex, float balances, zero
  power, zero allocations, duplicate addresses, empty validator sets and
  `u128` supply overflow are all rejected. Balances are **decimal strings** —
  `u128` amounts never round-trip through `f64`.
- **Canonical binary:** C2 encoding (`varint(len) ‖ chain_id`, `u32`, `u64`,
  validator and allocation vectors). `from_canonical_bytes` uses
  `decode_canonical()` and then re-runs every structural check. JSON is
  never hashed directly — `serde_json` is used only to read the config,
  exactly the exception allowed by `ARCHITECTURE.md`.
- **Commitments:** `genesis_hash` = C3 `hash_domain(chain_id,
  Domain::Genesis, GENESIS_CONTEXT ‖ canonical_bytes)`; `validator_set_hash`
  is now actually computed (C5 left the header field caller-supplied);
  `allocation_root` is a C6 Merkle root over C7 `StateKey::account` digests
  and `Account` values, sorted by address. It is an allocation commitment,
  **not** the JMT `state_root` (S1).
- **Determinism:** object key order and whitespace do not change any
  commitment; validator array order **does** (C8 NC); allocation array order
  does not change `allocation_root` because state is a set.
  `genesis_time_ms` is explicit — no local clock.
- **CLI:** the `aura` binary gains `genesis hash --file`, `genesis verify
  --file --expect` and `genesis show --file`. `verify` exits 1 on mismatch,
  bad arguments exit 2. `make genesis-hash` is a convenience wrapper.
  D8 owns the rest of the CLI.
- **Spec / sample:** `docs/spec/genesis-format.md` and
  `docs/spec/genesis-testnet.json` (the sample's hash is pinned by a test).
- **Independent golden vectors:** `tools/gen_c8_vectors.py` — pure-Python
  BLAKE3 (7 official vectors), an independent BIP-173 bech32 encoder, C3
  domain framing, C5/C6 tree rules, C7 key/account encodings. 44 fixture
  lines: **19 JSON documents** (3 equivalent, 2 order variants, 13 invalid)
  and **2 full commitment sets**. The generator self-checks every
  determinism property. Offline and byte-stable.
- **DoD gates:** three independent JSON representations of the same config
  give one hash and one canonical byte string; reordering validators changes
  both `genesis_hash` and `validator_set_hash`. Four CLI integration tests
  pin `hash`, `verify` (accept + reject), `show` and usage.
- **Dependency governance:** the C3 allowlist is **frozen** historical
  evidence (pinned by `docs/evidence/c3/SHA256SUMS`). C8 records its +4
  delta (`serde_json`, `itoa`, `memchr`, `zmij`) in
  `docs/spec/c8-dependency-addendum.txt`; `tools/check_deps.py` compares
  `Cargo.lock` against the union of baseline + addenda.
  `tools/record_c8_deps.py` / `make record-c8-deps` is the reviewed action.
- **Gate:** `make test` → **96 passed / 0 failed** (82 C0–C7 + 14 C8),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 24 markdown files,
  dependency gate 58 external crates allowlisted.
- **Negative controls:** `tools/nc_c8.py` → `make nc-c8` →
  `docs/evidence/c8/nc.log`, **6/6 PASS**: genesis context, validator-set
  context, golden hash, validator-order sorting, unknown-field rejection,
  Stage 1 chain-id check. Total C0–C8: **44 NC**.
- **Fixes found by the gate itself:** a CLI test used a path relative to the
  test file instead of `CARGO_MANIFEST_DIR`; a JSON string helper needed an
  explicit lifetime; an unused import was removed. All fixed in code.
- **Tooling / CI:** `make nc-all` covers C0–C8 (44 NC) without rewriting
  historical logs; CI regenerates only `make evidence-c8`. New targets:
  `make gen-c8-vectors`, `make nc-c8`, `make evidence-c8`,
  `make record-c8-deps`, `make genesis-hash`; `make test-core` covers all
  eight implemented crates.
- **Evidence:** `docs/evidence/c8/gates.txt` (pre-C8 counts from the C7
  commit, dependency delta, CLI hash, raw gates), `docs/evidence/c8/nc.log`,
  `SHA256SUMS` (also pins fixture, spec, sample config, addendum).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (C8 block + crate graph), `CHANGELOG.md`,
  `ROADMAP.md` (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C8 ✅,
  decision D-C10 + receipt), `NODE_PLAN.md` (C8 note), and the new
  `docs/spec/genesis-format.md`.
- **Not claimed:** genesis **block** / execution (B/D phases), JMT
  `state_root` (S1), validator key ceremony (W7/O5), BLS aggregation (B2),
  Stage 2 mainnet genesis, `cargo deny` (H1). Validators/blocks/tx remain
  **0/0/0**.

## [C7 — account model] 2026-10-26

**Eighth code phase.** Deliverable: `patches/0008-C7-account-model.patch`.
**No new crate and no new external dependency** — `aura-state`, an honest
empty placeholder since C0, gets its first real API.

- **Account value:** `Account { nonce, balance, code_hash, storage_root }`,
  88 canonical bytes (`u64 LE ‖ u128 LE ‖ 32 ‖ 32`), `decode_canonical()`
  round-trip. `NO_CODE = EMPTY_STORAGE_ROOT = Hash::ZERO` sentinels.
  `credit`/`debit`/`bump_nonce` use checked arithmetic — no silent wrap; a
  failed debit leaves the balance unchanged.
- **State key layout:** five protocol namespaces — `0x01` account (21 bytes),
  `0x02` code (21), `0x03` storage (53 = tag + contract[20] + slot[32]),
  `0x04` staking (21), `0x05` receipt (33). `0x00` and `0x06`–`0xFF` are
  **reserved**: `StateKey::from_raw` rejects them, so contract code cannot
  even construct such a key. Known prefix + exact namespace length only.
- **Contract isolation:** `assert_contract_may_write(contract, key)` allows
  only `0x03 || its_own_address || slot` — other namespaces give
  `ForbiddenContractWrite`, another contract's storage gives
  `WrongContractStorage`. The VM host ABI (X phases) must call it per write.
- **`StateKey::digest`:** `BLAKE3(STATE_KEY_CONTEXT || u64(len) || raw)` — a
  domain-separated, length-prefixed JMT path ready for S1. C7 does **not**
  compute a state root and has no storage backend.
- **Spec:** `docs/spec/account-model.md` (namespaces, reserved space,
  isolation, digest, account wire format, arithmetic, non-goals).
  `STATE_PLAN.md` carries a C7 note clarifying that S1 still owes the tree.
- **Independent golden vectors:** `tools/gen_c7_vectors.py` — pure-Python
  BLAKE3 (self-checked against 7 official vectors) plus a transcription of
  the key layout, digests and account encoding. 107 fixture lines: raw keys,
  encoded keys and digests for 5 namespaces × 4 indices (generator asserts
  all 20 digests differ), 3 account encodings, contract-isolation operands
  and **12 rejection vectors** (5 reserved prefixes, wrong key lengths, empty
  key, non-minimal varint, trailing byte, truncation). Offline, byte-stable.
- **DoD gates:** 1000 distinct addresses → 5000 keys/digests with zero
  collisions; all 256 prefixes iterated with exactly five reachable;
  contracts cannot write outside their own storage namespace.
- **Gate:** `make test` → **82 passed / 0 failed** (71 C0–C6 + 11 C7),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 23 markdown files,
  dependency gate 54 external crates allowlisted.
- **Negative controls:** `tools/nc_c7.py` → `make nc-c7` →
  `docs/evidence/c7/nc.log`, **6/6 PASS**: state-key context, golden digest,
  reserved-prefix rejection, namespace length, contract namespace, contract
  address. Total C0–C7: **38 NC**.
- **Defects found by the gate itself:** the first deterministic address
  pattern had a period of 256 and collided at index 256 — replaced by a
  pattern whose first 8 bytes encode the index. Two wrong test assumptions
  (account length 76 vs 88, storage-key length 21 vs 53) were corrected.
  All fixed in code, not suppressed.
- **Tooling / CI:** `make nc-all` now covers C0–C7 (38 NC) without rewriting
  historical logs; CI regenerates only `make evidence-c7`. New targets:
  `make gen-c7-vectors`, `make nc-c7`, `make evidence-c7`; `make test-core`
  now covers all six implemented crates.
- **Evidence:** `docs/evidence/c7/gates.txt` (pre-C7 counts from the C6
  commit, DoD gates, raw output), `docs/evidence/c7/nc.log`, `SHA256SUMS`
  (also pins the golden fixture and the spec).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (C7 block), `CHANGELOG.md`, `ROADMAP.md` (ledger,
  §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C7 ✅, decision D-C9 +
  receipt), `STATE_PLAN.md` (C7 note), and the new
  `docs/spec/account-model.md`.
- **Not claimed:** versioned state tree / `state_root` (S1), persistence
  (S2), execution overlays (S3), genesis allocation (C8), test rig/fuzzing
  (C9), VM host ABI (X phases), `cargo deny` (H1). Validators/blocks/tx
  remain **0/0/0**.

## [C6 — Merkle proofs] 2026-10-25

**Seventh code phase.** Deliverable:
`patches/0007-C6-merkle-proofs.patch`. **No new crate and no new external
dependency** — the proof module lives inside `aura-block`.

- **Compact inclusion proofs:** `MerkleProof = varint(index) ||
  varint(total) || leaf[32] || varint(sibling_count) || sibling[32]*count`.
  Sibling direction is derived from `index >> level`, so no direction bits are
  stored. `MAX_PROOF_DEPTH = 64`; minimal LEB128 varints and
  `decode_canonical()` reject non-canonical encodings. For 1000 leaves the
  depth is 10 and a proof is ~358 bytes.
- **Two leaf kinds, one tree shape.** Transaction leaves are the C5
  `leaf_hash(txid)`; event leaves are
  `BLAKE3(EVENT_LEAF_CONTEXT || u64(len) || event)`, so a tx leaf cannot be
  replayed as an event leaf and ambiguous concatenations cannot collide.
  `verify_tx_inclusion` / `verify_event_inclusion` bind the supplied value to
  `proof.leaf` before checking the path — without that, a proof would only
  show that *some* hash is in the tree.
- **C5 tree unchanged.** `compute_tx_root` now delegates to `build_tx_tree`;
  root and proof share one padded-level implementation, so they cannot
  disagree. A regression test compares the C5 root of four real C4
  transactions with the C6 tree root. The C5 and C6 generators agree on
  `LEAF_CONTEXT`/`NODE_CONTEXT` — pinned by a test.
- **Spec:** `docs/spec/merkle-proof-format.md` (tree construction, wire
  format, verification order, vectors, non-goals).
- **Independent golden vectors:** `tools/gen_c6_vectors.py` — pure-Python
  BLAKE3 (self-checked against 7 official BLAKE3 vectors) plus a
  transcription of the C5 tree and C6 encoding. 69 fixture lines: roots and
  depths for **1/2/3/5/8/1000** tx leaves, roots for 1/3/5 event leaves,
  **34 canonical proofs** and **11 rejection vectors**, each pinned to a
  specific error. Generator is offline and byte-stable.
- **C6 DoD gate:** 1000 leaves, all **1000** proofs generated and verified,
  then one substituted leaf rejected — both by leaf binding (`LeafMismatch`)
  and by the reconstructed root (`RootMismatch`).
- **Gate:** `make test` → **71 passed / 0 failed** (60 C0–C5 + 11 C6),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 22 markdown files,
  dependency gate 54 external crates allowlisted.
- **Negative controls:** `tools/nc_c6.py` → `make nc-c6` →
  `docs/evidence/c6/nc.log`, **6/6 PASS**: event-leaf context, golden root,
  leaf/value binding, sibling-count check, sibling direction, root
  comparison. Total C0–C6: **32 NC**.
- **Gate-found fixes:** clippy required `u64::div_ceil`,
  `is_multiple_of(2)` and an iterator instead of an index loop. All fixed in
  code, not suppressed.
- **Tooling / CI:** `make nc-all` now covers C0–C6 (32 NC) without rewriting
  historical logs; CI regenerates only `make evidence-c6`. New targets:
  `make gen-c6-vectors`, `make nc-c6`, `make evidence-c6`.
- **Evidence:** `docs/evidence/c6/gates.txt` (pre-C6 counts from the C5
  commit, the 1000-leaf gate, raw output), `docs/evidence/c6/nc.log`,
  `SHA256SUMS` (also pins the golden fixture and the spec).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (C6 block), `CHANGELOG.md`, `ROADMAP.md` (ledger,
  §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C6 ✅, decision D-C8 +
  receipt), and the new `docs/spec/merkle-proof-format.md`.
- **Not claimed:** accounts and state key layout (C7), genesis (C8), test rig
  and fuzzing (C9), batch/multiproofs, sparse state trees (S1), proof serving
  over RPC (N/D phases), `cargo deny` (H1). Validators/blocks/tx remain
  **0/0/0**.

## [C5 — block and header] 2026-10-25

**Sixth code phase.** Deliverable: `patches/0006-C5-block-header.patch`.
Adds the **13th crate**, `aura-block`, with **no new external dependencies**.

- **Format:** `Block = Header || Body`. `Header` has 12 fields: `version,
  chain_id, height, parent_hash, tx_root, state_root, validator_set_hash,
  timestamp_ms, proposer, qc, gas_used, base_fee`. `HeaderParams` groups the
  11 caller-supplied fields; `tx_root` is deliberately absent and always
  derived, so it cannot be forged at construction. `Body::new` sorts by
  `(signer, nonce)` (both derive `Ord`) and rejects duplicate keys.
- **C5 amends `ARCHITECTURE.md` §1 (decision D-C7):** `chain_id` is added to
  the header because rule D5 requires chain separation for block ids. No
  migration is needed — the network has produced **0 blocks**.
  `state_root`, `validator_set_hash` and `qc` are caller-supplied values:
  state (S1), validator sets and QCs (B phases) are not implemented. C5 fixes
  their wire representation and validation, not their computation.
- **`tx_root`:** binary BLAKE3 Merkle tree over C4 txids —
  `leaf = BLAKE3(LEAF_CONTEXT || txid)`,
  `node = BLAKE3(NODE_CONTEXT || left || right)`, odd final node duplicated,
  empty body → `Hash::ZERO`. Order-sensitive: the DoD vector proves
  `tx_root_forward != tx_root_backward`. The shape is fixed now so C6 can add
  proofs without changing the root scheme.
- **`block_hash`:** C3 `hash_domain(chain_id, Domain::Block,
  BLOCK_HASH_PAYLOAD_CONTEXT || canonical(Header))`. Header only; `tx_root`
  commits to the body. Any header field or chain change alters the id.
- **Validation:** `validate(expected_chain)` checks version, chain, body/QC
  size limits, strict `(signer, nonce)` ordering, duplicates and then a
  recomputed `tx_root`. `validate_transactions()` is separate and checks each
  tx signature/expiry at `header.height`. `timestamp_ms` is caller supplied —
  no local clock anywhere.
- **Spec:** `docs/spec/block-format.md` (field table, tree, hash, validation
  order, error catalogue, non-goals).
- **Independent golden vectors:** `tools/gen_c5_vectors.py` — pure-Python
  BLAKE3 (self-checked against 7 official vectors), C2/C3 wire transcription
  and the committed C4 tx vectors as bodies. 95 fixture lines: **4 canonical
  blocks** (2/3/0/1 transactions covering even/odd tree levels, empty tree,
  a real `b1 → b2` parent link, `Some(qc)`, u64/u128 maxima), an
  order-sensitivity pair and **9 rejection vectors** (trailing byte,
  truncation, bad version, bad root, reordered body, reordered body **with a
  recomputed root**, duplicate key, bad QC tag, non-minimal count). Each bad
  vector is pinned to a specific error. Generator is offline and byte-stable.
- **Gate:** `make test` → **60 passed / 0 failed** (49 C0–C4 + 11 C5),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 21 markdown files,
  dependency gate 54 external crates allowlisted.
- **Negative controls:** `tools/nc_c5.py` → `make nc-c5` →
  `docs/evidence/c5/nc.log`, **6/6 PASS**: leaf context, node context,
  block-hash context, golden `tx_root`, `tx_root` check, canonical-order
  check. Total C0–C5: **26 NC**.
- **Gate-found fixes:** clippy caught an unused import, a collapsible `if`,
  `chunks_exact(2)` and a cloned single-ref slice. Fixed in code
  (`as_chunks::<2>()`, `std::slice::from_ref`), not suppressed.
- **Tooling / CI:** `make nc-all` now covers C0–C5 (26 NC) without rewriting
  historical logs; CI regenerates only `make evidence-c5`. New targets:
  `make gen-c5-vectors`, `make nc-c5`, `make evidence-c5`; `make test-core`
  now covers all five implemented crates.
- **Evidence:** `docs/evidence/c5/gates.txt` (pre-C5 counts from the C4
  commit, 12→13 crates, raw gate output), `docs/evidence/c5/nc.log`,
  `SHA256SUMS` (also pins the golden fixture and the format spec).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (§1 header + §3 graph + C5 block), `CHANGELOG.md`,
  `ROADMAP.md` (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C5 ✅,
  decision D-C7 + receipt), and the new `docs/spec/block-format.md`.
- **Not claimed:** Merkle proofs (C6), accounts (C7), genesis (C8), fuzzing
  (C9), QC/validator-set/state computations (B/S phases), BLS (B2), PQ (H2),
  `cargo deny` (H1). Validators/blocks/tx remain **0/0/0**.

## [C4 — transaction format] 2026-10-25

**Fifth code phase.** Deliverable: `patches/0005-C4-tx-format.patch`.
Adds the **12th crate**, `aura-tx`, with **no new external dependencies**.

- **Transaction format:** `Tx = UnsignedTx || Signature`. `UnsignedTx` has
  the ten C4 fields (`chain_id, signer, nonce, kind, payload, gas_limit,
  max_fee, tip, expiry`) plus `codec_version` from `ARCHITECTURE.md` §1.
  `Signature` is an envelope `{ algo, public_key[32], bytes[64] }` —
  Ed25519 cannot recover a public key, so it travels with the tx and is
  bound to `signer` by `BLAKE3(ADDRESS_CONTEXT || public_key)[0..20]`.
  `TxKind` has five strict tags; `SignatureAlgo` accepts Ed25519 only
  (secp256k1 tag 1 reserved, PQ tags for H2).
- **Signing:** `signing_bytes = SIGNING_CONTEXT || canonical(UnsignedTx)`,
  signed with C3's domain-separated `aura_crypto::sign(Domain::Tx,
  chain_id)` over the BLAKE3 digest. `txid = BLAKE3(TXID_CONTEXT ||
  canonical(Tx))` includes the signature. `TxParams` groups the nine
  constructor fields so clippy's `too_many_arguments` is fixed, not silenced.
- **Replay protection (DoD gate):** `validate(expected_chain, height)`
  rejects a tx minted for `auralite-1` on `auralite-test-1`; rewriting the
  chain-id after signing breaks the signature because the chain-id is inside
  both `signing_bytes` and the C3 domain prefix. Validation also checks
  version, payload ≤ 65536, gas ≠ 0, tip ≤ max_fee and `height ≤ expiry`.
  `height`/`expiry` are consensus heights — no local clock.
- **Spec:** `docs/spec/tx-format.md` — field/wire tables, tags, signing,
  txid, validation order, error catalogue and non-goals.
- **Independent golden vectors:** `tools/gen_c4_vectors.py` contains
  pure-Python BLAKE3 (self-checked against 7 official BLAKE3 vectors),
  pure-Python Ed25519 (self-checked against RFC 8032 §7.1 TEST 1/2/3) and a
  C2 wire encoder. 104 fixture lines: **5 canonical transactions** (including
  u64/u128 maxima and a mainnet-chain tx) and **9 rejection vectors**
  (trailing byte, truncation, bad version, unknown kind, unknown algorithm,
  tampered signer/signature, non-minimal chain and payload length). Each bad
  vector is pinned to a specific error. Generator is offline and byte-stable.
- **Additive C2 change:** `CodecError::InvalidEnumTag(&'static str, u8)` for
  caller-defined enums. No C2 wire-format change; C2 vectors, evidence and
  checksums untouched.
- **Gate:** `make test` → **49 passed / 0 failed** (39 C0–C3 + 10 C4),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 20 markdown files,
  dependency gate 54 external crates allowlisted.
- **Negative controls:** `tools/nc_c4.py` → `make nc-c4` →
  `docs/evidence/c4/nc.log`, **6/6 PASS**: signing context, txid context,
  golden signature, replay protection, signer↔key binding, Ed25519
  verification. Total C0–C4: **20 NC**.
- **Tooling / CI:** `make nc-all` runs all 20 NCs **without rewriting**
  historical C0–C3 logs, so their committed `SHA256SUMS` stay valid; CI now
  regenerates only the current phase's evidence (`make evidence-c4`). New
  targets: `make gen-c4-vectors`, `make nc-c4`, `make evidence-c4`.
  `make test-core` now covers all four implemented crates.
- **Evidence:** `docs/evidence/c4/gates.txt` (pre-C4 counts from the C3
  commit, 11→12 crates, raw gate output), `docs/evidence/c4/nc.log`,
  `SHA256SUMS` (also pins the golden fixture and the format spec).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (crate graph + C4 block), `CHANGELOG.md`,
  `ROADMAP.md` (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C4 ✅,
  decision D-C6 + receipt), and the new `docs/spec/tx-format.md`.
- **Not claimed:** block/header and `tx_root` (C5), Merkle proofs (C6),
  accounts (C7), genesis (C8), fuzzing (C9), fee charging/gas metering,
  secp256k1 in tx signatures, hardware keys (W5), PQ signatures (H2),
  `cargo deny` (H1). Validators/blocks/tx remain **0/0/0**.

## [C3 — cryptography] 2026-10-25

**Fourth code phase. First phase with external dependencies.**
Deliverable: `patches/0004-C3-crypto.patch`.

- **`aura-crypto` crate:** BLAKE3 hashing (upstream `pure` feature — portable
  Rust, no C/assembly in the hash path), Ed25519 signatures (RFC 8032),
  secp256k1 deterministic ECDSA (RFC 6979, low-S, compatibility only),
  domain-separated hashing and signing, constant-time byte comparison
  (`subtle`) and zeroizing secret storage (`zeroize`).
- **Domain separation:** `HASH_CONTEXT || u64(len(prefix)) || prefix ||
  u64(len(payload)) || payload`, where `prefix = chain_id || 0x00 ||
  domain_tag || 0x00` comes from C1's `Domain::prefix`. A testnet signature
  is invalid on mainnet; a vote cannot be replayed as a transaction. Changing
  `HASH_CONTEXT` is a protocol change and is pinned by an NC.
- **Independent golden vectors** (`tools/gen_c3_vectors.py`, 66 fixture
  lines, byte-stable, offline): 15 official BLAKE3 vectors from
  `BLAKE3-team/BLAKE3/test_vectors.json`; a self-checking pure-Python BLAKE3
  producing 5 domain-hash vectors; RFC 8032 §7.1 Ed25519 TEST 1/2/3; and a
  pure-Python secp256k1 + RFC 6979 implementation producing 2 signatures,
  additionally verified with OpenSSL `pkeyutl -verify`. `k256` matched the
  independent signatures byte for byte.
- **Dependencies (reviewed, not silent):** exact pins in
  `[workspace.dependencies]` — `blake3 =1.8.7`, `ed25519-dalek =2.2.0`,
  `k256 =0.13.4`, `zeroize =1.8.2`, `subtle =2.6.1`. `Cargo.lock` resolves
  54 external crates, all crates.io-sourced with sha256 checksums.
  `docs/spec/c3-dependency-allowlist.txt` records the exact resolved set;
  `tools/check_deps.py` is now part of `make test` and rejects unpinned/git
  specs, missing checksums, direct `serde` in consensus crates and allowlist
  drift. `tools/record_c3_deps.py` is the deliberate re-record action.
  **Full `cargo deny` (licences + advisories) is still owed to H1.**
- **Gate:** `make test` → **39 passed / 0 failed** (28 C0–C2 + 11 C3),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate OK.
- **Negative controls:** `tools/nc_c3.py` → `make nc-c3` →
  `docs/evidence/c3/nc.log`, **5/5 PASS**: changed `HASH_CONTEXT`, corrupted
  domain-hash vector, corrupted RFC 8032 vector, Ed25519 verification
  bypass, dependency allowlist drift (this one runs `make test`, proving the
  new gate is actually wired in). Total across C0–C3: **14 NC**.
- **Evidence:** `docs/evidence/c3/gates.txt` (pre-C3 counts from the C2
  commit, dependency audit, raw gate output), `docs/evidence/c3/nc.log`,
  `SHA256SUMS`.
- **Tooling:** `make gen-c3-vectors`, `make nc-c3`, `make evidence-c3`,
  `make check-deps`, `make record-c3-deps`; CI runs all four phases' NCs and
  uploads C0–C3 evidence.
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (C3 status block), `CHANGELOG.md`, `ROADMAP.md`
  (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C3 ✅ + receipt).
- **Not claimed:** BLS12-381 QC aggregation (B2), PQ migration to
  ML-DSA/SLH-DSA (H2), hardware-backed keys (W5), tx/block formats (C4/C5),
  `cargo deny` (H1). `cc` remains an upstream build-dependency of `blake3`
  but no native hash code is compiled with the `pure` feature.

## [C2 — canonical codec] 2026-10-25

**Third code phase.** Deliverable: `patches/0003-C2-codec.patch`. Still zero
external dependencies (one internal path dependency
`aura-codec -> aura-types`).

- **`aura-codec` crate:** `Encode`/`Decode` traits, `Writer`/`Reader`,
  `write_varint`/`read_varint` (minimal unsigned LEB128; non-minimal,
  overflowing and >10-byte encodings rejected), fixed-width LE integers for
  `u8..u128` (deliberately **no** `usize`/`isize`), strict tags for `bool`,
  `Option`, `Domain`, `Denom`, length-prefixed `String`/`Vec<T>`/`ChainId`,
  fixed 32-byte `Hash` and 20-byte `Address`, checked `Amount`/`Nonce`/
  `Height`/`Gas`, and `BTreeMap` decoding that requires strictly ascending
  keys (rejecting disorder and duplicates).
- **Two independent canonicality layers:** strict decoders plus
  `decode_canonical()`, which re-encodes the decoded value and requires exact
  byte equality with the input (`NonCanonicalEncoding` otherwise).
  `CODEC_VERSION = 1`, `MAX_LENGTH = 1 MiB`, `MAX_ITEMS = 65536`.
- **Reference record:** `CanonicalRecord` with 11 fields in a fixed wire
  order, covering every primitive kind. `serde` remains absent.
- **Independent golden vectors:** `tools/gen_c2_vectors.py` re-implements the
  wire format in Python; `crates/aura-codec/tests/fixtures/c2_golden.txt`
  holds 21 lines — 3 canonical records (incl. `u64`/`u128` maxima and
  multi-byte UTF-8), 5 constants and 12 non-canonical vectors (bad version,
  domain, bool, option tag; trailing byte; non-minimal chain length and map
  count; oversized length; invalid UTF-8; unordered and duplicate map keys;
  varint overflow). Each bad vector is pinned to a specific `CodecError`.
- **Gate:** `make test` → **28 passed / 0 failed** (17 C0/C1 + 11 C2),
  fmt/clippy clean, drift guard 12 plans / 112 rows.
- **Negative controls:** `tools/nc_c2.py` → `make nc-c2` →
  `docs/evidence/c2/nc.log`, **4/4 PASS**: `CODEC_VERSION` 1→2, corrupted
  golden fixture, disabled `Reader::finish()` trailing-byte guard, disabled
  non-minimal-varint predicate. All restore, with a mandatory green check.
- **Evidence:** `docs/evidence/c2/gates.txt` (raw output incl. pre-C2 counts
  from the C1 commit and a dependency audit showing 0 external crates),
  `docs/evidence/c2/nc.log`, `SHA256SUMS`.
- **Tooling:** `make gen-c2-vectors`, `make nc-c2`, `make evidence-c2`; CI now
  runs all three phases' NCs (9 total) and uploads C0/C1/C2 evidence.
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (C2 wire format), `CHANGELOG.md`, `ROADMAP.md`
  (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C2 ✅ + receipt).
- **Not claimed:** hashing/signatures (C3), tx/block formats (C4/C5),
  `cargo deny` (H1). Any wire format change needs a `CODEC_VERSION` bump and a
  migration test.

## [C1 — canonical types] 2026-10-25

**Second code phase.** Deliverable: `patches/0002-C1-types.patch`. Still zero
external dependencies.

- **New types in `aura-types`:** `Hash` (32 bytes, hex/slice decode, `ZERO`),
  `Address` (20 bytes, in-tree classic BIP-173 bech32 encode/decode),
  `Amount`/`Denom` (checked `u128` in `aalcry`, `from_coins`),
  `ChainId` (validated `[a-z0-9-]`, 3..64), `Nonce`/`Height`/`Gas`
  (checked `u64`), `Domain` (5 tags, `chain-id || 0 || tag || 0` prefix for
  D5), and `CanonicalMap = BTreeMap` (rule D3).
- **Independent golden vectors:** `tools/gen_c1_vectors.py` implements BIP-173
  separately in Python; `crates/aura-types/tests/fixtures/c1_golden.txt` holds
  23 lines — 4 hash vectors, 4 address vectors, 2 corrupted checksums, wrong
  prefix, uppercase, amount bounds. `make gen-c1-vectors` is byte-stable and
  its sha256 is checked in evidence.
- **Gate:** `make test` → **17 passed / 0 failed** (5 unit + 11 C1 + 1
  identity), fmt/clippy clean, drift guard still 12 plans / 112 rows.
- **Negative controls:** `tools/nc_c1.py` → `make nc-c1` →
  `docs/evidence/c1/nc.log`, **3/3 PASS**: checksum constant 1→0, address
  version `0x00`→`0x01`, corrupted golden vector. Each requires red, then
  restores; a final green restore check is mandatory.
- **Evidence:** `docs/evidence/c1/gates.txt` (raw output incl. pre-C1 counts
  from the C0 commit), `docs/evidence/c1/nc.log`, `SHA256SUMS` (gates, NC log,
  golden fixture).
- **Tooling:** `make gen-c1-vectors`, `make nc-c1`, `make evidence-c1`; CI now
  runs both phases' NCs and uploads both evidence directories. C0 evidence is
  explicitly marked historical and is **not** regenerated in CI (it would
  relabel the wrong tree).
- **Defects caught by the existing C0 gate while writing C1** (fixed in this
  patch, not hidden): a compile error `&String + &str` in a vector test and
  rustfmt drift in four files. The gate did its job.
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`, `CHANGELOG.md`,
  `ROADMAP.md` (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C1 ✅
  + receipt).
- **Not claimed:** bech32m, serialization (C2), hashing or signatures (C3),
  tx/block formats (C4/C5). bech32m is a versioned decision, not a C1 tweak.

## [C0 — baseline workspace] 2026-10-25

**First code.** Deliverable: `patches/0001-C0-baseline-workspace.patch`.
No business logic: C0 changes only the measurable foundation, exactly as the
CORE_PLAN DoD requires.

- **Workspace:** `Cargo.toml` with 11 member crates and **zero external
  dependencies**; `Cargo.lock` committed; `rust-toolchain.toml` pinned to
  rustc 1.99.1 with `rustfmt` + `clippy`.
- **Identity in code:** `crates/aura-types` exports `NETWORK_NAME`,
  `CHAIN_ID="auralite-test-1"`, `MAINNET_CHAIN_ID="auralite-1"` (reserved),
  `DENOM="ALCRY"`, `SMALLEST_UNIT="aalcry"`, `DECIMALS=18`,
  `BECH32_PREFIX="aura"`, `CONSENSUS="bft-pos"`, `VM="wasm"`,
  `PROTOCOL_VERSION=1`, plus `is_our_chain()` and `units_per_coin()`.
  4 unit tests + 1 integration test lock these values; the testnet/mainnet
  chain-id separation is asserted.
- **Honest placeholders:** 10 crates (`aura-codec`, `aura-crypto`,
  `aura-state`, `aura-storage`, `aura-exec`, `aura-consensus`, `aura-p2p`,
  `aura-mempool`, `aura-rpc`, `aura-node`) compile with no public API —
  CONTRIBUTING.md forbids stubs that silently return success.
- **Gates:** `Makefile` (`deps-check`, `test`, `test-core`, `nc-c0`, plus
  explicitly labelled SKIP targets for sanitizers/determinism/fuzzing), and
  `.github/workflows/ci.yml` running the same commands (one honest C0 shard;
  the 10 shards arrive with O0).
- **Docs drift guard:** `tools/check_claims.py` is part of `make test`. It
  validates every local markdown link, the exact 112-row phase ledger, the
  4-column phase-row shape, Status/Stage lines and CHANGELOG sync — the
  `MATURITY_AUDIT.md` lesson turned into a gate.
- **Negative controls:** `tools/nc_c0.py` (→ `make nc-c0` →
  `docs/evidence/c0/nc.log`). NC-1 tampers `DENOM` and requires
  `cargo test -p aura-types` to go red; NC-2 deletes the C0 phase row in a
  temp doc tree and requires `check_claims.py` to go red. Both then restore.
- **Defect found by the new gate (measured, not assumed):** the first run of
  `make test` went red because `check_claims.py` compiled its phase-row regex
  without `re.MULTILINE` and therefore reported all 112 phase rows missing.
  Fixed in this same patch; NC-2 now pins that removing a single row really
  does turn the guard red. This is the `MATURITY_AUDIT.md` pattern applied to
  our own tooling: the guard is itself guarded.
- **Evidence:** `docs/evidence/c0-baseline.txt` (pre-C0 zeros measured from
  the baseline git commit, post-C0 numbers with raw command output),
  `docs/evidence/c0/nc.log`, `docs/evidence/SHA256SUMS`. Regenerate with
  `make evidence-c0` (run before the C0 commit so HEAD is the baseline).
- **Honest refusals:** `make devnet` exits 1 with "D7 not implemented";
  `make test-sanitize` / `test-determinism` / `fuzz-smoke` print SKIP lines
  naming H3/C9/X7 instead of pretending to pass.
- **Docs synced in this same patch (D9):** `README.md`, `docs/STATUS.md`,
  `CHANGELOG.md`, `ROADMAP.md` (ledger + §8), `CONTRIBUTING.md`,
  `docs/plans/CORE_PLAN.md` (C0 row ✅).
- **Not claimed:** sanitizers, determinism, fuzzing, devnet, `cargo deny`,
  10 CI shards. Those are named SKIPs with their owning phases.

## [English translation / A0.2] 2026-10-25

Owner request: "make everything in English and continue". All 19 markdown files
(`README.md`, `ROADMAP.md`, `CHANGELOG.md`, `CONTRIBUTING.md`,
`docs/ARCHITECTURE.md`, `docs/STATUS.md`, `docs/TICKER_AUDIT.md` and the 12
`docs/plans/*_PLAN.md`) were translated to English to match the language of
AuraLite-OS and AuraLite-Emulator. Technical identifiers, phase codes, file
paths, decisions D1–D12, Q1–Q7, Stage 1/Stage 2 marks and all measured numbers
are unchanged. This is a documentation-only change: still **0 lines of Rust**.

## [D0 locked / A0.1] 2026-10-25

**Owner decisions locked** (D0 questionnaire). Still no code.

- **Name:** project and repository renamed to `AuraLite-Crypto`;
  network `AuraLite-Crypto Network`; chain-id `auralite-test-1`
  (`auralite-1` reserved for mainnet).
- **Ticker:** **`ALCRY`** chosen (18 decimals, `aalcry`). Added
  [`docs/TICKER_AUDIT.md`](docs/TICKER_AUDIT.md): the CoinGecko search API and
  Yahoo Finance Symbol Lookup returned 0 matches for `ALCRY`; `AURA`, `AUR`,
  `AURS`, `ARA`, `AURC`, `ALC`, `ALCX`, `ALTC`, `ALCR`, `AURY` were rejected
  as taken. Caveat: there is no single global ticker registry, so a re-check is
  mandatory in T1 and before Stage 2.
- **Consensus (Q2):** BFT-PoS — HotStuff-like pipeline, 2f+1 QC, VRF.
- **VM (Q3):** WASM (`wasmtime`) + our ABI + Rust SDK; EVM (`revm`) remains
  the optional phase X9.
- **Scope (Q5):** Stage 1 = M0–M2 (core, devnet, public testnet) with no public
  market token. Mainnet, the MiCA white paper and the external audit moved to
  ⏸ Stage 2 and start by a separate decision.
- Q4 (accounts + JMT), Q6 (documentation language), Q7 (AuraLite OS integration
  — optional E9) accepted as defaults.
- Updated: `README.md`, `ROADMAP.md` (§0, north star, M0–M5 milestones, queue),
  `docs/STATUS.md`, `docs/ARCHITECTURE.md`, all 12 plans (`**Stage:**` lines
  with ⏸ Stage 2 marks on deferred phases).
- Workspace directory: `/home/user/AuraLite-Crypto/`.

## [Plan baseline / A0] 2026-10-25

**What landed:** the plan only. No code, no tests, no network.

- `README.md` — north star, ecosystem composition, architecture at a glance,
  honest Reality check.
- `ROADMAP.md` — house style: ledger of 12 plans, decisions D1–D12,
  open questions Q1–Q7, standing rules, state-of-tree audit, milestones M0–M5,
  critical path.
- `docs/STATUS.md` — measured baseline (0 crates, 0 lines of Rust, 0 tests)
  and maturity scale L0–L5.
- `docs/ARCHITECTURE.md` — invariants, formats, transaction flow, crate
  dependency graph.
- `CONTRIBUTING.md` — phase rules, patch naming, gates, NCs, secret ban.
- `docs/plans/*_PLAN.md` — 12 plans, each with phases carrying DoD, test gate,
  deliverable patch and a list of non-goals:
  `CORE` C0–C10, `STATE` S0–S7, `EXEC` X0–X9, `CONSENSUS` B0–B10,
  `NET` N0–N8, `NODE` D0–D8, `WALLET` W0–W7, `ECOSYSTEM` E0–E9,
  `ECONOMICS` T0–T8, `SECURITY` H0–H9, `PERF` P0–P6, `OPS` O0–O9.

**At the time of this entry** Q1–Q7 were open; they were locked by the next
entry `[D0 locked / A0.1]` (we do not rewrite history).

**Deliberately not done:** no `Cargo.toml`, `Makefile`, CI, `patches/`,
`crates/`. All of that is the deliverable of phase C0, not a "small thing" to
add silently. Just like C0 in AuraLite-Emulator, the first patch changes
*only* the measurable foundation and documentation.

**External sources used in the plan:** Regulation (EU) 2023/1114 (MiCA) on
public offers of crypto-assets
[2](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX%3A32023R1114),
the white paper notification guidance
[3](https://www.amf-france.org/en/news-publications/depth/mica),
the final NIST FIPS 203/204/205 standards
[7](https://csrc.nist.gov/news/2024/postquantum-cryptography-fips-approved).
These are planning references, not legal advice.
