# Implementation status — AuraLite-Crypto

> Updated **in the same patch** as the phase's code (rule D9).
> Snapshot date: **2026-10-28**, tree: "STATE_PLAN complete — S0–S7 landed,
> Stage 1 in progress". See [`CLOSEOUT_AUDIT.md`](CLOSEOUT_AUDIT.md) (CORE) and
> `docs/evidence/s7/report.md` (S7).

## Locked decisions (2026-10-25)

| Parameter | Value |
|---|---|
| Project / network | `AuraLite-Crypto` / `AuraLite-Crypto Network` |
| Ticker | `ALCRY` (18 decimals, `aalcry`) — see [`TICKER_AUDIT.md`](TICKER_AUDIT.md) |
| chain-id Stage 1 | `auralite-test-1` (mainnet `auralite-1` reserved) |
| Consensus | BFT-PoS, 2f+1 QC, VRF |
| VM | WASM (`wasmtime`) + our ABI; EVM optional in X9 |
| Current stage | **Stage 1 = M0–M2** (core → devnet → public testnet) |
| ⏸ Stage 2 | mainnet, white paper, external audit, DeFi, bridge — separate decision |
| Docs language | English (owner request, 2026-10-25) |

**Phase ledger:** C0–C10 ✅ (`patches/0001-C0-...` … `patches/0011-C10-...`)
— **CORE_PLAN is complete**; S0 ✅ (`patches/0012-S0-state-baseline.patch`),
S1 ✅ (`patches/0013-S1-merkle-tree.patch`), S2 ✅
(`patches/0014-S2-rocksdb-wal.patch`) and S3 ✅
(`patches/0015-S3-execution-overlay.patch`) and S4 ✅
(`patches/0016-S4-snapshots.patch`), S5 ✅
(`patches/0017-S5-state-sync.patch`), S6 ✅
(`patches/0018-S6-pruning-archive.patch`) and S7 ✅
(`patches/0019-S7-recovery-backup.patch`) done — **STATE_PLAN is COMPLETE**.
Next in dependency order: `EXEC_PLAN` X0 (WASM runtime), which is not started.

## Maturity level

| Level | Definition | Now |
|---|---|---|
| L0 | Docs/plan, no code | ✅ done at C0 |
| L1 | Core libraries + test rig, no network | ✅ **we are here** (CORE C0–C10 + STATE S0–S7; X/B phases still open) |
| L2 | Devnet: 4 validators in one process/host | ❌ |
| L3 | Public testnet with external validators | ❌ (Stage 1 target) |
| L4 | Mainnet candidate: audit, freeze, white paper | ⏸ Stage 2 |
| L5 | Mainnet + live application ecosystem | ⏸ Stage 2 |

L3 is the ceiling of **Stage 1**. L4–L5 belong to ⏸ Stage 2 (decision Q5).

## Working now

