//! T5 example: run the pinned slashing and insurance scenario and
//! print its digest and totals.

use aura_storage::slashing::{run_t5_scenario, scenario_digest};

fn main() {
    let run = run_t5_scenario();
    let digest = scenario_digest(&run.steps);
    println!(
        "T5-SIM-OK {{\"steps\": {}, \"records\": {}, \"digest\": \"{}\", \
\"final_balance\": {}, \"pending\": {}, \"total_penalty\": {}, \
\"total_applied\": {}, \"total_covered\": {}, \"total_shortfall\": {}}}",
        run.steps.len(),
        run.records.len(),
        digest,
        run.final_balance,
        run.pending,
        run.total_penalty,
        run.total_applied,
        run.total_covered,
        run.total_shortfall
    );
}
