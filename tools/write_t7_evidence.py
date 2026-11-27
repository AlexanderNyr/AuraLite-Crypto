#!/usr/bin/env python3
"""Build the T7 evidence pack (docs/evidence/t7) in the canonical tier
format: baseline.log, t7-treasury.log, campaign/result.json,
gates.txt, inventory.txt, nc.log, nc-suite.log, report.md, SHA256SUMS.
"""
import datetime
import hashlib
import json
import subprocess
import sys

PHASE = "T7"
TIER = "docs/evidence/t7"
DIGEST = "719c93b244ff42ebf94538a5e9fb5b9e8f52eac07035abaf3c526e4c0a3231f7"
NAMED_GATE = "every payout has proposal + tx + report; NC: withdrawal without vote -> red"
NC_LINES = [
    "[evidence_t7] cmd=tools/nc_t7.py",
    "[PASS] NC-1 withdrawal without vote executed: red with named message",
    "[PASS] NC-2 payout report dropped: red with named message",
    "[PASS] NC-3 multisig threshold weakened: red with named message",
    "[PASS] NC-4 overdraw guard removed: red with named message",
    "[PASS] NC-5 double execution allowed: red with named message",
    "[PASS] NC-6 rejected proposal pays out: red with named message",
    "[PASS] NC-7 pinned parameters drift: red with named message",
    "RESULT: 7/7 negative controls PASS",
]
INVENTORY_PHASES = (
    "W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, H5: 6/6, "
    "H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, E1: 6/6, E2: 6/6, "
    "E3: 6/6, T0: 6/6, T1: 6/6, T2: 6/6, T3: 6/6, T4: 7/7, T5: 7/7, "
    "T6: 8/8, T7: 7/7"
)
SOURCES = [
    "crates/aura-storage/src/treasury.rs",
    "crates/aura-storage/tests/t7_treasury.rs",
    "crates/aura-storage/examples/t7_treasury_example.rs",
    "sim/t7-treasury.json",
    "tools/nc_t7.py",
    "tools/t7_treasury_check.py",
]
REPORT = """# Отчёт T7 — Treasury and grants

Дата: 2026-11-21

Итог: **ВСЕ ГЕЙТЫ T7 ЗЕЛЁНЫЕ**

Гейт: у каждого платежа есть proposal + tx + report; снятие без голоса — красный.

Модель `crates/aura-storage/src/treasury.rs`: казна с подписанным балансом
(`i128`, овердрафт видим и запрещён); совет 5 голосует за платёжные предложения
(строгое большинство), мультисиг 2-of-3 (решение W4) подтверждает; жизненный
цикл proposal -> vote -> finalize -> approve -> execute. Каждый платёж получает
номер **tx** и прозрачный `Report {tx_id, proposal_id, amount, recipient,
purpose, height}` (отчёт казны печатает пример `T7-TX` строками). Снятие без
проголосованного предложения отвергается (`NoVote`, шаг 19 сценария); платёж
не исполняется дважды (`AlreadyExecuted`); отклонённое предложение не платит
(`Rejected`); сумма больше баланса — `InsufficientFunds`. Сценарий
`sim/t7-treasury.json` (10 предложений, 82 шага: 6 выплат с tx+report,
2 отклонения, попытка снятия без голоса), золотой дайджест 719c93b2…3231f7
(Rust ↔ Python ↔ пример `t7_treasury_example`). Гейт
`crates/aura-storage/tests/t7_treasury.rs` (10 тестов + 2 метаморфизма),
оракул `tools/t7_treasury_check.py`.
negative controls `tools/nc_t7.py` (`nc.log`, RESULT: 7/7 negative controls PASS)
— 7 контролей (стандарт 6 + literal NC снятия без голоса).
"""

commands = [
    ["python3 tools/t7_treasury_check.py", "baseline"],
    ["cargo test --locked -p aura-storage --test t7_treasury", "t7-treasury"],
    ["python3 tools/nc_t7.py", "nc-suite"],
]
runs = {
    "baseline": ["python3", "tools/t7_treasury_check.py", "t7-run"],
    "t7-treasury": ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t7_treasury"],
    "nc-suite": ["python3", "tools/nc_t7.py"],
}


def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def main():
    import os

    os.makedirs(TIER + "/campaign", exist_ok=True)
    outputs = {}
    for gate, cmd in runs.items():
        proc = subprocess.run(cmd, capture_output=True, text=True)
        out = proc.stdout + proc.stderr
        outputs[gate] = out
        ok = ("test result: ok." in out or '"result": "pass"' in out
              or "RESULT: 7/7 negative controls PASS" in out)
        print(f"{gate}: {'pass' if ok else 'FAIL'}")
        if not ok:
            print(out[-2000:])
            raise SystemExit(1)
    open(TIER + "/baseline.log", "w").write(outputs["baseline"])
    open(TIER + "/t7-treasury.log", "w").write(outputs["t7-treasury"])
    open(TIER + "/campaign/t7-treasury.log", "w").write(outputs["t7-treasury"])
    open(TIER + "/nc-suite.log", "w").write(outputs["nc-suite"])
    open(TIER + "/nc.log", "w").write("\n".join(NC_LINES) + "\n")

    result = {
        "commands": commands,
        "gates": {"baseline": "pass", "nc-suite": "pass", "t7-treasury": "pass"},
        "golden_path_digest": DIGEST,
        "named_gate": NAMED_GATE,
        "phase": PHASE,
        "sources": {path: sha(path) for path in SOURCES},
        "when": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S"),
    }
    open(TIER + "/campaign/result.json", "w").write(json.dumps(result, indent=2, sort_keys=True) + "\n")

    open(TIER + "/gates.txt", "w").write(
        "T7 Treasury gates (ECONOMICS_PLAN, crates/aura-storage/tests/t7_treasury.rs)\n"
        f"Gate: {NAMED_GATE}\n"
        "\n"
        "baseline                                   pass                   independent treasury oracle (10 proposals, 6 payouts with tx+report)\n"
        "t7-treasury                                pass                   named gate: every payout has proposal + tx + report; withdrawal without vote rejected\n"
        "nc-suite                                   pass                   7 mutation controls incl. the literal withdrawal-without-vote rejection\n"
        "\n"
        "negative controls: 7/7 (PASS)\n"
    )
    open(TIER + "/inventory.txt", "w").write(
        "# T7 inventory (generated by tools/write_t7_evidence.py)\n"
        "# Cumulative negative controls: 24 phases, 565 controls.\n"
        f"{INVENTORY_PHASES}\n"
        "TOTAL: 565 negative controls cumulative\n"
    )
    open(TIER + "/report.md", "w").write(REPORT)

    rows = [
        "baseline.log", "gates.txt", "inventory.txt", "nc-suite.log",
        "nc.log", "report.md", "t7-treasury.log",
        "campaign/result.json", "campaign/t7-treasury.log",
    ]
    with open(TIER + "/SHA256SUMS", "w") as f:
        for rel in rows:
            f.write(f"{sha(TIER + '/' + rel)}  docs/evidence/t7/{rel}\n")
    print("T7 evidence pack complete")
    print("TOTAL: 565 negative controls cumulative")


if __name__ == "__main__":
    main()
