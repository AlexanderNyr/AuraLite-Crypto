//! T7 example: the pinned treasury and grants simulation.
//!
//! Run: `cargo run --example t7_treasury_example -p aura-storage`.
//! Emits the deterministic marker `T7-SIM-OK` with the golden digest
//! (mirrored by `tools/t7_treasury_check.py`).

use aura_storage::treasury::{run_t7_scenario, scenario_digest};

fn main() {
    let run = run_t7_scenario();
    let digest = scenario_digest(&run.steps);
    println!(
        "T7-SIM-OK {{\"steps\": {}, \"proposals\": {}, \"payouts\": {}, \"final_balance\": \"{}\", \"total_paid\": \"{}\", \"digest\": \"{}\"}}",
        run.steps.len(),
        run.states.len(),
        run.payouts.len(),
        run.final_balance,
        run.total_paid,
        digest
    );
    for payout in &run.payouts {
        let report = payout.report.as_ref().expect("report");
        println!(
            "T7-TX tx={} proposal={} amount={} recipient={} purpose={} height={}",
            payout.tx_id,
            payout.proposal_id,
            payout.amount,
            payout.recipient,
            payout.purpose,
            report.height
        );
    }
}
