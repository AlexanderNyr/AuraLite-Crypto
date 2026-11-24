#!/usr/bin/env python3
"""P5 consensus negative-control mutations (PERF_PLAN P5 DoD:
"p99 finality at 100 nodes < 2 s"; D-P2: every optimization revert ->
red).

Kept OUTSIDE the repo source (tools-only) so nothing can compile
against the mutated text. Each mutation **reverts one P5 optimization
aspect or structural rule** (six mutations, one per named message
target) in exactly one anchored source block, runs `cargo test
--locked -p aura-bench --test p5_consensus` and requires a non-zero
exit, then restores the exact original bytes in a `finally` block.

The suite must fail on every mutation (red) and pass after restore
(green). The named messages asserted by the tests:

  P5: p99 finality exceeds SLO ({p99}/{slo})            <- bad-timeout
  P5: pipelining overlap missing ({inc}/{full})         <- bad-pipeline
  P5: QC cache win reverted ({before}/{after})          <- bad-qc-cache
  P5: 3-chain rule violated (block {block})             <- bad-chain
  P5: quorum accounting corrupted ({have}/{need})       <- bad-quorum
  P5: determinism violated (run {run})                  <- bad-determinism
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
MODEL = ROOT / "crates" / "aura-bench" / "src" / "p5.rs"
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-bench", "--test", "p5_consensus"]

MUTATIONS = [
    # Revert the faster view change (300 -> 2000 ms, the shipped
    # DEFAULT_VIEW_TIMEOUT_MS): p99 finality goes back over the SLO.
    (
        "bad-timeout",
        MODEL,
        "pub const TIMEOUT_MS_AFTER: u64 = 300;",
        "pub const TIMEOUT_MS_AFTER: u64 = 2000;",
    ),
    # Revert the pipelining overlap: the chain extension pays the full
    # view again (proposal no longer overlapped).
    (
        "bad-pipeline",
        MODEL,
        "    VOTE_HOPS * HOP_MS + VOTE_VERIFY_MS + QC_FORM_MS + QC_HOPS * qc_verify_ms + COMMIT_MS",
        "    view_full_latency(qc_verify_ms)",
    ),
    # Revert the QC cache: O(1) check becomes per-signature again.
    (
        "bad-qc-cache",
        MODEL,
        "pub const QC_VERIFY_CACHED_MS: u64 = 1;",
        "pub const QC_VERIFY_CACHED_MS: u64 = 12;",
    ),
    # Revert the 3-chain rule to a 2-chain.
    (
        "bad-chain",
        MODEL,
        "pub const CHAIN_DEPTH: usize = 3;",
        "pub const CHAIN_DEPTH: usize = 2;",
    ),
    # Corrupt quorum accounting: threshold drops from 2f+1 to f+1.
    (
        "bad-quorum",
        MODEL,
        "    2 * ((n - 1) / 3) + 1",
        "    (n - 1) / 3 + 1",
    ),
    # Break determinism: one repeat run differs.
    (
        "bad-determinism",
        MODEL,
        "        let total = p99 * runs as u64;",
        "        let total = p99 * runs as u64 + 1;",
    ),
]

AUDIT_LINES = {
    "bad-timeout": "NC-1 faster view change reverted (300 -> 2000 ms): red with named message (bad-timeout)",
    "bad-pipeline": "NC-2 pipelining overlap reverted (full view again): red with named message (bad-pipeline)",
    "bad-qc-cache": "NC-3 QC cache reverted (O(1) -> per-signature): red with named message (bad-qc-cache)",
    "bad-chain": "NC-4 3-chain rule reverted (depth 3 -> 2): red with named message (bad-chain)",
    "bad-quorum": "NC-5 quorum accounting corrupted (2f+1 -> f+1): red with named message (bad-quorum)",
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
    print("[evidence_p5] cmd=tools/nc_p5.py")
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
