#!/usr/bin/env python3
"""Mutation controls for T1 (ECONOMICS_PLAN T1).

Each control injects ONE bug (restored after) and requires the named T1
message to appear in the red run. NC-1 is the phase gate literally: the
10-year supply must match the issuance formula within 1 unit — an extra
unit per year must turn it red.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ISSUANCE = os.path.join(ROOT, "crates", "aura-storage/src/issuance.rs")

TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage",
            "--test", "t1_issuance"]

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> formula mismatch must go red",
        ISSUANCE,
        "            .checked_add(issued)",
        "            .checked_add(issued + 1)",
        "T1: the supply must match the issuance formula",
    ),
    (
        "NC-2 halving removed",
        ISSUANCE,
        "        let shift = (year - 1) / self.params.halving_years;",
        "        let shift = 0;",
        "T1: the halving must cut the emission in half every period",
    ),
    (
        "NC-3 cap ignored",
        ISSUANCE,
        "        let issued = raw.min(remaining);",
        "        let issued = raw;",
        "T1: the max supply must never be exceeded",
    ),
    (
        "NC-4 genesis supply off by one",
        ISSUANCE,
        "            supply: params.genesis_supply,",
        "            supply: params.genesis_supply + 1,",
        "T1: the genesis supply must match the declared endowment",
    ),
    (
        "NC-5 staked ratio validation removed",
        ISSUANCE,
        "        if params.target_staked_ratio_bps > 10_000 {",
        "        if false {",
        "T1: the target staked ratio must stay within basis points",
    ),
    (
        "NC-6 horizon silently truncated",
        ISSUANCE,
        "        for _ in 0..years {",
        "        for _ in 0..years / 2 {",
        "T1: the reference model must agree with the simulation",
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
