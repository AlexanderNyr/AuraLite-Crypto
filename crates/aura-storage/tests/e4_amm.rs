//! E4 AMM gate suite (ECOSYSTEM_PLAN E4).
//!
//! Gate (literal): the constant-product invariant `k` never decreases
//! across any swap; NC (literal): the slippage guard must hold —
//! `min_out` above the quote goes red. Mutation controls live in
//! `tools/nc_e4.py` (including "slippage guard off" -> red).

use aura_storage::amm::{Amount, E4_ONE, E4Error, Pool, Token, swap_route, wide_ge};

fn token_a() -> Token {
    *b"ALCRY..........."
}

fn token_b() -> Token {
    *b"AUSD............"
}

fn token_c() -> Token {
    *b"ABTC............"
}

/// Pool AB seeded with 100 A / 200 B.
fn pool_ab() -> Pool {
    let mut pool = Pool::new(token_a(), token_b());
    pool.add_liquidity(100 * E4_ONE, 200 * E4_ONE).unwrap();
    pool
}

/// Pool BC seeded with 100 B / 300 C.
fn pool_bc() -> Pool {
    let mut pool = Pool::new(token_b(), token_c());
    pool.add_liquidity(100 * E4_ONE, 300 * E4_ONE).unwrap();
    pool
}

/// Gate (literal): across a deterministic swap sequence (both
/// directions, wildly varying sizes, plus routes) the invariant
/// `k = reserve_a * reserve_b` never decreases on any pool.
#[test]
fn e4_gate_k_never_decreases() {
    let mut pools = [pool_ab(), pool_bc()];
    let steps: Vec<(Token, Amount)> = vec![
        (token_a(), E4_ONE),
        (token_b(), E4_ONE),
        (token_a(), 3 * E4_ONE),
        (token_b(), 7 * E4_ONE / 10),
        (token_a(), E4_ONE / 1000),
        (token_b(), 50 * E4_ONE),
        (token_a(), 25 * E4_ONE),
        (token_b(), 1),
        (token_a(), 1),
        (token_b(), 3 * E4_ONE),
    ];
    for (token_in, amount_in) in steps {
        let k0 = pools[0].invariant_k();
        let out = pools[0].swap_exact_in(token_in, amount_in, 0).unwrap();
        assert!(out > 0 || amount_in == 1, "swap must move value");
        assert!(
            wide_ge(&pools[0].invariant_k(), &k0),
            "E4: k decreased on a swap ({token_in:?}, {amount_in})"
        );
    }
    // Routes keep each hop's k non-decreasing too.
    for amount in [E4_ONE, 5 * E4_ONE, E4_ONE / 3] {
        let k_ab = pools[0].invariant_k();
        let k_bc = pools[1].invariant_k();
        let out = swap_route(&mut pools, &[token_a(), token_b(), token_c()], amount, 0).unwrap();
        assert!(out > 0);
        assert!(wide_ge(&pools[0].invariant_k(), &k_ab));
        assert!(wide_ge(&pools[1].invariant_k(), &k_bc));
    }
}

/// NC (literal): the slippage guard holds — a `min_out` above the
/// quote is rejected with the named message.
#[test]
fn e4_nc_slippage_guard() {
    let mut pool = pool_ab();
    let quote = pool.get_amount_out(token_a(), E4_ONE).unwrap();
    let err = pool
        .swap_exact_in(token_a(), E4_ONE, quote + 1)
        .expect_err("slippage guard must trip");
    assert_eq!(
        err.to_string(),
        format!(
            "E4: slippage guard: output {quote} below minimum {}",
            quote + 1
        )
    );
    assert!(matches!(
        err,
        E4Error::Slippage { out, min_out } if out == quote && min_out == quote + 1
    ));
}

/// Golden quote: 1 A into (100 A, 200 B) at 30 bps.
#[test]
fn e4_swap_golden() {
    let mut pool = pool_ab();
    let out = pool.swap_exact_in(token_a(), E4_ONE, 0).unwrap();
    assert_eq!(out, 1_974_316_068_794_122_597);
    assert_eq!(pool.reserve_a(), 101 * E4_ONE);
    assert_eq!(pool.reserve_b(), 198_025_683_931_205_877_403u128);
}

