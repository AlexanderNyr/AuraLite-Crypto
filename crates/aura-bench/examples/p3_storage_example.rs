//! P3 storage bench example — deterministic golden output.
//!
//! Output is byte-identical to `tools/p3_storage_check.py` (the
//! independent Python model). `--measured` adds honest wall-clock
//! numbers of a real file with real fsync (evidence only, D-P9 —
//! never byte-compared, never a gate).

use aura_bench::p3::{
    COMMIT_SLO_UNITS, P3Report, Store, gate_batch_equiv, gate_bloom, gate_compact, gate_durability,
    measure,
};

fn main() {
    // 1. S7 durability: a committed batch survives a crash (fsync is
    //    in the commit path).
    gate_durability().expect("S7 durability");
    // 2. Bloom: no false negatives; the fixture's exact FP count.
    let fp = gate_bloom().expect("bloom correctness");
    // 3. Batch equivalence: one batch == many single commits.
    let batch: Vec<(u64, Vec<u8>)> = (0..16u64).map(|i| (i, vec![i as u8; 8])).collect();
    gate_batch_equiv(&batch).expect("batch equivalence");
    // 4. Compaction preserves the state exactly.
    gate_compact().expect("compaction preserves state");
    // 5. Engine determinism (X7).
    let mut a = Store::new();
    a.commit(&batch);
    let mut b = Store::new();
    b.commit(&batch);
    assert_eq!(a, b, "the store is deterministic");

    // 6. Deterministic report + optimization gates.
    let report = P3Report::build(fp, 5);
    report.validate().expect("P3 gates green");
    assert_eq!(report.slo, COMMIT_SLO_UNITS);

    println!("P3-RUNS-OK {}", report.runs);
    println!("P3-STORAGE-OK {}", report.json());

    // 7. Measured layer (evidence only, D-P9).
    if std::env::args().any(|a| a == "--measured") {
        let m = measure(5);
        println!("P3-MEASURED {}", m.json());
    }
}
