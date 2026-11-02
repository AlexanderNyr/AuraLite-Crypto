# N4 — Block sync: header-first verification, QC validation, and parallel body download evidence receipt

Generated (UTC): 2026-11-01T09:22:32+00:00.

## Delivered behavior

`aura-p2p` implements the **N4** block-sync layer (`aura_p2p::block_sync`) and
updates `aura_p2p::baseline` to report 4 implemented network components
(`transport`, `gossipsub`, `discovery`, `block-sync`) and 4 remaining
placeholders (`state-sync` N5 .. `chaos-harness` N8):

- **Header-first chain verification (`D-N4`, `SyncedHeader`)**: validates
  contiguous height progression, `parent_hash`, monotonic timestamp, and
  canonical `BLAKE3` `block_hash` before any block body is accepted.
- **Quorum Certificate verification (`SyncQc`, `SyncValidatorSet`)**: verifies
  domain-separated Ed25519 signatures (`BLOCK_SYNC_QC_DOMAIN`), rejects unknown
  or duplicate signers, and enforces `>= 2/3 + 1` quorum stake.
- **Parallel body download & `tx_root` verification (`D-N3`, `SyncedBlockBody`, `BlockSyncEngine`)**:
  verifies body chunks in parallel across worker threads (`std::thread::scope`),
  supports out-of-order worker completion, checks `tx_root` against the verified
  header, and automatically bans any peer serving an invalid header, QC, or
  corrupted body (`BanList`).
- **10,000-block catch-up gate (`crates/aura-p2p/tests/n4_block_sync.rs`)**:
  a lagging node starting at height `0` verifies 10,000 headers + 3-of-4 QCs and
  downloads 10,000 block bodies in parallel across 4 peers (with out-of-order
  chunk completion), reaching height `10,000` with the exact canonical
  `block_hash` and `state_root`.

## Gate results

- `make test-n4`: 36 `aura-p2p` library unit tests and 2 `n4_block_sync`
  integration tests passed.
- `make nc-n4`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact block_sync::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **279/279** cumulative negative controls through N4; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean GitHub-HEAD + `0040..0047` apply and revalidation:
  [`clean-apply.txt`](clean-apply.txt).