- `make test` is green: `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace` (**169 tests**),
  the docs drift guard, the dependency gate, the vector inventory, the
  test-rig wiring check and the plan↔tree close-out audit.
  Evidence receipts for every phase live in `docs/evidence/`; the newest are
  `docs/evidence/s2/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `native.txt`,
  `docs/evidence/s3/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/s4/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/s5/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/s6/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/s7/gates.txt`, `nc.log`, `report.md`, `inventory.txt`.
  ⚠️ AUDIT: the §3 crate graph in `docs/ARCHITECTURE.md` had a cycle
  (`aura-state <- aura-storage` while `aura-storage <- aura-state`); S3
  corrected it to match the real `Cargo.toml` edges.
- **S7 (current).** `aura-storage::backup` gives recovery: a self-verifying
  bundle (`u32le(manifest_len) || manifest || WAL blob`, BLAKE3 manifest
  checksum + SHA-256 blob + per-record BLAKE3) and a restore that **replays**
  every record through the real commit path, so a lying manifest fails with
  `RootMismatch`. **50 `kill -9` + restore cycles** (17 inside the WAL/head
  crash window) -> **0 head, 0 restore, 0 continuation mismatches**, 338
  records replayed. 28 fixture lines; 6 S7 negative controls. 177 tests,
  4,349 vectors. Report: `docs/evidence/s7/report.md`.
  **STATE_PLAN is COMPLETE**: recover from zero to the current `state_root`,
  prove any balance, survive a sudden `kill -9`.
- **S6.** `aura-state::history` + `aura-storage::prune` give
  version retention. `HistoryTree` is an incremental compressed sparse
  Merkle tree with **S1-identical roots** (measured: S1 rebuild = 16.4 ms
  per commit at 1 024 leaves, ~27 min for 100 000 blocks; incremental =
  ~12 hashes per write, ~6.6 us). `RetentionStore` keeps a materialised
  base plus the last `keep_versions` deltas (prune mode) or everything
  (archive mode), and folds deltas into the base on `compact()`.
  **100 000 blocks:** archive 36,500,045 bytes serving every historical
  root and proof; prune (`keep = 128`) peaks at 113,326 bytes —
  **322.1x** smaller and under the analytic bound. A pruned version returns
  `PruneError::Pruned`; archive mode refuses to compact. 35 fixture lines;
  6 S6 negative controls. 169 tests, 4,326 vectors. Report:
  `docs/evidence/s6/report.md`. Build profile: `blake3`/`sha2` are
  `opt-level = 3` in dev builds (measured 5.785 us -> 0.178 us per hash).
- **S5.** `aura-state::sync` gives state sync: a fresh node
  downloads one package (3-head quorum + S4 snapshot + compact delta),
  verifies all SHA-256 checksums, applies the sorted delta and recomputes
  the S1 `state_root`. Package = 16,992 bytes vs full history 998,400
  bytes (58.8x smaller). Substituted diff chunk rejected; 3/3 heads must
  agree. 11 rejection vectors; 6 S5 negative controls. 154 tests, 4,309
  vectors. Report: `docs/evidence/s5/report.md`.
- **S4.** `aura-state::snapshot` gives deterministic snapshots:
  SHA-256 chunk digests, a checksummed manifest with descriptors, a BLAKE3
  `chunk_root` and an S1 `state_root`. Export/import hash chunks in
  parallel (1..8 workers, deterministic output). 1,000,000 keys reimport
  byte-identically; a substituted chunk is rejected. 11 rejection
  vectors; 6 S4 negative controls. Report: `docs/evidence/s4/report.md`.
  146 tests, 4,254 vectors. `sha2 = 0.10.9` became a DIRECT dep of
  `aura-state` (already in the C3 lock via `k256`; no new external crate).
- **S3.** `aura-state::overlay` gives `StateView` +
  `StateDiff` + rollback. A `Revert` or `OutOfGas` leaves `base_root`
  unchanged and the account balance byte-identical; 10 canonical diff
  rejection vectors; 6 S3 negative controls. Report:
  `docs/evidence/s3/report.md`. 138 tests, 4,205 vectors.
- **S2.** `aura-storage` (real API since it was an honest
  placeholder at C0) gives durable state. A `StateBackend` trait with two
  implementations: a pure-Rust WAL + `BTreeMap` backend by default
  (reproducible in any CI), and real RocksDB column families (`state`,
  `nonce`, `meta`, `wal_index`) with one atomic `sync = true` `WriteBatch`
  behind the optional `rocksdb` feature. A framed, BLAKE3-checksummed WAL
  record is fsynced BEFORE head publication; `kill -9` at 10 commit points
  and 10 WAL/head crash-window points all recover the same head and root.
  8 independent WAL rejection vectors; 6 S2 negative controls; 9 new tests.
  Report: `docs/evidence/s2/report.md`.
- **S1.** `aura-state::tree` is a deterministic versioned sparse binary
  Merkle tree over 256-bit `StateKey::digest` paths, with single-leaf
  compression, depth-dependent empty hashes and inclusion proofs bound to a
  block height. 10 000 operations matched an independent Python reference at
  12 versioned roots; 1 000 proofs verified. Report: `docs/evidence/s1/report.md`.
- **S0.** `aura-state::baseline` is a deliberately naive append-only write
  log with last-write-wins lookup, kept as a MEASURED "before" picture:
  `state_root()` → `Err(NoRoot)`, `persist()`/`load()` →
  `Err(NoPersistence)`, `report().deterministic == false`. Two nodes with
  the same writes in opposite order get different balances (300 vs 100
  aalcry in scenario 1). Report: `docs/evidence/s0/baseline-report.md`.
  **Defect intentional** — S1 replaced it. Do not build consensus on it.
- **CORE C0–C10.** Workspace, CI, gates, D0 identity, canonical types
  (`aura-types`), `aura-codec` canonical serialization, `aura-crypto`
  (BLAKE3/Ed25519/secp256k1), `aura-tx` format with replay protection,
  `aura-block` header/body + `tx_root`, compact Merkle proofs (C6), account
  model (C7), genesis CLI (C8), `aura-testkit` (C9) and the plan↔tree
  close-out audit (C10). Every ✅ row has a numbered patch, a `### Cx
  receipt`, a CHANGELOG entry, an NC driver/log and checksummed evidence.
- **Dependencies:** 95 external crates, all with exact `=x.y.z` pins,
  crates.io sources and sha256 checksums. The C3 baseline
  (`docs/spec/c3-dependency-allowlist.txt`) is **frozen** historical
  evidence; C8/C9 addenda live in `docs/spec/`, the S2 addendum in
  `docs/evidence/s2-dependency-addendum.txt` (S2 must not add files to
  `docs/spec/` because its index is checksummed C10 evidence). RocksDB is
  **optional**: default builds need no native library; the `rocksdb` feature
  needs `librocksdb-dev >= 9.9` + `libclang` (Ubuntu 24.04 = 8.9.1 is
  incompatible; CI uses a Debian trixie job). `cargo deny` (licences/
  advisories) is still owed to H1.
- 7 placeholder crates still compile but expose **no API** (an honest
  empty crate, not a fake `Ok(())`). CI runs the same commands as `make
  test`.

## Measured numbers (2026-10-27)

```
find crates -name Cargo.toml       -> 15 crates (+1 workspace root)
ls docs/plans/*_PLAN.md           -> 12 plans / 112 phase rows
cargo test --workspace            -> 154 passed; 0 failed
make test                         -> ALL GATES GREEN (incl. close-out audit)
make vector-inventory             -> 4309 vectors >= 200
make audit-closeout               -> every ✅ has a receipt
make nc-all                       -> 2/2+3/3+4/4+5/5+6/6*13 = 92 NC PASS
make check-deps                   -> 95 external crates allowlisted
revert / OutOfGas                -> root unchanged, balance unchanged
snapshot 1M keys                 -> byte-identical reimport
state sync 3 heads               -> new node matches their root
kill -9 commit points             -> 10 + 10 crash-window, all recover
validators / blocks / tx          -> 0 / 0 / 0
```

Pre-C0 baseline and post-C0/C1/C2/C3/C4/C5/C6/C7/C8/C9/C10/S0/S1/S2
numbers with raw command output are in `docs/evidence/c0-baseline.txt`,
`docs/evidence/c1/gates.txt` … `docs/evidence/c10/gates.txt`,
`docs/evidence/s0/gates.txt`, `docs/evidence/s1/gates.txt` and
`docs/evidence/s2/gates.txt`.

## What is missing (and must not be lost)

- No genesis **block**/execution, no execution overlay / rollback (S3),
  no chunked snapshots (S4), no state sync (S5), no pruning (S6), no
  backup/chaos (S7).
