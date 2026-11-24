//! P1 codec/crypto bench example — deterministic golden output.
//!
//! Output is byte-identical to `tools/p1_crypto_check.py` (the
//! independent Python model). `--measured` adds honest wall-clock
//! before/after numbers of the real optimizations (evidence only,
//! D-P9 — never byte-compared, never a gate).

use aura_bench::p1::{
    self, BATCH_K, P1Report, gate_batch, gate_malformed, gate_zero_copy, measure,
};
use aura_bench::{MachineProfile, Workload, hex, sha256_32};
use aura_codec::transfer;
use aura_crypto::{BatchItem, ed25519_public_key, sign_raw};

/// Deterministic batch fixture: `BATCH_K` signed items
/// (public keys, messages, signatures).
type SignedBatch = (Vec<[u8; 32]>, Vec<Vec<u8>>, Vec<[u8; 64]>);

/// Deterministic batch fixture: `BATCH_K` signed items.
fn signed_batch() -> SignedBatch {
    let mut seeds = vec![[0u8; 32]; BATCH_K];
    let mut publics = vec![[0u8; 32]; BATCH_K];
    let mut messages = Vec::with_capacity(BATCH_K);
    let mut signatures = vec![[0u8; 64]; BATCH_K];
    for i in 0..BATCH_K {
        for (j, b) in seeds[i].iter_mut().enumerate() {
            *b = (0x31 + i as u8) ^ (j as u8).wrapping_mul(3);
        }
        messages.push(format!("p1-example-{i}").into_bytes());
        publics[i] = ed25519_public_key(&seeds[i]);
        signatures[i] = sign_raw(&seeds[i], &messages[i]);
    }
    (publics, messages, signatures)
}

fn main() {
    // 1. The four malformed cases must be rejected by both decoders.
    let wire = transfer::sample_wire();
    let mut short = wire[..100].to_vec();
    short.truncate(100);
    gate_malformed("short-buffer", &short).expect("short buffer rejected");
    let mut overlong = wire.to_vec();
    overlong.push(0);
    gate_malformed("overlong", &overlong).expect("overlong buffer rejected");
    let mut bad_version = wire;
    bad_version[1] = 9;
    gate_malformed("bad-version", &bad_version).expect("bad version rejected");
    let mut bad_padding = transfer::sample_wire();
    bad_padding[transfer::TRANSFER_LEN - 1] = 1;
    gate_malformed("bad-memo-padding", &bad_padding).expect("bad padding rejected");

    // 2. Zero-copy decode == copy decode on the sample block.
    gate_zero_copy(&transfer::sample_wire()).expect("zero-copy equivalence");

    // 3. Batch verification == sequential verification on the fixture.
    let (publics, messages, signatures) = signed_batch();
    let items: Vec<BatchItem<'_>> = (0..BATCH_K)
        .map(|i| BatchItem {
            public: &publics[i],
            message: &messages[i],
            signature: &signatures[i],
        })
        .collect();
    gate_batch(&items).expect("batch == sequential");

    // 4. Deterministic report + optimization gates.
    let work = Workload::TRANSFER_350;
    let machine = MachineProfile::REFERENCE;
    let report = P1Report::build(&work, &machine, 5);
    report.validate().expect("P1 gates green");

    let speedup = p1::VERIFY_BEFORE * 100 / report.verify_after - 100;
    let decode_cut = (report.decode_before - report.decode_after) * 100 / report.decode_before;
    let agg_individual = p1::individual_verify_cost(report.agg_k);

    println!("P1-RUNS-OK {}", report.runs);
    let base = report.json();
    let json = format!(
        "{}, \"verify_speedup_pct\": {}, \"decode_cut_pct\": {}, \"agg_individual\": {}}}",
        &base[..base.len() - 1],
        speedup,
        decode_cut,
        agg_individual
    );
    println!("P1-CRYPTO-OK {json}");

    // 5. sim/p1-crypto.json is written by `tools/p1_crypto_check.py`
    //    (the independent Python model owns the fixture file).

    // 6. Measured layer (evidence only, D-P9).
    if std::env::args().any(|a| a == "--measured") {
        let m = measure(7);
        println!("P1-MEASURED {}", m.json());
    }

    // Keep the golden digests honest in the example itself (X7): the
    // canonical form must hash to the pinned value.
    assert_eq!(
        hex(&sha256_32(report.canonical().as_bytes())),
        report.bench_digest
    );
}
