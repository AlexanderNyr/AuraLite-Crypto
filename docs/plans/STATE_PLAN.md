# STATE_PLAN — state tree and storage

**Status:** ✅ COMPLETE (S0–S7 ✅, closed 2026-10-28)  
**Stage:** Stage 1: S0–S7 (all phases active).  
**Deliverable:** `patches/0012-S0-...` … `patches/0019-S7-...` (all 8 patches cut)
**Dependencies:** CORE C1–C7 (codec, hash, accounts).

## Problem

"State in an in-memory `HashMap`" works on one node and breaks the network:
there is no `state_root`, nothing to sync, nothing to prove to a light client,
and everything disappears after `kill -9`. We need a versioned Merkle tree with
atomic persistence and snapshots.

## Decisions

- **D-S1.** JMT-like binary Merkle tree: `key = BLAKE3(namespace || address || slot)`, version = block height.
- **D-S2.** RocksDB as the only permanent store, WAL with fsync **before** publishing the new head.
- **D-S3.** An overlay / "state view" for execution: a tx sees only its own copy; commit → new root.
- **D-S4.** Snapshots split into chunks with a sha256 manifest; state sync verifies every chunk.
- **D-S0 (S0).** The baseline is a MEASUREMENT, not a design: an append-only
  write log with last-write-wins lookup, no `state_root`, no persistence and
  a documented insertion-order defect. It must not be used by consensus code.
  S1 replaces it with a deterministic versioned Merkle tree keyed by
  `StateKey::digest`. S0's report lives in `docs/evidence/s0/`, not
  `docs/spec/`, because the frozen C10 spec index is checksummed evidence.
- **D-S1a (S1).** The S1 tree is a **sparse binary Merkle tree** over the
  256 bits of `StateKey::digest`, MSB first, with single-leaf subtree
  compression and depth-dependent empty-subtree hashes. It is deliberately
  in-memory: no RocksDB (S2), no overlay (S3), no snapshots (S4). The version
  is bound into `state_root(version, tree_root)` and into every proof, so a
  proof cannot be replayed against a different height.


**C7 note (2026-10-26).** `CORE_PLAN` C7 gave `aura-state` its first real API:
the `Account` value and the `StateKey`/`KeySpace` layout
(`docs/spec/account-model.md`). This plan's S1 still owes the versioned Merkle
tree, insert/delete/lookup, proofs and `state_root`; C7 deliberately did not
implement any of them. S1 must consume `StateKey::digest` as its JMT path.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| ✅ **S0** | Baseline | in-memory state, measure "no root / no persistence", report | NC: two nodes with different insertion order → different balances (documented defect) |
| ✅ **S1** | Merkle tree | insert/delete/lookup, root, proof, versioning | 10 000 operations, root matches an independent reference implementation; proof check on 1000 keys |
| ✅ **S2** | RocksDB + WAL | column families, atomic batch, fsync, reopen | kill -9 at 10 commit points → after restart head and root match |
| ✅ **S3** | Execution overlay | `StateView`, diff, rollback on `OutOfGas`/revert | revert does not change root; NC: remove rollback → balance changed, test red |
| ✅ **S4** | Snapshots | chunks, manifest, parallel export/import | snapshot of 1M keys imported byte-identical; substituted chunk → reject |
| ✅ **S5** | State sync | node catches up via snapshot + diff without downloading all history | new node reaches the same root as 3 old ones within N minutes |
| ✅ **S6** | Pruning + archive | pruning old versions, archive mode, compaction | 100 000 blocks: disk size bounded in prune mode; archive serves history |
| ✅ **S7** | Recovery and backup | backup/restore, checksum verification, chaos gate | 50 `kill -9` + restore cycles, zero root mismatches |


### S0 receipt (2026-10-26)

- Deliverable: `patches/0012-S0-state-baseline.patch`. No new crate, no new
  external dependency — the baseline lives in `aura-state::baseline`.
- Landed: `BaselineState` (append-only `Vec<BaselineWrite>` with
  last-write-wins lookup), `BaselineWrite`, `BaselineError`, `BaselineReport`
  and `FINAL_VIEW_CONTEXT`. `state_root()` returns `Err(NoRoot)`, `persist()`
  and `load()` return `Err(NoPersistence)`, and `report().deterministic`
  is `false`. The module is explicitly documented as a measurement that S1
  must replace — not a design to build on.
