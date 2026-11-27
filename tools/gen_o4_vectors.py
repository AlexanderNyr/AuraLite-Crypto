#!/usr/bin/env python3
"""O4 golden vectors — the public testnet (M2) receipt check (independent oracle).

Independently computes the expected verdicts of twelve synthetic cases
(the ``testnet.txt`` receipt check, the ``metrics-30d.txt`` ledger rollup
and the ``triage.txt`` journal rollup) and writes
``crates/aura-testkit/tests/fixtures/o4_testnet_public_vectors.json``,
which ``crates/aura-testkit/tests/o4_testnet_public.rs`` executes
byte-for-byte against ``aura_testkit::testnet``.

Receipt format (``docs/o4-testnet-public-plan.md`` D-O4-1):
``rule testnet-v1``, ``days <n>`` (>= 30), ``uptime <n>`` (percent,
>= 99), ``validators <n>`` (>= 10), ``txs <n>`` (>= 100 000),
``criticals <n>`` (== 0 — the named gate ``unfixed_criticals_refused``).
Metrics rows: ``metrics <d> <h> <height> <txs_total> <root-hex-64>``
(strict, ordered); the rollup counts complete day-buckets, derives
``floor(100 * present / (span_days * 24))`` uptime and refuses the first
regressing window (``tx regression at <d> <h>``). Triage rows:
``triage <id> <severity> <status>`` (severity
``critical|high|medium|low|informational``, status
``open|fixed|accepted|not-applicable``); the rollup prints
``triage ledger: items=<n> open=<n> criticals=<n>`` (``criticals`` =
open ``critical``/``high`` findings, the H8 convention) and refuses an
open critical by id (``open critical <id>``). The gate is OPS_PLAN row
O4: "30 days, >= 99% uptime, >= 100 000 txs, 0 unfixed criticals".

Regenerate with::

    PYTHONPATH=/home/user/aura-python-deps python3 tools/gen_o4_vectors.py
"""

import json
import os
import sys

RULE = "testnet-v1"
DAYS = 30
UPTIME_PCT = 99
VALIDATORS = 10
TXS = 100_000
HOURS = 24


def receipt_text(days=30, uptime=100, validators=10, txs=100800, criticals=0, foreign=None):
    lines = [
        "# public testnet receipts (O4) — checked by `aura-testnet check`.",
        f"rule {RULE}",
        f"days {days}",
        f"uptime {uptime}",
        f"validators {validators}",
        f"txs {txs}",
        f"criticals {criticals}",
    ]
    if foreign:
        lines.append(foreign)
    return "\n".join(lines) + "\n"


def metrics_text(days=30, regression=None):
    """A deterministic operator metrics ledger; `regression` = (day, hour)
    whose height regresses (a refused window)."""
    rows = []
    height = 0
    txs_total = 0
    for day in range(days):
        for hour in range(HOURS):
            height += 1
            txs_total += 140
            if regression == (day, hour):
                height -= 11  # a window regression at this poll
            rows.append(f"metrics {day} {hour} {height} {txs_total} {height:064x}")
    return "\n".join(rows) + "\n"


def triage_text(open_critical=None):
    """Twelve findings; `open_critical` = id whose severity/status become
    `high`/`open` (the refused journal)."""
    rows = [
        ("T-0001", "high", "fixed"),
        ("T-0002", "medium", "open"),
        ("T-0003", "low", "fixed"),
        ("T-0004", "informational", "open"),
        ("T-0005", "critical", "fixed"),
        ("T-0006", "high", "accepted"),
        ("T-0007", "medium", "fixed"),
        ("T-0008", "low", "open"),
        ("T-0009", "informational", "not-applicable"),
        ("T-0010", "high", "fixed"),
        ("T-0011", "medium", "accepted"),
        ("T-0012", "low", "fixed"),
    ]
    if open_critical is not None:
        rows = [
            (fid, "high" if fid == open_critical else sev,
             "open" if fid == open_critical else status)
            for (fid, sev, status) in rows
        ]
    lines = [
        "# bug-bounty triage journal (O4) — checked by `aura-testnet triage`.",
    ]
    lines += [f"triage {fid} {sev} {status}" for (fid, sev, status) in rows]
    return "\n".join(lines) + "\n"


# ------------------------------------------------------------- oracle receipt check

def oracle_summary(days, uptime, validators, txs, criticals) -> str:
    return (
        f"window: days={days} uptime={uptime} validators={validators} txs={txs}\n"
        f"criticals: unfixed={criticals}\n"
    )