/// Any fee-bearing swap grows `k` strictly.
#[test]
fn e4_swap_fee_grows_k_strictly() {
    let mut pool = pool_ab();
    let k0 = pool.invariant_k();
    pool.swap_exact_in(token_a(), E4_ONE, 0).unwrap();
    let k1 = pool.invariant_k();
    assert!(wide_ge(&k1, &k0));
    assert_ne!(k1, k0, "E4: fee must grow k strictly");
}

/// Initial mint: LP supply = amount_a + amount_b (explained deviation).
#[test]
fn e4_mint_initial() {
    let mut pool = Pool::new(token_a(), token_b());
    let lp = pool.add_liquidity(100 * E4_ONE, 200 * E4_ONE).unwrap();
    assert_eq!(lp, 300 * E4_ONE);
    assert_eq!(pool.lp_supply(), 300 * E4_ONE);
}

/// Proportional mint mints `a * S / ra` shares and keeps price.
#[test]
fn e4_mint_proportional() {
    let mut pool = pool_ab();
    let lp = pool.add_liquidity(50 * E4_ONE, 100 * E4_ONE).unwrap();
    assert_eq!(lp, 150 * E4_ONE);
    assert_eq!(pool.lp_supply(), 450 * E4_ONE);
    assert_eq!(pool.reserve_a(), 150 * E4_ONE);
    assert_eq!(pool.reserve_b(), 300 * E4_ONE);
}

#[test]
fn e4_mint_ratio_rejected() {
    let mut pool = pool_ab();
    let err = pool
        .add_liquidity(50 * E4_ONE, 90 * E4_ONE)
        .expect_err("unbalanced mint must abort");
    assert_eq!(
        err.to_string(),
        "E4: mint amounts must be proportional to reserves"
    );
}

#[test]
fn e4_mint_zero_rejected() {
    let mut pool = Pool::new(token_a(), token_b());
    let err = pool
        .add_liquidity(0, E4_ONE)
        .expect_err("zero initial mint must abort");
    assert_eq!(
        err.to_string(),
        "E4: initial mint amounts must both be positive"
    );
}

/// Add then burn all: pro-rata amounts come back (exact here).
#[test]
fn e4_burn_round_trip() {
    let mut pool = pool_ab();
    let lp = pool.add_liquidity(50 * E4_ONE, 100 * E4_ONE).unwrap();
    let (back_a, back_b) = pool.remove_liquidity(lp).unwrap();
    assert_eq!((back_a, back_b), (50 * E4_ONE, 100 * E4_ONE));
    assert_eq!(pool.lp_supply(), 300 * E4_ONE);
    let (rest_a, rest_b) = pool.remove_liquidity(pool.lp_supply()).unwrap();
    assert_eq!((rest_a, rest_b), (100 * E4_ONE, 200 * E4_ONE));
}

#[test]
fn e4_burn_exceeds_rejected() {
    let mut pool = pool_ab();
    let err = pool
        .remove_liquidity(300 * E4_ONE + 1)
        .expect_err("over-burn must abort");
    assert_eq!(
        err.to_string(),
        "E4: burn exceeds LP supply (300000000000000000001/300000000000000000000)"
    );
}

#[test]
fn e4_swap_zero_input_rejected() {
    let mut pool = pool_ab();
    let err = pool
        .swap_exact_in(token_a(), 0, 0)
        .expect_err("zero input must abort");
    assert_eq!(err.to_string(), "E4: swap input must be positive");
}

#[test]
fn e4_swap_empty_pool_rejected() {
    let mut pool = Pool::new(token_a(), token_b());
    let err = pool
        .swap_exact_in(token_a(), E4_ONE, 0)
        .expect_err("empty pool must abort");
    assert_eq!(err.to_string(), "E4: pool has no liquidity");
}

#[test]
fn e4_swap_unknown_token_rejected() {
    let mut pool = pool_ab();
    let err = pool
        .swap_exact_in(token_c(), E4_ONE, 0)
        .expect_err("foreign token must abort");
    assert!(matches!(err, E4Error::TokenNotInPool { .. }));
    assert_eq!(
        err.to_string(),
        format!("E4: swap token not in pool ({:?})", token_c())
    );
}

