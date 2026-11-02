# CONSENSUS B8 — Finality: Fork Choice, Finalized Checkpoint, Light-Client Proof & Deep-Reorg Defence (`0055-B8-finality-fork-choice.patch`)

## Scope & DoD (`D-B3`, `D-B4`)
- **Fork Choice & Finalized Checkpoints (`crates/aura-consensus/src/fork_choice.rs`)**:
  - Selects the canonical head descending from the latest `FinalizedCheckpoint` by highest `justified_qc_view`, height, and deterministic hash tie-break.
  - Enforces `D-B3` weak-subjectivity window (`DEFAULT_WEAK_SUBJECTIVITY_WINDOW = 10_000` blocks) and `2f+1` BLS `QuorumCertificate` verification.
- **Consensus Light-Client Proof (`ConsensusLightClientProof`)**:
  - Verifies 3-chain parent links, same-view rule, and `2f+1` BLS QCs (`target_qc`, `child_qc`, `grandchild_qc`).
- **Deep-Reorg Defence (`attempt to reorg a finalized block -> reject + evidence`)**:
  - Any attempt to reorganize a finalized block is rejected with `ForkChoiceError::DeepReorgPastFinalized` and emits cryptographic `SlashingEvidence` accepted by `SlashingEngine`.
- **Negative Controls (`tools/nc_b8.py`)**: 6/6 library unit-test negative controls pass (`NC-1`..`NC-6`).
