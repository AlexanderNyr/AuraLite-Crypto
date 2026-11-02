#!/usr/bin/env python3
"""Six B5 negative controls for VRF leader election, grinding protection, and N1/N2 BFT."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from nc_rust import run_controls

VRF_LEADER = "crates/aura-consensus/src/vrf_leader.rs"

CASES = [
    (
        "NC-1 VRF verification must reject wrong or forged BLS proof signature",
        VRF_LEADER,
        "    bls::verify(\n        public_key,\n        &input.chain_id,\n        Domain::Block,\n        &transcript,\n        &proof.signature,\n    )\n    .map_err(|_| VrfError::InvalidProofSignature)?;",
        "    let _ = &proof.signature;",
        "vrf_leader::tests::vrf_rejects_wrong_public_key",
    ),
    (
        "NC-2 VRF verification must bind the output hash to proof and transcript",
        VRF_LEADER,
        "    if proof.output != expected_output {\n        return Err(VrfError::OutputMismatch);\n    }",
        "    let _ = expected_output;",
        "vrf_leader::tests::vrf_rejects_tampered_output_hash",
    ),
    (
        "NC-3 grinding protection must reject zero epoch randomness after epoch 0",
        VRF_LEADER,
        "        if self.epoch > 0 && self.epoch_randomness == Hash::ZERO {\n            return Err(VrfError::ZeroEpochRandomness);\n        }",
        "        let _ = self.epoch_randomness;",
        "vrf_leader::tests::grinding_protection_rejects_zero_epoch_randomness_after_genesis_epoch",
    ),
    (
        "NC-4 epoch randomness derivation must verify the boundary QC",
        VRF_LEADER,
        "    boundary_qc\n        .verify(chain_id, Domain::Vote)\n        .map_err(|_| VrfError::InvalidBoundaryQc)?;",
        "    let _ = &boundary_qc;",
        "vrf_leader::tests::epoch_randomness_rejects_unverified_boundary_qc",
    ),
    (
        "NC-5 leader claim verification must enforce stake-weighted VRF leader identity",
        VRF_LEADER,
        "        let expected = self.select_leader(input)?;\n        if claim.proof.validator != expected {\n            return Err(VrfError::NotElectedLeader {\n                claimed: claim.proof.validator,\n                expected,\n            });\n        }",
        "        let _ = self.select_leader(input)?;",
        "vrf_leader::tests::leader_claim_rejects_non_elected_validator",
    ),
    (
        "NC-6 stake-weighted VRF leader selection must weight validators by stake",
        VRF_LEADER,
        "            cumulative = cumulative.saturating_add(entry.stake);",
        "            cumulative = cumulative.saturating_add(self.total_stake);",
        "vrf_leader::tests::ten_thousand_rounds_proposer_distribution_within_confidence_interval",
    ),
]

if __name__ == "__main__":
    raise SystemExit(run_controls("B5", CASES))
