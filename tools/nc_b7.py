#!/usr/bin/env python3
"""Six B7 negative controls for consensus rewards, fee burn, delegation, commission, and the sum invariant."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from nc_rust import run_controls

REWARDS = "crates/aura-consensus/src/rewards.rs"

CASES = [
    (
        "NC-1 dropping integer-division remainder breaks the 100,000-block sum invariant",
        REWARDS,
        "        if remainder > 0 {\n            *self.balances.entry(proposer).or_insert(0) += remainder;\n        }",
        "        let _ = remainder;",
        "rewards::tests::hundred_thousand_blocks_sum_invariant_holds_within_one_unit",
    ),
    (
        "NC-2 fee burn must deduct and record burned transaction fees",
        REWARDS,
        "        let burned_fees = (fees * (self.params.fee_burn_bps as u128)) / (MAX_BPS as u128);",
        "        let burned_fees = 0u128;",
        "rewards::tests::fee_burn_deducts_and_accumulates_burned_fees",
    ),
    (
        "NC-3 validator commission must be deducted before splitting the delegator pool",
        REWARDS,
        "        let commission = (gross_reward * (val.commission_bps as u128)) / (MAX_BPS as u128);",
        "        let commission = 0u128;",
        "rewards::tests::validator_commission_is_paid_before_delegator_split",
    ),
    (
        "NC-4 delegator rewards must scale proportionally with delegated stake",
        REWARDS,
        "            let share = (delegator_pool * (del_stake as u128)) / v_total_stake;",
        "            let share = 0u128;\n            let _ = del_stake;",
        "rewards::tests::delegator_rewards_scale_proportionally_with_delegated_stake",
    ),
    (
        "NC-5 verify_sum_invariant must reject discrepancy above 1 unit",
        REWARDS,
        "        if discrepancy > 1 {\n            return Err(RewardError::SumInvariantViolation {\n                sum_balances,\n                burned: self.total_burned,\n                issued,\n                discrepancy,\n            });\n        }",
        "        let _ = (sum_balances, issued, discrepancy);",
        "rewards::tests::sum_invariant_detects_unbacked_mint_or_leak",
    ),
    (
        "NC-6 validator commission rate above 10,000 bps must be rejected",
        REWARDS,
        "        if commission_bps > MAX_BPS {\n            return Err(RewardError::InvalidCommissionBps(commission_bps));\n        }",
        "        let _ = commission_bps;",
        "rewards::tests::invalid_commission_rate_above_hundred_percent_is_rejected",
    ),
]

if __name__ == "__main__":
    raise SystemExit(run_controls("B7", CASES))
