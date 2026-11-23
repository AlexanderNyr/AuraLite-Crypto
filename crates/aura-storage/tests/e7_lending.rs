//! E7 lending gate suite (ECOSYSTEM_PLAN E7).
//!
//! Literal gate: **30-day simulation, `collateral >= debt` invariant
//! never breaks** per account (liquidations restore positions every
//! day). Functional NC: the same path without liquidations breaks the
//! invariant on day 18 — "no liquidation → red". Message NCs assert
//! exact named-message strings.

use aura_storage::lending::{E7_ONE, LendingMarket};

const LIQ_HEALTH: u64 = 12_500;

const PRICES: [(u64, u128); 5] = [
    (6, 85 * E7_ONE / 100),
    (12, 72 * E7_ONE / 100),
    (18, 61 * E7_ONE / 100),
    (24, 52 * E7_ONE / 100),
    (30, 44 * E7_ONE / 100),
];

struct Sim {
    day10: (u128, u128),
    day20: (u128, u128),
    day30: (u128, u128),
    liqs: Vec<(u64, u128, u128)>,
    total_interest: u128,
    min_health: u64,
    breach_day: Option<u64>,
    final_col: (u128, u128),
    invariant_held: bool,
}

/// The canonical 30-day stress run (two accounts, five shocks).
fn run_sim(do_liquidations: bool) -> Sim {
    let mut m = LendingMarket::new();
    let a = m.open_account();
    m.supply(a, 1_000 * E7_ONE).unwrap();
    m.borrow(a, 700 * E7_ONE).unwrap();
    let b = m.open_account();
    m.supply(b, 500 * E7_ONE).unwrap();
    m.borrow(b, 300 * E7_ONE).unwrap();

    let mut liqs = Vec::new();
    let mut min_health = u64::MAX;
    let mut breach_day = None;
    let mut invariant_held = true;
    let mut day10 = (0, 0);
    let mut day20 = (0, 0);
    let mut day30 = (0, 0);

    for day in 1..=30u64 {
        if let Some((_, price)) = PRICES.iter().find(|(d, _)| *d == day) {
            m.set_price(*price).unwrap();
        }
        m.advance_day().unwrap();
        if do_liquidations {
            for id in [a, b] {
                while m.health_bps(id).unwrap_or(u64::MAX) < LIQ_HEALTH {
                    let liq = m.liquidate(id).unwrap();
                    liqs.push((day, liq.repaid, liq.seized));
                }
            }
        }
        for id in [a, b] {
            let health = m.health_bps(id).unwrap_or(u64::MAX);
            min_health = min_health.min(health);
            let value = m.value_of(id).unwrap();
            let debt = m.account(id).unwrap().debt;
            if value < debt {
                invariant_held = false;
                if breach_day.is_none() {
                    breach_day = Some(day);
                }
            }
        }
        if day == 10 {
            day10 = (m.account(a).unwrap().debt, m.account(b).unwrap().debt);
        } else if day == 20 {
            day20 = (m.account(a).unwrap().debt, m.account(b).unwrap().debt);
        } else if day == 30 {
            day30 = (m.account(a).unwrap().debt, m.account(b).unwrap().debt);
        }
    }
    Sim {
        day10,
        day20,
        day30,
        liqs,
        total_interest: m.total_interest(),
        min_health,
        breach_day,
        final_col: (
            m.account(a).unwrap().collateral,
            m.account(b).unwrap().collateral,
        ),
        invariant_held,
    }
}

