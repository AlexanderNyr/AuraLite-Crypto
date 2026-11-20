#!/usr/bin/env python3
"""Mutation controls for E0 (ECOSYSTEM_PLAN E0).

Each control injects ONE bug (restored after) and requires the named E0
gate to go RED with the named message. NC-1 and NC-6 are the phase gate
literally: the checklist must keep being exactly 10 user scenarios with
an honest 0/10 readiness count.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCENARIOS = os.path.join(ROOT, "tools", "fixtures", "e0_user_scenarios.json")
REPORT = os.path.join(ROOT, "docs", "ecosystem", "e0-baseline-report.md")

CHECK = [sys.executable, os.path.join(ROOT, "tools", "e0_baseline_check.py")]

US07 = ('    {"id": "US-07", "title": "Deploy and verify a contract", '
        '"need": "deploy a contract and verify its source so others can read '
        'what runs", "surface": "contracts", "status": "not-ready"},\n')
US05 = ('    {"id": "US-05", "title": "Inspect validators and consensus health", '
        '"need": "see the validator set, participation and finality health of '
        'the network", "surface": "explorer", "status": "not-ready"},')

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> checklist must keep exactly 10 scenarios",
        SCENARIOS,
        US07,
        "",
        "E0: checklist must have exactly 10 user scenarios",
    ),
    (
        "NC-2 scenario loses its user need",
        SCENARIOS,
        '"need": "look up an account balance and past transactions without trusting a third party"',
        '"need": ""',
        "E0: every scenario must state what a user must be able to do",
    ),
    (
        "NC-3 scenario status leaves the vocabulary",
        SCENARIOS,
        US05,
        US05.replace('"status": "not-ready"', '"status": "unknown"'),
        "E0: scenario status must be ready or not-ready",
    ),
    (
        "NC-4 baseline report loses the scenario anchor",
        REPORT,
        "<!-- E0: scenarios -->",
        "<!-- x -->",
        "E0: baseline report must cover the user scenarios",
    ),
    (
        "NC-5 duplicate scenario id",
        SCENARIOS,
        '{"id": "US-04", "title": "Follow a transaction to finality"',
        '{"id": "US-02", "title": "Follow a transaction to finality"',
        "E0: scenario ids must be unique",
    ),
    (
        "NC-6 readiness counter stops matching the checklist",
        REPORT,
        "Readiness: 0/10 ready.",
        "Readiness: 1/10 ready.",
        "E0: readiness count must match the checklist",
    ),
]

PASS_MARKERS = ("RESULT: E0 baseline check PASS",)


def run_control(name, path, anchor, replacement, expected):
    with open(path, encoding="utf-8") as f:
        original = f.read()
    if original.count(anchor) != 1:
        return False, "anchor count %d" % original.count(anchor)
    mutated = original.replace(anchor, replacement)
    try:
        with open(path, "w", encoding="utf-8") as f:
            f.write(mutated)
        proc = subprocess.run(CHECK, cwd=ROOT, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True,
                              errors="replace")
        output = proc.stdout or ""
    finally:
        with open(path, "w", encoding="utf-8") as f:
            f.write(original)
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
