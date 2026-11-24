#!/usr/bin/env python3
"""P1 codec/crypto negative-control mutations (PERF_PLAN P1 DoD:
"NC: revert -> red").

Kept OUTSIDE the repo source (tools-only) so nothing can compile
against the mutated text. Each mutation **reverts one P1 optimization
aspect** (six mutations, one per named message target) in exactly one
anchored source line, runs `cargo test --locked -p aura-bench --test
p1_crypto` and requires a non-zero exit, then restores the exact
original bytes in a `finally` block.

The suite must fail on every mutation (red) and pass after restore
(green). The named messages asserted by the tests:

  P1: batch verify mismatch (item {n})          <- bad-batch
  P1: zero-copy decode mismatch                 <- bad-zc
  P1: malformed input accepted ({case})         <- bad-malformed
  P1: verify speedup regression ({before}/{after}) <- bad-verify-cost
  P1: aggregate verify regression ({before}/{after}) <- bad-agg-cost
  P1: determinism violated (run {run})          <- bad-determinism
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-bench", "--test", "p1_crypto"]

BATCH = ROOT / "crates" / "aura-crypto" / "src" / "ed25519.rs"
CODEC = ROOT / "crates" / "aura-codec" / "src" / "transfer.rs"
MODEL = ROOT / "crates" / "aura-bench" / "src" / "p1.rs"

MUTATIONS = [
    # Revert the batch diagnostic contract: blame item 0 instead of the
    # first failing item (the batch then disagrees with the sequential
    # verdict's index and gate_batch reports a mismatch).
    (
        "bad-batch",
        BATCH,
        "            return Err((i, e));",
        "            return Err((0, e));",
    ),
    # Revert zero-copy field borrowing: decode `from` from the `to` slot.
    (
        "bad-zc",
        CODEC,
        "    let from: &[u8; 32] = bytes[OFF_FROM..OFF_TO]",
        "    let from: &[u8; 32] = bytes[OFF_TO..OFF_AMOUNT]",
    ),
    # Revert the canonical wire check: accept any version byte pair.
    (
        "bad-malformed",
        CODEC,
        "    if version != TRANSFER_WIRE_VERSION {",
        "    if false {",
    ),
    # Revert the batch cost model: marginal cost 20 -> 36 puts the
    # amortized per-signature cost back to the single-verify 38.
    (
        "bad-verify-cost",
        MODEL,
        "pub const VERIFY_BATCH_PER: u64 = 20;",
        "pub const VERIFY_BATCH_PER: u64 = 36;",
    ),
    # Revert the BLS aggregate model: per-key add 2 -> 40 puts the
    # aggregate cost at/above individual verification.
    (
        "bad-agg-cost",
        MODEL,
        "pub const AGG_ADD: u64 = 2;",
        "pub const AGG_ADD: u64 = 40;",
    ),
    # Break determinism: one repeat run differs from the others.
    (
        "bad-determinism",
        MODEL,
        "        let run_costs = vec![per_run; runs];",
        "        let mut run_costs = vec![per_run; runs]; run_costs[2] += 1;",
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
