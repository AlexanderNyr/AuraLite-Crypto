//! E6 stablecoin CDP simulation driver (ECOSYSTEM_PLAN E6).
//!
//! Deterministic driver mirroring `tools/e6_cdp_check.py`: plays the
//! fixed `sim/e6-cdp.json` scenario — the literal gate (200% collateral
//! at −40% price → liquidation with no bad debt) plus the peg stress
//! sequence (three vaults, two shock waves, three liquidations) — and
//! prints a one-line JSON summary. Expected values are checked at the
//! end, so a divergent engine fails the run.

use std::process::exit;

use aura_storage::cdp::{CdpEngine, E6_ONE};

fn main() {
    // Optional: confirm the human-readable fixture is in place.
    if let Some(path) = std::env::args().nth(1)
        && !std::path::Path::new(&path).exists()
    {
        eprintln!("E6-SIM-ERR fixture file missing: {path}");
        exit(1);
    }

    // 1. Literal gate: 200% collateral, -40% price, liquidation.
    let mut gate = CdpEngine::new();
    let g = gate.open_vault();
    gate.deposit(g, 200 * E6_ONE).expect("deposit");
    gate.mint(g, 100 * E6_ONE).expect("mint");
    gate.set_price(6 * E6_ONE / 10).expect("shock");
    let ratio_after = gate.ratio_bps(g).expect("ratio");
    let liq = gate.liquidate(g).expect("liquidation");

    // 2. Peg stress: three vaults (200/220/300), shocks 0.9/0.6/0.45.
    let mut stress = CdpEngine::new();
    let mut ids = Vec::new();
    for col in [200 * E6_ONE, 220 * E6_ONE, 300 * E6_ONE] {
        let id = stress.open_vault();
        stress.deposit(id, col).expect("deposit");
        stress.mint(id, 100 * E6_ONE).expect("mint");
        ids.push(id);
    }
    let mut backed = stress.peg_holds();
    stress.set_price(9 * E6_ONE / 10).expect("shock 1");
    backed &= stress.peg_holds();
    stress.set_price(6 * E6_ONE / 10).expect("shock 2");
    backed &= stress.peg_holds();
    let la = stress.liquidate(ids[0]).expect("liq a");
    let lb = stress.liquidate(ids[1]).expect("liq b");
    backed &= stress.peg_holds();
    stress.set_price(45 * E6_ONE / 100).expect("shock 3");
    backed &= stress.peg_holds();
    let lc = stress.liquidate(ids[2]).expect("liq c");
    backed &= stress.peg_holds();

    println!(
        "E6-SIM-OK {{\"gate\": {{\"ratio_after_shock_bps\": {}, \"seized\": {}, \"repaid\": {}, \"shortfall\": {}, \"returned\": {}}}, \"stress\": {{\"liquidations\": {}, \"bad_debt\": {}, \"backed\": {}, \"final_supply\": {}}}, \"min_ratio_bps\": 15000, \"penalty_bps\": 500}}",
        ratio_after,
        liq.seized,
        liq.repaid,
        liq.shortfall,
        liq.returned,
        stress.liquidations(),
        stress.bad_debt(),
        backed,
        stress.stable_supply(),
    );

    // Expected scenario values (sim/e6-cdp.json "expected").
    let ok = ratio_after == 12_000
        && (liq.seized, liq.repaid, liq.shortfall, liq.returned)
            == (175 * E6_ONE, 100 * E6_ONE, 0, 25 * E6_ONE)
        && (la.seized, la.returned) == (175 * E6_ONE, 25 * E6_ONE)
        && (lb.seized, lb.returned) == (175 * E6_ONE, 45 * E6_ONE)
        && lc.seized == 233_333_333_333_333_333_333
        && lc.returned == 66_666_666_666_666_666_667
        && la.shortfall + lb.shortfall + lc.shortfall == 0
        && stress.bad_debt() == 0
        && stress.stable_supply() == 0
        && backed;
    if !ok {
        eprintln!(
            "E6-SIM-ERR scenario diverged: ratio {ratio_after} seized {} shortfall {} bad_debt {} backed {backed}",
            liq.seized,
            liq.shortfall,
            stress.bad_debt(),
        );
        exit(1);
    }
}
