#!/usr/bin/env python3
"""Mutation controls for T2 (ECONOMICS_PLAN T2).

Each control injects ONE bug (restored after) and requires the named T2
message to appear in the red run. NC-1 is the phase gate literally: one
extra coin in the table must turn the reconciliation red.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ALLOCATION = os.path.join(ROOT, "crates", "aura-storage/src/allocation.rs")

TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage",
            "--test", "t2_allocation"]

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> extra coins must go red",
        ALLOCATION,
        "        total += entry.amount;",
        "        total += entry.amount + 1;",
        "T2: the allocation must sum to the genesis supply",
    ),
    (
        "NC-2 vesting exceeds allocation allowed",
        ALLOCATION,
        "            if schedule.amount > entry.amount {",
        "            if false {",
        "T2: vesting must not exceed its allocation",
    ),
    (
        "NC-3 cliff removed",
        ALLOCATION,
        "    if month < schedule.cliff_months {",
        "    if false {",
        "T2: the cliff must lock everything before it",
    ),
    (
        "NC-4 vesting never completes",
        ALLOCATION,
        "    if month >= schedule.total_months {",
        "    if false {",
        "T2: vesting must complete exactly at the end",
    ),
    (
        "NC-5 lockup ignored",
        ALLOCATION,
        "    entry.amount - locked(entry, month)",
        "    entry.amount",
        "T2: locked coins must not be spendable",
    ),
    (
        "NC-6 table drifts from the reference",
        ALLOCATION,
        "        amount: 1_200_000 * ALCRY,",
        "        amount: 1_200_001 * ALCRY,",
        "T2: the reconciliation must agree with the reference table",
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
