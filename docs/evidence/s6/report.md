# S6 — pruning, archive and compaction (measured)

Generated (UTC): 2026-10-28T16:32:18+00:00. Every number below comes from the
independent Python model `tools/gen_s6_vectors.py` (S1 tree rebuild +
canonical byte accounting, no Rust code) and is re-verified by
`crates/aura-storage/tests/s6_prune.rs`.

This report lives in `docs/evidence/s6/`, not `docs/spec/`, because
`docs/spec/FORMAT_INDEX.md` is frozen, checksummed C10 evidence.

## What landed

* `aura-state::HistoryTree` — an incrementally maintained compressed
  sparse Merkle tree with **S1-identical roots**. S1 `MerkleTree`
  rebuilds the whole arena per commit (measured 16.4 ms at 1 024
  leaves = ~27 min for 100 000 blocks); `HistoryTree` touches only the
  changed path (~12 hashes per write).
* `aura-storage::prune` — `RetentionPolicy` (prune / archive),
  `VersionRecord`, canonical `encode_base`, `RetentionStore` with
  `commit`, `root_at`, `state_at`, `proof_at`, `compact` and exact
  disk accounting.

```
record payload = "ALP6" || u64 version || 32 parent_root ||
  32 state_root || v change_count ||
  (v key_len || key || tag || [v value_len || value] || u64 nonce)*
record bytes   = payload || BLAKE3(S6_RECORD_CONTEXT || payload)

base payload   = "ALB6" || u64 version || v entry_count ||
  (32 path || v value_len || value)*
base bytes     = payload || BLAKE3(S6_BASE_CONTEXT || payload)
```

Retention rule, applied after every commit:
`base_version = max(base_version, head - keep_versions)`. A pruned
version returns `PruneError::Pruned { version, oldest }` — never a
plausible-looking wrong state.

## Measured DoD gates

* **100,000 blocks** simulated twice on the same
  deterministic schedule (4 writes per block over
  1024 live keys).
* Archive mode: **36,500,045 bytes** retained
  (base 45 + 100,000 records of
  365 bytes). Serves `root_at`/`proof_at` for
  every height: verified at 12 checkpoints against the Python rebuild
  and by recomputing whole historical states from genesis.
* Prune mode (`keep_versions = 128`): peak disk
  **113,326 bytes**, final **113,326 bytes**, i.e. **322.1x**
  smaller than the archive of the same history. Peak also sits under
  the analytic bound `base_bytes + keep * max_record_bytes`
  (113,326 <= 113,326 bytes — equality is the expected steady state).
* Retention bookkeeping: 99,872 records
  pruned (36,453,280 bytes), exactly
  128 retained deltas, base at version
  99872.
* Compaction folds every retained delta into the base at the head root
  (root unchanged, disk shrinks), and archive mode refuses to compact.
* Head root at 100 000 blocks re-derived by the S1 rebuild and equal to
  the incremental root; record and base bytes byte-identical to the
  Python encoder.

## Negative controls

NC-1 record context, NC-2 base context, NC-3 retention boundary,
NC-4 pruning disabled, NC-5 archive compaction allowed, NC-6 pruned
version faked. 6/6 PASS; total 98 across C0–C10 + S0–S6.

## Build-profile decision (measured)

`[profile.dev.package.blake3]` and `[profile.dev.package.sha2]` are
`opt-level = 3` in the workspace manifest. Measured in a debug build:
`blake3_hash(65 B)` 5.785 us -> 0.178 us (32x). No crate version
changed; `docs/evidence/s6-dependency-addendum.txt` records it.

## Honest scope

* Pruning here is *version retention over the in-process store*; the
  RocksDB column-family compaction (`S2` backend) is unchanged and the
  native backend is still covered only by the `s2-native` CI job.
* No real P2P, no light-client protocol, no backup/chaos (S7), no VM
  (X1). `cargo deny` remains H1.
* The 100 000-block run is a single process on one machine; it is not a
  devnet and produces no blocks on any network.

Validators / blocks / transactions remain **0 / 0 / 0**.