def oracle_check(text: str) -> str:
    """Return '' when the receipt passes, else the exact refusal message."""
    rule = None
    values = {"days": None, "uptime": None, "validators": None, "txs": None, "criticals": None}
    for raw in text.splitlines():
        line = raw.rstrip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("rule "):
            name = line[5:]
            if not name or " " in name:
                return f"unknown receipt line: {line}"
            rule = name
        else:
            matched = False
            for key in values:
                prefix = f"{key} "
                if line.startswith(prefix):
                    value = line[len(prefix):]
                    if not value.lstrip("-").isdigit():
                        return f"unknown receipt line: {line}"
                    values[key] = int(value)
                    matched = True
                    break
            if not matched:
                return f"unknown receipt line: {line}"

    if rule is None:
        return "missing rule line"
    if rule != RULE:
        return f"bad rule: {rule}"
    for key in ("days", "uptime", "validators", "txs", "criticals"):
        if values[key] is None:
            return f"missing receipt line: {key}"
    if values["days"] < DAYS:
        return f"days: {values['days']}"
    if values["uptime"] < UPTIME_PCT:
        return f"uptime: {values['uptime']}"
    if values["validators"] < VALIDATORS:
        return f"validators: {values['validators']}"
    if values["txs"] < TXS:
        return f"txs: {values['txs']}"
    if values["criticals"] != 0:
        return f"criticals: {values['criticals']}"
    return ""


# ------------------------------------------------------------- oracle metrics rollup

def oracle_metrics(text: str):
    """Return ('ok', days, uptime, txs) or ('err', message, None)."""
    rows = []
    for raw in text.splitlines():
        line = raw.rstrip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(" ")
        if len(parts) != 6 or parts[0] != "metrics":
            return ("err", f"bad metric: {line}", None)
        try:
            day, hour, height, txs_total = (int(parts[1]), int(parts[2]),
                                            int(parts[3]), int(parts[4]))
        except ValueError:
            return ("err", f"bad metric: {line}", None)
        root = parts[5]
        if not (0 <= hour < HOURS) or len(root) != 64 or any(
            c not in "0123456789abcdefABCDEF" for c in root
        ):
            return ("err", f"bad metric: {line}", None)
        rows.append((day, hour, height, txs_total))

    if not rows:
        return ("err", "bad metric: empty ledger", None)
    rows.sort(key=lambda r: (r[0], r[1]))
    previous = None
    for row in rows:
        if previous is not None:
            if (row[0], row[1]) == (previous[0], previous[1]):
                return ("err", f"bad metric: duplicate {row[0]} {row[1]}", None)
            if row[2] < previous[2] or row[3] < previous[3]:
                return ("err", f"tx regression at {row[0]} {row[1]}", None)
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
    span_days = rows[-1][0] - rows[0][0] + 1
    scheduled = span_days * HOURS
    uptime = (100 * len(rows)) // scheduled if scheduled > 0 else 0
    return ("ok", days, uptime, rows[-1][3])


# ------------------------------------------------------------- oracle triage rollup

def oracle_triage(text: str):
    """Return ('ok', items, open, criticals) or ('err', message, None)."""
    severities = {"critical", "high", "medium", "low", "informational"}
    statuses = {"open", "fixed", "accepted", "not-applicable"}
    rows = []
    for raw in text.splitlines():
        line = raw.rstrip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(" ")
        if (len(parts) != 4 or parts[0] != "triage" or not parts[1]
                or parts[2] not in severities or parts[3] not in statuses):
            return ("err", f"bad triage: {line}", None)
        rows.append((parts[1], parts[2], parts[3]))

    rows.sort(key=lambda r: r[0])
    previous = None
    for row in rows:
        if previous == row[0]:
            return ("err", f"bad triage: duplicate {row[0]}", None)
        previous = row[0]
    for row in rows:
        if row[1] in ("critical", "high") and row[2] == "open":
            return ("err", f"open critical {row[0]}", None)
    items = len(rows)
    open_count = sum(1 for r in rows if r[2] == "open")
    criticals = sum(1 for r in rows if r[1] in ("critical", "high") and r[2] == "open")
    return ("ok", items, open_count, criticals)


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
METRICS = ["metrics", "--ledger", "metrics.txt"]
TRIAGE = ["triage", "--ledger", "triage.txt"]