/// The literal gate: 30-day simulation, collateral >= debt every day.
#[test]
fn e7_gate_30day_sim_collateral_never_below_debt() {
    let s = run_sim(true);
    assert!(
        s.invariant_held,
        "E7: collateral >= debt broke on {:?}",
        s.breach_day
    );
    assert_eq!(s.breach_day, None);
    // Golden checkpoints (Python model `tools/e7_lending_check.py`).
    assert_eq!(
        s.day10,
        (351_031_035_158_122_967_106, 300_883_744_421_248_257_515)
    );
    assert_eq!(
        s.day20,
        (22_023_004_292_320_781_723, 37_753_721_643_978_482_954)
    );
    assert_eq!(
        s.day30,
        (690_912_489_586_789_599, 4_737_685_642_880_842_971)
    );
    assert_eq!(s.liqs.len(), 16);
    assert_eq!(
        s.liqs[0],
        (6, 350_609_153_368_496_978_254, 433_105_424_749_319_796_666)
    );
    assert_eq!(
        s.liqs[15],
        (30, 4_737_685_642_880_842_970, 11_305_840_738_692_920_723)
    );
    assert_eq!(s.total_interest, 4_011_969_083_602_784_267);
    assert_eq!(s.min_health, 12_530);
    assert_eq!(
        s.final_col,
        (2_122_542_301_295_018_158, 13_492_127_786_458_407_685)
    );
}

/// Functional NC: the same path without liquidations breaks the
/// invariant — "no liquidation → red".
#[test]
fn e7_gate_no_liquidation_breaks_invariant() {
    let s = run_sim(false);
    assert!(
        !s.invariant_held,
        "E7: invariant must break without liquidations"
    );
    assert_eq!(s.breach_day, Some(18));
}

#[test]
fn e7_golden_rate_model() {
    assert_eq!(aura_storage::lending::rate_bps_for(0), 200);
    assert_eq!(aura_storage::lending::rate_bps_for(6_666), 1_033);
    assert_eq!(aura_storage::lending::rate_bps_for(8_000), 1_200);
    assert_eq!(aura_storage::lending::rate_bps_for(9_000), 2_700);
}

#[test]
fn e7_golden_borrow_capacity() {
    let mut m = LendingMarket::new();
    let a = m.open_account();
    m.supply(a, 1_000 * E7_ONE).unwrap();
    m.borrow(a, 700 * E7_ONE).unwrap();
    assert_eq!(m.borrow_capacity(a), Some(50 * E7_ONE));
    let err = m.borrow(a, 50 * E7_ONE + 1).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "E7: borrow exceeds collateral capacity ({}/{})",
            50 * E7_ONE + 1,
            50 * E7_ONE
        )
    );
}

#[test]
fn e7_golden_withdraw_capacity() {
    let mut m = LendingMarket::new();
    let a = m.open_account();
    m.supply(a, 1_000 * E7_ONE).unwrap();
    m.borrow(a, 700 * E7_ONE).unwrap();
    let err = m.withdraw(a, 66_666_666_666_666_666_667).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "E7: withdraw would breach collateral capacity ({}/{})",
            66_666_666_666_666_666_667u128, 66_666_666_666_666_666_666u128
        )
    );
    m.withdraw(a, 66_666_666_666_666_666_666).unwrap();
    assert_eq!(m.health_bps(a), Some(13_333));
}

#[test]
fn e7_healthy_liquidation_refused() {
    let mut m = LendingMarket::new();
    let a = m.open_account();
    m.supply(a, 1_000 * E7_ONE).unwrap();
    m.borrow(a, 700 * E7_ONE).unwrap();
    let err = m.liquidate(a).unwrap_err();
    assert_eq!(
        err.to_string(),
        "E7: account is healthy, liquidation refused (14285 >= 12500)"
    );
}

#[test]
fn e7_account_not_found_rejected() {
    let mut m = LendingMarket::new();
    let err = m.supply(7, E7_ONE).unwrap_err();
    assert_eq!(err.to_string(), "E7: account not found (7)");
    let err = m.borrow(9, E7_ONE).unwrap_err();
    assert_eq!(err.to_string(), "E7: account not found (9)");
    let err = m.liquidate(11).unwrap_err();
    assert_eq!(err.to_string(), "E7: account not found (11)");
}

