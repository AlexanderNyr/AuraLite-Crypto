#!/usr/bin/env python3
"""Six B4 negative controls for validator set, staking, epochs, and set updates."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from nc_rust import run_controls

VALIDATOR_SET = "crates/aura-consensus/src/validator_set.rs"

CASES = [
    (
        "NC-1 below-minimum stake must be rejected from enter queue",
        VALIDATOR_SET,
        "        if stake < self.min_stake {\n            return;\n        }",
        "        let _ = self.min_stake;",
        "validator_set::tests::below_min_stake_ignored",
    ),
    (
        "NC-2 duplicate active validator must be rejected from enter queue",
        VALIDATOR_SET,
        "        if self.validators.contains_key(&address)\n            || self.enter_queue.iter().any(|r| r.address == address)\n        {\n            return;\n        }",
        "        let _ = &address;",
        "validator_set::tests::duplicate_enter_ignored",
    ),
    (
        "NC-3 epoch boundary must process enter and exit queues",
        VALIDATOR_SET,
        "        if self.blocks_in_epoch >= self.epoch_length {\n            self.process_epoch_change()\n        }",
        "        if false {\n            self.process_epoch_change()\n        }",
        "validator_set::tests::enter_and_exit_queue",
    ),
    (
        "NC-4 set update targeting wrong epoch must be rejected",
        VALIDATOR_SET,
        "        if update.epoch != self.epoch.number + 1 {\n            return Err(SetUpdateError::WrongEpoch(\n                update.epoch,\n                self.epoch.number + 1,\n            ));\n        }",
        "        let _ = update.epoch;",
        "validator_set::tests::set_update_wrong_epoch",
    ),
    (
        "NC-5 set update with insufficient validator stake must be rejected",
        VALIDATOR_SET,
        "        if update.validators.is_empty()\n            || update.validators.iter().any(|v| v.stake < self.min_stake)\n        {\n            return Err(SetUpdateError::InsufficientStake);\n        }",
        "        let _ = self.min_stake;",
        "validator_set::tests::set_update_rejects_insufficient_stake",
    ),
    (
        "NC-6 set update with mismatched or insufficient QC must be rejected",
        VALIDATOR_SET,
        "        if update.qc.block_hash != *current_tip_hash\n            || update.qc.signatures.len() < required_sigs\n            || update.qc.signatures.len() != update.qc.public_keys.len()\n        {\n            return Err(SetUpdateError::InvalidQc);\n        }",
        "        let _ = (current_tip_hash, required_sigs);",
        "validator_set::tests::set_update_rejects_invalid_qc",
    ),
]

if __name__ == "__main__":
    raise SystemExit(run_controls("B4", CASES))
