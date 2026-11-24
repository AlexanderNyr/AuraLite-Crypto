//! P0 baseline bench gates (PERF_PLAN P0 DoD).
//!
//! * golden numbers: 5 repeat runs, spread 0% < 5% limit;
//! * functional gate control: substituted workload → different numbers;
//! * exact NC literals (6 named messages) + validate-order suite;
//! * metamorphic controls (determinism, run scaling, percentiles).

use aura_bench::{
    BenchError, BenchReport, CostModel, MachineProfile, REQUIRED_RUNS, Workload, percentile,
    spread_pct, validate,
};

const GOLDEN_WORKLOAD_DIGEST: &str =
    "02daa5a3698437b44c9356ba317fc0b07e10a8226d6ac792b599148e253ca3c5";
const GOLDEN_MACHINE_DIGEST: &str =
    "cc5b0e9e932a00ca5acc4dbec21c0f77f0a548c1f7fdc6a89fb742def1a6ab2d";
const GOLDEN_BENCH_DIGEST: &str =
    "cfc469203166bce7439bf4bec51a9ba545e8b6c3df2edcb27cd8ff41dce220ec";
const GOLDEN_JSON: &str = "{\"workload_digest\": \"02daa5a3698437b44c9356ba317fc0b07e10a8226d6ac792b599148e253ca3c5\", \
\"machine_digest\": \"cc5b0e9e932a00ca5acc4dbec21c0f77f0a548c1f7fdc6a89fb742def1a6ab2d\", \
\"runs\": 5, \"spread_pct\": 0, \"p50\": 128, \"p95\": 128, \"p99\": 128, \
\"total\": 640, \"bench_digest\": \"cfc469203166bce7439bf4bec51a9ba545e8b6c3df2edcb27cd8ff41dce220ec\"}";

/// DoD: 5 repeat runs, spread < 5%, pinned golden numbers.
#[test]
fn p0_golden_baseline() {
    let report = BenchReport::build(
        &Workload::TRANSFER_350,
        &MachineProfile::REFERENCE,
        &CostModel::BASELINE,
        REQUIRED_RUNS,
    );
    assert_eq!(report.workload_digest, GOLDEN_WORKLOAD_DIGEST);
    assert_eq!(report.machine_digest, GOLDEN_MACHINE_DIGEST);
    assert_eq!(report.runs, 5);
    assert_eq!(report.run_costs, vec![128u64, 128, 128, 128, 128]);
    assert_eq!(report.spread_pct, 0);
    assert!(report.spread_pct < 5, "DoD: spread < 5%");
    assert_eq!(report.p50, 128);
    assert_eq!(report.p95, 128);
    assert_eq!(report.p99, 128);
    assert_eq!(report.total, 640);
    assert_eq!(report.bench_digest, GOLDEN_BENCH_DIGEST);
    assert_eq!(report.json(), GOLDEN_JSON);
    assert!(validate(&report, &Workload::TRANSFER_350, &MachineProfile::REFERENCE).is_ok());
}

/// NC: substituted workload → different numbers (the literal P0 DoD
/// negative control) AND the named-message gate turns red.
#[test]
fn p0_functional_substituted_workload() {
    let good = BenchReport::build(
        &Workload::TRANSFER_350,
        &MachineProfile::REFERENCE,
        &CostModel::BASELINE,
        REQUIRED_RUNS,
    );
    let bad = BenchReport::build(
        &Workload::TRANSFER_352,
        &MachineProfile::REFERENCE,
        &CostModel::BASELINE,
        REQUIRED_RUNS,
    );
    assert_ne!(good.run_costs, bad.run_costs, "350B -> 128, 352B -> 129");
    assert_ne!(good.bench_digest, bad.bench_digest);
    assert_ne!(good.workload_digest, bad.workload_digest);
    let err = validate(&bad, &Workload::TRANSFER_350, &MachineProfile::REFERENCE)
        .expect_err("substituted workload must be rejected");
    assert_eq!(
        err.to_string(),
        format!(
            "P0: workload mismatch ({}/{})",
            GOLDEN_WORKLOAD_DIGEST,
            Workload::TRANSFER_352.digest()
        )
    );
}

/// NC: the six named-message literals are exact.
#[test]
fn p0_nc_literals() {
    assert_eq!(
        BenchError::WorkloadMismatch {
            expected: "AA".to_string(),
            actual: "BB".to_string(),
        }
        .to_string(),
        "P0: workload mismatch (AA/BB)"
    );
    assert_eq!(
        BenchError::MachineMismatch {
            expected: "AA".to_string(),
            actual: "BB".to_string(),
        }
        .to_string(),
        "P0: machine profile mismatch (AA/BB)"
    );
    assert_eq!(
        BenchError::SpreadExceeded {
            spread: 7,
            limit: 5
        }
        .to_string(),
        "P0: spread exceeds limit (7%/5%)"
    );
    assert_eq!(
        BenchError::RunCount { runs: 3, need: 5 }.to_string(),
        "P0: repeat count violated (3/5)"
    );
    assert_eq!(
        BenchError::Determinism { run: 2 }.to_string(),
        "P0: determinism violated (run 2)"
    );
    assert_eq!(
        BenchError::MetricNotPublished {
            metric: "p99".to_string()
        }
        .to_string(),
        "P0: metric not published (p99)"
    );
}

