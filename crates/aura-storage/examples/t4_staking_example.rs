//! T4 staking simulation runner (ECONOMICS_PLAN T4).
//!
//! Prints a machine-readable summary consumed by the independent Python
//! oracle (`tools/t4_staking_check.py`).

use aura_storage::econ::Amount;
use aura_storage::staking::{run_t4_scenario, scenario_digest};

fn main() {
    let steps = run_t4_scenario();
    let withdrawn: Amount = steps
        .iter()
        .filter(|step| step.kind == 6)
        .map(|step| step.amount)
        .sum();
    let final_step = steps.last().expect("nonempty");
    println!(
        "T4-SIM-OK {{\"steps\":{},\"final_bonded\":{},\"final_total_shares\":{},\"final_shares_sum\":{},\"withdrawn_total\":{},\"digest\":\"{}\"}}",
        steps.len(),
        final_step.bonded,
        final_step.total_shares,
        final_step.shares_sum,
        withdrawn,
        scenario_digest(&steps)
    );
}
