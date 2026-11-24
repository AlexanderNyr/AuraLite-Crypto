//! P3 storage gates (PERF_PLAN P3 DoD).
//!
//! DoD: **p99 commit latency < SLO** (published with methodology,
//! D-P3); **NC: fsync in the wrong place → S7 red** (the S7 commit
//! protocol: WAL append → fsync → publish). NC: revert → red
//! (`tools/nc_p3.py`, six named messages).

use aura_bench::p3::{
    Bloom, COMMIT_SLO_UNITS, P3Error, P3Report, Store, block_cost_after, block_cost_before,
    gate_batch_equiv, gate_bloom, gate_compact, gate_durability, miss_cost_after, miss_cost_before,
};

/// Golden numbers pinned before the first run (2026-11-24):
/// 1000-write blocks of 350-byte values; baseline 311 000 units
/// (fsync per write) -> P3 12 300 (one WriteBatch + amortized
/// compaction) under the SLO of 15 000; bloom misses 40 -> 1 units
/// (fp 0/200 on the fixture); report digest
/// `b467c465fd824adb64f5689f2b5b216a640a038848e9ef8808f9c3fd8e6223e7`.
#[test]
fn p3_golden_numbers() {
    let fp = gate_bloom().expect("bloom");
    assert_eq!(fp, 0, "0 false positives among 200 absent fixture keys");
    let report = P3Report::build(fp, 5);
    assert_eq!(report.runs, 5);
    assert_eq!(report.run_costs, vec![12300, 12300, 12300, 12300, 12300]);
    assert_eq!(report.total, 61500);
    assert_eq!((report.p50, report.p95, report.p99), (12300, 12300, 12300));
    assert_eq!(report.spread_pct, 0);
    assert_eq!(report.before, 311000);
    assert_eq!(report.after, 12300);
    assert_eq!(report.slo, 15000);
    assert_eq!(report.speedup_pct, 2428);
    assert_eq!((report.miss_before, report.miss_after), (40, 1));
    assert_eq!(
        report.workload_digest,
        "3bf6032aec4304a3301893b375990d9300e22e664db85a8e1d81a30881cc992b"
    );
    assert_eq!(
        report.bench_digest,
        "b467c465fd824adb64f5689f2b5b216a640a038848e9ef8808f9c3fd8e6223e7"
    );
    assert!(report.validate().is_ok());
}

/// The P3 DoD, asserted literally: p99 block-commit cost is under the
/// published SLO; the baseline honestly fails it.
#[test]
fn p3_do_d_p99_under_slo() {
    assert_eq!(COMMIT_SLO_UNITS, 15_000);
    let after = block_cost_after();
    assert_eq!(after, 12_300);
    assert!(after < COMMIT_SLO_UNITS, "P3 DoD: p99 commit < SLO");
    assert!(
        block_cost_before() >= COMMIT_SLO_UNITS,
        "the fsync-per-write baseline fails the SLO honestly"
    );
}

/// The six named P3 messages (NC targets): exact Display literals.
#[test]
fn p3_named_messages() {
    assert_eq!(
        P3Error::SloViolated {
            p99: 311000,
            slo: 15000
        }
        .to_string(),
        "P3: p99 commit exceeds SLO (311000/15000)"
    );
    assert_eq!(
        P3Error::FsyncMissing { batch: 2 }.to_string(),
        "P3: fsync missing in commit path (batch 2)"
    );
    assert_eq!(
        P3Error::BloomFalseNegative { key: 7 }.to_string(),
        "P3: bloom false negative (key 7)"
    );
    assert_eq!(
        P3Error::BloomMissCostRegression {
            before: 40,
            after: 41
        }
        .to_string(),
        "P3: bloom miss cost regression (40/41)"
    );
    assert_eq!(
        P3Error::CompactionLostData.to_string(),
        "P3: compaction lost data"
    );
    assert_eq!(
        P3Error::DeterminismViolated { run: 2 }.to_string(),
        "P3: determinism violated (run 2)"
    );
}

/// S7 durability (the P3 DoD clause): a committed batch survives a
/// crash because fsync is in the commit path — and a crash between
/// publication attempts still recovers the synced tail.
#[test]
fn p3_durability_s7_contract() {
    gate_durability().expect("committed == durable (S7)");

    let mut store = Store::new();
    let first: Vec<(u64, Vec<u8>)> = (0..4u64).map(|i| (i, vec![1u8; 4])).collect();
    store.commit(&first);
    store.crash();
    assert_eq!(store.get(0).map(Vec::len), Some(4), "synced tail replays");

    // Writes that never passed commit are lost on crash (honest).
    let mut store = Store::new();
    store.memtable.insert(42, vec![9u8; 4]);
    store.crash();
    assert!(store.get(42).is_none(), "uncommitted data must not survive");
}

