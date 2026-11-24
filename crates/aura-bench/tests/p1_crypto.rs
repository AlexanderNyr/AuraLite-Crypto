//! P1 codec/crypto gates (PERF_PLAN P1 DoD).
//!
//! DoD: batch verification + BLS aggregation (cost model) + zero-copy
//! decode; **+72% verify/s** in the deterministic model with the same
//! determinism; NC: revert → red (`tools/nc_p1.py`, six named messages).

use aura_bench::p1::{
    BATCH_K, P1Error, P1Report, VERIFY_SPEEDUP_PCT, aggregate_verify_cost, decode_cost,
    decode_cost_before, gate_batch, gate_malformed, gate_zero_copy, individual_verify_cost,
    run_cost, verify_cost_amortized,
};
use aura_bench::{MachineProfile, Workload};
use aura_codec::transfer::{
    TRANSFER_LEN, TransferError, TransferView, decode_copy, decode_zero_copy, encode, sample_wire,
};
use aura_crypto::{BatchItem, ed25519_public_key, sign_raw, verify_batch_raw, verify_raw};

fn signed_item(i: u8) -> ([u8; 32], Vec<u8>, [u8; 64]) {
    let mut seed = [0u8; 32];
    for (j, b) in seed.iter_mut().enumerate() {
        *b = (0x71 + i) ^ (j as u8).wrapping_mul(5);
    }
    let message = format!("p1-test-{i}").into_bytes();
    let public = ed25519_public_key(&seed);
    let signature = sign_raw(&seed, &message);
    (public, message, signature)
}

fn signed_batch() -> Vec<([u8; 32], Vec<u8>, [u8; 64])> {
    (0..BATCH_K as u8).map(signed_item).collect()
}

fn items_of(batch: &[([u8; 32], Vec<u8>, [u8; 64])]) -> Vec<BatchItem<'_>> {
    batch
        .iter()
        .map(|(p, m, s)| BatchItem {
            public: p,
            message: m,
            signature: s,
        })
        .collect()
}

/// Golden numbers pinned before the first run (2026-11-24):
/// per-run 99 units, 5 runs -> total 495, verify 38 -> 22 (+72%
/// verify/s), decode 25 -> 12, BLS model 60 vs 352, digest
/// `340a884d17a5ca58111c00fcd55229f7cfce4175fcfb8c97cf69fe2f52c80241`.
#[test]
fn p1_golden_numbers() {
    let report = P1Report::build(&Workload::TRANSFER_350, &MachineProfile::REFERENCE, 5);
    assert_eq!(report.runs, 5);
    assert_eq!(report.run_costs, vec![99, 99, 99, 99, 99]);
    assert_eq!(report.total, 495);
    assert_eq!((report.p50, report.p95, report.p99), (99, 99, 99));
    assert_eq!(report.spread_pct, 0);
    assert_eq!((report.verify_before, report.verify_after), (38, 22));
    assert_eq!((report.decode_before, report.decode_after), (25, 12));
    assert_eq!(report.agg_k, 8);
    assert_eq!(report.agg_cost, 60);
    assert_eq!(
        report.bench_digest,
        "340a884d17a5ca58111c00fcd55229f7cfce4175fcfb8c97cf69fe2f52c80241"
    );
    assert!(report.validate().is_ok());
}

/// The P1 DoD number, asserted literally: +72% verify/s in the model.
#[test]
fn p1_do_d_verify_speedup() {
    assert_eq!(VERIFY_SPEEDUP_PCT, 72);
    let gain = 38 * 100 / verify_cost_amortized(BATCH_K) - 100;
    assert_eq!(gain, 72, "P1 DoD: +72% verify/s (38 -> 22 cost units)");
    // BLS model: 5.8x cheaper than individual verification (D-P4).
    assert_eq!(aggregate_verify_cost(8), 60);
    assert_eq!(individual_verify_cost(8), 352);
    // Zero-copy decode: 12 vs 25 units for the 350-byte block.
    assert_eq!(decode_cost(350), 12);
    assert_eq!(decode_cost_before(350), 25);
}