- Measured defects, in `docs/evidence/s0/baseline-report.md`: no state root,
  no persistence, and two nodes applying the same writes in opposite order
  end with different balances (scenario 1: 300 vs 100 aalcry). A disjoint-key
  scenario is order-independent, proving the defect is last-write-wins on
  conflicting keys, not a false positive.
- Golden vectors: `tools/gen_s0_vectors.py` — independent Python model of
  the write log and final view. 57 fixture lines covering 4 scenarios
  (boundary `u64`/`u128` values included); generator self-checks that
  conflicting writes diverge and disjoint writes do not. Offline,
  byte-stable. Vector inventory: **1006** across 10 fixtures.
- Gate: `make test` → **117 passed / 0 failed** (109 CORE + 8 S0),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate 71
  external crates, `make audit-closeout` green.
- NC: `make nc-s0` → **6/6 PASS**: final-view context, golden final view,
  fake root, fake persistence, fake determinism, falsified no-root
  measurement. Total C0–C10 + S0: **62 NC**.
- Evidence: `docs/evidence/s0/gates.txt`, `nc.log`, `baseline-report.md`,
  `inventory.txt`, `SHA256SUMS`.
- Audit update: `tools/audit_closeout.py` now accepts ✅ rows in
  `STATE_PLAN.md` (plan ownership keyed by phase prefix), requires an S0
  receipt/driver/log/patch/evidence, and expects 62 NCs. The frozen CORE
  `closeout.txt` still reports its own 56.
- Deferred to S1: JMT-like versioned Merkle tree with insert/delete/lookup,
  root, proofs and versioning, verified against an independent reference
  over 10,000 operations. Then S2 (RocksDB + WAL), S3 (overlay), S4–S7.


### S1 receipt (2026-10-26)

- Deliverable: `patches/0013-S1-merkle-tree.patch`. No new crate, no new
  external dependency — `aura-state::tree` is a new module.
- Landed: `MerkleTree` with `insert`/`remove`/`get`, versioned `commit()`,
  `tree_root()`, `state_root()`, `root_at()`, `prove`, `prove_many_at` and
  `prove_at`; `MerkleProof` with canonical `to_bytes()`; `verify_proof` and
  `verify_inclusion`; `TreeError`. Paths are C7 `StateKey::digest()`.
- Tree shape: binary over 256-bit paths, MSB first, single-leaf subtree
  compression, `empty_hash(depth)`, `leaf_hash(depth, path, value)`,
  `node_hash(left, right)`, and `versioned_root(version, root)`. Proofs carry
  sibling hashes bottom-up and the version; verification reconstructs the
  versioned root, so a proof is bound to a height.
- **DoD gates:** 10,000 deterministic operations (insert/delete/lookup) with
  **12 versioned checkpoint roots** matching an independent Python reference;
  **1,000 inclusion proofs** byte-compared and verified; tampered sibling,
  value, key and truncated siblings rejected.
- Golden vectors: `tools/gen_s1_vectors.py` — pure-Python BLAKE3
  (self-checked against 7 official vectors) plus an independent sparse-tree
  implementation. 3,066 fixture lines; offline and byte-stable. Vector
  inventory rises to **4,061** across 11 fixtures.
- Performance fix found by the gate: the first proof implementation rebuilt
  the node arena for each of 1,000 keys (42 s). `prove_many_at` now builds the
  arena once — the whole S1 suite runs in ~1 s in debug mode.
- Correction found by the gate: proofs initially verified against the raw
  tree root while the published commitment is `versioned_root(...)`. The proof
  now carries the version and verification applies `versioned_root` before
  comparing. Fixed in code, not papered over.
- Gate: `make test` → **123 passed / 0 failed** (117 CORE+S0 + 6 S1),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate 71
  external crates, `make audit-closeout` green.
- NC: `make nc-s1` → **6/6 PASS**: leaf context, node context, independent
  checkpoint root, proof root comparison, path bit ordering, depth-independent
  empty subtrees. Total C0–C10 + S0 + S1: **68 NC**.
- Evidence: `docs/evidence/s1/gates.txt`, `nc.log`, `report.md`,
  `inventory.txt`, `SHA256SUMS`. Report is in evidence (not `docs/spec/`)
  because the C10 spec index is frozen, checksummed evidence.
- Deferred: RocksDB + WAL and crash recovery (S2), execution overlay (S3),
  snapshots/state sync/pruning/backup (S4–S7), VM (X1), `cargo deny` (H1).


