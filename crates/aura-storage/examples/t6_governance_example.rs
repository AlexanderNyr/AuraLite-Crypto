//! T6 example: run the pinned governance scenario and print its digest
//! and outcome counts.

use aura_storage::governance::{run_t6_scenario, scenario_digest};

fn main() {
    let run = run_t6_scenario();
    let digest = scenario_digest(&run.steps);
    println!(
        "T6-SIM-OK {{\"steps\": {}, \"proposals\": {}, \"digest\": \"{}\", \
\"executed\": {}, \"rejected\": {}, \"vetoed\": {}}}",
        run.steps.len(),
        run.states.len(),
        digest,
        run.executed,
        run.rejected,
        run.vetoed
    );
}
