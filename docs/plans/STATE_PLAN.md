# STATE_PLAN — state tree and storage

**Status:** 📋 PLAN (S0–S7)  
**Stage:** Stage 1: S0–S7 (all phases active).  
**Deliverable:** `patches/0012-S0-...` … `patches/0019-S7-...`
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


**C7 note (2026-10-26).** `CORE_PLAN` C7 gave `aura-state` its first real API:
the `Account` value and the `StateKey`/`KeySpace` layout
(`docs/spec/account-model.md`). This plan's S1 still owes the versioned Merkle
tree, insert/delete/lookup, proofs and `state_root`; C7 deliberately did not
implement any of them. S1 must consume `StateKey::digest` as its JMT path.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| **S0** | Baseline | in-memory state, measure "no root / no persistence", report | NC: two nodes with different insertion order → different balances (documented defect) |
| **S1** | Merkle tree | insert/delete/lookup, root, proof, versioning | 10 000 operations, root matches an independent reference implementation; proof check on 1000 keys |
| **S2** | RocksDB + WAL | column families, atomic batch, fsync, reopen | kill -9 at 10 commit points → after restart head and root match |
| **S3** | Execution overlay | `StateView`, diff, rollback on `OutOfGas`/revert | revert does not change root; NC: remove rollback → balance changed, test red |
| **S4** | Snapshots | chunks, manifest, parallel export/import | snapshot of 1M keys imported byte-identical; substituted chunk → reject |
| **S5** | State sync | node catches up via snapshot + diff without downloading all history | new node reaches the same root as 3 old ones within N minutes |
| **S6** | Pruning + archive | pruning old versions, archive mode, compaction | 100 000 blocks: disk size bounded in prune mode; archive serves history |
| **S7** | Recovery and backup | backup/restore, checksum verification, chaos gate | 50 `kill -9` + restore cycles, zero root mismatches |

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
