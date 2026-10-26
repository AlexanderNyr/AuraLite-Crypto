//! C9 gate: the portable fuzz harness must actually do work.

use aura_testkit::{FUZZ_TARGETS, FuzzOutcome, fuzz_one, run_portable_fuzz};

#[test]
fn harness_processes_the_requested_number_of_inputs() {
    // NC target: zeroing the iteration counter must turn this red without
    // hanging the run (the harness counts `work` separately).
    for target in FUZZ_TARGETS {
        let report = run_portable_fuzz(target, 0, 500, 0xC0FFEE);
        assert_eq!(report.iterations, 500, "target {target}");
        assert_eq!(report.crashes, 0, "target {target}");
        assert_eq!(report.target, target);
        assert_eq!(report.seed, 0xC0FFEE);
    }
}

#[test]
fn fuzz_one_handles_empty_and_long_inputs_and_unknown_targets() {
    for target in FUZZ_TARGETS {
        assert_eq!(fuzz_one(target, &[]), FuzzOutcome::Completed);
        assert_eq!(fuzz_one(target, &[0xFF; 4096]), FuzzOutcome::Completed);
    }
    assert_eq!(fuzz_one("does-not-exist", b"x"), FuzzOutcome::UnknownTarget);
}

#[test]
fn fuzz_seed_is_deterministic() {
    let first = run_portable_fuzz("codec", 0, 1000, 42);
    let second = run_portable_fuzz("codec", 0, 1000, 42);
    assert_eq!(first.iterations, second.iterations);
    assert_eq!(first.crashes, second.crashes);
}
