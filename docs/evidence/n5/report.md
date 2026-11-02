# N5 — State sync: S4 chunks, manifest, root verification, and corrupt-chunk rollback evidence receipt

Generated (UTC): 2026-11-01T10:36:56+00:00.

## Delivered behavior

`aura-p2p` implements the **N5** state-sync layer (`aura_p2p::state_sync`)
integrating with S4 (`aura-state`) and updates `aura_p2p::baseline` to report
5 implemented network components (`transport`, `gossipsub`, `discovery`,
`block-sync`, `state-sync`) and 3 remaining placeholders (`dos-defence` N6 ..
`chaos-harness` N8):

- **Header-first certified anchor (`D-N4`, `StateSyncSession::from_certified_header`)**:
  verifies the target `SyncedHeader` and `SyncQc` against `SyncValidatorSet`
  before accepting any S4 manifest or chunk.
- **S4 manifest verification (`StateSyncSession::accept_manifest`)**: decodes
  and verifies the canonical S4 `SnapshotManifest` (`ALS4` magic, SHA-256
  checksum, `chunk_root`, `chain_id`, `height`, and `state_root` equality with
  the QC-certified header) and bans peers serving invalid manifests.
- **Per-chunk SHA-256 verification, rollback, peer ban, and re-request (`D-N3`, `ingest_chunk`, `sync_from_peers`)**:
  checks every chunk against its S4 descriptor (`length`, `chunk_hash`, and
  canonical `decode_chunk`); on any corrupt chunk, rolls back staged data for
  that chunk, bans the offending peer (`BanList`), and re-requests the chunk
  from an honest fallback peer.
- **S1 Merkle root verification (`finalize_and_verify_root`)**: executes S4
  parallel `import_snapshot` and recomputes the versioned S1 sparse Merkle
  `state_root(&entries)` before committing state.

## Gate results

- `make test-n5`: 42 `aura-p2p` library unit tests and 1 `n5_state_sync`
  integration test (1,000-key 10-chunk S4 state sync + corrupt-chunk rollback &
  re-request) passed.
- `make nc-n5`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact state_sync::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **285/285** cumulative negative controls through N5; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean GitHub-HEAD + `0040..0048` apply and revalidation:
  [`clean-apply.txt`](clean-apply.txt).