### S2 receipt (2026-10-26)

- Deliverable: `patches/0014-S2-rocksdb-wal.patch`. `aura-storage` gains a
  real API (it was an honest placeholder since C0).
- Landed: a `StateBackend` trait with **two** implementations. The default is
  a pure-Rust WAL + `BTreeMap` backend, so `make test` is reproducible
  anywhere. The `rocksdb` Cargo feature enables the real RocksDB backend
  with four column families (`state`, `nonce`, `meta`, `wal_index`) and one
  atomic `sync = true` `WriteBatch`.
- **WAL-before-head protocol:** a framed record
  `varint(len) || payload || BLAKE3(S2_WAL_CONTEXT || payload)` is appended
  and `fsync`ed BEFORE head publication. `payload` = magic `ALW1`, version,
  parent root, state root, ordered changes with per-key nonces. A checksummed
  `head.bin` is written atomically (tmp + rename + directory fsync).
  `Storage::open` replays WAL records newer than the published head, so
  `kill -9` between WAL fsync and publish is recoverable.
- **DoD gates:** `kill -9` at 10 commit points AND at 10 WAL/head crash-window
  points → after restart head and root match the independent golden roots.
  Two backends tested (default pure-Rust + optional real RocksDB).
- Golden vectors: `tools/gen_s2_vectors.py` — pure-Python BLAKE3
  (self-checked against 7 official vectors), the S1 sparse Merkle tree and
  the S2 WAL framing. 11 golden commits, 110 fixture lines, 8 independent
  rejection vectors (bad checksum/magic/truncated/trailing/non-sequential/
  parent/root/value).
- Native dependency: `rocksdb = 0.23.0`, `librocksdb-sys = 0.17.1+9.9.3`,
  pinned exactly and linked **dynamically** to the system `librocksdb-dev`
  (the bundled C++ build needs GB of RAM and is disabled). 24 new external
  crates recorded in `docs/evidence/s2-dependency-addendum.txt`. Ubuntu 24.04
  ships librocksdb 8.9.1, whose C API is incompatible with 0.23; CI runs the
  native job on Debian trixie (9.10). Default builds need no native library.
- Gate: `make test` → **132 passed / 0 failed** (123 S1 + 9 S2), fmt/clippy
  clean, drift guard 12 plans / 112 rows, dependency gate 95 external crates.
- NC: `make nc-s2` → **6/6 PASS**: WAL context, key context, root context,
  golden WAL record, checksum verification, WAL-before-head invariant.
  Total C0–C10 + S0–S2: **74 NC**.
- Evidence: `docs/evidence/s2/gates.txt`, `nc.log`, `report.md`,
  `inventory.txt`, `native.txt`, `SHA256SUMS`.
- Deferred: execution overlay / rollback (S3), chunked snapshots (S4), state
  sync (S5), pruning (S6), backup/chaos (S7).


### S3 receipt (2026-10-27)

- Deliverable: `patches/0015-S3-execution-overlay.patch`. No new crate and no
  new external dependency — the overlay lives in `aura-state`, which already
  owns `MerkleTree`, `Account` and `StateKey`.
- Landed: `StateView` (copy-on-write overlay over an immutable S1 tree),
  `StateDiff` (canonical wire format + strict decoder), `events_root()`,
  `execute()` and `apply_diff()`. Reads fall through to the base tree; writes
  and deletes are keyed by the C7 state-key digest in a `BTreeMap`.
- **Wire format:** `payload || BLAKE3(S3_DIFF_CONTEXT || payload)` with
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
- Gate: `make test` → **138 passed / 0 failed** (132 S2 + 6 S3), fmt/clippy
  clean, drift guard 12 plans / 112 rows, dependency gate 95 external crates,
  `make audit-closeout` green.
- Deferred: real VM/WASM (X1), snapshots (S4), state sync (S5), pruning
  (S6), backup/chaos (S7), `cargo deny` (H1).


### S4 receipt (2026-10-27)

- Deliverable: `patches/0016-S4-snapshots.patch`. No new crate; no new
  external dependency. `sha2 = 0.10.9` was already in `Cargo.lock` via
  `k256` and in the frozen C3 baseline; S4 only makes it a DIRECT
  dependency of `aura-state` for SHA-256 manifest and chunk digests.
  `docs/evidence/s4-dependency-addendum.txt` records the decision.
