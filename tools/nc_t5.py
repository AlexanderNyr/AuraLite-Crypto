#!/usr/bin/env python3
"""Mutation controls for T5 (ECONOMICS_PLAN T5).

Each control injects ONE bug (restored after) and requires the named T5
message to appear in the red run. NC-1/NC-2 are the phase invariants
literally: penalty >= damage, fund never negative.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SLASHING = os.path.join(ROOT, "crates", "aura-storage/src/slashing.rs")

TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t5_slashing"]

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 penalty below damage",
        SLASHING,
        "pub const T5_PENALTY_BPS: u64 = 15_000;",
        "pub const T5_PENALTY_BPS: u64 = 9_999;",
        "T5: the penalty must cover the damage",
    ),
    (
        "NC-2 coverage clamp removed (fund overdrawn)",
        SLASHING,
        "let covered = damage.min(free);",
        "let covered = damage;",
        "T5: the insurance fund must never go negative",
    ),
    (
        "NC-3 mass-slash cap raised",
        SLASHING,
        "pub const T5_MASS_SLASH_CAP_BPS: u64 = 500;",
        "pub const T5_MASS_SLASH_CAP_BPS: u64 = 5_000;",
        "T5: the mass-slash cap must hold in every window",
    ),
    (
        "NC-4 deferred penalty dropped",
        SLASHING,
        "self.pending -= taken;",
        "self.pending = 0;",
        "T5: deferred penalties must not be dropped",
    ),
    (
        "NC-5 insurance income off by one",
        SLASHING,
        "    applied * Amount::from(T5_FUND_SHARE_BPS) / Amount::from(T5_BPS)",
        "    applied * Amount::from(T5_FUND_SHARE_BPS) / Amount::from(T5_BPS) + 1",
        "T5: the insurance income must be exactly the fund share of the applied penalty",
    ),
    (
        "NC-6 penalty formula drift",
        SLASHING,
        "    damage * Amount::from(T5_PENALTY_BPS) / Amount::from(T5_BPS)",
        "    damage * Amount::from(T5_PENALTY_BPS) / Amount::from(T5_BPS + 1)",
        "T5: the penalty must be exactly damage times the pinned parameter",
    ),
    (
        "NC-7 pinned parameters drift",
        SLASHING,
        "pub const T5_FUND_SHARE_BPS: u64 = 5_000;",
        "pub const T5_FUND_SHARE_BPS: u64 = 5_001;",
        "T5: the reference simulation must agree with the pinned digest",
    ),
]

PASS_MARKERS = ("test result: ok.",)


def run_control(name, path, anchor, replacement, expected):
    with open(path, encoding="utf-8") as f:
        original = f.read()
    if original.count(anchor) != 1:
        return False, "anchor count %d" % original.count(anchor)
    mutated = original.replace(anchor, replacement)
    try:
        with open(path, "w", encoding="utf-8") as f:
            f.write(mutated)
        proc = subprocess.run(TEST_CMD, cwd=ROOT, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True,
                              errors="replace")
        output = proc.stdout or ""
    finally:
        with open(path, "w", encoding="utf-8") as f:
            f.write(original)
    compile_failed = "error[" in output and "test result" not in output
    if compile_failed:
        return False, "mutation broke compilation"
    if any(marker in output for marker in PASS_MARKERS):
        return False, "mutated gate still passes"
    if expected in output:
        return True, "red with named message"
    return False, "red without named message"


def main():
    passed = 0
    for name, path, anchor, replacement, expected in CONTROLS:
        ok, why = run_control(name, path, anchor, replacement, expected)
        print("[%s] %s: %s" % ("PASS" if ok else "FAIL", name, why))
        if ok:
            passed += 1
    total = len(CONTROLS)
    print("RESULT: %d/%d negative controls PASS" % (passed, total))
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
