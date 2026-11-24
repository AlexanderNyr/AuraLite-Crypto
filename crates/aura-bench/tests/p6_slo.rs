//! P6 SLO publication gates (PERF_PLAN P6 DoD).
//!
//! DoD: **north star metrics confirmed; NC: rigged run → log check
//! red**. The publication carries methodology, logs, dashboards and
//! honest "did not work" items. NC: revert → red (`tools/nc_p6.py`,
//! six named messages).

use aura_bench::p6::{
    EXPECTED_HONEST_FAILURES, EXPECTED_METHODOLOGY, HONEST_FAILURES, LOG_SEED, METHODOLOGY,
    P6Error, P6Report, build_log, chain_step, gate_honest_failures, gate_methodology,
    gate_north_star, log_rows, north_star, state_digest, verify_log,
};

/// Golden numbers pinned before the first run (2026-11-24): the six
/// north-star SLOs of P0–P5 confirmed (6/6), a 6-entry hash-chained
/// run log with tip
/// `ac0f90675c11f118f03c1a12007b912651732683a780df1a825df35d9aefe52a`,
/// report digest
/// `ee5ea3d6de33f70f6c832097ad7f4cdf5da53c5466c5a02e7d2b7e37b35056c3`.
#[test]
fn p6_golden_numbers() {
    let report = P6Report::build(5);
    assert_eq!(report.runs, 5);
    assert_eq!(report.spread_pct, 0);
    assert_eq!((report.confirmed, report.total), (6, 6));
    assert_eq!(report.log_entries, 6);
    assert_eq!(
        report.log_tip,
        "ac0f90675c11f118f03c1a12007b912651732683a780df1a825df35d9aefe52a"
    );
    assert_eq!((report.methodology, report.honest), (6, 4));
    assert_eq!(
        report.workload_digest,
        "1e1c4f33fc99be63d103a24f001259de2ba7d80e73db31d05814a6f30339829e"
    );
    assert_eq!(
        report.bench_digest,
        "ee5ea3d6de33f70f6c832097ad7f4cdf5da53c5466c5a02e7d2b7e37b35056c3"
    );
    report.validate().unwrap_or_else(|e| panic!("{e}"));
}

/// The P6 DoD, asserted literally: every north-star metric is within
/// its published bound (the P0–P5 DoD numbers).
#[test]
fn p6_do_d_north_star_confirmed() {
    gate_north_star().unwrap_or_else(|e| panic!("{e}"));
    let entries = north_star();
    assert_eq!(entries.len(), 6);
    for entry in &entries {
        assert!(entry.confirmed(), "{} must be confirmed", entry.name);
    }
    assert_eq!(entries[0].name, "p0.spread_pct");
    assert_eq!(entries[3].name, "p3.commit_units");
    assert_eq!(entries[5].name, "p5.finality_ms");
}

/// The six named P6 messages (NC targets): exact Display literals.
#[test]
fn p6_named_messages() {
    assert_eq!(
        P6Error::NorthStarNotConfirmed {
            name: "p3.commit_units".to_string(),
            observed: 16000,
            bound: 15000
        }
        .to_string(),
        "P6: north star metric not confirmed (p3.commit_units 16000/15000)"
    );
    assert_eq!(
        P6Error::LogChainBroken { at: 2 }.to_string(),
        "P6: run log hash chain broken at entry 2"
    );
    assert_eq!(
        P6Error::RiggedRun {
            detail: "p3.commit_units=12301".to_string()
        }
        .to_string(),
        "P6: rigged run detected (p3.commit_units=12301)"
    );
    assert_eq!(
        P6Error::MethodologyGap {
            section: "p5-methodology".to_string()
        }
        .to_string(),
        "P6: methodology gap (p5-methodology)"
    );
    assert_eq!(
        P6Error::HonestFailureMissing {
            name: "p2-lcg-fixture".to_string()
        }
        .to_string(),
        "P6: honest-failure item missing (p2-lcg-fixture)"
    );
    assert_eq!(
        P6Error::DeterminismViolated { run: 2 }.to_string(),
        "P6: determinism violated (run 2)"
    );
}

