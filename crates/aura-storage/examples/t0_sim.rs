//! T0 baseline simulation runner (ECONOMICS_PLAN T0).
//!
//! Runs the deterministic 1000-step schedule from `sim/t0-baseline.json`
//! and prints a machine-readable summary consumed by the independent
//! Python oracle (`tools/t0_economics_check.py`).

use aura_storage::econ::{EconSim, T0_AGENTS, T0_SEED, T0_STEPS, generate_schedule};

fn main() {
    let mut sim = EconSim::from_genesis(&T0_AGENTS).expect("genesis");
    let schedule = generate_schedule(T0_SEED, T0_STEPS, &T0_AGENTS);
    sim.run(&schedule).expect("schedule");
    let balances: Vec<String> = T0_AGENTS
        .iter()
        .map(|(name, _)| format!("\"{}\":{}", name, sim.balance(name).expect("known")))
        .collect();
    println!(
        "T0-SIM-OK {{\"seed\":{T0_SEED},\"steps\":{},\"supply\":{},\"sum\":{},\"digest\":\"{}\",\"balances\":{{{}}}}}",
        sim.steps(),
        sim.total_supply(),
        sim.sum_balances(),
        sim.state_digest(),
        balances.join(",")
    );
}
