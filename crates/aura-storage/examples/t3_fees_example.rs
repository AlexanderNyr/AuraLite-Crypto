//! T3 fees simulation runner (ECONOMICS_PLAN T3).
//!
//! Prints a machine-readable summary consumed by the independent Python
//! oracle (`tools/t3_fees_check.py`).

use aura_storage::econ::Amount;
use aura_storage::fees::{T3_BLOCKS, T3_INITIAL_BASE_FEE, run_scenario, scenario_digest};

fn main() {
    let outcomes = run_scenario(T3_BLOCKS);
    let burned: Amount = outcomes.iter().map(|outcome| outcome.burned).sum();
    let tips: Amount = outcomes.iter().map(|outcome| outcome.tips).sum();
    let peak = outcomes
        .iter()
        .map(|outcome| outcome.base_after)
        .max()
        .expect("nonempty");
    let final_base = outcomes.last().expect("nonempty").base_after;
    println!(
        "T3-SIM-OK {{\"blocks\":{},\"initial\":{},\"final_base\":{},\"peak_base\":{},\"total_burned\":{},\"total_tips\":{},\"digest\":\"{}\"}}",
        outcomes.len(),
        T3_INITIAL_BASE_FEE,
        final_base,
        peak,
        burned,
        tips,
        scenario_digest(&outcomes)
    );
}