/// The six named P1 messages (NC targets): exact Display literals.
#[test]
fn p1_named_messages() {
    assert_eq!(
        P1Error::BatchMismatch { item: 3 }.to_string(),
        "P1: batch verify mismatch (item 3)"
    );
    assert_eq!(
        P1Error::ZeroCopyMismatch.to_string(),
        "P1: zero-copy decode mismatch"
    );
    assert_eq!(
        P1Error::MalformedAccepted {
            case: "bad-version".into()
        }
        .to_string(),
        "P1: malformed input accepted (bad-version)"
    );
    assert_eq!(
        P1Error::VerifySpeedupRegression {
            before: 38,
            after: 38
        }
        .to_string(),
        "P1: verify speedup regression (38/38)"
    );
    assert_eq!(
        P1Error::AggregateVerifyRegression {
            before: 352,
            after: 352
        }
        .to_string(),
        "P1: aggregate verify regression (352/352)"
    );
    assert_eq!(
        P1Error::DeterminismViolated { run: 2 }.to_string(),
        "P1: determinism violated (run 2)"
    );
}

/// Batch verification agrees with the sequential verdict on a valid
/// batch and on a batch with one corrupted signature.
#[test]
fn p1_batch_matches_sequential() {
    let batch = signed_batch();
    let items = items_of(&batch);
    assert!(verify_batch_raw(&items).is_ok());
    assert!(gate_batch(&items).is_ok());

    let mut corrupt = signed_batch();
    corrupt[3].2[10] ^= 0xFF;
    let items = items_of(&corrupt);
    assert!(verify_batch_raw(&items).is_err());
    assert!(gate_batch(&items).is_ok(), "both paths reject: agreement");
}

/// The batch path keeps the strict canonical-S semantics of the single
/// verifier (H2): an S+L malleated twin must be rejected by
/// `verify_batch_raw` exactly like `verify_raw` (NC `bad-batch` removes
/// the pre-check and lets the ZIP215 batch equation accept the twin).
#[test]
fn p1_batch_rejects_malleated() {
    let (public, message, signature) = signed_item(1);
    const GROUP_ORDER_LE: [u8; 32] = [
        0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde,
        0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x10,
    ];
    let mut twin = signature;
    let mut carry = 0u16;
    for i in 0..32 {
        let sum = u16::from(signature[32 + i]) + u16::from(GROUP_ORDER_LE[i]) + carry;
        twin[32 + i] = sum as u8;
        carry = sum >> 8;
    }
    let items = [BatchItem {
        public: &public,
        message: &message,
        signature: &twin,
    }];
    assert_eq!(
        verify_batch_raw(&items),
        Err((0, aura_crypto::CryptoError::NonCanonicalSignature))
    );
    assert_eq!(
        verify_raw(&public, &message, &twin),
        Err(aura_crypto::CryptoError::NonCanonicalSignature),
        "the sequential verifier rejects the twin too — same semantics"
    );
}

/// Zero-copy decode == copy decode (field by field) on the sample block.
#[test]
fn p1_zero_copy_equivalence() {
    let wire = sample_wire();
    let view = decode_zero_copy(&wire).expect("zero-copy decode");
    let owned = decode_copy(&wire).expect("copy decode");
    assert_eq!(view.version, owned.version);
    assert_eq!(*view.from, owned.from);
    assert_eq!(*view.to, owned.to);
    assert_eq!(view.amount, owned.amount);
    assert_eq!(view.nonce, owned.nonce);
    assert_eq!(view.memo, owned.memo.as_slice());
    assert_eq!(view.memo, b"p1-sample");
    assert!(gate_zero_copy(&wire).is_ok());

    // Re-encode round trip: one value, one encoding.
    let mut out = [0u8; TRANSFER_LEN];
    encode(&view, &mut out).expect("encode");
    assert_eq!(out, wire, "canonical wire form is stable");
}

/// All four malformed cases are rejected by both decoders with the
/// right case names.
#[test]
fn p1_malformed_rejected() {
    let wire = sample_wire();

    let short = &wire[..100];
    assert_eq!(
        decode_zero_copy(short).unwrap_err(),
        TransferError::TooShort { len: 100 }
    );
    assert!(decode_copy(short).is_err());
    assert!(gate_malformed("short-buffer", short).is_ok());

    let mut overlong = wire.to_vec();
    overlong.push(0);
    assert_eq!(
        decode_zero_copy(&overlong).unwrap_err(),
        TransferError::Overlong { len: 351 }
    );
    assert!(decode_copy(&overlong).is_err());
    assert!(gate_malformed("overlong", &overlong).is_ok());

    let mut bad_version = wire;
    bad_version[1] = 9;
    assert_eq!(
        decode_zero_copy(&bad_version).unwrap_err(),
        TransferError::BadVersion { got: 0x0901 }
    );
    assert!(decode_copy(&bad_version).is_err());
    assert!(gate_malformed("bad-version", &bad_version).is_ok());

    let mut bad_padding = sample_wire();
    bad_padding[TRANSFER_LEN - 1] = 1;
    assert_eq!(
        decode_zero_copy(&bad_padding).unwrap_err(),
        TransferError::BadMemoPadding
    );
    assert!(decode_copy(&bad_padding).is_err());
    assert!(gate_malformed("bad-memo-padding", &bad_padding).is_ok());
}

