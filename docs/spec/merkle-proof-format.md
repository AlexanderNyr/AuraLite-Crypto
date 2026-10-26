# Merkle proof format — C6 specification

**Status:** ✅ implemented in `crates/aura-block/src/proof.rs` (2026-10-25).
**Authority:** [`CORE_PLAN.md`](../plans/CORE_PLAN.md) C6. Depends on
[`block-format.md`](block-format.md) §3 — **the C5 tree shape is unchanged**.

## 1. What a proof commits to

The C5 `tx_root` is a binary BLAKE3 Merkle tree over `leaf_hash(txid)`.
C6 adds proofs for two leaf kinds over the **same** tree shape:

```
tx leaf    = BLAKE3(LEAF_CONTEXT       ‖ txid[32])          (C5)
event leaf = BLAKE3(EVENT_LEAF_CONTEXT ‖ u64(len) ‖ event)  (C6)
node       = BLAKE3(NODE_CONTEXT       ‖ left ‖ right)      (C5)
```

A distinct `EVENT_LEAF_CONTEXT` plus an explicit length prefix means a
transaction leaf can never be replayed as an event leaf and two events with
ambiguous concatenation cannot collide. `compute_tx_root` (C5) and
`build_tx_tree` (C6) now share **one** tree implementation, so a root and a
proof can never disagree about padding.

## 2. Tree construction

Levels are stored padded: at every level, if the node count is odd and > 1,
the final node is appended again. Then consecutive pairs are hashed with
`node_hash`. A single leaf is the root; an empty tree is `Hash::ZERO`.

Example, 3 leaves `[a, b, c]`:

```
level 0: [a, b, c, c]        (c duplicated)
level 1: [H(a,b), H(c,c)]
level 2: [H(H(a,b), H(c,c))]  = root
```

`depth(total)` is the number of levels above the leaves:

```
len = total; depth = 0
while len > 1: len = ceil(len / 2); depth += 1
```

For 1000 leaves the depth is **10**.

## 3. Wire format (compact)

```
MerkleProof = varint(index)        // 0-based leaf index, original tree
           ‖ varint(total)         // original leaf count, before padding
           ‖ leaf[32]             // hash of the leaf being proven
           ‖ varint(sibling_count)
           ‖ sibling[32] * count  // bottom-up, raw hashes, no lengths
```

* All varints are **minimal unsigned LEB128** (C2 `read_varint`);
* sibling direction is **derived** from the index — no direction byte is
  stored: at level `L` the node sits at `position = index >> L`, and its
  sibling is at `position ^ 1`;
* `sibling_count ≤ MAX_PROOF_DEPTH = 64` (a `u64` leaf count needs at most
  64 levels);
* decoding uses `decode_canonical()`, so trailing bytes, non-minimal varints
  and re-encode mismatches are rejected.

Size for 1000 leaves: ~6 bytes of varints + 32 + 10 × 32 ≈ **358 bytes**.

## 4. Verification

`verify_proof(root, proof)`:

1. `total != 0` → else `EmptyTree`;
2. `index < total` → else `IndexOutOfBounds`;
3. `siblings.len() == depth(total)` → else `WrongSiblingCount`;
4. starting from `proof.leaf`, fold the siblings:
   `computed = (position & 1) == 0 ? node_hash(computed, sibling)
                                   : node_hash(sibling, computed)`,
   then `position /= 2`;
5. `computed == root` → else `RootMismatch`.

`verify_tx_inclusion(root, txid, proof)` and
`verify_event_inclusion(root, event, proof)` first recompute the expected
leaf and compare it to `proof.leaf` (`LeafMismatch`), then run
`verify_proof`. Without that binding, a proof would only show that *some*
leaf hash is in the tree — not the value the caller cares about.

## 5. Golden vectors and negative controls

`tools/gen_c6_vectors.py` is independent Python with pure-Python BLAKE3
(self-checked against 7 official BLAKE3 vectors) and a transcription of the
C5 tree + C6 proof encoding. `c6_golden.txt` has 69 lines:

* roots and depths for **1 / 2 / 3 / 5 / 8 / 1000** transaction leaves;
* roots for **1 / 3 / 5** event leaves;
* **34 canonical proofs** (all leaves for trees ≤ 8, and indices
  0, 1, 2, 499, 998, 999 for 1000 leaves);
* **11 rejection vectors**: substituted leaf, wrong sibling, wrong index,
  wrong total, truncated siblings, empty total, out-of-bounds index,
  depth overflow, trailing byte, truncation, non-minimal index varint.

The Rust gate additionally builds all **1000** proofs and verifies every one,
then substitutes one leaf and confirms rejection (the C6 DoD gate).

`tools/nc_c6.py` runs 6 negative controls: event-leaf context, a golden
fixture, leaf binding, sibling-count check, sibling direction and root
comparison. Total across C0–C6: **32 NC**.

## 6. Non-goals at C6

Multi-proofs / batch compression, sparse Merkle trees for state (S1),
receipt roots and event indexing (X/D phases), persisted proof caches,
proofs served over RPC (N/D phases). C6 proves membership in an in-memory
tree built from values the caller already has.
