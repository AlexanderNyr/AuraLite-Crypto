# S1 — versioned sparse Merkle tree (measured)

Generated (UTC): 2026-10-26T19:49:11+00:00. Numbers come from the independent Python
reference `tools/gen_s1_vectors.py` and are re-verified by
`crates/aura-state/tests/s1_tree.rs`.

S1 replaces the naive S0 baseline with a deterministic, versioned,
provable state commitment. Binary tree over 256-bit `StateKey::digest`
paths (MSB first), with single-leaf subtree compression and
depth-dependent empty-subtree hashes. `state_root(version, tree_root)`
binds the root to a block height. Inclusion proofs carry sibling
hashes from the leaf up to the root and include the version.

## Measured gates

- operations: **10000** (insert / delete / lookup)
- checkpoints: **12** versioned roots
- proofs verified: **1000**
- final leaves: **2408**
- empty subtree hash depth 0: `a36f69a87c9bb34ce742d9c815118b195cf2c35e09163a34a22decf4d02d2657`
- empty subtree hash depth 256: `b5f27e5a067472f2fee96367c3757feda14b99c64ac906bd7df6a1d4d6f85ed2`

All 12 checkpoint roots match the independent Python implementation.
All 1,000 proof byte strings match, and each proof reconstructs the
versioned state root. Tampering a sibling, value, key or sibling count
is rejected. Insertion order no longer changes the root (the S0 defect
is fixed); deleting a leaf changes the root and re-adding restores it.

## Honest scope

- In memory only: no persistence / WAL / crash recovery (S2).
- No execution overlay or rollback (S3).
- No chunked snapshots, state sync, pruning or backup (S4–S7).
- Values are opaque bytes; account encoding stays the caller's job (C7).
- No network, no blocks, no validators, no VM.

Validators / blocks / transactions remain **0 / 0 / 0**.