/// Golden route: 1 A -> B -> C through (100/200) and (100/300).
#[test]
fn e4_route_two_hops_golden() {
    let ab = pool_ab();
    let bc = pool_bc();
    let out = swap_route(
        &mut [ab.clone(), bc.clone()],
        &[token_a(), token_b(), token_c()],
        E4_ONE,
        0,
    )
    .unwrap();
    assert_eq!(out, 5_791_186_053_878_244_695);
}

#[test]
fn e4_route_slippage_guard() {
    let ab = pool_ab();
    let bc = pool_bc();
    let out = swap_route(
        &mut [ab.clone(), bc.clone()],
        &[token_a(), token_b(), token_c()],
        E4_ONE,
        5_791_186_053_878_244_695 + 1,
    )
    .expect_err("route slippage guard must trip");
    assert!(matches!(out, E4Error::Slippage { .. }));
}

#[test]
fn e4_route_short_rejected() {
    let ab = pool_ab();
    let err =
        swap_route(&mut [ab], &[token_a()], E4_ONE, 0).expect_err("one-token path must abort");
    assert_eq!(err.to_string(), "E4: route must contain at least one hop");
    let ab = pool_ab();
    let err = swap_route(&mut [ab], &[token_a(), token_b(), token_c()], E4_ONE, 0)
        .expect_err("undersized pools must abort");
    assert_eq!(err.to_string(), "E4: route must contain at least one hop");
}

#[test]
fn e4_route_token_mismatch_rejected() {
    let ab = pool_ab();
    let bc = pool_bc();
    let err = swap_route(&mut [ab, bc], &[token_a(), token_c(), token_b()], E4_ONE, 0)
        .expect_err("mismatched hop must abort");
    assert!(matches!(err, E4Error::TokenNotInPool { .. }));
}

/// Metamorphic: splitting an order in two never beats one piece
/// (double fee, convex curve).
#[test]
fn e4_metamorphic_split_swap() {
    let mut single = pool_ab();
    let one_piece = single.swap_exact_in(token_a(), 2 * E4_ONE, 0).unwrap();
    assert_eq!(one_piece, 3_910_033_923_564_131_223);

    let mut split = pool_ab();
    let first = split.swap_exact_in(token_a(), E4_ONE, 0).unwrap();
    let second = split.swap_exact_in(token_a(), E4_ONE, 0).unwrap();
    assert_eq!(first + second, 3_909_976_989_011_504_086);
    assert!(
        first + second <= one_piece,
        "E4: split must not beat single"
    );
}

/// Determinism: the same round of operations twice -> identical state.
#[test]
fn e4_deterministic_repeat() {
    let run = || -> (Amount, Amount, Amount) {
        let mut pool = pool_ab();
        let lp = pool.add_liquidity(5 * E4_ONE, 10 * E4_ONE).unwrap();
        let out1 = pool.swap_exact_in(token_a(), E4_ONE, 0).unwrap();
        let (ra, _rb) = pool.remove_liquidity(lp / 2).unwrap();
        (out1, lp, ra)
    };
    assert_eq!(run(), run(), "E4: pool math must be deterministic");
}

/// Zero-fee swaps keep `k` exactly constant (pure curve, floor rounding
/// still cannot decrease it).
#[test]
fn e4_zero_fee_keeps_k() {
    let mut pool = pool_ab().with_fee(0);
    let k0 = pool.invariant_k();
    pool.swap_exact_in(token_a(), E4_ONE, 0).unwrap();
    let k1 = pool.invariant_k();
    assert!(wide_ge(&k1, &k0), "E4: floor rounding must not decrease k");
}

/// Final-rescue: the route helper returns the last hop output even on
/// a two-pool path when the middle quote rounds down.
#[test]
fn e4_route_result_is_last_output() {
    let ab = pool_ab();
    let bc = pool_bc();
    // Manual hops on pristine clones must equal the route run on the
    // same pristine pools.
    let mid = ab.clone().swap_exact_in(token_a(), E4_ONE, 0).unwrap();
    let manual = bc.clone().swap_exact_in(token_b(), mid, 0).unwrap();
    let route_out =
        swap_route(&mut [ab, bc], &[token_a(), token_b(), token_c()], E4_ONE, 0).unwrap();
    assert_eq!(route_out, manual);
}
