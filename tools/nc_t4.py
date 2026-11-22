#!/usr/bin/env python3
"""Mutation controls for T4 (ECONOMICS_PLAN T4).

Each control injects ONE bug (restored after) and requires the named T4
message to appear in the red run. NC-2 is the phase control literally:
a withdrawal without unbonding must be red.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STAKING = os.path.join(ROOT, "crates", "aura-storage/src/staking.rs")

TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t4_staking"]

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> share accounting must go red",
        STAKING,
        "        self.total_shares += minted;",
        "        self.total_shares += minted + 1;",
        "T4: the delegator shares must sum to exactly one",
    ),
    (
        "NC-2 withdrawal without unbonding allowed",
        STAKING,
        "            return Err(StakeError::NoUnbonding);",
        "            return Ok(0);",
        "T4: a withdrawal without unbonding must be rejected",
    ),
    (
        "NC-3 unbonding window shortened",
        STAKING,
        "entry.matures_at <= now_block",
        "entry.matures_at <= now_block + 1",
        "T4: unbonding must complete after exactly 21 days",
    ),
    (
        "NC-4 commission taken twice (principal cut)",
        STAKING,
        "        self.bonded += reward - commission;",
        "        self.bonded += reward - commission - commission;",
        "T4: the commission must be taken from rewards only",
    ),
    (
        "NC-5 minimum stake not enforced",
        STAKING,
        "        if amount < T4_MIN_STAKE {",
        "        if false {",
        "T4: the minimum stake must be enforced",
    ),
    (
        "NC-6 slash not proportional on unbonding",
        STAKING,
        "entry.amount * Amount::from(rate_bps) / Amount::from(T4_BPS)",
        "entry.amount * Amount::from(rate_bps) / Amount::from(T4_BPS) / 2",
        "T4: a slash must reduce every stake proportionally",
    ),
    (
        "NC-7 pinned parameters drift",
        STAKING,
        "pub const T4_COMMISSION_BPS: u64 = 1_000;",
        "pub const T4_COMMISSION_BPS: u64 = 1_001;",
        "T4: the reference simulation must agree with the pinned digest",
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