/// The malformed-input gate fires with its named message when an input
/// labeled malformed is actually accepted.
#[test]
fn p1_malformed_gate_named() {
    let wire = sample_wire();
    let err = gate_malformed("bad-version", &wire).unwrap_err();
    assert_eq!(
        err.to_string(),
        "P1: malformed input accepted (bad-version)"
    );
}

/// NC: revert → red. Each reverted optimization state fails with its
/// named message (the evidence for the P1 DoD clause).
#[test]
fn p1_validate_reverts_are_red() {
    let good = P1Report::build(&Workload::TRANSFER_350, &MachineProfile::REFERENCE, 5);

    // Revert the batch coefficient: verify_after back to single cost.
    let mut reverted_verify = good.clone();
    reverted_verify.verify_after = 38;
    assert_eq!(
        reverted_verify.validate().unwrap_err(),
        P1Error::VerifySpeedupRegression {
            before: 38,
            after: 38
        }
    );

    // Revert zero-copy decode: decode_after back to the baseline cost.
    let mut reverted_decode = good.clone();
    reverted_decode.decode_after = 25;
    assert_eq!(
        reverted_decode.validate().unwrap_err(),
        P1Error::VerifySpeedupRegression {
            before: 25,
            after: 25
        }
    );

    // Revert the BLS aggregate to individual cost.
    let mut reverted_agg = good.clone();
    reverted_agg.agg_cost = individual_verify_cost(reverted_agg.agg_k);
    assert_eq!(
        reverted_agg.validate().unwrap_err(),
        P1Error::AggregateVerifyRegression {
            before: 352,
            after: 352
        }
    );

    // Break determinism: one run differs.
    let mut nondet = good.clone();
    nondet.run_costs[2] += 1;
    assert_eq!(
        nondet.validate().unwrap_err(),
        P1Error::DeterminismViolated { run: 2 }
    );
}

/// X7 determinism: two builds of the report are bit-identical and the
/// cost functions are pure.
#[test]
fn p1_determinism_pure() {
    let a = P1Report::build(&Workload::TRANSFER_350, &MachineProfile::REFERENCE, 5);
    let b = P1Report::build(&Workload::TRANSFER_350, &MachineProfile::REFERENCE, 5);
    assert_eq!(a, b, "the model is a pure function of its inputs");
    assert_eq!(a.canonical(), b.canonical());
    assert_eq!(run_cost(&Workload::TRANSFER_350, BATCH_K), 99);
    assert_eq!(run_cost(&Workload::TRANSFER_350, BATCH_K), 99);
}

/// Metamorphic scaling: the run count scales the total linearly and
/// never changes the per-run cost (spread stays 0).
#[test]
fn p1_scale_runs() {
    let report = P1Report::build(&Workload::TRANSFER_350, &MachineProfile::REFERENCE, 10);
    assert_eq!(report.total, 990);
    assert_eq!(report.spread_pct, 0);
    assert_eq!(report.run_costs.len(), 10);
    assert!(report.validate().is_ok());
}

/// The zero-copy view borrows from the input: the same bytes decode to
/// equal views twice, and a decoded view matches a fresh encode/decode
/// cycle (metamorphism of the canonical wire form).
#[test]
fn p1_wire_canonical_metamorphism() {
    let wire = sample_wire();
    let view = decode_zero_copy(&wire).expect("decode");
    let mut rebuilt = [0u8; TRANSFER_LEN];
    encode(&view, &mut rebuilt).expect("encode");
    let view2 = decode_zero_copy(&rebuilt).expect("decode rebuilt");
    assert_eq!(view, view2);
    // Hand-built view with a longer memo still round-trips canonically.
    let from = [7u8; 32];
    let to = [9u8; 32];
    let long = TransferView {
        version: 1,
        from: &from,
        to: &to,
        amount: u64::MAX,
        nonce: u64::MAX,
        memo: b"longer-memo-still-fits",
    };
    let mut buf = [0u8; TRANSFER_LEN];
    encode(&long, &mut buf).expect("encode");
    let decoded = decode_copy(&buf).expect("decode");
    assert_eq!(decoded.amount, u64::MAX);
    assert_eq!(decoded.memo, b"longer-memo-still-fits");
}
