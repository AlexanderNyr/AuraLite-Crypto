#!/usr/bin/env python3
"""P0 baseline bench negative-control mutations (PERF_PLAN P0).

Kept OUTSIDE the repo source (tools-only) so nothing can compile
against the mutated text. Each mutation flips exactly one P0 named
message line (six mutations, one per targeted message; `bad-workload`
is the gate mutation for the DoD "substituted workload -> different
numbers"), expects exactly one anchored occurrence in
`crates/aura-bench/src/lib.rs`, runs
`cargo test --locked -p aura-bench --test p0_bench` and requires a
non-zero exit, then restores the exact original bytes in a `finally`
block.

The suite must fail on every mutation (red) and pass after restore
(green).
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SRC = ROOT / "crates" / "aura-bench" / "src" / "lib.rs"
ANCHOR = "pub enum BenchError {"
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-bench", "--test", "p0_bench"]

MUTATIONS = [
    (
        "bad-workload",
        '"P0: workload mismatch ({expected}/{actual})"',
        '"P0: workload mismatch"',
    ),
    (
        "bad-machine",
        '"P0: machine profile mismatch ({expected}/{actual})"',
        '"P0: machine profile mismatch"',
    ),
    (
        "bad-spread",
        '"P0: spread exceeds limit ({spread}%/{limit}%)"',
        '"P0: spread exceeds limit"',
    ),
    (
        "bad-runs",
        '"P0: repeat count violated ({runs}/{need})"',
        '"P0: repeat count violated"',
    ),
    (
        "bad-determinism",
        '"P0: determinism violated (run {run})"',
        '"P0: determinism violated"',
    ),
    (
        "bad-metric",
        '"P0: metric not published ({metric})"',
        '"P0: metric not published"',
    ),
]


def main() -> int:
    original = SRC.read_bytes()
    text = original.decode()
    anchor_count = text.count(ANCHOR)
    if anchor_count != 1:
        print(f"NC-BAD anchor count != 1: {anchor_count}")
        return 1
    red = 0
    try:
        for name, needle, replacement in MUTATIONS:
            if text.count(needle) != 1:
                print(f"NC-BAD [{name}] needle count != 1: {text.count(needle)}")
                return 1
            mutated = text.replace(needle, replacement)
            SRC.write_text(mutated, encoding="utf-8")
            proc = subprocess.run(TEST_CMD, cwd=ROOT, capture_output=True, text=True)
            if proc.returncode == 0:
                print(f"NC-BAD [{name}] suite still green after mutation")
                return 1
            red += 1
            print(f"NC-RED [{name}] ok")
    finally:
        SRC.write_bytes(original)
    proc = subprocess.run(TEST_CMD, cwd=ROOT, capture_output=True, text=True)
    if proc.returncode != 0:
        print("NC-BAD restore did not turn the suite green")
        print(proc.stdout[-2000:])
        return 1
    print(f"NC-GREEN restore ok ({red}/6 red as required)")
    return 0 if red == 6 else 1


if __name__ == "__main__":
    raise SystemExit(main())
