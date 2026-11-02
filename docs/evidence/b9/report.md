# CONSENSUS B9 Close-out Report — On-chain Governance

- **Phase:** `B9` (`CONSENSUS_PLAN.md`)
- **Patch:** `patches/0056-B9-on-chain-governance.patch`
- **Generated (UTC):** `2026-11-01T20:36:16+00:00`

## Summary
1. **`aura_consensus::governance`**:
   - `GovernanceProposal`, `ProposalPayload::{RuntimeWasmUpgrade, ParameterChange}`, `GovernanceVote`, `GovernanceVoteChoice`, `TallyResult`, `ProposalStatus`, `ActiveRuntimeState`, `EmergencyPauseState`, `GovernanceEngine`, and `HotUpgradableTestnet`.
2. **Test Gate (`first protocol upgrade in testnet without a manual validator restart`)**:
   - Verified in `governance::tests::testnet_protocol_upgrade_activates_without_manual_validator_restart` and `crates/aura-consensus/tests/b9_governance.rs` (plus 64 deterministic vectors in `b9_governance.txt`).
3. **Negative Controls**:
   - `tools/nc_b9.py`: 6/6 library unit-test mutations PASS (`327/327` cumulative).
