//! E5 oracle gate suite (ECOSYSTEM_PLAN E5).
//!
//! Gate: 5 providers, 2 malicious -> price unchanged (the two liars cannot
//! move the median); NC: one provider -> red (`E5: oracle requires a quorum
//! of provider reports`). Mutation controls live in `tools/nc_e5.py`.

use aura_crypto::ed25519_public_key;
use aura_storage::oracle::{
    AggregatedPrice, E5_ONE, E5Error, OracleNetwork, Price, PriceReport, Provider,
};

const FEED: [u8; 16] = *b"ALCRY/USD.......";

fn seed_for(id: u32) -> [u8; 32] {
    let mut seed = [0u8; 32];
    seed[0] = id as u8;
    seed[31] = 0xE5;
    seed
}

fn provider(id: u32) -> Provider {
    Provider {
        id,
        pubkey: ed25519_public_key(&seed_for(id)),
    }
}

fn five_provider_network() -> OracleNetwork {
    OracleNetwork::new(FEED, (1..=5).map(provider).collect())
}

fn report(id: u32, price: Price, timestamp: u64) -> PriceReport {
    PriceReport::sign(&seed_for(id), id, FEED, price, timestamp)
}

/// Gate (literal): 5 providers, 2 malicious -> price unchanged.
///
/// The three honest providers quote 100.00, 100.10 and 99.90 (18 decimals);
/// the two liars quote x100 manipulation values. The published price must be
/// the honest median.
#[test]
fn e5_property_two_malicious_do_not_move_price() {
    let mut net = five_provider_network();
    let now: u64 = 1_000_000;

    let honest: Vec<Price> = vec![
        100 * E5_ONE,
        100 * E5_ONE + E5_ONE / 10,
        99 * E5_ONE + 9 * E5_ONE / 10,
    ];
    let mut reports = vec![
        report(1, honest[0], now - 10),
        report(2, honest[1], now - 12),
        report(3, honest[2], now - 11),
    ];
    // Liars try to drag the price up by 100x and down to near zero.
    reports.push(report(4, 10_000 * E5_ONE, now - 10));
    reports.push(report(5, E5_ONE / 10_000, now - 10));

    let agg = net.submit(now, reports).expect("round must finalize");
    let mut sorted_honest = honest.clone();
    sorted_honest.sort_unstable();
    assert_eq!(
        agg.price, sorted_honest[1],
        "E5: two malicious providers must not move the median"
    );
    assert_eq!(agg.admitted, 5);
    assert!(
        agg.rejected_outliers >= 2,
        "liars must land outside the band"
    );
    assert_eq!(net.latest().map(|a| a.price), Some(sorted_honest[1]));
}

/// Gate (literal): NC — one provider -> red.
#[test]
fn e5_quorum_required() {
    let mut net = five_provider_network();
    let now: u64 = 1_000_000;
    let err = net
        .submit(now, vec![report(1, E5_ONE, now - 5)])
        .expect_err("one provider must not finalize a round");
    assert!(
        matches!(
            err,
            E5Error::Quorum {
                valid: 1,
                required: 3
            }
        ),
        "unexpected error: {err:?}"
    );
    assert_eq!(
        err.to_string(),
        "E5: oracle requires a quorum of provider reports (1/3)"
    );
}

#[test]
fn e5_bad_signature_rejected() {
    let mut net = five_provider_network();
    let now: u64 = 1_000_000;
    let mut evil = report(1, E5_ONE, now - 5);
    evil.signature[0] ^= 0x5A;
    let err = net
        .submit(now, vec![evil])
        .expect_err("forged signature must abort the round");
    assert!(matches!(err, E5Error::BadSignature { provider: 1 }));
    assert_eq!(
        err.to_string(),
        "E5: provider report signature must verify (provider 1)"
    );
}

#[test]
fn e5_stale_report_rejected() {
    let mut net = five_provider_network().with_staleness(100);
    let now: u64 = 1_000_000;
    let err = net
        .submit(now, vec![report(1, E5_ONE, now - 101)])
        .expect_err("stale report must abort the round");
    assert!(matches!(err, E5Error::Stale { provider: 1, .. }));
    assert!(
        err.to_string()
            .starts_with("E5: provider report must be fresh")
    );
}

#[test]
fn e5_future_report_rejected() {
    let mut net = five_provider_network();
    let now: u64 = 1_000_000;
    let err = net
        .submit(now, vec![report(1, E5_ONE, now + 1)])
        .expect_err("future report must abort the round");
    assert!(matches!(err, E5Error::Stale { provider: 1, .. }));
}

#[test]
fn e5_duplicate_provider_rejected() {
    let mut net = five_provider_network();
    let now: u64 = 1_000_000;
    let err = net
        .submit(
            now,
            vec![report(1, E5_ONE, now - 5), report(1, 2 * E5_ONE, now - 5)],
        )
        .expect_err("second report from one provider must abort the round");
    assert!(matches!(err, E5Error::DuplicateProvider { provider: 1 }));
    assert_eq!(
        err.to_string(),
        "E5: one report per provider per round (provider 1)"
    );
}

