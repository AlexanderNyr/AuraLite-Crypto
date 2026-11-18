#!/usr/bin/env python3
"""H8 re-audit gate (docs/h8-audit-plan.md, SECURITY_PLAN H8).

Deterministic oracle over three registries:
  1. tools/fixtures/h8_known_bugs.json — every old bug stays fixed
     (named gate: 'H8: re-audit finds an old bug' must fire when one is
     re-introduced; that is the phase gate literally).
  2. tools/fixtures/h8_findings.json — 0 unfixed critical/high, every
     fixed finding cites its fix_ref, all four audited areas covered.
  3. SECURITY.md disclosure/bounty sections and the public audit report
     area markers.

Exit 0 with 'RESULT: H8 re-audit PASS' when every check holds; otherwise
exit 1 printing one named message per failed check. Output is stable
(registries are iterated in file order).
"""

import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
KNOWN_BUGS = os.path.join(ROOT, "tools", "fixtures", "h8_known_bugs.json")
FINDINGS = os.path.join(ROOT, "tools", "fixtures", "h8_findings.json")
SECURITY = os.path.join(ROOT, "SECURITY.md")
REPORT = os.path.join(ROOT, "docs", "audit", "h8-public-audit-report.md")

AREAS = ["consensus", "vm", "bridge", "wallet"]

MSG_OLD_BUG = "H8: re-audit finds an old bug"
MSG_UNFIXED = "H8: 0 unfixed critical/high"
MSG_REGISTRY = "H8: known-bug registry must pin every old bug"
MSG_DISCLOSURE = "H8: disclosure policy must stay complete"
MSG_BOUNTY = "H8: bounty must pin the severity table"
MSG_COVERAGE = "H8: audit report must cover consensus/VM/bridge/wallet"
MSG_FIX_REF = "H8: every fix must cite its patch"

BUG_FIELDS = ("id", "area", "symptom", "fixed_in", "file",
              "must_contain", "must_not_contain")
FINDING_FIELDS = ("id", "area", "severity", "status", "fix_ref", "title")


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def check_known_bugs(errors):
    try:
        data = load(KNOWN_BUGS)
    except (OSError, ValueError) as exc:
        errors.append("%s (%s)" % (MSG_REGISTRY, exc))
        return 0
    bugs = data.get("bugs")
    if not isinstance(bugs, list) or not bugs:
        errors.append(MSG_REGISTRY)
        return 0
    pinned = 0
    for bug in bugs:
        if not isinstance(bug, dict) or any(k not in bug for k in BUG_FIELDS):
            errors.append("%s (%s)" % (MSG_REGISTRY,
                                       bug.get("id", "?") if isinstance(bug, dict) else "?"))
            continue
        path = os.path.join(ROOT, bug["file"])
        if not os.path.exists(path):
            errors.append("%s (%s: file missing)" % (MSG_REGISTRY, bug["id"]))
            continue
        with open(path, encoding="utf-8") as f:
            body = f.read()
        for needle in bug["must_contain"]:
            if needle not in body:
                errors.append("%s (%s: %s)" % (MSG_OLD_BUG, bug["id"], bug["symptom"]))
                break
        else:
            for needle in bug["must_not_contain"]:
                if needle in body:
                    errors.append("%s (%s: %s)" % (MSG_OLD_BUG, bug["id"], bug["symptom"]))
                    break
            else:
                pinned += 1
    return pinned


def check_findings(errors):
    try:
        data = load(FINDINGS)
    except (OSError, ValueError) as exc:
        errors.append("%s (%s)" % (MSG_UNFIXED, exc))
        return
    findings = data.get("findings")
    if not isinstance(findings, list) or not findings:
        errors.append(MSG_UNFIXED)
        return
    seen_areas = set()
    for finding in findings:
        if not isinstance(finding, dict) or any(k not in finding for k in FINDING_FIELDS):
            errors.append("%s (%s)" % (MSG_FIX_REF,
                                       finding.get("id", "?") if isinstance(finding, dict) else "?"))
            continue
        seen_areas.add(finding["area"].strip().lower())
        severity = finding["severity"].strip().lower()
        status = finding["status"].strip().lower()
        if severity in ("critical", "high") and status != "fixed":
            errors.append("%s (%s open %s)" % (MSG_UNFIXED, finding["id"], severity))
        if status == "fixed" and not finding["fix_ref"].strip():
            errors.append("%s (%s)" % (MSG_FIX_REF, finding["id"]))
    for area in AREAS:
        if area not in seen_areas:
            errors.append("%s (findings lack %s)" % (MSG_COVERAGE, area))


def check_documents(errors):
    for path, anchors, message in (
        (SECURITY, ["<!-- H8: disclosure -->"], MSG_DISCLOSURE),
        (SECURITY, ["<!-- H8: bounty -->"], MSG_BOUNTY),
        (REPORT, ["<!-- H8 area: %s -->" % area for area in AREAS], MSG_COVERAGE),
    ):
        if not os.path.exists(path):
            errors.append("%s (%s missing)" % (message, os.path.relpath(path, ROOT)))
            continue
        with open(path, encoding="utf-8") as f:
            body = f.read()
        for anchor in anchors:
            if anchor not in body:
                errors.append(message)


def main():
    errors = []
    pinned = check_known_bugs(errors)
    check_findings(errors)
    check_documents(errors)
    for message in errors:
        print("RED: %s" % message)
    if errors:
        print("RESULT: H8 re-audit FAIL (%d findings)" % len(errors))
        return 1
    print("RESULT: H8 re-audit PASS — 0 unfixed critical/high, "
          "%d known bugs pinned, %d findings tracked" % (pinned, len(load(FINDINGS)["findings"])))
    return 0


if __name__ == "__main__":
    sys.exit(main())
