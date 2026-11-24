#!/usr/bin/env python3
"""P4 network negative-control mutations (PERF_PLAN P4 DoD:
"100 nodes: traffic/validator within budget"; D-P2: every
optimization revert -> red).

Kept OUTSIDE the repo source (tools-only) so nothing can compile
against the mutated text. Each mutation **reverts one P4 optimization
aspect** (six mutations, one per named message target) in exactly one
anchored source block, runs `cargo test --locked -p aura-bench --test
p4_network` and requires a non-zero exit, then restores the exact
original bytes in a `finally` block.

The suite must fail on every mutation (red) and pass after restore
(green). The named messages asserted by the tests:

  P4: traffic/validator exceeds budget ({bytes}/{budget})          <- bad-budget
  P4: vote batching win reverted ({before}/{after})                <- bad-batching
  P4: compression roundtrip mismatch (byte {byte})                 <- bad-compression
  P4: gossip parameter set inconsistent (D={d}/{lo}/{hi})          <- bad-params
  P4: mesh disconnected at {nodes} nodes                           <- bad-mesh
  P4: determinism violated (run {run})                             <- bad-determinism
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
MODEL = ROOT / "crates" / "aura-bench" / "src" / "p4.rs"
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-bench", "--test", "p4_network"]

MUTATIONS = [
    # Revert mesh tuning (D 5 -> 8): per-validator traffic jumps back
    # over the published budget.
    (
        "bad-budget",
        MODEL,
        "pub const D_AFTER: usize = 5;",
        "pub const D_AFTER: usize = 8;",
    ),
    # Revert vote batching (k 8 -> 1): the batching win disappears.
    (
        "bad-batching",
        MODEL,
        "pub const BATCH_K: usize = 8;",
        "pub const BATCH_K: usize = 1;",
    ),
    # Revert the lossless codec: the final RLE run loses a byte.
    (
        "bad-compression",
        MODEL,
        "        }\n        out.push((value, count));",
        "        }\n        out.push((value, count.saturating_sub(1)));",
    ),
    # Revert the parameter set: D_high drops below the tuned D.
    (
        "bad-params",
        MODEL,
        "pub const D_HI: usize = 8;",
        "pub const D_HI: usize = 4;",
    ),
    # Revert the mesh ring: +-1 links become +-2 (even/odd split).
    (
        "bad-mesh",
        MODEL,
        "vec![(i + 1) % n, back(1), (i + 2) % n, back(2), (i + n / 2) % n]",
        "vec![(i + 2) % n, back(2), (i + 2) % n, back(2), (i + n / 2) % n]",
    ),
    # Break determinism: one repeat run differs.
    (
        "bad-determinism",
        MODEL,
        "        let run_costs = vec![after; runs];",
        "        let mut run_costs = vec![after; runs]; run_costs[2] += 1;",
    ),
]

AUDIT_LINES = {
    "bad-budget": "NC-1 mesh tuning reverted (D 5 -> 8): red with named message (bad-budget)",
    "bad-batching": "NC-2 vote batching reverted (k 8 -> 1): red with named message (bad-batching)",
    "bad-compression": "NC-3 lossless codec reverted (RLE loses a byte): red with named message (bad-compression)",
    "bad-params": "NC-4 parameter set reverted (D_high below D): red with named message (bad-params)",
    "bad-mesh": "NC-5 mesh ring reverted (even/odd split): red with named message (bad-mesh)",
    "bad-determinism": "NC-6 determinism broken: red with named message (bad-determinism)",
}


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
    print("[evidence_p4] cmd=tools/nc_p4.py")
    failures = 0
    red_names = []
    for name, path, old, new in MUTATIONS:
        if run(name, path, old, new):
            red_names.append(name)
        else:
            failures += 1
    proc = subprocess.run(TEST_CMD, capture_output=True, text=True, cwd=ROOT)
    green = proc.returncode == 0
    print(f"NC-GREEN restore ok ({len(MUTATIONS) - failures}/{len(MUTATIONS)} red as required)"
          if green else "NC-GREEN restore: FAIL (tests still red)")
    for name, _, _, _ in MUTATIONS:
        if name in red_names:
            print(f"[PASS] {AUDIT_LINES[name]}")
        else:
            print(f"[FAIL] {AUDIT_LINES[name]} (tests stayed green)")
    if failures == 0 and green:
        print(f"RESULT: {len(MUTATIONS)}/{len(MUTATIONS)} negative controls PASS")
    return 0 if failures == 0 and green else 1


if __name__ == "__main__":
    sys.exit(main())
