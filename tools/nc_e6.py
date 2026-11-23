#!/usr/bin/env python3
"""E6 stablecoin CDP negative-control mutations (ECOSYSTEM_PLAN E6).

Kept OUTSIDE the repo source (tools-only) so nothing can compile against the
mutated text. Each mutation flips exactly one E6 named message line, expects
exactly one anchored occurrence in `crates/aura-storage/src/cdp.rs`, runs
`cargo test --locked -p aura-storage --test e6_cdp` and requires a non-zero
exit (the mint-ratio mutation is the gate mutation), then restores the exact
original bytes in a `finally` block.

The suite must fail on every mutation (red) and pass after restore (green).
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SRC = ROOT / "crates" / "aura-storage" / "src" / "cdp.rs"
ANCHOR = "pub struct CdpEngine {"
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "e6_cdp"]

MUTATIONS = [
    (
        "bad-not-found",
        '"E6: vault not found ({id})"',
        '"E6: vault missing"',
    ),
    (
        "bad-zero-deposit",
        '"E6: deposit must be positive"',
        '"E6: deposit"',
    ),
    (
        "bad-mint-ratio",
        '"E6: mint exceeds collateral ratio ({ratio_bps}/{min_bps})"',
        '"E6: mint exceeds collateral ratio"',
    ),
    (
        "bad-repay-over",
        '"E6: repay exceeds debt ({repay}/{debt})"',
        '"E6: repay exceeds debt"',
    ),
    (
        "bad-withdraw-ratio",
        '"E6: withdraw would breach collateral ratio ({ratio_bps}/{min_bps})"',
        '"E6: withdraw would breach collateral ratio"',
    ),
    (
        "bad-healthy-liq",
        '"E6: vault is healthy, liquidation refused ({ratio_bps} >= {min_bps})"',
        '"E6: vault is healthy"',
    ),
]


def main() -> int:
    original = SRC.read_bytes()
    text = original.decode()
    if text.count(ANCHOR) != 1:
        print(f"NC-BAD anchor count != 1: {text.count(ANCHOR)}", file=sys.stderr)
        return 1

    passed = 0
    try:
        for name, old, new in MUTATIONS:
            if text.count(old) != 1:
                print(f"NC-BAD [{name}] anchor count != 1: {text.count(old)}", file=sys.stderr)
                return 1
            mutated = text.replace(old, new, 1)
            SRC.write_bytes(mutated.encode())
            proc = subprocess.run(TEST_CMD, cwd=ROOT, capture_output=True, text=True)
            if proc.returncode == 0:
                print(f"NC-BAD [{name}] mutation survived (test still green)", file=sys.stderr)
                return 1
            passed += 1
            print(f"NC-RED [{name}] ok")
    finally:
        SRC.write_bytes(original)

    proc = subprocess.run(TEST_CMD, cwd=ROOT, capture_output=True, text=True)
    if proc.returncode != 0:
        print("NC-BAD restore did not return to green", file=sys.stderr)
        return 1
    print(f"NC-GREEN restored: {passed}/{len(MUTATIONS)} mutations turned the gate red")
    return 0


if __name__ == "__main__":
    sys.exit(main())
