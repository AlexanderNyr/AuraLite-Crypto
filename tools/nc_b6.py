#!/usr/bin/env python3
"""Six B6 negative controls for slashing, evidence verification, penalties, and distribution."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from nc_rust import run_controls

SLASHING = "crates/aura-consensus/src/slashing.rs"

CASES = [
    (
        "NC-1 turning slashing off must fail the 6-attack slashing gate",
        SLASHING,
        "        if !self.params.slashing_enabled {\n            return Err(SlashingError::SlashingDisabled);\n        }",
        "        if true {\n            return Err(SlashingError::SlashingDisabled);\n        }",
        "slashing::tests::six_attack_types_are_accepted_and_apply_penalties",
    ),
    (
        "NC-2 double-sign evidence must require two distinct block hashes",
        SLASHING,
        "                if first.block_hash == second.block_hash {\n                    return Err(SlashingError::NotAnOffence(\n                        \"double-sign votes must target two distinct block hashes\",\n                    ));\n                }",
        "                let _ = (&first.block_hash, &second.block_hash);",
        "slashing::tests::double_sign_requires_distinct_blocks_and_valid_signatures",
    ),
    (
        "NC-3 surround-vote evidence must enforce strict outer/inner epoch span",
        SLASHING,
        "                if !(outer.source_epoch < inner.source_epoch\n                    && inner.target_epoch < outer.target_epoch)\n                {\n                    return Err(SlashingError::NotAnOffence(\n                        \"attestation pair does not satisfy surround inequality (s_outer < s_inner && t_inner < t_outer)\",\n                    ));\n                }",
        "                let _ = (outer.source_epoch, inner.source_epoch);",
        "slashing::tests::surround_vote_requires_strict_outer_inner_span_and_valid_bls_signatures",
    ),
    (
        "NC-4 downtime slashing must enforce max_missed_blocks_per_window threshold",
        SLASHING,
        "                if *missed_blocks < self.params.max_missed_blocks_per_window\n                    || *missed_blocks > *window_blocks\n                {\n                    return Err(SlashingError::NotAnOffence(\n                        \"missed_blocks is below max_missed_blocks_per_window threshold\",\n                    ));\n                }",
        "                let _ = (missed_blocks, window_blocks);",
        "slashing::tests::downtime_slashing_requires_missed_blocks_threshold_and_valid_qc",
    ),
    (
        "NC-5 penalty distribution must conserve slashed stake across reporter, treasury, and burn",
        SLASHING,
        "        let burned_amount = total_slashed\n            .saturating_sub(reporter_reward)\n            .saturating_sub(treasury_share);",
        "        let burned_amount = 0;",
        "slashing::tests::penalty_distribution_conserves_slashed_stake_across_reporter_treasury_and_burn",
    ),
    (
        "NC-6 duplicate slashing evidence replay must be rejected",
        SLASHING,
        "        if self.processed_evidence.contains(&evidence_id) {\n            return Err(SlashingError::DuplicateEvidence(evidence_id));\n        }",
        "        let _ = &evidence_id;",
        "slashing::tests::duplicate_evidence_replay_is_rejected",
    ),
]

if __name__ == "__main__":
    raise SystemExit(run_controls("B6", CASES))