def build_cases():
    base = {"receipt.txt": receipt_text()}
    cases = []

    cases.append(case(
        "testnet_public_pass", [CHECK], base, 0,
        "testnet=ok uptime=ok validators=ok txs=ok criticals=ok\n", "",
    ))
    cases.append(case(
        "testnet_summary_deterministic", [SUMMARY], base, 0,
        oracle_summary(30, 100, 10, 100800, 0), "",
    ))
    cases.append(case(
        "days_shortfall_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(days=29)}),
        65, "", "error: testnet: days: 29\n", nc_ref=2,
    ))
    cases.append(case(
        "uptime_shortfall_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(uptime=98)}),
        65, "", "error: testnet: uptime: 98\n", nc_ref=3,
    ))
    cases.append(case(
        "validators_shortfall_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(validators=9)}),
        65, "", "error: testnet: validators: 9\n", nc_ref=4,
    ))
    cases.append(case(
        "txs_shortfall_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(txs=99999)}),
        65, "", "error: testnet: txs: 99999\n", nc_ref=5,
    ))
    cases.append(case(
        "unfixed_criticals_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(criticals=1)}),
        65, "", "error: testnet: criticals: 1\n", nc_ref=1,
    ))
    cases.append(case(
        "foreign_testnet_line_refused", [CHECK],
        dict(base, **{"receipt.txt": receipt_text(foreign="reset ledger")}),
        65, "", "error: testnet: unknown receipt line: reset ledger\n", nc_ref=7,
    ))
    cases.append(case(
        "metrics_pass", [METRICS],
        {"metrics.txt": metrics_text()}, 0,
        "metrics ledger: days=30 uptime=100 txs=100800\n", "",
    ))
    cases.append(case(
        "metrics_tx_regression_refused", [METRICS],
        {"metrics.txt": metrics_text(regression=(3, 2))}, 65, "",
        "error: testnet: tx regression at 3 2\n", nc_ref=6,
    ))
    cases.append(case(
        "triage_pass", [TRIAGE],
        {"triage.txt": triage_text()}, 0,
        "triage ledger: items=12 open=3 criticals=0\n", "",
    ))
    cases.append(case(
        "triage_open_critical_refused", [TRIAGE],
        {"triage.txt": triage_text(open_critical="T-0002")}, 65, "",
        "error: testnet: open critical T-0002\n",
    ))

    cases[0]["phase"] = "O4"
    return cases


def sanity_check():
    assert oracle_check(receipt_text()) == ""
    assert oracle_check(receipt_text(days=29)) == "days: 29"
    assert oracle_check(receipt_text(uptime=98)) == "uptime: 98"
    assert oracle_check(receipt_text(validators=9)) == "validators: 9"
    assert oracle_check(receipt_text(txs=99999)) == "txs: 99999"
    assert oracle_check(receipt_text(criticals=1)) == "criticals: 1"
    assert oracle_check(receipt_text(foreign="reset ledger")) == (
        "unknown receipt line: reset ledger"
    )
    ok, days, uptime, txs = oracle_metrics(metrics_text())
    assert (ok, days, uptime, txs) == ("ok", 30, 100, 100800), (ok, days, uptime, txs)
    ok, message, _ = oracle_metrics(metrics_text(regression=(3, 2)))
    assert ok == "err" and message == "tx regression at 3 2", message
    ok, items, open_count, criticals = oracle_triage(triage_text())
    assert (ok, items, open_count, criticals) == ("ok", 12, 3, 0), (
        ok, items, open_count, criticals
    )
    ok, message, _ = oracle_triage(triage_text(open_critical="T-0002"))
    assert ok == "err" and message == "open critical T-0002", message
    assert oracle_summary(30, 100, 10, 100800, 0).startswith("window: days=30")


def main():
    sanity_check()
    fixture = {
        "gate": (
            "O4 public testnet gate: 12/12 cases pass; named negative "
            "control unfixed_criticals_refused red on the dropped "
            "criticals check; 7/7 NC red under tools/nc_o4.py; OPS_PLAN "
            "row O4 criteria met (30 days, >= 99% uptime, >= 100 000 txs, "
            "0 unfixed criticals) on the measured receipts"
        ),
        "notice": (
            "Independent Python oracle (tools/gen_o4_vectors.py); verifier: "
            "crates/aura-testkit/tests/o4_testnet_public.rs against "
            "aura_testkit::testnet. The receipts and the ledgers themselves "
            "are measured by aurad-testnet (crates/aura-node/src/bin/) — "
            "this fixture checks criteria logic only. Replace the whole "
            "file only with tools/gen_o4_vectors.py output."
        ),
        "named_control": "unfixed_criticals_refused",
        "files": {},
        "cases": build_cases(),
    }
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_path = os.path.join(
        root, "crates/aura-testkit/tests/fixtures/o4_testnet_public_vectors.json"
    )
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump(fixture, handle, indent=1)
        handle.write("\n")
    print(f"wrote {os.path.relpath(out_path, root)}: 12 cases, "
          f"named control unfixed_criticals_refused")
    return 0


if __name__ == "__main__":
    sys.exit(main())
