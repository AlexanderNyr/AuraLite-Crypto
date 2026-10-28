# S5 — state sync (measured)

Generated (UTC): 2026-10-27T19:21:07+00:00. Numbers come from the independent Python
reference `tools/gen_s5_vectors.py` and are re-verified by
`crates/aura-state/tests/s5_sync.rs`.

This report lives in `docs/evidence/s5/`, not `docs/spec/`, because
`docs/spec/FORMAT_INDEX.md` is frozen, checksummed C10 evidence.

## What landed

A fresh node downloads ONE package (`SyncPackage`) instead of the full
block history. The package contains a quorum of three independent
heads, an S4 snapshot (`manifest_bytes || data_blob`) and a compact
delta of net changes between heights 90 and
100.

```
SyncHead payload   = "ALH5" || u32 version || chain_id ||
  u64 height || 32 state_root || u64 snapshot_height || 32 snapshot_root
SyncDelta payload  = "ALD5" || u32 version || u64 from || u64 to ||
  u32 count || (v-len key || key || tag || [v-len value || value])*
SyncPackage payload = "ALS5" || u32 version || u32 head_count ||
  (v-len head || head)* || v-len manifest || manifest ||
  v-len data || data || v-len delta || delta
*_bytes = payload || sha256(CONTEXT || payload)
```

Verification chain: package checksum -> 3 head checksums -> unanimous
quorum (chain-id, height, state_root, snapshot_height, snapshot_root) ->
S4 manifest/chunk digests -> `chunk_root` binding -> apply sorted
delta -> recompute S1 `state_root` -> compare with the quorum.

## Measured gates

- state: **256** C7 keys, snapshot height
  90 -> target height 100.
- package: **16992** bytes (manifest 336,
  data 7940, delta 8252) vs full event history
  **998400** bytes / 25600 events.
- reduction: **58.8x** — the new
  node never downloads the history.
- 3/3 heads agree; a single disagreeing head is rejected.
- State root after sync matches the independent Python S1 root.
- Substituted snapshot data chunk and substituted delta value both
  rejected; bad checksums, magic, truncation and trailing bytes
  rejected; wrong chain-id and wrong height rejected.
- Sync completes within the 60 s budget (measured).

## Negative controls

NC-1 head context, NC-2 delta context, NC-3 package context, NC-4
golden fixture, NC-5 quorum comparison, NC-6 final state-root
comparison. 6/6 PASS; total 92 across C0–C10 + S0–S5.

## Dependencies

No new external crate. `docs/evidence/s5-dependency-addendum.txt`
records the reviewed decision.

## Honest scope

- No real P2P transport or peer discovery (NET N1–N8).
- No pruning/archive (S6), backup/chaos (S7), VM (X1).
- 'Within N minutes' is measured against a 60 s budget in a single
  process; a networked deployment still needs NET phases.

Validators / blocks / transactions remain **0 / 0 / 0**.
