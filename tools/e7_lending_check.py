#!/usr/bin/env python3
"""E7 lending deterministic model check (ECOSYSTEM_PLAN E7).

Mirrors `crates/aura-storage/src/lending.rs` exactly (floor rounding at
every division) in unbounded Python integers and replays the fixed
`sim/e7-lending.json` scenario: the 30-day gate simulation (two
accounts, five price shocks 0.85/0.72/0.61/0.52/0.44, daily interest
accrual at the kinked utilization rate, liquidations at health < 12500)
and the no-liquidation control run.

Golden values:
  WITH liquidations: invariant `collateral >= debt` holds every day;
    16 liquidations; day30 debts (690912489586789599, 4737685642880842971);
    total interest 4011969083602784267; min health 12530.
  WITHOUT liquidations: invariant breaks on day 18 (the functional
    "no liquidation -> red" control).
"""
import json

E7_ONE = 10 ** 18
BPS = 10_000
LTV, LIQ_HEALTH, BONUS, CLOSE = 7500, 12500, 500, 5000
BASE, SLOPE1, KINK, SLOPE2 = 200, 1000, 8000, 3000

PRICES = {6: 85 * E7_ONE // 100, 12: 72 * E7_ONE // 100,
          18: 61 * E7_ONE // 100, 24: 52 * E7_ONE // 100,
          30: 44 * E7_ONE // 100}


def rate_bps(util):
    util = min(util, BPS)
    if util <= KINK:
        return BASE + SLOPE1 * util // KINK
    return BASE + SLOPE1 + SLOPE2 * (util - KINK) // (BPS - KINK)


def health_bps(value, debt):
    return None if debt == 0 else value * BPS // debt


def run(do_liquidations):
    price = E7_ONE
    accts = [[1000 * E7_ONE, 700 * E7_ONE], [500 * E7_ONE, 300 * E7_ONE]]

    def value(a):
        return a[0] * price // E7_ONE

    def util():
        tv = sum(value(a) for a in accts)
        td = sum(a[1] for a in accts)
        return min(td * BPS // tv, BPS) if tv else 0

    total_interest = 0
    liqs = []
    min_health = 10 ** 9
    breach_day = None
    invariant_held = True
    day30 = None
    for day in range(1, 31):
        if day in PRICES:
            price = PRICES[day]
        r = rate_bps(util())
        for a in accts:
            i = a[1] * r // (365 * BPS)
            a[1] += i
            total_interest += i
        if do_liquidations:
            for a in accts:
                while True:
                    h = health_bps(value(a), a[1])
                    if h is None or h >= LIQ_HEALTH:
                        break
                    repay = a[1] * CLOSE // BPS
                    seize = repay * (BPS + BONUS) * E7_ONE // (price * BPS)
                    if seize > a[0]:
                        seize = a[0]
                    a[1] -= repay
                    a[0] -= seize
                    liqs.append((day, repay, seize))
        for a in accts:
            h = health_bps(value(a), a[1])
            min_health = min(min_health, h if h is not None else 10 ** 9)
            if value(a) < a[1]:
                invariant_held = False
                if breach_day is None:
                    breach_day = day
        if day == 30:
            day30 = (accts[0][1], accts[1][1])
    return day30, liqs, total_interest, min_health, breach_day, invariant_held


def main():
    day30, liqs, interest, min_health, breach, held = run(True)
    assert held and breach is None, f"invariant broke on {breach}"
    assert len(liqs) == 16, len(liqs)
    assert liqs[0] == (6, 350_609_153_368_496_978_254, 433_105_424_749_319_796_666), liqs[0]
    assert liqs[15] == (30, 4_737_685_642_880_842_970, 11_305_840_738_692_920_723), liqs[15]
    assert day30 == (690_912_489_586_789_599, 4_737_685_642_880_842_971), day30
    assert interest == 4_011_969_083_602_784_267, interest
    assert min_health == 12_530, min_health

    _, _, _, _, no_liq_breach, no_liq_held = run(False)
    assert not no_liq_held and no_liq_breach == 18, no_liq_breach

    summary = {
        "days": 30,
        "liq_count": len(liqs),
        "day30_debt_a": day30[0],
        "day30_debt_b": day30[1],
        "total_interest": interest,
        "min_health_bps": min_health,
        "invariant_held": held,
        "no_liq_breach_day": no_liq_breach,
    }
    sim = {
        "phase": "E7",
        "model": "over-collateralized lending, LTV 75%, liquidation health 12500bps, "
                 "bonus 500bps, close factor 5000bps, kinked utilization rate model",
        "invariant": "30-day simulation: collateral_value >= debt never breaks per account",
        "scenario": {
            "accounts": "A 1000/700, B 500/300 (collateral/debt, 18 decimals)",
            "price_path": {str(d): str(p) for d, p in sorted(PRICES.items())},
            "control": "same path without liquidations breaks the invariant on day 18",
        },
        "expected": summary,
    }
    with open("sim/e7-lending.json", "w", encoding="utf-8") as handle:
        json.dump(sim, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print("E7-MODEL-OK")
    print("E7-SIM-OK " + json.dumps(summary, separators=(", ", ": "), sort_keys=False))


if __name__ == "__main__":
    main()
