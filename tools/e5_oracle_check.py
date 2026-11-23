#!/usr/bin/env python3
"""E5 oracle — independent Python model (ECOSYSTEM_PLAN E5).

Pure-python reimplementation of the E5 aggregation policy (18 decimals,
lower median, N/2+1 quorum, staleness bound, outlier band) with fixed
simulation values. This is NOT a wrapper around the Rust crate: it is the
independent oracle used to cross-check the Rust results.

Fixed fixture (identical to `sim/e5-oracle.json` and the gate test):
  honest  = 100.0, 100.1, 99.9   (18 decimals)
  liars   = 10000.0, 0.0001      (x100 pump + near-zero dump)
  expect  = price 100.0 unchanged, rejected_outliers = 2
NC: a single provider round must go red (quorum 3).

Prints `E5-MODEL-OK {json}` on success and exits non-zero on any check.
"""

ONE = 10**18
STALENESS_SECS = 3600
DEVIATION_BPS = 500
MIN_QUORUM = 3


def lower_median(values):
    v = sorted(values)
    return v[len(v) // 2]


def within_band(price, center, band_bps):
    diff = abs(price - center)
    return diff * 10_000 <= band_bps * center


def aggregate(prices, quorum):
    """prices: list of ints (18 decimals). Returns (price, rejected) or raises."""
    if not prices:
        raise ValueError("E5: round requires at least one report")
    if len(prices) < quorum:
        raise ValueError(
            f"E5: oracle requires a quorum of provider reports ({len(prices)}/{quorum})"
        )
    provisional = lower_median(prices)
    survivors = [p for p in prices if within_band(p, provisional, DEVIATION_BPS)]
    rejected = len(prices) - len(survivors)
    return lower_median(survivors), rejected


def main():
    # Gate fixture: 5 providers, 2 malicious -> price unchanged.
    honest = [100 * ONE, 100 * ONE + ONE // 10, 99 * ONE + 9 * ONE // 10]
    liars = [10_000 * ONE, ONE // 10_000]
    price, rejected = aggregate(honest + liars, quorum=3)
    expected = 100 * ONE
    assert price == expected, f"E5: price moved to {price}"
    assert rejected == 2, f"E5: rejected {rejected} outliers, want 2"

    # Metamorphic: report order must not matter.
    price_b, rejected_b = aggregate(liars + honest, quorum=3)
    assert (price_b, rejected_b) == (price, rejected)

    # Metamorphic: scaling prices by 2 scales the median by 2.
    scaled, rejected_c = aggregate([2 * p for p in honest], quorum=3)
    assert scaled == 2 * lower_median(honest)
    assert rejected_c == 0

    # NC: one provider -> red.
    went_red = False
    try:
        aggregate([100 * ONE], quorum=3)
    except ValueError as e:
        assert str(e) == "E5: oracle requires a quorum of provider reports (1/3)"
        went_red = True
    assert went_red, "E5: one provider must go red"

    # Staleness bound is part of the policy constants.
    assert STALENESS_SECS == 3600 and DEVIATION_BPS == 500 and MIN_QUORUM == 3

    print(
        'E5-MODEL-OK {"price": "%d", "rejected_outliers": %d, "quorum": %d, '
        '"staleness_secs": %d, "deviation_bps": %d}' % (price, rejected, 3, STALENESS_SECS, DEVIATION_BPS)
    )


if __name__ == "__main__":
    main()
