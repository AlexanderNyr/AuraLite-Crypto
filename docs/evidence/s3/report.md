# S3 — execution overlay (measured)

Generated (UTC): 2026-10-27T15:30:54+00:00. Numbers come from the independent Python
reference `tools/gen_s3_vectors.py` and are re-verified by
`crates/aura-state/tests/s3_overlay.rs`.

## What landed

- `StateView`: copy-on-write overlay over an immutable S1 `MerkleTree`.
  Reads fall through to the base tree; writes/deletes are recorded by
  256-bit C7 state-key digest in a `BTreeMap`. Gas is checked with
  `u64` arithmetic; events are collected in order.
- `StateDiff`: canonical wire format
  `payload || BLAKE3(S3_DIFF_CONTEXT || payload)`, where `payload` =
  `ALD3 || version || base_root || result_root || gas_used ||
  events || changes`. Strict decode enforces the checksum, magic,
  minimal LEB128 varints, strictly ascending change paths, caps, no
  trailing bytes, and byte-identical re-encoding.
- `execute()` runs a closure. On `OutOfGas` **or** `Revert` it calls
  `rollback()`: changes, events and gas are discarded. A successful run
  yields a `StateDiff` whose `apply()` re-verifies the base and result
  roots before materialising a new tree. The base is never mutated.

## Measured gates

- base accounts: **3** (C7 `Account`, 88 canonical bytes each)
- base version: **7**
- canonical diffs: **3** (credit, debit+events, mixed writes with delete)
- rejection vectors: **10** (checksum, trailing, truncated, magic, root,
  unsorted, duplicate, non-minimal varint, oversized count, option tag)
- `revert` and `OutOfGas` both leave `base_root` unchanged; the balance
  of the touched account is byte-identical afterwards.
- NC-6 removes `rollback()`: the revert gate MUST go red (balance
  changes) — verified.

## Honest scope

- No real VM: `execute` takes a Rust closure, not WASM. X1 owns the VM.
- `ChainId` is exposed on the view but not signed into the diff yet;
  replay separation is already handled by C3/C4 at the tx layer.
- No snapshots (S4), state sync (S5), pruning (S6) or backup (S7).
- No network, no consensus, no P2P, no node.

Validators / blocks / transactions remain **0 / 0 / 0**.
