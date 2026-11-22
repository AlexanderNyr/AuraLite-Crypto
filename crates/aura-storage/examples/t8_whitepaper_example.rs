//! T8 example: the pinned MiCA white paper checklist simulation.
//!
//! Run: `cargo run --example t8_whitepaper_example -p aura-storage`.
//! Emits the deterministic marker `T8-SIM-OK` with the golden digest
//! (mirrored by `tools/t8_whitepaper_check.py`).

use aura_storage::whitepaper::{T8_NOT_LEGAL_ADVICE, run_t8_scenario, scenario_digest};

fn main() {
    let run = run_t8_scenario();
    let digest = scenario_digest(&run.steps);
    println!(
        "T8-SIM-OK {{\"steps\": {}, \"documents\": {}, \"valid\": {}, \"digest\": \"{}\"}}",
        run.steps.len(),
        run.total,
        run.valid,
        digest
    );
    for (index, outcome) in run.outcomes.iter().enumerate() {
        println!("T8-CHECK doc={} outcome={}", index + 1, outcome);
    }
    println!("T8-DISCLAIMER {}", T8_NOT_LEGAL_ADVICE);
}
