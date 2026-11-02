# N7 — Light client: header+QC proof, finalized checkpoint check, and simplified DAS evidence receipt

Generated (UTC): 2026-11-01T13:23:11+00:00.

## Delivered behavior

`aura-p2p` implements the **N7** light-client layer (`aura_p2p::light_client`)
and updates `aura_p2p::baseline` to report 7 implemented network components
(`transport`, `gossipsub`, `discovery`, `block-sync`, `state-sync`,
`dos-defence`, `light-client`) and 1 remaining placeholder (`chaos-harness` N8):

- **Bounded proof byte size and verification gas (`LightClientBounds`, `LightClientProof`)**:
  enforces `max_proof_bytes` (`16 KiB` default) and `max_verification_gas`
  (`150,000` default) before performing Ed25519 QC signature verification.
- **Finalized checkpoint progression (`FinalizedCheckpoint`, `LightClientVerifier`)**:
  anchors verification on a trusted finalized checkpoint (`height`,
  `block_hash`, `state_root`, `timestamp_ms`), rejecting mismatched anchors,
  non-advancing heights, timestamp regressions, and mismatched adjacent
  `parent_hash`.
- **Header + 2f+1 QC verification (`D-N3`, `D-N4`)**: verifies
  `SyncedHeader::verify_self` and `SyncQc::verify` against `SyncValidatorSet`,
  rejecting forged headers or tampered `state_root` and banning the offending
  peer.
- **Simplified Data-Availability Sampling (`DasChunkSample`, `build_das_tree`, `sample_das_chunks`)**:
  verifies at least `min_das_samples` distinct chunk inclusion branches against
  `header.tx_root` (`LIGHT_CLIENT_LEAF_DOMAIN`, `LIGHT_CLIENT_BRANCH_DOMAIN`),
  banning peers that serve corrupted or withheld DAS samples.

## Gate results

- `make test-n7`: 54 `aura-p2p` library unit tests and 1 `n7_light_client`
  integration test (forged header/QC/DAS rejection + bounded gas/size multi-block
  checkpoint verification) passed.
- `make nc-n7`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact light_client::tests::<test>`); restored
  library tests passed.
- `make nc-all`: **297/297** cumulative negative controls through N7; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean GitHub-HEAD + `0040..0050` apply and revalidation:
  [`clean-apply.txt`](clean-apply.txt).
