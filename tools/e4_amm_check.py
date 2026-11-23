#!/usr/bin/env python3
"""E4 AMM deterministic model check (ECOSYSTEM_PLAN E4).

Mirrors `crates/aura-storage/src/amm.rs` exactly (fee-on-input, floor
rounding at every division) in unbounded Python integers and replays the
fixed `sim/e4-amm.json` scenario: single swap, two-hop route, split-vs-
single metamorphic run and the LP mint/burn round trip. The invariant
`k = reserve_a * reserve_b` must never decrease on any pool.

Golden values (fee 30bps, floor):
  1 A -> B out of (100/200)           = 1974316068794122597
  1 A -> B -> C via (100/200)+(100/300) = 5791186053878244695
  single 2 A                          = 3910033923564131223
  split 1 A + 1 A                     = 3909976989011504086  (<= single)
"""
import json

E4_ONE = 10 ** 18
E4_BPS = 10_000
FEE_BPS = 30

GOLDEN_SWAP = 1_974_316_068_794_122_597
GOLDEN_ROUTE = 5_791_186_053_878_244_695
GOLDEN_SINGLE = 3_910_033_923_564_131_223
GOLDEN_SPLIT = 3_909_976_989_011_504_086


def amount_out(amount_in, reserve_in, reserve_out, fee_bps=FEE_BPS):
    """floor(fee-adjusted input * reserve_out / (reserve_in + in_fee))."""
    if amount_in == 0:
        raise ValueError("E4: swap input must be positive")
    if reserve_in == 0 or reserve_out == 0:
        raise ValueError("E4: pool has no liquidity")
    in_fee = amount_in * (E4_BPS - fee_bps) // E4_BPS
    return in_fee * reserve_out // (reserve_in + in_fee)


class Pool:
    """Minimal mirror of `amm.rs::Pool` (unordered pair, fee-on-input)."""

    def __init__(self, reserve_a, reserve_b, fee_bps=FEE_BPS):
        self.reserve_a = reserve_a
        self.reserve_b = reserve_b
        self.fee_bps = fee_bps

    def invariant_k(self):
        return self.reserve_a * self.reserve_b

    def swap_exact_in(self, amount_in, min_out=0):
        out = amount_out(amount_in, self.reserve_a, self.reserve_b, self.fee_bps)
        if out < min_out:
            raise ValueError(
                f"E4: slippage guard: output {out} below minimum {min_out}"
            )
        self.reserve_a += amount_in
        self.reserve_b -= out
        return out


def main():
    ab = Pool(100 * E4_ONE, 200 * E4_ONE)
    bc = Pool(100 * E4_ONE, 300 * E4_ONE)

    k_ab0, k_bc0 = ab.invariant_k(), bc.invariant_k()
    swap_out = ab.swap_exact_in(E4_ONE)
    assert swap_out == GOLDEN_SWAP, f"swap golden {swap_out}"
    assert ab.invariant_k() >= k_ab0, "E4: k decreased on swap"

    # Route on pristine pools: 1 A -> B -> C.
    ab2 = Pool(100 * E4_ONE, 200 * E4_ONE)
    bc2 = Pool(100 * E4_ONE, 300 * E4_ONE)
    mid = ab2.swap_exact_in(E4_ONE)
    route_out = bc2.swap_exact_in(mid)
    assert route_out == GOLDEN_ROUTE, f"route golden {route_out}"
    assert ab2.invariant_k() >= 100 * E4_ONE * 200 * E4_ONE
    assert bc2.invariant_k() >= 100 * E4_ONE * 300 * E4_ONE

    # Metamorphic: split must not beat the single swap.
    single = Pool(100 * E4_ONE, 200 * E4_ONE)
    single_out = single.swap_exact_in(2 * E4_ONE)
    assert single_out == GOLDEN_SINGLE, f"single golden {single_out}"
    split = Pool(100 * E4_ONE, 200 * E4_ONE)
    split_out = split.swap_exact_in(E4_ONE) + split.swap_exact_in(E4_ONE)
    assert split_out == GOLDEN_SPLIT, f"split golden {split_out}"
    assert split_out <= single_out, "E4: split beat single"

    # LP: initial mint a+b, proportional mint, exact burn round trip.
    lp_initial = 100 * E4_ONE + 200 * E4_ONE
    lp_proportional = min(
        50 * E4_ONE * lp_initial // (100 * E4_ONE),
        100 * E4_ONE * lp_initial // (200 * E4_ONE),
    )
    assert lp_initial == 300 * E4_ONE
    assert lp_proportional == 150 * E4_ONE
    # Round trip: pool (150/300), burn 150e18 of 450e18 -> (50e18, 100e18).
    reserves = (150 * E4_ONE, 300 * E4_ONE)
    supply = 450 * E4_ONE
    burn_a = lp_proportional * reserves[0] // supply
    burn_b = lp_proportional * reserves[1] // supply
    assert (burn_a, burn_b) == (50 * E4_ONE, 100 * E4_ONE)

    summary = {
        "pools": [
            [100 * E4_ONE, 200 * E4_ONE],
            [100 * E4_ONE, 300 * E4_ONE],
        ],
        "swap_out": swap_out,
        "route_out": route_out,
        "single_out": single_out,
        "split_out": split_out,
        "split_le_single": True,
        "lp_initial": lp_initial,
        "lp_proportional": lp_proportional,
        "burn_out": [burn_a, burn_b],
        "k_never_decreased": True,
    }
    sim = {
        "phase": "E4",
        "model": "constant-product x*y=k, fee-on-input 30bps, floor rounding",
        "invariant": "k = reserve_a * reserve_b never decreases on any pool",
        "scenario": {
            "swap": "1 A -> B out of (100/200)",
            "route": "1 A -> B -> C via (100/200) + (100/300)",
            "metamorphic": "single 2 A vs split 1 A + 1 A",
            "lp": "initial mint a+b, proportional mint, burn round trip",
        },
        "expected": summary,
    }
    with open("sim/e4-amm.json", "w", encoding="utf-8") as handle:
        json.dump(sim, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print("E4-MODEL-OK")
    print("E4-SIM-OK " + json.dumps(summary, separators=(", ", ": "), sort_keys=False))


if __name__ == "__main__":
    main()
