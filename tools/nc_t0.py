#!/usr/bin/env python3
"""Mutation controls for T0 (ECONOMICS_PLAN T0).

Each control injects ONE bug (restored after) and requires the named T0
message to appear in the red run. NC-1 is the phase gate literally:
'emission from nowhere' in the simulation must go red.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ECON = os.path.join(ROOT, "crates", "aura-storage/src/econ.rs")

TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage",
            "--test", "t0_economics"]

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> emission from nowhere must go red",
        ECON,
        "        *to_bal += amount;",
        "        *to_bal += amount + 1;",
        "T0: transfers must conserve the total supply",
    ),
    (
        "NC-2 overspend guard removed",
        ECON,
        "            *from_bal = from_bal\n"
        "                .checked_sub(amount)\n"
        "                .ok_or(EconError::InsufficientFunds)?;",
        "            *from_bal = from_bal.wrapping_sub(amount);",
        "T0: a transfer must not spend more than the sender has",
    ),
    (
        "NC-3 schedule generator silently changed",
        ECON,
        "        self.state ^= self.state >> 7;",
        "        self.state ^= self.state >> 6;",
        "T0: the simulation must be deterministic",
    ),
    (
        "NC-4 mint leaves emission undeclared",
        ECON,
        "        self.declared += amount;",
        "        // emission left undeclared;",
        "T0: only declared emission may change the total supply",
    ),
    (
        "NC-5 desired properties lose scarcity",
        ECON,
        'pub const DESIRED_PROPERTIES: [&str; 3] = ["scarcity", "security", "accessibility"];',
        'pub const DESIRED_PROPERTIES: [&str; 3] = ["security", "accessibility", "security"];',
        "T0: the baseline properties must cover scarcity, security and accessibility",
    ),
    (
        "NC-6 schedule silently drops transfers",
        ECON,
        "            self.apply_step(step)?;",
        "            if step.amount % 2 == 0 { self.apply_step(step)?; }",
        "T0: the reference model must agree with the simulation",
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
