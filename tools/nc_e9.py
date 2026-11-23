#!/usr/bin/env python3
"""E9 bridge negative-control mutations (ECOSYSTEM_PLAN E9).

Kept OUTSIDE the repo source (tools-only) so nothing can compile against the
mutated text. Each mutation flips exactly one E9 named message line (six
mutations, one per targeted message; `bad-finalized` is the gate mutation
for the "OS client checks finalized checkpoint" DoD), expects exactly one
anchored occurrence in `crates/aura-storage/src/bridge.rs`, runs
`cargo test --locked -p aura-storage --test e9_bridge` and requires a
non-zero exit, then restores the exact original bytes in a `finally` block.

The suite must fail on every mutation (red) and pass after restore (green).
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SRC = ROOT / "crates" / "aura-storage" / "src" / "bridge.rs"
ANCHOR = "pub struct Bridge {"
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "e9_bridge"]

MUTATIONS = [
    (
        "bad-forged-header",
        '"E9: forged header signature ({signer})"',
        '"E9: forged header signature"',
    ),
    (
        "bad-quorum",
        '"E9: commit lacks quorum ({signed}/{required})"',
        '"E9: commit lacks quorum"',
    ),
    (
        "bad-chain",
        '"E9: header does not chain ({height})"',
        '"E9: header does not chain"',
    ),
    (
        "bad-finalized",
        '"E9: checkpoint not finalized ({height})"',
        '"E9: checkpoint not finalized"',
    ),
    (
        "bad-replay",
        '"E9: message already processed ({id})"',
        '"E9: message already processed"',
    ),
    (
        "bad-pause",
        '"E9: bridge is paused"',
        '"E9: bridge paused"',
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
                print(f"NC-BAD [{name}] anchor count != 1: {text.count(old)}",
                      file=sys.stderr)
                return 1
            SRC.write_bytes(text.replace(old, new, 1).encode())
            proc = subprocess.run(TEST_CMD, cwd=ROOT, capture_output=True, text=True)
            if proc.returncode == 0:
                print(f"NC-BAD [{name}] mutation survived (test still green)",
                      file=sys.stderr)
                return 1
            passed += 1
            print(f"NC-RED [{name}] ok")
    finally:
        SRC.write_bytes(original)

    proc = subprocess.run(TEST_CMD, cwd=ROOT, capture_output=True, text=True)
    if proc.returncode != 0:
        print("NC-BAD restore: suite still red", file=sys.stderr)
        return 1
    print(f"NC-GREEN restore ok ({passed}/6 red as required)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
