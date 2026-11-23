#!/usr/bin/env python3
"""E4 AMM negative-control mutations (ECOSYSTEM_PLAN E4).

Kept OUTSIDE the repo source (tools-only) so nothing can compile against the
mutated text. Each mutation flips exactly one E4 named message line, expects
exactly one anchored occurrence in `crates/aura-storage/src/amm.rs`, runs
`cargo test --locked -p aura-storage --test e4_amm` and requires a non-zero
exit (the slippage-guard mutation is the gate mutation), then restores the
exact original bytes in a `finally` block.

The suite must fail on every mutation (red) and pass after restore (green).
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SRC = ROOT / "crates" / "aura-storage" / "src" / "amm.rs"
ANCHOR = "pub struct Pool {"
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "e4_amm"]

MUTATIONS = [
    (
        "bad-zero-input",
        '"E4: swap input must be positive"',
        '"E4: swap input"',
    ),
    (
        "bad-no-liquidity",
        '"E4: pool has no liquidity"',
        '"E4: pool empty"',
    ),
    (
        "bad-slippage",
        '"E4: slippage guard: output {out} below minimum {min_out}"',
        '"E4: slippage guard"',
    ),
    (
        "bad-proportional",
        '"E4: mint amounts must be proportional to reserves"',
        '"E4: mint amounts"',
    ),
    (
        "bad-burn",
        '"E4: burn exceeds LP supply ({lp}/{supply})"',
        '"E4: burn exceeds LP supply"',
    ),
    (
        "bad-token",
        '"E4: swap token not in pool ({token:?})"',
        '"E4: swap token not in pool"',
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
