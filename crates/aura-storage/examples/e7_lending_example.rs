//! E7 lending simulation driver (ECOSYSTEM_PLAN E7).
//!
//! Deterministic driver mirroring `tools/e7_lending_check.py`: plays the
//! fixed `sim/e7-lending.json` scenario — the 30-day gate simulation
//! (two accounts, five price shocks, interest accrual, liquidations)
//! plus the no-liquidation control run — and prints a one-line JSON
//! summary. Expected values are checked at the end, so a divergent
//! market fails the run.

use std::process::exit;

use aura_storage::lending::{E7_ONE, LendingMarket};

const LIQ_HEALTH: u64 = 12_500;

const PRICES: [(u64, u128); 5] = [
    (6, 85 * E7_ONE / 100),
    (12, 72 * E7_ONE / 100),
    (18, 61 * E7_ONE / 100),
    (24, 52 * E7_ONE / 100),
    (30, 44 * E7_ONE / 100),
];

fn run(do_liquidations: bool) -> (u128, u128, Vec<u64>, u128, u64, Option<u64>, bool) {
    let mut m = LendingMarket::new();
    let a = m.open_account();
    m.supply(a, 1_000 * E7_ONE).expect("supply a");
    m.borrow(a, 700 * E7_ONE).expect("borrow a");
    let b = m.open_account();
    m.supply(b, 500 * E7_ONE).expect("supply b");
    m.borrow(b, 300 * E7_ONE).expect("borrow b");

    let mut liq_days = Vec::new();
    let mut min_health = u64::MAX;
    let mut breach_day = None;
    let mut invariant_held = true;
    let mut day30 = (0, 0);

    for day in 1..=30u64 {
        if let Some((_, price)) = PRICES.iter().find(|(d, _)| *d == day) {
            m.set_price(*price).expect("price");
        }
        m.advance_day().expect("accrue");
        if do_liquidations {
            for id in [a, b] {
                while m.health_bps(id).unwrap_or(u64::MAX) < LIQ_HEALTH {
                    m.liquidate(id).expect("liquidation");
                    liq_days.push(day);
                }
            }
        }
        for id in [a, b] {
            min_health = min_health.min(m.health_bps(id).unwrap_or(u64::MAX));
            if m.value_of(id).unwrap() < m.account(id).unwrap().debt {
                invariant_held = false;
                if breach_day.is_none() {
                    breach_day = Some(day);
                }
            }
        }
        if day == 30 {
            day30 = (m.account(a).unwrap().debt, m.account(b).unwrap().debt);
        }
    }
    (
        day30.0,
        day30.1,
        liq_days,
        m.total_interest(),
        min_health,
        breach_day,
        invariant_held,
    )
}

fn main() {
    // Optional: confirm the human-readable fixture is in place.
    if let Some(path) = std::env::args().nth(1)
        && !std::path::Path::new(&path).exists()
    {
        eprintln!("E7-SIM-ERR fixture file missing: {path}");
        exit(1);
    }

    let (debt_a, debt_b, liq_days, interest, min_health, breach, held) = run(true);
    let (_, _, _, _, _, no_liq_breach, _) = run(false);
    let no_liq_day = no_liq_breach.unwrap_or(0);

    println!(
        "E7-SIM-OK {{\"days\": 30, \"liq_count\": {}, \"day30_debt_a\": {}, \"day30_debt_b\": {}, \"total_interest\": {}, \"min_health_bps\": {}, \"invariant_held\": {}, \"no_liq_breach_day\": {}}}",
        liq_days.len(),
        debt_a,
        debt_b,
        interest,
        min_health,
        held,
        no_liq_day,
    );

    // Expected scenario values (sim/e7-lending.json "expected").
    let ok = liq_days.len() == 16
        && debt_a == 690_912_489_586_789_599
        && debt_b == 4_737_685_642_880_842_971
        && interest == 4_011_969_083_602_784_267
        && min_health == 12_530
        && held
        && breach.is_none()
        && no_liq_day == 18;
    if !ok {
        eprintln!(
            "E7-SIM-ERR scenario diverged: liqs {} debt_a {debt_a} interest {interest} min_health {min_health} breach {breach:?} no_liq {no_liq_day}",
            liq_days.len(),
        );
        exit(1);
    }
}
