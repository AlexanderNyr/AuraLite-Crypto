#!/usr/bin/env python3
"""Mutation controls for H8 (docs/h8-audit-plan.md §5).

Each control injects ONE bug (restored after) and requires the named H8
gate to go RED with the named message. NC-1 and NC-2 are the phase gate
literally: re-audit must find a re-introduced old bug (wallet side and
consensus side) and turn red.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
KEYSTORE = os.path.join(ROOT, "crates", "aura-wallet/src/keystore.rs")
REWARDS = os.path.join(ROOT, "crates", "aura-consensus/src/rewards.rs")
KNOWN_BUGS = os.path.join(ROOT, "tools", "fixtures", "h8_known_bugs.json")
FINDINGS = os.path.join(ROOT, "tools", "fixtures", "h8_findings.json")
SECURITY = os.path.join(ROOT, "SECURITY.md")
REPORT = os.path.join(ROOT, "docs", "audit", "h8-public-audit-report.md")

REAUDIT = [sys.executable, os.path.join(ROOT, "tools", "h8_reaudit.py")]

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> re-audit must find the old wallet bug",
        KEYSTORE,
        "                & 0o077\n",
        "                & 0o000\n",
        "H8: re-audit finds an old bug",
    ),
    (
        "NC-2 injected bug -> re-audit must find the old consensus bug",
        REWARDS,
        "        let accounted = sum_balances.saturating_add(self.total_burned);\n",
        "        let accounted = sum_balances.wrapping_add(self.total_burned);\n",
        "H8: re-audit finds an old bug",
    ),
    (
        "NC-3 reopened high finding must fail the zero-open gate",
        FINDINGS,
        '"id": "H8-F-003",\n      "area": "consensus",\n      "severity": "high",\n      "status": "fixed",\n',
        '"id": "H8-F-003",\n      "area": "consensus",\n      "severity": "high",\n      "status": "open",\n',
        "H8: 0 unfixed critical/high",
    ),
    (
        "NC-4 empty known-bug registry must fail",
        KNOWN_BUGS,
        '"bugs": [',
        '"bugs": [], "unused": [',
        "H8: known-bug registry must pin every old bug",
    ),
    (
        "NC-5 disclosure anchor removed from SECURITY.md",
        SECURITY,
        "<!-- H8: disclosure -->",
        "<!-- x -->",
        "H8: disclosure policy must stay complete",
    ),
    (
        "NC-6 bounty anchor removed from SECURITY.md",
        SECURITY,
        "<!-- H8: bounty -->",
        "<!-- x -->",
        "H8: bounty must pin the severity table",
    ),
    (
        "NC-7 wallet area marker removed from the audit report",
        REPORT,
        "<!-- H8 area: wallet -->",
        "<!-- x -->",
        "H8: audit report must cover consensus/VM/bridge/wallet",
    ),
]

PASS_MARKERS = ("RESULT: H8 re-audit PASS",)


def run_control(name, path, anchor, replacement, expected):
    with open(path, encoding="utf-8") as f:
        original = f.read()
    if original.count(anchor) != 1:
        return False, "anchor count %d" % original.count(anchor)
    mutated = original.replace(anchor, replacement)
    try:
        with open(path, "w", encoding="utf-8") as f:
            f.write(mutated)
        proc = subprocess.run(REAUDIT, cwd=ROOT, stdout=subprocess.PIPE,
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
