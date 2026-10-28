# S4 — deterministic state snapshots (measured)

Generated (UTC): 2026-10-27T17:49:46+00:00. Numbers come from the independent Python
reference `tools/gen_s4_vectors.py` and are re-verified by
`crates/aura-state/tests/s4_snapshot.rs`.

This report lives in `docs/evidence/s4/`, not `docs/spec/`, because
`docs/spec/FORMAT_INDEX.md` is frozen, checksummed C10 evidence.

## Format

A snapshot is `manifest_bytes || data_blob`. The blob is a
concatenation of independently SHA-256-hashed chunks. The manifest
holds a descriptor per chunk (offset, length, sha256), the S1
`state_root`, a BLAKE3 `chunk_root` over the ordered digests, and a
SHA-256 checksum of the whole manifest payload.

```
manifest payload = "ALS4" || u32 version || varint(len) chain_id ||
  u64 height || 32 state_root || 32 chunk_root || u32 chunk_size ||
  u64 total_keys || u32 chunk_count ||
  (u64 offset || u64 length || 32 sha256(chunk))*count
manifest bytes  = payload || sha256(SNAPSHOT_MANIFEST_CONTEXT || payload)
chunk payload   = varint(count) || (varint(len) key || key ||
                 varint(len) value || value)*count
chunk hash      = sha256(SNAPSHOT_CHUNK_CONTEXT || chunk payload)
chunk_root      = BLAKE3(SNAPSHOT_ROOT_CONTEXT || u32 count || concat(hashes))
state_root      = S1 versioned_root(1, sparse_merkle_root(C7 key -> value))
```

Export and import hash chunks in parallel via `std::thread::scope`
with an atomic work index and a bounded worker pool (1..8 workers).
Output is deterministic: descriptor order is input chunk order.

Protection comes from strict canonical decode (minimal varints, magic,
version, contiguous offsets, caps, no trailing bytes, byte-identical
re-encode) plus SHA-256 manifest/chunk digests. A substituted chunk is
rejected by NC-6.

## Measured gates

- canonical snapshot: **7** C7 account keys in
  **3** chunks (chunk_size=3)
- **1,000,000 keys** exported and reimported byte-identically;
  data blob = **31000048** bytes in
  **16** chunks; SHA-256 and chunk_root match the
  independent Python reference computed without writing 30 MB of hex.
- parallel hashing: **1024** keys /
  **128** per chunk; identical digests for 1/2/4/8
  workers.
- rejection vectors: substituted chunk, bad manifest checksum/magic,
  bad chunk count/root, truncated/trailing/empty data, unsorted and
  duplicate keys.

## Dependencies

`sha2 = 0.10.9` was already in `Cargo.lock` via `k256` and in the
frozen C3 baseline allowlist. S4 only makes it a DIRECT dependency of
`aura-state` (manifest checksum + chunk digests). **No new external
crate.** `docs/evidence/s4-dependency-addendum.txt` records the decision.

## Honest scope

- No state sync (S5), pruning (S6), backup/chaos (S7).
- No network, no consensus, no P2P, no node, no VM.
- The 1M gate uses `Hash::ZERO` as the S1 state_root placeholder: the
  Python reference computes the S1 root only for the small snapshot.

Validators / blocks / transactions remain **0 / 0 / 0**.
