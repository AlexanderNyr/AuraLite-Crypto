#!/usr/bin/env python3
"""E5 oracle negative-control mutations (ECOSYSTEM_PLAN E5).

Kept OUTSIDE the repo source (tools-only) so nothing can compile against the
mutated text. Each mutation flips exactly one E5 named message / policy line,
expects exactly one anchored occurrence in `crates/aura-storage/src/oracle.rs`,
runs `cargo test --locked -p aura-storage --test e5_oracle` and requires a
non-zero exit, then restores the exact original bytes in a `finally` block.

The suite must fail on every mutation (red) and pass after restore (green).
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SRC = ROOT / "crates" / "aura-storage" / "src" / "oracle.rs"
ANCHOR = "pub struct OracleNetwork {"
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "e5_oracle"]

MUTATIONS = [
    (
        "bad-quorum",
        '"E5: oracle requires a quorum of provider reports ({valid}/{required})"',
        '"E5: oracle requires a quorum of provider reports"',
    ),
    (
        "bad-duplicate",
        '"E5: one report per provider per round (provider {provider})"',
        '"E5: provider sent a report"',
    ),
    (
        "bad-unknown",
        '"E5: report from unregistered provider {provider}"',
        '"E5: report from provider {p}"',
    ),
    (
        "bad-feed",
        '"E5: all reports must target this feed (provider {provider})"',
        '"E5: feed mismatch (provider {p})"',
    ),
    (
        "bad-stale",
        '"E5: provider report must be fresh (provider {provider}, ts {timestamp}, now {now})"',
        '"E5: provider report must be fresh (provider {p})"',
    ),
    (
        "bad-empty",
        '"E5: round requires at least one report"',
        '"E5: round requires reports"',
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
