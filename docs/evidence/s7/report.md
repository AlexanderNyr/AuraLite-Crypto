# S7 — recovery and backup (measured)

Generated (UTC): 2026-10-28T17:05:38+00:00. Every number below comes from real command
output or from the independent Python model `tools/gen_s7_vectors.py`
(which reuses the independent S2 reference for batches, tree and WAL
framing).

This report lives in `docs/evidence/s7/`, not `docs/spec/`, because
`docs/spec/FORMAT_INDEX.md` is frozen, checksummed C10 evidence.

## What landed

* `aura-storage::backup` — `BackupManifest`, `BackupBundle`,
  `create_backup`, `restore_backup`, `RestoreReport`, `BackupError`.
* `aura-storage` binary `s7-chaos` — the `kill -9` harness.

```
bundle         = u32le(manifest_len) || manifest_bytes || blob
manifest bytes = payload || BLAKE3(S7_MANIFEST_CONTEXT || payload)
payload        = "ALM7" || u32 format_version || v-len chain_id ||
                 u64 head_version || 32 head_root || u64 record_count ||
                 u64 blob_len || 32 blob_sha256
blob           = framed S2 WAL records 1..=head (each BLAKE3-checksummed)
blob_sha256    = SHA-256(blob)
```

Verification is layered and every layer must pass: manifest length and
checksum, magic and format version, blob length and SHA-256, per-record
BLAKE3, consecutive versions, parent-root chain, replay of every record
through the real commit path (roots recomputed, not trusted), final head
and root, and `Storage::verify_state_root()` over the leaves.

## Measured DoD gate

* **50 `kill -9` + restore cycles**, commit counts
  1..=12, 17 of them killed inside the WAL/head crash
  window (WAL fsynced, head not yet published).
* Result: **0 head mismatches, 0 restore mismatches, 0 continuation
  mismatches**, 50 recovered stores, 338 WAL records
  replayed in total; every recovered head and root equals the
  independent Python root for that height.
* After each restore, both the recovered and the restored store commit
  the next batch and must produce the same receipt and the same
  Python root — a restored store is deterministic, not merely equal.

## Rejection gates

* Flipped blob byte -> `BlobHashMismatch`.
* Flipped manifest byte -> `ChecksumMismatch`.
* Truncated / trailing bytes -> `Truncated` / `LengthMismatch`.
* Wrong magic -> `BadMagic`; unknown format -> `UnsupportedVersion`.
* Wrong chain-id -> `WrongChainId`.
* Restore into a directory that already holds a store ->
  `TargetNotEmpty` (a restore never merges into live state).
* A fully re-checksummed lie about a record root -> `RootMismatch`
  (caught by replaying the state, NC-3).
* A fully re-checksummed broken parent chain -> `ParentRootMismatch`
  (NC-4).
* A perfectly checksummed manifest that lies about the head ->
  `HeadMismatch`.

## Canonical bytes

* bundle **1261 bytes** (manifest
  144, blob 1113) for a
  3-record store, byte-identical to the Python
  encoder, and reproducible: backing up the same store twice yields the
  same bytes, and backing up a *restored* store reproduces the bundle.

## Negative controls

NC-1 manifest context, NC-2 blob SHA-256, NC-3 replayed root, NC-4
parent chain, NC-5 restore into a live store, NC-6 crash-window replay.
6/6 PASS; total **104** across C0–C10 + S0–S7.

## Honest scope

* Chaos is *process* `kill -9` on one host, not power loss, not network
  partition, not disk corruption, and not a real devnet.
* Backups are single-file bundles; there is no incremental backup, no
  remote storage and no key management yet (OPS_PLAN O-phases).
* The optional RocksDB backend is exercised by the `s2-native` CI job;
  the default reproducible backend is pure Rust.
* No VM (X1), no consensus, no P2P, no node, no wallet.

Validators / blocks / transactions remain **0 / 0 / 0**.