- Landed: `aura-state::snapshot` — `SnapshotManifest`, `ChunkDescriptor`,
  `Snapshot`, `export_snapshot`, `import_snapshot`, `encode_chunk`,
  `decode_chunk`, `chunk_hash`, `chunk_root`, `state_root`,
  `sha256_bytes`, `parallel_chunk_hashes` and `SnapshotError`.
- **Format:** `manifest_bytes || data_blob`. The blob is a concatenation of
  canonical chunk payloads, each with its own
  `SHA256(SNAPSHOT_CHUNK_CONTEXT || payload)`. The manifest carries descriptor
  triples `(offset, length, sha256)`, the S1 `state_root`, a BLAKE3
  `chunk_root` over the ordered digests and a SHA-256 checksum of the full
  payload. Strict decode enforces magic, version, minimal varints,
  contiguous offsets, caps, no trailing bytes, byte-identical re-encoding
  and a recomputed `chunk_root`.
- **Parallel export/import.** `std::thread::scope`, an atomic work index
  and a bounded worker pool (1..8). Output is deterministic: descriptor
  order is input chunk order. Verified identical digests and chunk_root for
  1/2/4/8 workers.
- **DoD gate:** 1,000,000 keys exported to 16 chunks (31,000,048 data
  bytes) and reimported **byte-identically**; data SHA-256 and chunk_root
  match the independent Python reference. NC-6 disables the digest
  comparison and requires the substituted-chunk rejection test to go red.
  6 S4 negative controls, total 86 across C0–C10 + S0–S4.
- Golden vectors: `tools/gen_s4_vectors.py` — hashlib SHA-256, pure-Python
  BLAKE3 (self-checked against 7 official vectors), the C7 state-key digest
  and the S1 sparse Merkle tree. 7-key canonical snapshot (3 chunks),
  parallel reference (1024 keys / 8 chunks), 1M-key gate digest, 11
  rejection vectors. Offline and byte-stable; 58 fixture lines.
- Gate: `make test` → **146 passed / 0 failed** (138 S3 + 8 S4),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate 95
  external crates, `make audit-closeout` green.
- Deferred: state sync (S5), pruning (S6), backup/chaos (S7), real VM
  (X1), `cargo deny` (H1).


### S5 receipt (2026-10-27)

- Deliverable: `patches/0017-S5-state-sync.patch`. No new crate, no new
  external dependency. State sync lives in `aura-state::sync`.
- Landed: `SyncHead`, `SyncDelta`, `SyncPackage`, `SyncResult`, `SyncError`,
  and the strict canonical decoders for heads, deltas and packages.
- **Protocol:** a fresh node downloads ONE package (three independent heads +
  S4 snapshot + a compact delta of net changes) instead of the full event
  history. All objects carry domain-separated SHA-256 checksums. The quorum
  must be unanimous on chain-id, height, `state_root`, snapshot height and
  `snapshot_root`. The S4 `chunk_root` is bound to the head. Delta changes
  must be strictly ascending; the recomputed S1 `state_root` must equal the
  quorum root.
- **DoD gate:** 256-key state, snapshot height 90 -> target height 100;
  package 16,992 bytes vs full history 998,400 bytes (**58.8x smaller**).
  New node root matches 3 old nodes; a substituted snapshot data chunk and a
  substituted delta value are rejected; bad checksums/magic/truncation/
  trailing bytes, wrong chain-id and wrong height rejected. Sync completes
  within the measured 60 s budget. NC-6 disables the final root comparison
  and requires the substituted-diff rejection test to go red.
- Golden vectors: `tools/gen_s5_vectors.py` — pure-Python BLAKE3, hashlib
  SHA-256, C7 state-key digest, S1 sparse Merkle tree and S4 snapshot format.
  48 fixture lines, offline and byte-stable.
- Gate: `make test` → **154 passed / 0 failed** (146 S4 + 8 S5), fmt/clippy
  clean, drift guard 12 plans / 112 rows, dependency gate 95 external crates,
  `make audit-closeout` green. 6 S5 negative controls, total 92 across
  C0–C10 + S0–S5.
- Deferred: pruning/archive (S6), backup/chaos (S7), real VM (X1), `cargo
  deny` (H1).

### S6 receipt (2026-10-28)

- Deliverable: `patches/0018-S6-pruning-archive.patch`. No new crate and no
  new external dependency — the incremental tree lives in `aura-state::history`
  and the retention layer in `aura-storage::prune`.
