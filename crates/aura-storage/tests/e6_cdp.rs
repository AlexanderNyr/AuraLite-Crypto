//! E6 stablecoin CDP gate suite (ECOSYSTEM_PLAN E6).
//!
//! Literal gate: **200% collateral at −40% price → liquidation with no
//! bad debt**. Property: the peg backing invariant
//! (`bad_debt == 0 && backed_value >= stable_supply`) never breaks
//! across the documented stress sequence. NC: weakening the mint-ratio
//! guard message (or any targeted named message) turns the suite red.

use aura_storage::cdp::{CdpEngine, E6_ONE, E6Error};

const MIN_BPS: u64 = 15_000;
const PEN_BPS: u64 = 500;

fn engine() -> CdpEngine {
    CdpEngine::new()
}

/// Funded vault: 200e18 collateral, 100e18 debt at price 1.0 (200%).
fn funded(engine: &mut CdpEngine, col: u128) -> u64 {
    let id = engine.open_vault();
    engine.deposit(id, col).unwrap();
    engine.mint(id, 100 * E6_ONE).unwrap();
    id
}

/// The literal gate scenario from ECOSYSTEM_PLAN E6.
#[test]
fn e6_gate_200pct_minus40_liquidates_no_bad_debt() {
    let mut e = engine();
    assert_eq!((e.min_ratio_bps(), e.penalty_bps()), (MIN_BPS, PEN_BPS));
    let id = funded(&mut e, 200 * E6_ONE);
    assert_eq!(e.ratio_bps(id), Some(20_000));
    e.set_price(6 * E6_ONE / 10).unwrap();
    assert_eq!(e.ratio_bps(id), Some(12_000), "ratio after -40% shock");
    assert!(e.peg_holds(), "backing intact before liquidation");
    let liq = e.liquidate(id).expect("120% < 150% must liquidate");
    assert_eq!(liq.seized, 175 * E6_ONE);
    assert_eq!(liq.repaid, 100 * E6_ONE);
    assert_eq!(liq.shortfall, 0, "no bad debt");
    assert_eq!(liq.returned, 25 * E6_ONE);
    assert_eq!(e.stable_supply(), 0);
    assert_eq!(e.bad_debt(), 0);
    assert_eq!(e.liquidations(), 1);
    assert!(e.peg_holds(), "peg backed after liquidation");
}

/// Property: the peg backing invariant never breaks across a
/// deterministic stress sequence (shocks, mints, repayments, three
/// liquidations at different depths).
#[test]
fn e6_gate_peg_backing_never_breaks() {
    let mut e = engine();
    let a = funded(&mut e, 200 * E6_ONE);
    let b = funded(&mut e, 220 * E6_ONE);
    let c = funded(&mut e, 300 * E6_ONE);
    let check = |e: &CdpEngine, step: &str| {
        assert_eq!(e.bad_debt(), 0, "bad debt at {step}");
        assert!(e.peg_holds(), "peg backing broken at {step}");
    };
    check(&e, "open");
    e.set_price(9 * E6_ONE / 10).unwrap();
    check(&e, "shock -10%");
    e.set_price(6 * E6_ONE / 10).unwrap();
    check(&e, "shock -40%");
    let la = e.liquidate(a).unwrap();
    assert_eq!(
        (la.seized, la.repaid, la.shortfall, la.returned),
        (175 * E6_ONE, 100 * E6_ONE, 0, 25 * E6_ONE)
    );
    check(&e, "liq A");
    let lb = e.liquidate(b).unwrap();
    assert_eq!(
        (lb.seized, lb.repaid, lb.shortfall, lb.returned),
        (175 * E6_ONE, 100 * E6_ONE, 0, 45 * E6_ONE)
    );
    check(&e, "liq B");
    // V3 stays healthy at 180%.
    assert_eq!(e.ratio_bps(c), Some(18_000));
    e.set_price(45 * E6_ONE / 100).unwrap();
    check(&e, "shock -25%");
    let lc = e.liquidate(c).unwrap();
    assert_eq!(lc.seized, 233_333_333_333_333_333_333);
    assert_eq!(lc.repaid, 100 * E6_ONE);
    assert_eq!(lc.shortfall, 0);
    assert_eq!(lc.returned, 66_666_666_666_666_666_667);
    check(&e, "liq C");
    assert_eq!(e.stable_supply(), 0);
    assert_eq!(e.liquidations(), 3);
}

#[test]
fn e6_golden_mint_boundary() {
    let mut e = engine();
    let id = e.open_vault();
    e.deposit(id, 200 * E6_ONE).unwrap();
    let dmax = 133_333_333_333_333_333_333u128;
    e.mint(id, dmax).unwrap();
    assert_eq!(e.ratio_bps(id), Some(15_000));
    let err = e.mint(id, 1).unwrap_err();
    assert_eq!(
        err.to_string(),
        "E6: mint exceeds collateral ratio (14999/15000)"
    );
}

#[test]
fn e6_golden_withdraw_boundary() {
    let mut e = engine();
    let id = funded(&mut e, 200 * E6_ONE);
    e.withdraw(id, 50 * E6_ONE).unwrap();
    assert_eq!(e.ratio_bps(id), Some(15_000));
    let err = e.withdraw(id, 1).unwrap_err();
    assert_eq!(
        err.to_string(),
        "E6: withdraw would breach collateral ratio (14999/15000)"
    );
}

#[test]
fn e6_vault_not_found_rejected() {
    let mut e = engine();
    let err = e.deposit(7, E6_ONE).unwrap_err();
    assert_eq!(err.to_string(), "E6: vault not found (7)");
    let err = e.mint(9, E6_ONE).unwrap_err();
    assert_eq!(err.to_string(), "E6: vault not found (9)");
    let err = e.liquidate(11).unwrap_err();
    assert_eq!(err.to_string(), "E6: vault not found (11)");
}