- No VM (X1), no Miri/ASan/UBSan or 24 h libFuzzer campaign (H3), no
  state-root determinism gate (X7), no BLS aggregation (B2), no PQ
  migration (H2), no `cargo deny` (H1). M0 still needs S3 and X0–X2.
- No consensus, no P2P, no node, no RPC.
- No wallet, explorer, contracts, token.
- No audit, no bug bounty, no legal opinion.
- No users and no validators.

Each item above becomes a "✅ with receipt" line only in the patch of its
phase. If a line appears without a receipt, that is a documentation defect and
is fixed by a separate NC (following the example of `MATURITY_AUDIT.md` in
AuraLite-OS).

## How to read plan statuses

| Label | Meaning |
|---|---|
| 📋 PLAN | written, not started |
| 🚧 IN PROGRESS | open phases remain, `make test` green at the last closed one |
| ✅ CLOSED | all phases closed, receipts, NCs, CHANGELOG line present |
| ⚠️ AUDIT | external check found a plan/tree mismatch |
| ⛔ NO-GO | measurement showed the approach does not work (like PERF P4 in the Emulator) |

## Target platforms (goal, not fact)

Linux x86_64 (primary), macOS aarch64 (development), Docker (validators),
WASM (browser light client), later AuraLite OS as a light client
(`ECOSYSTEM_PLAN` E9). None of this has been verified on any platform yet.