- **Measured problem first.** S1 `MerkleTree` rebuilds the whole node arena on
  every `commit()`: 16.4 ms per commit at 1 024 leaves (5 000 commits timed),
  i.e. ~27 minutes for 100 000 blocks. A pruner needs a root per block, so S6
  had to make the tree incremental *before* it could measure retention.
- Landed `HistoryTree`: a compressed sparse Merkle tree held in a mutable
  arena, updating only the changed path. Roots are bit-for-bit S1
  (`empty_hash`, `leaf_hash`, `node_hash`, `versioned_root`), verified against
  `MerkleTree` rebuilds at every checkpoint. Measured cost: **~12 hashes per
  write**, ~6.6 us per write in a debug build (from 16.4 ms per commit).
  Two defects were found and fixed by the cross-check gates: a branch prefix
  computed from the parent instead of the child (sibling slots collided) and a
  return value that kept the deepest branch hash instead of the root.
- Landed `aura_storage::prune`: `RetentionMode`, `RetentionPolicy`
  (`prune(keep)` / `archive()`), `VersionRecord`, `CompactionReport`,
  `RetentionReceipt`, `RetentionStore` with `commit`, `root_at`, `state_at`,
  `proof_at`, `verify_root_at`, `compact`, exact byte accounting and
  `PruneError`.
- **Wire formats:** `record bytes = payload || BLAKE3(S6_RECORD_CONTEXT ||
  payload)` with `payload = ALP6 || u64 version || 32 parent_root ||
  32 state_root || varint count || changes` (S2 change shape, own magic and
  domain); `base bytes = payload || BLAKE3(S6_BASE_CONTEXT || payload)` with
  `payload = ALB6 || u64 version || varint count || entries`. Strict decode
  enforces checksum, magic, caps, no trailing bytes and byte-identical
  re-encoding.
- **Retention rule:** `base_version = max(base_version, head - keep_versions)`
  after every commit. A pruned version returns
  `PruneError::Pruned { version, oldest }` — never a plausible wrong state.
  Archive mode never prunes and refuses to compact
  (`ArchiveCompactionForbidden`).
- **DoD gates:** 100 000 blocks simulated twice on one deterministic schedule
  (4 writes per block over 1 024 keys). Archive retains **36 500 045 bytes**
  and serves `root_at`/`proof_at` for every height (12 checkpoints against the
  Python rebuild plus whole-state recomputation). Prune mode
  (`keep_versions = 128`) peaks at **113 326 bytes** and stays under the
  analytic bound `base_bytes + keep * max_record_bytes` — **322.1x** smaller
  than the archive of the same history. Compaction folds the retained deltas
  into the base without moving the root. Record and base bytes are
  byte-identical to the independent Python encoder.
- Golden vectors: `tools/gen_s6_vectors.py` — pure-Python BLAKE3
  (self-checked against 7 official vectors), C7 key digest, S2 key path, a
  from-scratch S1 rebuild, exact canonical byte accounting for all 100 000
  blocks and canonical hex for one record, one later record and one base
  snapshot. 35 fixture lines, offline and byte-stable (generator runs in ~52 s
  because it rebuilds the tree 12 times in pure Python).
- Gate: `make test` -> **169 passed / 0 failed** (154 S5 + 9 S6 storage +
  6 S6 tree), fmt/clippy clean, drift guard 12 plans / 112 rows, dependency
  gate 95 external crates, vector inventory 4 326 across 16 fixtures,
  `make audit-closeout` green.
- Build-profile decision: `[profile.dev.package.blake3]` and
  `[profile.dev.package.sha2]` are `opt-level = 3`. Measured
  `blake3_hash(65 B)`: 5.785 us -> 0.178 us in a debug build (32x). No crate
  version changed; recorded in `docs/evidence/s6-dependency-addendum.txt`.
- NC: `make nc-s6` -> **6/6 PASS**: record context, base context, retention
  boundary (keep + 1), pruning disabled, archive compaction allowed, pruned
  version faked. Total C0–C10 + S0–S6: **98 NC**.
- Evidence: `docs/evidence/s6/gates.txt`, `nc.log`, `report.md`,
  `inventory.txt`, `SHA256SUMS`, `docs/evidence/s6-dependency-addendum.txt`.
  The report lives in evidence, not `docs/spec/`, because
  `docs/spec/FORMAT_INDEX.md` is frozen checksummed C10 evidence.
