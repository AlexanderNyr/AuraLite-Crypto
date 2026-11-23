//! E5 oracle simulation driver (ECOSYSTEM_PLAN E5).
//!
//! Deterministic driver mirroring the T8 whitepaper scenario driver: plays
//! the fixed `sim/e5-oracle.json` fixture (five signed provider reports, two
//! of them malicious), aggregates the price and prints a one-line JSON
//! summary. The expected aggregate is checked at the end, so a divergent
//! aggregation fails the run.

use std::process::exit;

use aura_crypto::ed25519_public_key;
use aura_storage::oracle::{OracleNetwork, Price, PriceReport, Provider};

const FEED: [u8; 16] = *b"ALCRY/USD.......";
const NOW: u64 = 1_000_000;
const STALENESS_SECS: u64 = 3600;
const DEVIATION_BPS: u64 = 500;

/// Fixture identical to `sim/e5-oracle.json`: (provider, price, timestamp).
fn fixture() -> Vec<(u32, Price, u64)> {
    vec![
        (1, 100 * E5_ONE_LOCAL, NOW - 10),
        (2, 100 * E5_ONE_LOCAL + E5_ONE_LOCAL / 10, NOW - 12),
        (3, 99 * E5_ONE_LOCAL + 9 * E5_ONE_LOCAL / 10, NOW - 11),
        (4, 10_000 * E5_ONE_LOCAL, NOW - 10),
        (5, E5_ONE_LOCAL / 10_000, NOW - 10),
    ]
}

const E5_ONE_LOCAL: Price = 1_000_000_000_000_000_000;

fn seed_for(id: u32) -> [u8; 32] {
    let mut seed = [0u8; 32];
    seed[0] = id as u8;
    seed[31] = 0xE5;
    seed
}

fn main() {
    // Optional: confirm the human-readable fixture is in place.
    if let Some(path) = std::env::args().nth(1)
        && !std::path::Path::new(&path).exists()
    {
        eprintln!("E5-SIM-ERR fixture file missing: {path}");
        exit(1);
    }

    let rows = fixture();
    let mut providers = Vec::new();
    let mut signed = Vec::new();
    for &(id, price, ts) in &rows {
        providers.push(Provider {
            id,
            pubkey: ed25519_public_key(&seed_for(id)),
        });
        signed.push(PriceReport::sign(&seed_for(id), id, FEED, price, ts));
    }

    let mut net = OracleNetwork::new(FEED, providers)
        .with_staleness(STALENESS_SECS)
        .with_deviation(DEVIATION_BPS);

    match net.submit(NOW, signed) {
        Ok(agg) => {
            println!(
                "E5-SIM-OK {{\"providers\": {}, \"admitted\": {}, \"rejected_outliers\": {}, \"price\": \"{}\", \"timestamp\": {}, \"quorum\": {}, \"staleness_secs\": {}}}",
                rows.len(),
                agg.admitted,
                agg.rejected_outliers,
                agg.price,
                agg.timestamp,
                net.quorum(),
                STALENESS_SECS,
            );
            // Expected aggregate (sim/e5-oracle.json "expected").
            let want_price: Price = 100 * E5_ONE_LOCAL;
            if agg.price != want_price
                || agg.admitted != 5
                || agg.rejected_outliers != 2
                || net.quorum() != 3
            {
                eprintln!(
                    "E5-SIM-ERR aggregate diverged from expected: price {}/{} admitted {}/5 rejected {}/2 quorum {}/3",
                    agg.price,
                    want_price,
                    agg.admitted,
                    agg.rejected_outliers,
                    net.quorum(),
                );
                exit(1);
            }
        }
        Err(e) => {
            eprintln!("E5-SIM-ERR {e}");
            exit(1);
        }
    }
}