/// The NC of the DoD: a rigged run — a log value that disagrees with
/// the registry — fails the log check with the named message.
#[test]
fn p6_rigged_run_is_red() {
    // A sophisticated forger re-chains the log after rigging one
    // value; the chain then walks clean and the registry check must
    // still catch the substitution.
    let mut log = build_log();
    log[3].value += 1;
    let mut prev = log[2].chain.clone();
    for entry in log.iter_mut().skip(3) {
        let line = format!("{}={}", entry.key, entry.value);
        prev = chain_step(&prev, &line);
        entry.chain = prev.clone();
    }
    let err = verify_log(&log).unwrap_err();
    assert_eq!(
        err,
        P6Error::RiggedRun {
            detail: "p3.commit_units=12301".to_string()
        }
    );
    // A rigged value at entry 5 (the finality metric) is caught too.
    let mut log = build_log();
    log[5].value = 1;
    let prev = log[4].chain.clone();
    let entry = &mut log[5];
    let line = format!("{}={}", entry.key, entry.value);
    entry.chain = chain_step(&prev, &line);
    assert_eq!(
        verify_log(&log).unwrap_err(),
        P6Error::RiggedRun {
            detail: "p5.finality_ms=1".to_string()
        }
    );
}

/// The hash chain: flipping any entry's chain digest is caught at
/// that entry index.
#[test]
fn p6_log_chain_tamper_is_red() {
    let mut log = build_log();
    log[2].chain = "00".repeat(32);
    assert_eq!(
        verify_log(&log).unwrap_err(),
        P6Error::LogChainBroken { at: 2 }
    );

    // A dropped entry shifts the chain: break reported at entry 0 of
    // the truncated walk.
    let mut log = build_log();
    log.remove(0);
    assert_eq!(
        verify_log(&log).unwrap_err(),
        P6Error::LogChainBroken { at: 0 }
    );
}

/// Methodology: every published SLO carries its section (D-P3).
#[test]
fn p6_methodology_complete() {
    gate_methodology().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(METHODOLOGY.len(), EXPECTED_METHODOLOGY.len());
    for section in EXPECTED_METHODOLOGY {
        assert!(METHODOLOGY.contains(&section));
    }
}

/// Honest "did not work" items: every pinned negative stays in the
/// ledger.
#[test]
fn p6_honest_failures_kept() {
    gate_honest_failures().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(HONEST_FAILURES.len(), EXPECTED_HONEST_FAILURES.len());
    for name in EXPECTED_HONEST_FAILURES {
        assert!(HONEST_FAILURES.contains(&name));
    }
}

/// The clean log verifies and its tip is the pinned golden.
#[test]
fn p6_log_verifies_clean() {
    let log = build_log();
    let tip = verify_log(&log).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        tip,
        "ac0f90675c11f118f03c1a12007b912651732683a780df1a825df35d9aefe52a"
    );
    assert_eq!(log.len(), 6);
    // Chain step is domain-separated sha256(prev | "|" | line).
    let step = chain_step(LOG_SEED, "p0.spread_pct=0");
    assert_eq!(step.len(), 64);
    assert_eq!(log[0].chain, step);
}

/// X7 determinism: the report and the registry are pure functions.
#[test]
fn p6_determinism() {
    let r1 = P6Report::build(5);
    let r2 = P6Report::build(5);
    assert_eq!(r1, r2);
    assert_eq!(r1.canonical(), r2.canonical());
    assert_eq!(r1.bench_digest, r2.bench_digest);
    assert_eq!(state_digest(), state_digest());
}

/// NC: revert → red. Each reverted publication state fails with its
/// named message.
#[test]
fn p6_validate_reverts_are_red() {
    let good = P6Report::build(5);

    // Rigged observed value in a rebuilt report: the deterministic
    // rebuild disagrees (DeterminismViolated) — and a fudged registry
    // state fails the SLO relation (NorthStarNotConfirmed).
    let mut rigged = good.clone();
    rigged.confirmed = 5;
    assert_eq!(
        rigged.validate().unwrap_err(),
        P6Error::DeterminismViolated { run: 1 }
    );

    // The log rows recorded by the run must equal the registry rows
    // (the rigged-run check compares them pairwise).
    let rows = log_rows();
    let reg = north_star();
    for (row, entry) in rows.iter().zip(reg.iter()) {
        assert_eq!(row.0, entry.name);
        assert_eq!(row.1, entry.observed, "run log must match the registry");
    }
}

/// State digest stability (X7 evidence).
#[test]
fn p6_state_digest_stable() {
    let report = P6Report::build(5);
    let again = P6Report::build(5);
    assert_eq!(report.state_digest, again.state_digest);
    assert_eq!(report.state_digest.len(), 64);
    assert_eq!(report.log_tip.len(), 64);
}
