#!/usr/bin/env python3
"""Deterministic oracle for E2 (ECOSYSTEM_PLAN E2).

Gate: "10 user scenarios green; reproducible contract build = same
code_hash". Checks the scenario checklist (exactly 10, all ready),
traceability (every gate message is asserted in the E2 test), and
recomputes the contract fixture hashes with an independent Python
builder (metamorphic: formatting-equivalent sources share one hash).
"""

import hashlib
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCENARIOS = os.path.join(ROOT, "tools", "fixtures", "e2_explorer_scenarios.json")
CONTRACTS = os.path.join(ROOT, "tools", "fixtures", "e2_contracts.json")
REPORT = os.path.join(ROOT, "docs", "ecosystem", "e2-explorer-report.md")
TEST = os.path.join(ROOT, "crates", "aura-storage", "tests", "e2_explorer.rs")

OPS = {"push": 1, "add": 2, "sub": 3, "mul": 4, "dup": 5, "drop": 6, "return": 7}


def build_contract(source):
    """Independent ALVM-1 builder (mirrors crates/aura-storage/src/explorer.rs)."""
    out = bytearray()
    for raw in source.split("\n"):
        line = " ".join(raw.split()).lower()
        if not line or line.startswith("#"):
            continue
        parts = line.split(" ")
        op = parts[0]
        out.append(OPS[op])
        if op == "push":
            out += int(parts[1]).to_bytes(8, "little")
    return bytes(out)


def code_hash(source):
    return hashlib.sha256(build_contract(source)).hexdigest()


def main():
    with open(SCENARIOS, encoding="utf-8") as f:
        scenarios = [json.loads(line) for line in f if line.strip()]
    with open(CONTRACTS, encoding="utf-8") as f:
        contracts = [json.loads(line) for line in f if line.strip()]
    with open(REPORT, encoding="utf-8") as f:
        report = f.read()
    with open(TEST, encoding="utf-8") as f:
        test_src = f.read()

    problems = []

    # --- checklist: exactly 10 unique ready scenarios -------------
    if len(scenarios) != 10:
        problems.append("checklist must have exactly 10 user scenarios")
    ids = [s.get("id", "") for s in scenarios]
    if len(set(ids)) != len(ids):
        problems.append("scenario ids must be unique")
    for s in scenarios:
        if not s.get("user_must", "").strip():
            problems.append("every scenario must state what a user must be able to do")
            break
    for s in scenarios:
        if s.get("status") != "ready":
            problems.append("scenario status must be ready")
            break

    # --- report covers the scenarios -------------------------------
    if "<!-- E2: scenarios -->" not in report:
        problems.append("baseline report must cover the user scenarios")
    for s in scenarios:
        if s["id"] not in report or s.get("gate", "") not in report:
            problems.append("baseline report must cover the user scenarios")
            break
    ready = sum(1 for s in scenarios if s.get("status") == "ready")
    if "Readiness: %d/10 ready." % ready not in report:
        problems.append("readiness count must match the checklist")
    if "reproducible contract build" not in report or "same code_hash" not in report:
        problems.append("report must state the reproducible contract build gate")

    # --- traceability: every gate message asserted in the test -----
    for s in scenarios:
        if s.get("gate", "") not in test_src:
            problems.append("every gate message must be asserted in the E2 test")
            break

    # --- independent contract build -------------------------------
    for c in contracts:
        if code_hash(c["source"]) != c["code_hash"]:
            problems.append("contract fixture code_hash must match the independent builder")
            break
    by_name = {c["name"]: c for c in contracts}
    if by_name.get("adder", {}).get("code_hash") != by_name.get("adder-alias", {}).get("code_hash"):
        problems.append("formatting-equivalent sources must share one code_hash")
    if by_name.get("adder", {}).get("code_hash") == by_name.get("counter", {}).get("code_hash"):
        problems.append("different sources must produce different code hashes")
    for c in contracts:
        if c["code_hash"] not in test_src:
            problems.append("fixture code hashes must be pinned in the E2 test")
            break

    for p in problems:
        print("FAIL:", p)
    ready = sum(1 for s in scenarios if s.get("status") == "ready")
    if problems:
        print("RESULT: E2 explorer check FAIL")
        return 1
    print("RESULT: E2 explorer check PASS — %d scenarios, %d/10 ready"
          % (len(scenarios), ready))
    return 0


if __name__ == "__main__":
    sys.exit(main())