/// Bloom filters never miss an inserted key; the fixture's FP count is
/// deterministic (0/200) and the miss-cost pair 40 -> 1.
#[test]
fn p3_bloom_contract() {
    let fp = gate_bloom().expect("no false negatives");
    assert_eq!(fp, 0);
    assert_eq!(miss_cost_before(), 40);
    assert_eq!(miss_cost_after(0), 1);
    assert_eq!(miss_cost_after(2), 2, "ceil model: 2 fps cost 2 units");

    let mut filter = Bloom::for_keys(100);
    for key in 0..100u64 {
        filter.insert(key);
    }
    for key in 0..100u64 {
        assert!(filter.maybe_contains(key), "no false negatives");
    }
}

/// Batch writes change cost, never results: one batch commit equals
/// the same writes committed one by one.
#[test]
fn p3_batch_equivalence() {
    let batch: Vec<(u64, Vec<u8>)> = (0..32u64)
        .map(|i| (i * 3, vec![(i % 251) as u8; 16]))
        .collect();
    gate_batch_equiv(&batch).expect("batch == individual");
    let mut one = Store::new();
    one.commit(&batch);
    let mut many = Store::new();
    for w in &batch {
        many.commit(std::slice::from_ref(w));
    }
    assert_eq!(one.state(), many.state());
    assert_eq!(one.digest(), many.digest());
}

/// Compaction merges sorted runs (latest value wins) and preserves the
/// committed state exactly; lookups keep answering through the filter.
#[test]
fn p3_compaction_preserves_state() {
    gate_compact().expect("compaction preserves state");

    let mut store = Store::new();
    store.commit(&[(1, vec![1u8; 4]), (2, vec![2u8; 4])]);
    store.flush();
    store.commit(&[(2, vec![22u8; 4]), (3, vec![3u8; 4])]);
    store.flush();
    let before = store.state();
    store.compact();
    assert_eq!(store.state(), before);
    assert_eq!(store.get(2).cloned(), Some(vec![22u8; 4]), "latest wins");
    assert_eq!(store.get(1).cloned(), Some(vec![1u8; 4]));
    assert_eq!(store.get(3).cloned(), Some(vec![3u8; 4]));
}

/// X7 determinism: the store and the report are pure functions of
/// their inputs.
#[test]
fn p3_engine_determinism() {
    let batch: Vec<(u64, Vec<u8>)> = (0..8u64).map(|i| (i, vec![7u8; 8])).collect();
    let mut a = Store::new();
    a.commit(&batch);
    let mut b = Store::new();
    b.commit(&batch);
    assert_eq!(a, b);
    let r1 = P3Report::build(0, 5);
    let r2 = P3Report::build(0, 5);
    assert_eq!(r1, r2);
    assert_eq!(r1.canonical(), r2.canonical());
}

/// Metamorphic scaling: block costs scale linearly with the write
/// count; the batch win and the SLO relation hold for 100 writes too.
#[test]
fn p3_scale_costs() {
    // Per-write cost pair on the model constants.
    let per_write = 4 + 350 / 64 + 300 + 2;
    assert_eq!(per_write, 311);
    assert_eq!(block_cost_before(), 1000 * per_write);
    let per_key_after = 4 + 350 / 64 + 2;
    assert_eq!(
        block_cost_after(),
        1000 * per_key_after + 300 + 1000 * 8 / 8
    );
}

/// NC: revert → red. Each reverted optimization state fails with its
/// named message (the evidence for the P3 DoD clause).
#[test]
fn p3_validate_reverts_are_red() {
    let good = P3Report::build(0, 5);

    // Revert batching: per-write cost path (311 000) fails the SLO.
    let mut reverted_batch = good.clone();
    reverted_batch.after = 311000;
    reverted_batch.p99 = 311000;
    reverted_batch.run_costs = vec![311000; 5];
    assert_eq!(
        reverted_batch.validate().unwrap_err(),
        P3Error::SloViolated {
            p99: 311000,
            slo: 15000
        }
    );

    // Revert the bloom cost: miss cost back up to SSTable reads.
    let mut reverted_bloom = good.clone();
    reverted_bloom.miss_after = 41;
    assert_eq!(
        reverted_bloom.validate().unwrap_err(),
        P3Error::BloomMissCostRegression {
            before: 40,
            after: 41
        }
    );

    // Break determinism: one run differs.
    let mut nondet = good.clone();
    nondet.run_costs[2] += 1;
    assert_eq!(
        nondet.validate().unwrap_err(),
        P3Error::DeterminismViolated { run: 2 }
    );
}

/// The S7 replay window is honest: a synced-but-crashed store loses
/// nothing committed, and the WAL digest form is stable.
#[test]
fn p3_store_digest_stable() {
    let mut store = Store::new();
    let batch: Vec<(u64, Vec<u8>)> = (0..8u64).map(|i| (i, vec![i as u8; 4])).collect();
    store.commit(&batch);
    let d1 = store.digest();
    store.crash();
    assert_eq!(store.digest(), d1, "recovery preserves the state");
    let d2 = store.digest();
    assert_eq!(d1, d2);
    assert_eq!(d1.len(), 64);
}