/// Functional gate control: every validate stage turns red when its
/// condition is broken, in the documented order.
#[test]
fn p0_validate_suite() {
    // run count
    let short = BenchReport::build(
        &Workload::TRANSFER_350,
        &MachineProfile::REFERENCE,
        &CostModel::BASELINE,
        3,
    );
    assert_eq!(
        validate(&short, &Workload::TRANSFER_350, &MachineProfile::REFERENCE)
            .unwrap_err()
            .to_string(),
        "P0: repeat count violated (3/5)"
    );
    // determinism
    let mut skewed = BenchReport::build(
        &Workload::TRANSFER_350,
        &MachineProfile::REFERENCE,
        &CostModel::BASELINE,
        REQUIRED_RUNS,
    );
    skewed.run_costs[2] += 1;
    assert_eq!(
        validate(&skewed, &Workload::TRANSFER_350, &MachineProfile::REFERENCE)
            .unwrap_err()
            .to_string(),
        "P0: determinism violated (run 2)"
    );
    // spread
    let mut wide = BenchReport::build(
        &Workload::TRANSFER_350,
        &MachineProfile::REFERENCE,
        &CostModel::BASELINE,
        REQUIRED_RUNS,
    );
    wide.spread_pct = 7;
    assert_eq!(
        validate(&wide, &Workload::TRANSFER_350, &MachineProfile::REFERENCE)
            .unwrap_err()
            .to_string(),
        "P0: spread exceeds limit (7%/5%)"
    );
    // machine substitution
    let swapped_machine = MachineProfile {
        vcpu: 16,
        mem_gib: 32,
        storage: "nvme",
        validators: 100,
        net_mbit: 1000,
    };
    let swapped = BenchReport::build(
        &Workload::TRANSFER_350,
        &swapped_machine,
        &CostModel::BASELINE,
        REQUIRED_RUNS,
    );
    let err = validate(
        &swapped,
        &Workload::TRANSFER_350,
        &MachineProfile::REFERENCE,
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "P0: machine profile mismatch ({}/{})",
            GOLDEN_MACHINE_DIGEST,
            swapped_machine.digest()
        )
    );
    // published metrics (D-P3)
    let mut unreported = BenchReport::build(
        &Workload::TRANSFER_350,
        &MachineProfile::REFERENCE,
        &CostModel::BASELINE,
        REQUIRED_RUNS,
    );
    unreported.p99 = 0;
    assert_eq!(
        validate(
            &unreported,
            &Workload::TRANSFER_350,
            &MachineProfile::REFERENCE
        )
        .unwrap_err()
        .to_string(),
        "P0: metric not published (p99)"
    );
}

/// Metamorphism: the deterministic gate is a pure function — two
/// builds of the same plan are byte-identical.
#[test]
fn p0_metamorphic_determinism() {
    let a = BenchReport::build(
        &Workload::TRANSFER_350,
        &MachineProfile::REFERENCE,
        &CostModel::BASELINE,
        REQUIRED_RUNS,
    );
    let b = BenchReport::build(
        &Workload::TRANSFER_350,
        &MachineProfile::REFERENCE,
        &CostModel::BASELINE,
        REQUIRED_RUNS,
    );
    assert_eq!(a.json(), b.json());
    assert_eq!(a.canonical(), b.canonical());
}

/// Metamorphism: scaling the repeat count scales the totals and
/// changes the digest, keeping spread at 0.
#[test]
fn p0_metamorphic_runs_scaling() {
    let five = BenchReport::build(
        &Workload::TRANSFER_350,
        &MachineProfile::REFERENCE,
        &CostModel::BASELINE,
        REQUIRED_RUNS,
    );
    let ten = BenchReport::build(
        &Workload::TRANSFER_350,
        &MachineProfile::REFERENCE,
        &CostModel::BASELINE,
        10,
    );
    assert_eq!(ten.total, 1280);
    assert_eq!(ten.spread_pct, 0);
    assert_ne!(ten.bench_digest, five.bench_digest);
    assert_eq!(ten.p50, 128);
}

/// Metamorphism: nearest-rank percentiles and integer spread on a
/// skewed sample.
#[test]
fn p0_metamorphic_percentiles() {
    let sample = [1u64, 2, 3, 4, 100];
    assert_eq!(percentile(&sample, 50), 3);
    assert_eq!(percentile(&sample, 95), 100);
    assert_eq!(percentile(&sample, 99), 100);
    assert_eq!(spread_pct(&sample), 450);
    // p50 <= p95 <= p99 always
    assert!(percentile(&sample, 50) <= percentile(&sample, 95));
    assert!(percentile(&sample, 95) <= percentile(&sample, 99));
}

/// Metamorphism: cost model arithmetic is exposed — 350B/1-in/1-out
/// costs 128 units stage by stage (4+21 decode, 38 verify,
/// 10+15 execute, 40 commit).
#[test]
fn p0_metamorphic_cost_breakdown() {
    let model = CostModel::BASELINE;
    let decode = model.decode_base + (350 / 16) * model.decode_per_16b;
    assert_eq!(decode, 25);
    assert_eq!(model.verify_per_input, 38);
    assert_eq!(model.execute_base + model.execute_per_output, 25);
    assert_eq!(model.commit, 40);
    assert_eq!(model.run_cost(&Workload::TRANSFER_350), 128);
    assert_eq!(model.run_cost(&Workload::TRANSFER_352), 129);
}
