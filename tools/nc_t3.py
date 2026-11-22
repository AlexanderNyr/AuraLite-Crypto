#!/usr/bin/env python3
"""Mutation controls for T3 (ECONOMICS_PLAN T3).

Each control injects ONE bug (restored after) and requires the named T3
message to appear in the red run.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FEES = os.path.join(ROOT, "crates", "aura-storage/src/fees.rs")

TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t3_fees"]

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> wrong base fee formula must go red",
        FEES,
        "Amount::from(T3_BASE_FEE_MAX_CHANGE_DENOMINATOR)",
        "Amount::from(T3_BASE_FEE_MAX_CHANGE_DENOMINATOR + 1)",
        "T3: the base fee must follow the EIP-1559 formula",
    ),
    (
        "NC-2 base part not burned",
        FEES,
        "    let burned = base_fee * gas;",
        "    let burned = 0;",
        "T3: the base part must be burned",
    ),
    (
        "NC-3 tip not credited to the validator",
        FEES,
        "    let tips = tip_per_gas * gas;",
        "    let tips = 0;",
        "T3: the tip must go to the validator",
    ),
    (
        "NC-4 congestion and slack directions swapped",
        FEES,
        "    if gas_used > gas_target {",
        "    if gas_used < gas_target {",
        "T3: congestion must raise the base fee and slack must lower it",
    ),
    (
        "NC-5 elastic gas limit not enforced",
        FEES,
        "    if gas_used > T3_GAS_LIMIT {",
        "    if false {",
        "T3: a block must not exceed the elastic gas limit",
    ),
    (
        "NC-6 pinned parameters drift",
        FEES,
        "pub const T3_INITIAL_BASE_FEE: Amount = 1_000_000_000;",
        "pub const T3_INITIAL_BASE_FEE: Amount = 1_000_000_001;",
        "T3: the reference simulation must agree with the pinned digest",
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
