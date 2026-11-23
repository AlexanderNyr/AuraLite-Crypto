#!/usr/bin/env python3
"""E7 lending negative-control mutations (ECOSYSTEM_PLAN E7).

Kept OUTSIDE the repo source (tools-only) so nothing can compile against the
mutated text. Each mutation flips exactly one E7 named message line, expects
exactly one anchored occurrence in `crates/aura-storage/src/lending.rs`, runs
`cargo test --locked -p aura-storage --test e7_lending` and requires a non-zero
exit (the borrow-capacity mutation is the gate mutation), then restores the
exact original bytes in a `finally` block.

The suite must fail on every mutation (red) and pass after restore (green).
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SRC = ROOT / "crates" / "aura-storage" / "src" / "lending.rs"
ANCHOR = "pub struct LendingMarket {"
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "e7_lending"]

MUTATIONS = [
    (
        "bad-account",
        '"E7: account not found ({id})"',
        '"E7: account missing"',
    ),
    (
        "bad-zero-supply",
        '"E7: supply must be positive"',
        '"E7: supply"',
    ),
    (
        "bad-borrow-capacity",
        '"E7: borrow exceeds collateral capacity ({amount}/{capacity})"',
        '"E7: borrow exceeds collateral capacity"',
    ),
    (
        "bad-repay-over",
        '"E7: repay exceeds debt ({repay}/{debt})"',
        '"E7: repay exceeds debt"',
    ),
    (
        "bad-withdraw-capacity",
        '"E7: withdraw would breach collateral capacity ({amount}/{capacity})"',
        '"E7: withdraw would breach collateral capacity"',
    ),
    (
        "bad-healthy-liq",
        '"E7: account is healthy, liquidation refused ({health_bps} >= {min_bps})"',
        '"E7: account is healthy"',
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