- Deferred: backup/restore + chaos gate (S7), real VM/WASM (X1), `cargo deny`
  (H1), RocksDB column-family compaction tuning (S2 backend unchanged).

### S7 receipt (2026-10-28)

- Deliverable: `patches/0019-S7-recovery-backup.patch`. No new crate and no
  new external dependency — the bundle lives in `aura-storage::backup`, the
  harness is a second `[[bin]]` (`s7-chaos`) of the same crate.
- Landed `BackupManifest`, `BackupBundle`, `create_backup`, `restore_backup`,
  `RestoreReport`, `BackupError`, plus `Storage::wal_records()`.
- **Bundle format:** `u32le(manifest_len) || manifest_bytes || blob`, with
  `manifest_bytes = payload || BLAKE3(S7_MANIFEST_CONTEXT || payload)`,
  `payload = ALM7 || u32 format_version || chain_id || u64 head_version ||
  32 head_root || u64 record_count || u64 blob_len || 32 blob_sha256`, and
  `blob` = the S2 framed, BLAKE3-checksummed WAL records 1..=head. The blob
  digest is SHA-256 (via S4's `sha256_bytes`), so the bundle is verifiable by
  any tool that knows SHA-256.
- **Restore replays, it does not trust.** Every record goes through the real
  `Storage::commit` path, so the declared roots are *recomputed*; a backup
  whose manifest is perfectly checksummed but whose roots lie fails with
  `RootMismatch`. Restore also refuses a non-empty target
  (`TargetNotEmpty`), the wrong chain-id (`WrongChainId`) and a broken
  parent chain (`ParentRootMismatch`).
- **DoD gate:** **50 `kill -9` + restore cycles**, commit counts 1..=12, 17 of
  them killed inside the WAL/head crash window (record fsynced, head not
  published). Every cycle: reopen, compare head + root with the independent
  Python fixture, back up, restore into a fresh directory, compare head, root
  and the whole WAL record-for-record, then commit the next batch in both
  stores and require the same receipt. Result: **0 head mismatches,
  0 restore mismatches, 0 continuation mismatches**, 338 records replayed.
- Golden vectors: `tools/gen_s7_vectors.py` — reuses the independent S2
  reference (batches, S1 tree, WAL framing) and adds the S7 manifest/bundle.
  28 fixture lines including the canonical bundle hex (1 261 bytes for a
  3-record store), the manifest hex, the blob SHA-256 and the S2 root for
  every reachable height. Offline, byte-stable, ~0.3 s.
- Gate: `make test` -> **177 passed / 0 failed** (169 S6 + 7 S7 + 1 unit),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate 95
  external crates, vector inventory 4 355 across 17 fixtures,
  `make audit-closeout` green.
- NC: `make nc-s7` -> **6/6 PASS**: manifest context, blob SHA-256, replayed
  root, parent chain, restore into a live store, crash-window replay.
  Total C0–C10 + S0–S7: **104 NC**.
- Evidence: `docs/evidence/s7/gates.txt`, `nc.log`, `report.md`,
  `inventory.txt`, `SHA256SUMS`, `docs/evidence/s7-dependency-addendum.txt`.
- ⚠️ AUDIT: S7 closed the plan, which invalidated C10 NC-2's assumption that a
  pending S row always exists. `tools/nc_c10.py` now falls back to removing
  the newest `### Sx receipt` heading and stays 6/6; the frozen
  `docs/evidence/c10/nc.log` was not rewritten.
- **STATE_PLAN is now COMPLETE.** Closure criterion is met and measured: any
  node can recover from zero to the current `state_root`, prove any account
  balance with a Merkle proof, and survive a sudden power loss (process
  `kill -9`, the strongest signal available without a VM).
- Deferred: distributed/power-loss chaos, incremental and remote backups,
  key management (OPS_PLAN), RocksDB column-family tuning, VM (X1),
  `cargo deny` (H1). Next plan in dependency order: `EXEC_PLAN` X0.

## Risks

- Tree root disagreeing with the "reference" implementation → caught by an
  independent test vector (S1), not by trusting our own library.
- Data loss from fsync ordering → S2/S7.
- State bloat → S6 + size metrics in `PERF_PLAN`.

## Non-goals

Contract execution (EXEC), block format (CORE), network transfer of chunks
(NET N5 — only a consumer of the API).

## Closure criteria

Any node can recover from zero to the current `state_root`, prove any account
balance with a Merkle proof, and survive a sudden power loss.
