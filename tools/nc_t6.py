#!/usr/bin/env python3
"""Mutation controls for T6 (ECONOMICS_PLAN T6).

Each control injects ONE bug (restored after) and requires the named T6
message to appear in the red run. NC-1/NC-2 are the phase gate literally:
valid proposals pass, the flash-stake attack is rejected.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GOV = os.path.join(ROOT, "crates", "aura-storage/src/governance.rs")

TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t6_governance"]

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 valid proposals flipped to rejected",
        GOV,
        "        let passed = voted >= quorum && proposal.yes > proposal.no;",
        "        let passed = voted >= quorum && proposal.yes <= proposal.no;",
        "T6: a valid proposal must pass",
    ),
    (
        "NC-2 flash-stake counted (snapshot replaced by live stake)",
        GOV,
        "        let power = proposal.snapshot[voter as usize];",
        "        let power = self.stakes[voter as usize];",
        "T6: a flash-stake attack must be rejected",
    ),
    (
        "NC-3 proposal threshold not enforced",
        GOV,
        "        if self.stakes[proposer as usize] < T6_PROPOSAL_THRESHOLD {",
        "        if false {",
        "T6: a proposal below the threshold must be rejected",
    ),
    (
        "NC-4 voting window extended by one block",
        GOV,
        "        if height >= proposal.created_at + T6_VOTING_PERIOD_BLOCKS {",
        "        if height > proposal.created_at + T6_VOTING_PERIOD_BLOCKS {",
        "T6: a vote outside the voting period must be rejected",
    ),
    (
        "NC-5 quorum disabled",
        GOV,
        "pub const T6_QUORUM_BPS: u64 = 2_000;",
        "pub const T6_QUORUM_BPS: u64 = 0;",
        "T6: the quorum must be met for a proposal to pass",
    ),
    (
        "NC-6 timelock removed",
        GOV,
        "pub const T6_TIMELOCK_BLOCKS: u64 = 34_560;",
        "pub const T6_TIMELOCK_BLOCKS: u64 = 0;",
        "T6: execution must wait for the timelock",
    ),
    (
        "NC-7 veto sunset removed",
        GOV,
        "        if height >= T6_VETO_SUNSET_BLOCK {",
        "        if false {",
        "T6: the veto power must sunset",
    ),
    (
        "NC-8 pinned parameters drift",
        GOV,
        "pub const T6_VOTING_PERIOD_BLOCKS: u64 = 120_960;",
        "pub const T6_VOTING_PERIOD_BLOCKS: u64 = 120_961;",
        "T6: the reference simulation must agree with the pinned digest",
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
