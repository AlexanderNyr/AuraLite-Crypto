//! P0 baseline bench example: prints the pinned deterministic report
//! (byte-identical to `tools/p0_bench_check.py`). With `--measured`
//! additionally prints honest wall-clock numbers (evidence only).

use aura_bench::{
    BenchReport, CostModel, MachineProfile, REQUIRED_RUNS, Workload, measure_pipeline_ns,
    percentile, spread_pct, validate,
};

fn main() {
    let report = BenchReport::build(
        &Workload::TRANSFER_350,
        &MachineProfile::REFERENCE,
        &CostModel::BASELINE,
        REQUIRED_RUNS,
    );
    validate(&report, &Workload::TRANSFER_350, &MachineProfile::REFERENCE)
        .expect("P0 plan must validate");
    println!("P0-RUNS-OK {}", report.runs);
    println!("P0-BENCH-OK {}", report.json());

    if std::env::args().any(|a| a == "--measured") {
        let mut samples = Vec::new();
        for _ in 0..REQUIRED_RUNS {
            samples.push(measure_pipeline_ns(&Workload::TRANSFER_350, 2000));
        }
        println!(
            "P0-MEASURED {{\"p50_ns\": {}, \"p95_ns\": {}, \"p99_ns\": {}, \
             \"spread_pct\": {}, \"iterations\": 2000}}",
            percentile(&samples, 50),
            percentile(&samples, 95),
            percentile(&samples, 99),
            spread_pct(&samples)
        );
    }
}
