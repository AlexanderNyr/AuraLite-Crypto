#!/usr/bin/env python3
"""Six B9 negative controls against `crates/aura-consensus/src/governance.rs` library unit tests."""

from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from nc_rust import run_controls

GOV = "crates/aura-consensus/src/governance.rs"

CASES = [
    (
        "NC-1 hot WASM runtime upgrade must update active protocol_version on execution",
        GOV,
        "protocol_version: *to_version,",
        "protocol_version: self.runtime.protocol_version,",
        "governance::tests::proposal_vote_timelock_and_wasm_runtime_upgrade_lifecycle",
    ),
    (
        "NC-2 timelock must reject proposal execution before earliest_execution_height",
        GOV,
        "if current_height < earliest_execution_height {",
        "if false {",
        "governance::tests::timelock_rejects_early_execution_before_delay_elapses",
    ),
    (
        "NC-3 tally must enforce quorum_bps participation threshold",
        GOV,
        "participating.saturating_mul(BPS_DENOMINATOR as u128)\n                < total_active_stake.saturating_mul(self.params.quorum_bps as u128)",
        "false",
        "governance::tests::quorum_and_supermajority_thresholds_reject_under_voted_or_vetoed_proposals",
    ),
    (
        "NC-4 WASM runtime upgrade must verify BLAKE3 wasm_code_hash matches bytecode",
        GOV,
        "if computed != *wasm_code_hash {",
        "if false {",
        "governance::tests::wasm_upgrade_rejects_invalid_magic_or_tampered_code_hash_or_version_downgrade",
    ),
    (
        "NC-5 emergency pause must require 2f+1 quorum threshold on authorizing QC",
        GOV,
        "if qc.len() < required || qc.verify(self.chain_id.as_str(), Domain::Vote).is_err() {",
        "if false {",
        "governance::tests::emergency_pause_requires_quorum_qc_and_halts_governance_execution_until_resumed",
    ),
    (
        "NC-6 testnet block production must execute ready timelocked upgrades without validator restart",
        GOV,
        "node.governance.execute_ready_proposals(self.height)?;",
        "let _ = self.height;",
        "governance::tests::testnet_protocol_upgrade_activates_without_manual_validator_restart",
    ),
]

if __name__ == "__main__":
    raise SystemExit(run_controls("B9", CASES))
