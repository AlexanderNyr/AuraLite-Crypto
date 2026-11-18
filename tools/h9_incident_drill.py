#!/usr/bin/env python3
"""H9 incident drill runner (docs/h9-incident-plan.md, SECURITY_PLAN H9).

Deterministic oracle over three artifacts:
  * docs/incident/PLAYBOOK.md — action anchors (<!-- H9: ... -->);
  * tools/fixtures/h9_drills.json — drill timelines in simulated minutes;
  * tools/fixtures/h0_threat_model_vectors.json — scenario ids.

Checks per drill: every step comes from the playbook, the incident state
machine order holds, every drill ends in a verified recovery, H0 refs
exist, post-mortem lands within SLO. The phase gate literally is drill
D1 'Byzantine validator + bridge bug': network halted/recovered within
SLO ('H9: drill Byzantine validator + bridge bug must halt/recover
within SLO' must fire when either half breaks).

Exit 0 with 'RESULT: H9 incident drills PASS'; otherwise exit 1 with one
named message per failed check. Output is stable (file order).
"""

import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PLAYBOOK = os.path.join(ROOT, "docs", "incident", "PLAYBOOK.md")
DRILLS = os.path.join(ROOT, "tools", "fixtures", "h9_drills.json")
H0 = os.path.join(ROOT, "tools", "fixtures", "h0_threat_model_vectors.json")

NAMED_DRILL = "Byzantine validator + bridge bug"

MSG_SLO = "H9: drill Byzantine validator + bridge bug must halt/recover within SLO"
MSG_STEPS = "H9: every drill step must come from the playbook"
MSG_PLAYBOOK_ACTIONS = "H9: playbook must cover halt/pause/upgrade/rollback"
MSG_PLAYBOOK_COMMS = "H9: playbook must cover communication and post-mortem"
MSG_H0 = "H9: drills must cover the H0 scenarios they claim"
MSG_RECOVER = "H9: every drill must end in a verified recovery"
MSG_PM = "H9: post-mortem must be produced within SLO"
MSG_ORDER = "H9: drill steps must follow the incident state machine"

ACTION_ANCHOR = re.compile(r"<!-- H9: ([a-z-]+) -->")

# Canonical incident state machine: these actions must appear in order.
ORDER = ["detect", "halt", "communicate", "recover", "post-mortem"]


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def playbook_actions(errors):
    if not os.path.exists(PLAYBOOK):
        errors.append("%s (PLAYBOOK.md missing)" % MSG_PLAYBOOK_ACTIONS)
        return set()
    with open(PLAYBOOK, encoding="utf-8") as f:
        body = f.read()
    actions = set(ACTION_ANCHOR.findall(body))
    for required in ("halt", "pause", "upgrade", "rollback"):
        if required not in actions:
            errors.append(MSG_PLAYBOOK_ACTIONS)
            break
    for required in ("communicate", "post-mortem"):
        if required not in actions:
            errors.append(MSG_PLAYBOOK_COMMS)
            break
    return actions


def check_order(steps, errors):
    ats = [s["at"] for s in steps]
    if ats != sorted(ats):
        errors.append(MSG_ORDER)
        return
    seen = [s["action"] for s in steps]
    idx = -1
    for action in ORDER:
        if action not in seen:
            continue
        pos = seen.index(action)
        if pos < idx:
            errors.append(MSG_ORDER)
            return
        idx = pos


def main():
    errors = []
    actions = playbook_actions(errors)
    drills_data = load(DRILLS)
    h0_ids = {s["id"] for s in load(H0)["scenarios"]}
    slo = drills_data["slo"]
    named_seen = False

    for drill in drills_data["drills"]:
        steps = drill.get("steps", [])
        for step in steps:
            if step["action"] not in actions:
                errors.append("%s (%s: %s)" % (MSG_STEPS, drill["id"], step["action"]))
        if not steps or steps[-1]["action"] != "post-mortem" or \
                not any(s["action"] == "recover" for s in steps):
            errors.append("%s (%s)" % (MSG_RECOVER, drill["id"]))
        for ref in drill.get("h0", []):
            if ref not in h0_ids:
                errors.append("%s (%s: %s)" % (MSG_H0, drill["id"], ref))
        pm = [s for s in steps if s["action"] == "post-mortem"]
        if not pm or pm[-1]["at"] > slo["post_mortem_minutes"]:
            errors.append("%s (%s)" % (MSG_PM, drill["id"]))
        check_order(steps, errors)

        if drill.get("title") == NAMED_DRILL:
            named_seen = True
            halt = [s for s in steps if s["action"] == "halt"]
            recover = [s for s in steps if s["action"] == "recover"]
            if not halt or halt[0]["at"] > slo["halt_minutes"]:
                errors.append(MSG_SLO)
            if not recover or recover[-1]["at"] > slo["recover_minutes"]:
                errors.append(MSG_SLO)

    if not named_seen:
        errors.append(MSG_SLO)

    for message in errors:
        print("RED: %s" % message)
    if errors:
        print("RESULT: H9 incident drills FAIL (%d findings)" % len(errors))
        return 1
    print("RESULT: H9 incident drills PASS — %d drills, "
          "SLO halt<=%dm recover<=%dm post-mortem<=%dm"
          % (len(drills_data["drills"]), slo["halt_minutes"],
             slo["recover_minutes"], slo["post_mortem_minutes"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