#[test]
fn e5_feed_mismatch_rejected() {
    let mut net = five_provider_network();
    let now: u64 = 1_000_000;
    // Sign for a foreign feed so the signature check cannot mask the feed check.
    let off = PriceReport::sign(&seed_for(1), 1, *b"BTC/USD.........", E5_ONE, now - 5);
    let err = net
        .submit(now, vec![off])
        .expect_err("wrong feed must abort the round");
    assert!(matches!(err, E5Error::FeedMismatch { provider: 1 }));
    assert_eq!(
        err.to_string(),
        "E5: all reports must target this feed (provider 1)"
    );
}

#[test]
fn e5_unknown_provider_rejected() {
    let mut net = five_provider_network();
    let now: u64 = 1_000_000;
    let err = net
        .submit(now, vec![report(9, E5_ONE, now - 5)])
        .expect_err("unregistered provider must abort the round");
    assert!(matches!(err, E5Error::UnknownProvider { provider: 9 }));
    assert_eq!(err.to_string(), "E5: report from unregistered provider 9");
}

#[test]
fn e5_no_reports_rejected() {
    let mut net = five_provider_network();
    let err = net
        .submit(1_000_000, vec![])
        .expect_err("empty round must abort");
    assert!(matches!(err, E5Error::NoReports));
    assert_eq!(err.to_string(), "E5: round requires at least one report");
}

#[test]
fn e5_outlier_rejected() {
    let mut net = five_provider_network();
    let now: u64 = 1_000_000;
    let agg = net
        .submit(
            now,
            vec![
                report(1, 100 * E5_ONE, now - 5),
                report(2, 101 * E5_ONE, now - 5),
                report(3, 99 * E5_ONE, now - 5),
                report(4, 102 * E5_ONE, now - 5),
                report(5, 10_000 * E5_ONE, now - 5),
            ],
        )
        .expect("round must finalize");
    assert_eq!(agg.rejected_outliers, 1);
    assert_eq!(agg.admitted, 5);
    assert_eq!(agg.price, 101 * E5_ONE);
}

/// Metamorphic: report order must not change the aggregate.
#[test]
fn e5_metamorphic_permutation() {
    let now: u64 = 1_000_000;
    let mk = || {
        vec![
            report(1, 100 * E5_ONE, now - 5),
            report(2, 101 * E5_ONE, now - 6),
            report(3, 99 * E5_ONE, now - 7),
            report(4, 102 * E5_ONE, now - 8),
            report(5, 10_000 * E5_ONE, now - 9),
        ]
    };
    let mut net_a = five_provider_network();
    let a = net_a.submit(now, mk()).unwrap();
    let mut flipped = mk();
    flipped.swap(0, 4);
    flipped.swap(1, 3);
    let mut net_b = five_provider_network();
    let b = net_b.submit(now, flipped).unwrap();
    assert_eq!(a, b, "E5: aggregation must be permutation invariant");
}

/// Metamorphic: scaling every price by 2 scales the median by 2.
#[test]
fn e5_metamorphic_scale() {
    let now: u64 = 1_000_000;
    let base_prices: Vec<Price> = vec![
        100 * E5_ONE,
        101 * E5_ONE,
        99 * E5_ONE,
        102 * E5_ONE,
        103 * E5_ONE,
    ];
    let base: Vec<PriceReport> = (1..=5)
        .map(|i| report(i, base_prices[i as usize - 1], now - 5))
        .collect();
    let mut net_a = five_provider_network();
    let a = net_a.submit(now, base).unwrap();

    let scaled: Vec<PriceReport> = (1..=5)
        .map(|i| report(i, 2 * base_prices[i as usize - 1], now - 5))
        .collect();
    let mut net_b = five_provider_network();
    let b = net_b.submit(now, scaled).unwrap();
    assert_eq!(b.price, a.price * 2, "E5: median must scale linearly");
}

/// Determinism: the same input round yields the same aggregate twice.
#[test]
fn e5_deterministic_repeat() {
    let now: u64 = 1_000_000;
    let mk = || {
        vec![
            report(1, 100 * E5_ONE, now - 5),
            report(2, 101 * E5_ONE, now - 6),
            report(3, 99 * E5_ONE, now - 7),
        ]
    };
    let mut net = five_provider_network();
    let a = net.submit(now, mk()).unwrap();
    let b = net.submit(now, mk()).unwrap();
    assert_eq!(a, b, "E5: aggregation must be deterministic");
}

#[test]
fn e5_aggregate_fields_sane() {
    let mut net = five_provider_network();
    let now: u64 = 1_000_000;
    let agg: AggregatedPrice = net
        .submit(
            now,
            vec![
                report(1, 100 * E5_ONE, now - 5),
                report(2, 101 * E5_ONE, now - 15),
                report(3, 99 * E5_ONE, now - 25),
            ],
        )
        .unwrap();
    assert_eq!(agg.feed, FEED);
    assert_eq!(agg.price, 100 * E5_ONE);
    assert_eq!(agg.timestamp, now - 15);
    assert_eq!(agg.admitted, 3);
    assert_eq!(agg.rejected_outliers, 0);
}
