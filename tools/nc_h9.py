#!/usr/bin/env python3
"""Mutation controls for H9 (docs/h9-incident-plan.md §5).

Each control injects ONE bug (restored after) and requires the named H9
gate to go RED with the named message. NC-1 and NC-2 are the phase gate
literally: drill "Byzantine validator + bridge bug" must fail when the
network stops being halted/recovered within SLO.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PLAYBOOK = os.path.join(ROOT, "docs", "incident", "PLAYBOOK.md")
DRILLS = os.path.join(ROOT, "tools", "fixtures", "h9_drills.json")

DRILL_RUNNER = [sys.executable, os.path.join(ROOT, "tools", "h9_incident_drill.py")]

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> named drill must fail when halt disappears",
        DRILLS,
        '        {"action": "halt", "at": 5},\n',
        "",
        "H9: drill Byzantine validator + bridge bug must halt/recover within SLO",
    ),
    (
        "NC-2 injected bug -> named drill must fail when recover disappears",
        DRILLS,
        '        {"action": "recover", "at": 180},\n',
        "",
        "H9: drill Byzantine validator + bridge bug must halt/recover within SLO",
    ),
    (
        "NC-3 rollback anchor removed from the playbook",
        PLAYBOOK,
        "<!-- H9: rollback -->",
        "<!-- x -->",
        "H9: playbook must cover halt/pause/upgrade/rollback",
    ),
    (
        "NC-4 communication anchor removed from the playbook",
        PLAYBOOK,
        "<!-- H9: communicate -->",
        "<!-- x -->",
        "H9: playbook must cover communication and post-mortem",
    ),
    (
        "NC-5 drill claims a nonexistent H0 scenario",
        DRILLS,
        '"h0": ["H0-S02", "H0-S13"],',
        '"h0": ["H0-S99", "H0-S13"],',
        "H9: drills must cover the H0 scenarios they claim",
    ),
    (
        "NC-6 recover step removed from D2",
        DRILLS,
        '        {"action": "recover", "at": 150},\n',
        "",
        "H9: every drill must end in a verified recovery",
    ),
    (
        "NC-7 D1 post-mortem delayed past the SLO",
        DRILLS,
        '{"action": "post-mortem", "at": 240}',
        '{"action": "post-mortem", "at": 9999}',
        "H9: post-mortem must be produced within SLO",
    ),
]

PASS_MARKERS = ("RESULT: H9 incident drills PASS",)


def run_control(name, path, anchor, replacement, expected):
    with open(path, encoding="utf-8") as f:
        original = f.read()
    if original.count(anchor) != 1:
        return False, "anchor count %d" % original.count(anchor)
    mutated = original.replace(anchor, replacement)
    try:
        with open(path, "w", encoding="utf-8") as f:
            f.write(mutated)
        proc = subprocess.run(DRILL_RUNNER, cwd=ROOT, stdout=subprocess.PIPE,
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
