//! E4 AMM simulation driver (ECOSYSTEM_PLAN E4).
//!
//! Deterministic driver mirroring `tools/e4_amm_check.py`: plays the fixed
//! `sim/e4-amm.json` scenario (constant-product pool, 30bps fee, floor
//! rounding) — single swap, two-hop route, split-vs-single metamorphic run,
//! LP mint/burn round trip — checks that the invariant `k` never decreases,
//! and prints a one-line JSON summary. The expected values are checked at
//! the end, so a divergent curve fails the run.

use std::process::exit;

use aura_storage::amm::{Amount, Pool, Token, swap_route, wide_ge};

const E4_ONE_LOCAL: Amount = 1_000_000_000_000_000_000;

fn token_a() -> Token {
    *b"ALCRY..........."
}

fn token_b() -> Token {
    *b"AUSD............"
}

fn token_c() -> Token {
    *b"ABTC............"
}

/// Fixture identical to `sim/e4-amm.json`: (reserve_a, reserve_b) per pool.
fn fixture() -> [(Amount, Amount); 2] {
    [
        (100 * E4_ONE_LOCAL, 200 * E4_ONE_LOCAL),
        (100 * E4_ONE_LOCAL, 300 * E4_ONE_LOCAL),
    ]
}

fn main() {
    // Optional: confirm the human-readable fixture is in place.
    if let Some(path) = std::env::args().nth(1)
        && !std::path::Path::new(&path).exists()
    {
        eprintln!("E4-SIM-ERR fixture file missing: {path}");
        exit(1);
    }

    let [(ab_a, ab_b), (bc_a, bc_b)] = fixture();
    let mut k_ok = true;

    // 1. Single swap: 1 A -> B on (100/200).
    let mut ab = Pool::new(token_a(), token_b());
    ab.add_liquidity(ab_a, ab_b).expect("seed AB");
    let k0 = ab.invariant_k();
    let swap_out = ab.swap_exact_in(token_a(), E4_ONE_LOCAL, 0).expect("swap");
    k_ok &= wide_ge(&ab.invariant_k(), &k0);

    // 2. Route: 1 A -> B -> C through (100/200) and (100/300).
    let mut pools = [
        Pool::new(token_a(), token_b()),
        Pool::new(token_b(), token_c()),
    ];
    pools[0].add_liquidity(ab_a, ab_b).expect("seed AB");
    pools[1].add_liquidity(bc_a, bc_b).expect("seed BC");
    let k_ab = pools[0].invariant_k();
    let k_bc = pools[1].invariant_k();
    let route_out = swap_route(
        &mut pools,
        &[token_a(), token_b(), token_c()],
        E4_ONE_LOCAL,
        0,
    )
    .expect("route");
    k_ok &= wide_ge(&pools[0].invariant_k(), &k_ab);
    k_ok &= wide_ge(&pools[1].invariant_k(), &k_bc);

    // 3. Metamorphic: one 2A swap vs two 1A swaps (split must not win).
    let mut single = Pool::new(token_a(), token_b());
    single.add_liquidity(ab_a, ab_b).expect("seed single");
    let single_out = single
        .swap_exact_in(token_a(), 2 * E4_ONE_LOCAL, 0)
        .expect("single");
    let mut split = Pool::new(token_a(), token_b());
    split.add_liquidity(ab_a, ab_b).expect("seed split");
    let first = split.swap_exact_in(token_a(), E4_ONE_LOCAL, 0).expect("s1");
    let second = split.swap_exact_in(token_a(), E4_ONE_LOCAL, 0).expect("s2");
    let split_out = first + second;
    let split_le_single = split_out <= single_out;
    k_ok &= split_le_single;

    // 4. LP: initial mint, proportional mint, burn round trip.
    let mut lp_pool = Pool::new(token_a(), token_b());
    let lp_initial = lp_pool.add_liquidity(ab_a, ab_b).expect("mint");
    let lp_proportional = lp_pool
        .add_liquidity(50 * E4_ONE_LOCAL, 100 * E4_ONE_LOCAL)
        .expect("mint prop");
    let (burn_a, burn_b) = lp_pool.remove_liquidity(lp_proportional).expect("burn");

    println!(
        "E4-SIM-OK {{\"pools\": [[{},{}],[{},{}]], \"swap_out\": {}, \"route_out\": {}, \"single_out\": {}, \"split_out\": {}, \"split_le_single\": {}, \"lp_initial\": {}, \"lp_proportional\": {}, \"burn_out\": [{},{}], \"k_never_decreased\": {}}}",
        ab_a,
        ab_b,
        bc_a,
        bc_b,
        swap_out,
        route_out,
        single_out,
        split_out,
        split_le_single,
        lp_initial,
        lp_proportional,
        burn_a,
        burn_b,
        k_ok,
    );

    // Expected aggregate (sim/e4-amm.json "expected").
    let want = (
        1_974_316_068_794_122_597u128,
        5_791_186_053_878_244_695u128,
        3_910_033_923_564_131_223u128,
        3_909_976_989_011_504_086u128,
    );
    if (swap_out, route_out, single_out, split_out) != want
        || !split_le_single
        || !k_ok
        || lp_initial != 300 * E4_ONE_LOCAL
        || lp_proportional != 150 * E4_ONE_LOCAL
        || (burn_a, burn_b) != (50 * E4_ONE_LOCAL, 100 * E4_ONE_LOCAL)
    {
        eprintln!(
            "E4-SIM-ERR scenario diverged from expected: swap {swap_out} route {route_out} single {single_out} split {split_out} k_ok {k_ok}"
        );
        exit(1);
    }
}