#[test]
fn e6_zero_amounts_rejected() {
    let mut e = engine();
    let id = e.open_vault();
    e.deposit(id, E6_ONE).unwrap();
    assert_eq!(
        e.deposit(id, 0).unwrap_err().to_string(),
        "E6: deposit must be positive"
    );
    assert_eq!(
        e.mint(id, 0).unwrap_err().to_string(),
        "E6: mint must be positive"
    );
    assert_eq!(
        e.repay(id, 0).unwrap_err().to_string(),
        "E6: repay must be positive"
    );
    assert_eq!(
        e.withdraw(id, 0).unwrap_err().to_string(),
        "E6: withdraw must be positive"
    );
    assert_eq!(
        e.set_price(0).unwrap_err().to_string(),
        "E6: price must be positive"
    );
}

#[test]
fn e6_repay_bounds_rejected() {
    let mut e = engine();
    let id = funded(&mut e, 200 * E6_ONE);
    let err = e.repay(id, 100 * E6_ONE + 1).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "E6: repay exceeds debt ({}/{})",
            100 * E6_ONE + 1,
            100 * E6_ONE
        )
    );
}

#[test]
fn e6_withdraw_exceeds_rejected() {
    let mut e = engine();
    let id = funded(&mut e, 200 * E6_ONE);
    e.repay(id, 100 * E6_ONE).unwrap();
    let err = e.withdraw(id, 200 * E6_ONE + 1).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "E6: withdraw exceeds collateral ({}/{})",
            200 * E6_ONE + 1,
            200 * E6_ONE
        )
    );
}

#[test]
fn e6_healthy_liquidation_refused() {
    let mut e = engine();
    let id = funded(&mut e, 200 * E6_ONE);
    let err = e.liquidate(id).unwrap_err();
    assert_eq!(
        err.to_string(),
        "E6: vault is healthy, liquidation refused (20000 >= 15000)"
    );
    let mut e = engine();
    let id = e.open_vault();
    e.deposit(id, E6_ONE).unwrap();
    let err = e.liquidate(id).unwrap_err();
    assert!(matches!(err, E6Error::HealthyVault { .. }));
}

#[test]
fn e6_metamorphic_deposit_split() {
    let mut single = engine();
    let a = single.open_vault();
    single.deposit(a, 100 * E6_ONE).unwrap();
    single.mint(a, 40 * E6_ONE).unwrap();
    let mut split = engine();
    let b = split.open_vault();
    split.deposit(b, 40 * E6_ONE).unwrap();
    split.deposit(b, 60 * E6_ONE).unwrap();
    split.mint(b, 40 * E6_ONE).unwrap();
    assert_eq!(single.vault(a), split.vault(b));
    assert_eq!(single.ratio_bps(a), split.ratio_bps(b));
}

#[test]
fn e6_metamorphic_mint_repay_roundtrip() {
    let mut e = engine();
    let id = funded(&mut e, 200 * E6_ONE);
    let before = *e.vault(id).unwrap();
    let supply_before = e.stable_supply();
    e.mint(id, 25 * E6_ONE).unwrap();
    e.repay(id, 25 * E6_ONE).unwrap();
    assert_eq!(*e.vault(id).unwrap(), before);
    assert_eq!(e.stable_supply(), supply_before);
    assert!(e.peg_holds());
}

#[test]
fn e6_metamorphic_liquidation_order() {
    let run = |order: [bool; 2]| -> (u128, u128, u128) {
        let mut e = engine();
        let x = funded(&mut e, 200 * E6_ONE);
        let y = funded(&mut e, 220 * E6_ONE);
        e.set_price(6 * E6_ONE / 10).unwrap();
        let seq: Vec<u64> = if order[0] { vec![x, y] } else { vec![y, x] };
        for id in seq {
            e.liquidate(id).unwrap();
        }
        (e.stable_supply(), e.bad_debt(), e.backed_value())
    };
    assert_eq!(run([true, false]), run([false, true]));
}

#[test]
fn e6_deterministic_repeat() {
    let run = || -> (u128, u128, u64) {
        let mut e = engine();
        let a = funded(&mut e, 200 * E6_ONE);
        let b = funded(&mut e, 220 * E6_ONE);
        e.set_price(6 * E6_ONE / 10).unwrap();
        e.liquidate(a).unwrap();
        e.liquidate(b).unwrap();
        (e.stable_supply(), e.bad_debt(), e.liquidations())
    };
    assert_eq!(run(), run(), "E6: engine must be deterministic");
}

#[test]
fn e6_supply_tracks_mint_repay_liquidation() {
    let mut e = engine();
    let id = funded(&mut e, 200 * E6_ONE);
    assert_eq!(e.stable_supply(), 100 * E6_ONE);
    e.mint(id, 30 * E6_ONE).unwrap();
    assert_eq!(e.stable_supply(), 130 * E6_ONE);
    e.repay(id, 10 * E6_ONE).unwrap();
    assert_eq!(e.stable_supply(), 120 * E6_ONE);
    // Price 0.6 -> ratio 100%: liquidation seizes ALL collateral
    // (200e18 worth exactly 120e18 = debt) and burns the full debt.
    e.set_price(6 * E6_ONE / 10).unwrap();
    let liq = e.liquidate(id).unwrap();
    assert_eq!(liq.seized, 200 * E6_ONE);
    assert_eq!(liq.repaid, 120 * E6_ONE);
    assert_eq!(liq.shortfall, 0);
    assert_eq!(liq.returned, 0);
    assert_eq!(e.stable_supply(), 0);
    assert_eq!(e.bad_debt(), 0);
    assert!(e.peg_holds());
}
