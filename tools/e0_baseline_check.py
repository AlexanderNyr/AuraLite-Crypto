#!/usr/bin/env python3
"""E0 baseline oracle (docs/ecosystem/e0-baseline-report.md, ECOSYSTEM_PLAN E0).

Deterministic checks over the 10 user scenarios:
  * the checklist has exactly 10 unique, well-formed scenarios
    (named gate: 'E0: checklist must have exactly 10 user scenarios');
  * every scenario states what a user must be able to do;
  * status vocabulary stays {ready, not-ready};
  * the baseline report covers every scenario and its readiness counter
    ('0/10 so far') matches the registry.

Exit 0 with 'RESULT: E0 baseline check PASS'; otherwise exit 1 with one
named message per failed check. Output is stable (file order).
"""

import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCENARIOS = os.path.join(ROOT, "tools", "fixtures", "e0_user_scenarios.json")
REPORT = os.path.join(ROOT, "docs", "ecosystem", "e0-baseline-report.md")

EXPECTED = 10
FIELDS = ("id", "title", "need", "surface", "status")
VOCAB = ("ready", "not-ready")

MSG_10 = "E0: checklist must have exactly 10 user scenarios"
MSG_NEED = "E0: every scenario must state what a user must be able to do"
MSG_STATUS = "E0: scenario status must be ready or not-ready"
MSG_REPORT = "E0: baseline report must cover the user scenarios"
MSG_IDS = "E0: scenario ids must be unique"
MSG_COUNT = "E0: readiness count must match the checklist"

READINESS = re.compile(r"Readiness: (\d+)/10 ready\.")


def main():
    errors = []
    with open(SCENARIOS, encoding="utf-8") as f:
        scenarios = json.load(f)["scenarios"]

    if len(scenarios) != EXPECTED:
        errors.append("%s (%d)" % (MSG_10, len(scenarios)))

    ids = [s.get("id", "?") for s in scenarios]
    if len(set(ids)) != len(ids):
        errors.append(MSG_IDS)

    ready = 0
    for scenario in scenarios:
        if not isinstance(scenario, dict) or any(k not in scenario for k in FIELDS):
            errors.append(MSG_NEED)
            continue
        if not str(scenario["need"]).strip():
            errors.append("%s (%s)" % (MSG_NEED, scenario["id"]))
        if scenario["status"] not in VOCAB:
            errors.append("%s (%s)" % (MSG_STATUS, scenario["id"]))
        elif scenario["status"] == "ready":
            ready += 1

    with open(REPORT, encoding="utf-8") as f:
        body = f.read()
    if "<!-- E0: scenarios -->" not in body:
        errors.append(MSG_REPORT)
    else:
        for scenario in scenarios:
            if scenario.get("id", "?") not in body:
                errors.append("%s (%s)" % (MSG_REPORT, scenario.get("id", "?")))
    match = READINESS.search(body)
    if not match or int(match.group(1)) != ready:
        errors.append("%s (report says %s, registry has %d ready)"
                      % (MSG_COUNT, match.group(1) if match else "?", ready))

    for message in errors:
        print("RED: %s" % message)
    if errors:
        print("RESULT: E0 baseline check FAIL (%d findings)" % len(errors))
        return 1
    print("RESULT: E0 baseline check PASS — %d user scenarios, %d/%d ready"
          % (len(scenarios), ready, EXPECTED))
    return 0


if __name__ == "__main__":
    sys.exit(main())