#[test]
fn e7_zero_amounts_rejected() {
    let mut m = LendingMarket::new();
    let a = m.open_account();
    m.supply(a, E7_ONE).unwrap();
    assert_eq!(
        m.supply(a, 0).unwrap_err().to_string(),
        "E7: supply must be positive"
    );
    assert_eq!(
        m.withdraw(a, 0).unwrap_err().to_string(),
        "E7: withdraw must be positive"
    );
    assert_eq!(
        m.borrow(a, 0).unwrap_err().to_string(),
        "E7: borrow must be positive"
    );
    assert_eq!(
        m.repay(a, 0).unwrap_err().to_string(),
        "E7: repay must be positive"
    );
    assert_eq!(
        m.set_price(0).unwrap_err().to_string(),
        "E7: price must be positive"
    );
}

#[test]
fn e7_repay_exceeds_rejected() {
    let mut m = LendingMarket::new();
    let a = m.open_account();
    m.supply(a, 1_000 * E7_ONE).unwrap();
    m.borrow(a, 700 * E7_ONE).unwrap();
    let err = m.repay(a, 700 * E7_ONE + 1).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "E7: repay exceeds debt ({}/{})",
            700 * E7_ONE + 1,
            700 * E7_ONE
        )
    );
}

#[test]
fn e7_withdraw_exceeds_rejected() {
    let mut m = LendingMarket::new();
    let a = m.open_account();
    m.supply(a, 1_000 * E7_ONE).unwrap();
    let err = m.withdraw(a, 1_000 * E7_ONE + 1).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "E7: withdraw exceeds supplied ({}/{})",
            1_000 * E7_ONE + 1,
            1_000 * E7_ONE
        )
    );
}

#[test]
fn e7_metamorphic_supply_split() {
    let mut single = LendingMarket::new();
    let a = single.open_account();
    single.supply(a, 100 * E7_ONE).unwrap();
    single.borrow(a, 40 * E7_ONE).unwrap();
    let mut split = LendingMarket::new();
    let b = split.open_account();
    split.supply(b, 40 * E7_ONE).unwrap();
    split.supply(b, 60 * E7_ONE).unwrap();
    split.borrow(b, 40 * E7_ONE).unwrap();
    assert_eq!(single.account(a), split.account(b));
    assert_eq!(single.health_bps(a), split.health_bps(b));
}

#[test]
fn e7_metamorphic_borrow_repay_roundtrip() {
    let mut m = LendingMarket::new();
    let a = m.open_account();
    m.supply(a, 1_000 * E7_ONE).unwrap();
    m.borrow(a, 700 * E7_ONE).unwrap();
    let before = *m.account(a).unwrap();
    m.advance_day().unwrap();
    m.repay(a, m.account(a).unwrap().debt).unwrap();
    m.borrow(a, 700 * E7_ONE).unwrap();
    let after = *m.account(a).unwrap();
    // Debt is exactly 700e18 again; collateral unchanged.
    assert_eq!(after.collateral, before.collateral);
    assert_eq!(after.debt, before.debt);
    assert!(m.invariant_holds());
}

#[test]
fn e7_interest_accrues_monotonically() {
    let mut m = LendingMarket::new();
    let a = m.open_account();
    m.supply(a, 1_000 * E7_ONE).unwrap();
    m.borrow(a, 700 * E7_ONE).unwrap();
    let mut prev = m.account(a).unwrap().debt;
    for _ in 0..5 {
        m.advance_day().unwrap();
        let now = m.account(a).unwrap().debt;
        assert!(now > prev, "E7: interest must accrue");
        prev = now;
    }
    assert!(m.total_interest() > 0);
}

#[test]
fn e7_deterministic_repeat() {
    let run = || -> (u128, u128, u64) {
        let s = run_sim(true);
        (s.day30.0, s.total_interest, s.liqs.len() as u64)
    };
    assert_eq!(run(), run(), "E7: market must be deterministic");
}
