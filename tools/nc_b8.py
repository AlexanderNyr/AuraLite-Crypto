#!/usr/bin/env python3
"""Six B8 negative controls for fork choice, finalized checkpoints, light-client proofs, and deep-reorg defence."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from nc_rust import run_controls

FORK_CHOICE = "crates/aura-consensus/src/fork_choice.rs"

CASES = [
    (
        "NC-1 attempting to reorg a finalized block must be rejected and emit slashing evidence",
        FORK_CHOICE,
        "        if conflicts_with_finalized {",
        "        if false {",
        "fork_choice::tests::attempt_to_reorg_finalized_block_is_rejected_and_emits_slashing_evidence",
    ),
    (
        "NC-2 fork choice must prefer the branch with the highest justified QC view",
        FORK_CHOICE,
        "            let score = (candidate.justified_qc_view, candidate.height);",
        "            let score = (0u64, candidate.height);",
        "fork_choice::tests::fork_choice_prefers_branch_with_highest_qc_view_descending_from_finalized",
    ),
    (
        "NC-3 fork choice must ignore branches that do not descend from the finalized checkpoint",
        FORK_CHOICE,
        "            if !self.descends_from(hash, finalized_hash) {\n                continue;\n            }",
        "            let _ = finalized_hash;",
        "fork_choice::tests::fork_not_descending_from_finalized_checkpoint_is_never_selected",
    ),
    (
        "NC-4 finalized checkpoint must reject a sub-quorum commit QC",
        FORK_CHOICE,
        "        checkpoint.verify(&self.chain_id, &self.active)?;",
        "        let _ = &self.active;",
        "fork_choice::tests::finalized_checkpoint_rejects_invalid_or_sub_quorum_qc",
    ),
    (
        "NC-5 consensus light-client proof must enforce same-view rule across 3-chain QCs",
        FORK_CHOICE,
        "        if self.target_qc.view != self.child_qc.view\n            || self.child_qc.view != self.grandchild_qc.view\n        {\n            return Err(ForkChoiceError::InvalidLightClientProof(\n                \"3-chain QCs must share the same view\",\n            ));\n        }",
        "        let _ = self.target_qc.view;",
        "fork_choice::tests::light_client_proof_verifies_3_chain_finality_and_rejects_forged_qc",
    ),
    (
        "NC-6 weak subjectivity must reject stale checkpoints beyond the unbonding window",
        FORK_CHOICE,
        "        if current_head_height.saturating_sub(checkpoint.height) > self.weak_subjectivity_window {\n            return Err(ForkChoiceError::WeakSubjectivityExpired {\n                checkpoint_height: checkpoint.height,\n                head_height: current_head_height,\n                window: self.weak_subjectivity_window,\n            });\n        }",
        "        let _ = current_head_height;",
        "fork_choice::tests::weak_subjectivity_rejects_stale_checkpoint_beyond_unbonding_window",
    ),
]

if __name__ == "__main__":
    raise SystemExit(run_controls("B8", CASES))
