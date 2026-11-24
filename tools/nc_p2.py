#!/usr/bin/env python3
"""P2 parallel execution negative-control mutations (PERF_PLAN P2 DoD:
"NC: revert -> red").

Kept OUTSIDE the repo source (tools-only) so nothing can compile
against the mutated text. Each mutation **reverts one P2 optimization
aspect** (six mutations, one per named message target) in exactly one
anchored source line, runs `cargo test --locked -p aura-bench --test
p2_parallel` and requires a non-zero exit, then restores the exact
original bytes in a `finally` block.

The suite must fail on every mutation (red) and pass after restore
(green). The named messages asserted by the tests:

  P2: parallel result mismatch ({expected}/{actual})  <- bad-parallel
  P2: conflict not rolled back (tx {n})               <- bad-rollback
  P2: retry order violated (tx {n})                   <- bad-retry-order
  P2: speedup regression ({before}/{after})           <- bad-speedup
  P2: conflict count mismatch ({expected}/{actual})   <- bad-conflicts
  P2: determinism violated (run {run})                <- bad-determinism
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
MODEL = ROOT / "crates" / "aura-bench" / "src" / "p2.rs"
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-bench", "--test", "p2_parallel"]

MUTATIONS = [
    # Revert conflict detection: every speculative incarnation validates
    # (stale cross-chunk writes commit -> parallel != sequential).
    (
        "bad-parallel",
        MODEL,
        "        for spec in specs.iter() {\n            if reads_valid(spec, &committed) {",
        "        for spec in specs.iter() {\n            if true {",
    ),
    # Revert honest rollback: apply the discarded speculative writes
    # before the retry (a leak must be diagnosed as
    # `P2: conflict not rolled back` or diverge the state).
    (
        "bad-rollback",
        MODEL,
        "                aborted += 1;",
        "                aborted += 1; for (cell, val) in &spec.writes { committed.insert(*cell, *val); }",
    ),
    # Revert the serializable order: commit chunks in reverse.
    (
        "bad-retry-order",
        MODEL,
        "    for specs in chunk_specs.iter() {",
        "    for specs in chunk_specs.iter().rev() {",
    ),
    # Revert the parallel win: one worker (par cost back above seq).
    (
        "bad-speedup",
        MODEL,
        "pub const WORKERS: usize = 8;",
        "pub const WORKERS: usize = 1;",
    ),
    # Corrupt the conflict accounting (each abort counted twice).
    (
        "bad-conflicts",
        MODEL,
        "                aborted += 1;",
        "                aborted += 2;",
    ),
    # Break determinism: one repeat run differs.
    (
        "bad-determinism",
        MODEL,
        "        let run_costs = vec![par; runs];",
        "        let mut run_costs = vec![par; runs]; run_costs[2] += 1;",
    ),
]


def run(name: str, path: pathlib.Path, old: str, new: str) -> bool:
    original = path.read_text(encoding="utf-8")
    if original.count(old) != 1:
        print(f"NC-RED [{name}] anchor not unique in {path.name}: FAIL")
        return False
    try:
        path.write_text(original.replace(old, new), encoding="utf-8")
        proc = subprocess.run(TEST_CMD, capture_output=True, text=True, cwd=ROOT)
        red = proc.returncode != 0
        print(f"NC-RED [{name}] {'ok' if red else 'FAIL (tests still green!)'}")
        return red
    finally:
        path.write_text(original, encoding="utf-8")


def main() -> int:
    failures = 0
    for name, path, old, new in MUTATIONS:
        if not run(name, path, old, new):
            failures += 1
    proc = subprocess.run(TEST_CMD, capture_output=True, text=True, cwd=ROOT)
    green = proc.returncode == 0
    print(f"NC-GREEN restore ok ({len(MUTATIONS) - failures}/{len(MUTATIONS)} red as required)"
          if green else "NC-GREEN restore: FAIL (tests still red)")
    return 0 if failures == 0 and green else 1


if __name__ == "__main__":
    sys.exit(main())
