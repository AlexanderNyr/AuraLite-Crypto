#!/usr/bin/env python3
"""E6 stablecoin CDP deterministic model check (ECOSYSTEM_PLAN E6).

Mirrors `crates/aura-storage/src/cdp.rs` exactly (floor rounding at every
division) in unbounded Python integers and replays the fixed
`sim/e6-cdp.json` scenario: the literal gate (200% collateral at −40%
price -> liquidation with no bad debt) and the peg stress sequence
(three vaults, shocks 0.9/0.6/0.45, three liquidations).

Golden values (min ratio 15000bps, penalty 500bps):
  gate: ratio 12000bps after shock; seized 175e18, repaid 100e18,
        shortfall 0, returned 25e18
  stress liq C at price 0.45: seized 233333333333333333333,
        returned 66666666666666666667
"""
import json

E6_ONE = 10 ** 18
BPS = 10_000
MIN_RATIO_BPS = 15_000
PENALTY_BPS = 500


def ratio_bps_of(collateral, debt, price):
    if debt == 0:
        return None
    value = collateral * price // E6_ONE
    return value * BPS // debt


class Engine:
    """Minimal mirror of `cdp.rs::CdpEngine`."""

    def __init__(self, min_ratio_bps=MIN_RATIO_BPS, penalty_bps=PENALTY_BPS):
        self.min_ratio_bps = min_ratio_bps
        self.penalty_bps = penalty_bps
        self.price = E6_ONE
        self.vaults = {}
        self.stable_supply = 0
        self.bad_debt = 0
        self.liquidations = 0
        self._next = 0

    def open_vault(self):
        vid = self._next
        self._next += 1
        self.vaults[vid] = [0, 0]  # collateral, debt
        return vid

    def deposit(self, vid, amount):
        assert amount > 0, "E6: deposit must be positive"
        self.vaults[vid][0] += amount

    def mint(self, vid, amount):
        assert amount > 0, "E6: mint must be positive"
        col, debt = self.vaults[vid]
        debt_after = debt + amount
        ratio = ratio_bps_of(col, debt_after, self.price)
        assert ratio >= self.min_ratio_bps, (
            f"E6: mint exceeds collateral ratio ({ratio}/{self.min_ratio_bps})"
        )
        self.vaults[vid][1] = debt_after
        self.stable_supply += amount

    def repay(self, vid, amount):
        assert amount > 0, "E6: repay must be positive"
        col, debt = self.vaults[vid]
        assert amount <= debt, f"E6: repay exceeds debt ({amount}/{debt})"
        self.vaults[vid][1] = debt - amount
        self.stable_supply -= amount

    def set_price(self, price):
        assert price > 0, "E6: price must be positive"
        self.price = price

    def liquidate(self, vid):
        col, debt = self.vaults[vid]
        ratio = ratio_bps_of(col, debt, self.price)
        assert debt > 0 and ratio < self.min_ratio_bps, (
            f"E6: vault is healthy, liquidation refused "
            f"({ratio if ratio is not None else 2**64 - 1} >= {self.min_ratio_bps})"
        )
        pen_debt = debt * (BPS + self.penalty_bps) // BPS
        wanted = pen_debt * E6_ONE // self.price
        seized = min(wanted, col)
        repaid = min(debt, seized * self.price // E6_ONE)
        shortfall = debt - repaid
        returned = col - seized
        self.vaults[vid] = [returned, 0]
        self.stable_supply -= repaid
        self.bad_debt += shortfall
        self.liquidations += 1
        return {"seized": seized, "repaid": repaid,
                "shortfall": shortfall, "returned": returned}

    def backed_value(self):
        return sum(col * self.price // E6_ONE for col, _ in self.vaults.values())

    def peg_holds(self):
        return self.bad_debt == 0 and self.backed_value() >= self.stable_supply


def main():
    # 1. Literal gate.
    gate = Engine()
    g = gate.open_vault()
    gate.deposit(g, 200 * E6_ONE)
    gate.mint(g, 100 * E6_ONE)
    gate.set_price(6 * E6_ONE // 10)
    ratio_after = ratio_bps_of(200 * E6_ONE, 100 * E6_ONE, 6 * E6_ONE // 10)
    assert ratio_after == 12_000, ratio_after
    liq = gate.liquidate(g)
    assert liq == {"seized": 175 * E6_ONE, "repaid": 100 * E6_ONE,
                   "shortfall": 0, "returned": 25 * E6_ONE}, liq
    assert gate.stable_supply == 0 and gate.bad_debt == 0 and gate.peg_holds()

    # 2. Peg stress.
    stress = Engine()
    ids = []
    for col in (200 * E6_ONE, 220 * E6_ONE, 300 * E6_ONE):
        vid = stress.open_vault()
        stress.deposit(vid, col)
        stress.mint(vid, 100 * E6_ONE)
        ids.append(vid)
    backed = stress.peg_holds()
    stress.set_price(9 * E6_ONE // 10)
    backed = backed and stress.peg_holds()
    stress.set_price(6 * E6_ONE // 10)
    backed = backed and stress.peg_holds()
    la = stress.liquidate(ids[0])
    lb = stress.liquidate(ids[1])
    backed = backed and stress.peg_holds()
    stress.set_price(45 * E6_ONE // 100)
    backed = backed and stress.peg_holds()
    lc = stress.liquidate(ids[2])
    backed = backed and stress.peg_holds()
    assert (la["seized"], la["returned"]) == (175 * E6_ONE, 25 * E6_ONE), la
    assert (lb["seized"], lb["returned"]) == (175 * E6_ONE, 45 * E6_ONE), lb
    assert lc["seized"] == 233_333_333_333_333_333_333, lc
    assert lc["returned"] == 66_666_666_666_666_666_667, lc
    assert la["shortfall"] + lb["shortfall"] + lc["shortfall"] == 0
    assert stress.bad_debt == 0 and stress.stable_supply == 0 and backed

    summary = {
        "gate": {
            "ratio_after_shock_bps": ratio_after,
            "seized": liq["seized"],
            "repaid": liq["repaid"],
            "shortfall": liq["shortfall"],
            "returned": liq["returned"],
        },
        "stress": {
            "liquidations": stress.liquidations,
            "bad_debt": stress.bad_debt,
            "backed": backed,
            "final_supply": stress.stable_supply,
        },
        "min_ratio_bps": MIN_RATIO_BPS,
        "penalty_bps": PENALTY_BPS,
    }
    sim = {
        "phase": "E6",
        "model": "over-collateralized CDP, min ratio 15000bps, penalty 500bps, floor rounding",
        "invariant": "peg backing never breaks: bad_debt == 0 and backed_value >= stable_supply",
        "scenario": {
            "gate": "200% collateral at -40% price -> liquidation with no bad debt",
            "stress": "3 vaults (200/220/300 vs 100 debt), shocks 0.9/0.6/0.45, 3 liquidations",
        },
        "expected": summary,
    }
    with open("sim/e6-cdp.json", "w", encoding="utf-8") as handle:
        json.dump(sim, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print("E6-MODEL-OK")
    print("E6-SIM-OK " + json.dumps(summary, separators=(", ", ": "), sort_keys=False))


if __name__ == "__main__":
    main()
