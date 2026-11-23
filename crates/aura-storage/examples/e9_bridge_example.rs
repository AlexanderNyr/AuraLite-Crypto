//! E9 example: replays the pinned bridge scenario (emulator lane +
//! attack lane + limits + OS client) and prints the pinned output that
//! `tools/e9_bridge_check.py` must reproduce byte for byte:
//!
//! ```text
//! E9-STEPS-OK 25
//! E9-SIM-OK {"steps": 25, ...}
//! ```
//!
//! Run: `cargo run --quiet --locked -p aura-storage --example e9_bridge_example`

use aura_storage::bridge::{run_e9_scenario, scenario_digest};

const SCENARIO_DIGEST: &str = "039d498ba6fcd17e7640094492283a94be2c3447f8ebec89f9306eb438339580";

fn main() {
    let run = run_e9_scenario();
    assert_eq!(run.steps.len(), 25);
    assert_eq!(scenario_digest(&run.steps), SCENARIO_DIGEST);
    assert_eq!(run.forged_rejected, 2);
    assert_eq!(run.finalized_height, 5);
    assert_eq!(run.locked, run.minted);
    assert_eq!(run.lane, "green");
    println!("E9-STEPS-OK {}", run.steps.len());
    println!(
        "E9-SIM-OK {{\"steps\": {}, \"forged_rejected\": {}, \
         \"finalized_height\": {}, \"locked\": {}, \"minted\": {}, \
         \"lane\": \"{}\"}}",
        run.steps.len(),
        run.forged_rejected,
        run.finalized_height,
        run.locked,
        run.minted,
        run.lane
    );
}
