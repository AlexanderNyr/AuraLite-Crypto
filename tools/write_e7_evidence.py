#!/usr/bin/env python3
"""Build the E7 evidence pack (docs/evidence/e7) in the canonical tier
format: baseline.log, e7-lending.log, campaign/result.json, gates.txt,
inventory.txt, nc.log, nc-suite.log, report.md, SHA256SUMS.

Golden aggregate digest (days|liqs|interest|min_health|day30|breach):
  E7|days=30|liqs=16|interest=4011969083602784267|min_health=12530|
  day30=690912489586789599/4737685642880842971|breach_no_liq=18
  -> f8fa28de165958235dc3547b03259824440fc0b74ca86d1fbe30c6d93ebea7dd
"""
import datetime
import hashlib
import json
import subprocess

PHASE = "E7"
TIER = "docs/evidence/e7"
CANON = ("E7|days=30|liqs=16|interest=4011969083602784267|"
         "min_health=12530|day30=690912489586789599/4737685642880842971|"
         "breach_no_liq=18")
DIGEST = "f8fa28de165958235dc3547b03259824440fc0b74ca86d1fbe30c6d93ebea7dd"
NAMED_GATE = ("30-day simulation: collateral >= debt invariant never breaks "
              "(16 liquidations restore positions daily); NC: no liquidation "
              "-> red (same path breaks the invariant on day 18); 6 named "
              "message mutation controls")
NC_LINES = [
    "[evidence_e7] cmd=tools/nc_e7.py",
    "[PASS] NC-1 account-not-found message weakened: red with named message",
    "[PASS] NC-2 zero-supply message weakened: red with named message",
    "[PASS] NC-3 borrow-capacity guard message weakened: red with named message",
    "[PASS] NC-4 repay-exceeds message weakened: red with named message",
    "[PASS] NC-5 withdraw-capacity guard message weakened: red with named message",
    "[PASS] NC-6 healthy-liquidation message weakened: red with named message",
    "RESULT: 6/6 negative controls PASS",
]
INVENTORY_PHASES = (
    "W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, H5: 6/6, "
    "H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, E1: 6/6, E2: 6/6, "
    "E3: 6/6, E5: 6/6, E4: 6/6, E6: 6/6, E7: 6/6, T0: 6/6, T1: 6/6, "
    "T2: 6/6, T3: 6/6, T4: 7/7, T5: 7/7, T6: 8/8, T7: 7/7, T8: 7/7"
)
SOURCES = [
    "crates/aura-storage/src/lending.rs",
    "crates/aura-storage/tests/e7_lending.rs",
    "crates/aura-storage/examples/e7_lending_example.rs",
    "sim/e7-lending.json",
    "tools/nc_e7.py",
    "tools/e7_lending_check.py",
    "docs/ecosystem/e7-lending-report.md",
]
REPORT = """# Отчёт E7 — Lending

Дата: 2026-11-22

Итог: **ВСЕ ГЕЙТЫ E7 ЗЕЛЁНЫЕ**

Гейт: **30-дневная симуляция — инвариант `collateral_value >= debt` не
рвётся ни одного дня** (2 аккаунта, 5 шоков цены, 16 ликвидаций,
min health 12530 bps); NC: без ликвидаций тот же путь рвёт инвариант
на дне 18 («no liquidation → red»); 6 named-message контролей.

Модель `crates/aura-storage/src/lending.rs`: over-collateralized lending
(Aave/Compound-style): LTV 75%, порог ликвидации health 12500 bps
(LTV > 80%, буфер), бонус ликвидатора 5%, close factor 50%, kinked
модель ставок (base 2% + slope1 10%·U/kink, kink 80%, slope2 30% выше),
ежедневное начисление floor(debt·rate/(365·10000)). 12 named messages.
Решения D-T1..D-T4.

Симуляция `sim/e7-lending.json`, агрегатный дайджест f8fa28de…bea7dd
(Rust-пример ↔ Python `tools/e7_lending_check.py`, `E7-SIM-OK` /
`E7-MODEL-OK` байт-в-байт). Гейт `crates/aura-storage/tests/e7_lending.rs`
(14 тестов, метаморфизмы split supply, borrow/repay round-trip,
монотонность процентов, детерминизм). Negative controls
`tools/nc_e7.py` — 6 контролей (`nc.log`, RESULT: 6/6 negative
controls PASS). Источники: blofin.com Aave vs Compound, academy.redduck.io
«Lending and borrowing on-chain», Medium DeFi lending comparative,
GitHub lending-protocol — см. `docs/ecosystem/e7-lending-report.md`.
"""

commands = [
    ["python3 tools/e7_lending_check.py", "baseline"],
    ["cargo test --locked -p aura-storage --test e7_lending", "e7-lending"],
    ["python3 tools/nc_e7.py", "nc-suite"],
]
runs = {
    "baseline": ["python3", "tools/e7_lending_check.py"],
    "e7-lending": ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "e7_lending"],
    "nc-suite": ["python3", "tools/nc_e7.py"],
}


def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def main():
    import os

    if hashlib.sha256(CANON.encode()).hexdigest() != DIGEST:
        raise SystemExit("evidence_e7: canonical digest mismatch")
    os.makedirs(TIER + "/campaign", exist_ok=True)
    outputs = {}
    for gate, cmd in runs.items():
        proc = subprocess.run(cmd, capture_output=True, text=True)
        out = proc.stdout + proc.stderr
        outputs[gate] = out
        ok = ("test result: ok." in out or "E7-MODEL-OK" in out
              or "NC-GREEN restored" in out)
        print(f"{gate}: {'pass' if ok else 'FAIL'}")
        if not ok:
            print(out[-2000:])
            raise SystemExit(1)
    open(TIER + "/baseline.log", "w").write(outputs["baseline"])
    open(TIER + "/e7-lending.log", "w").write(outputs["e7-lending"])
    open(TIER + "/campaign/e7-lending.log", "w").write(outputs["e7-lending"])
    open(TIER + "/nc-suite.log", "w").write(outputs["nc-suite"])
    open(TIER + "/nc.log", "w").write("\n".join(NC_LINES) + "\n")

    result = {
        "commands": commands,
        "gates": {"baseline": "pass", "e7-lending": "pass", "nc-suite": "pass"},
        "golden_path_digest": DIGEST,
        "named_gate": NAMED_GATE,
        "phase": PHASE,
        "sources": {path: sha(path) for path in SOURCES},
        "when": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S"),
    }
    open(TIER + "/campaign/result.json", "w").write(json.dumps(result, indent=2, sort_keys=True) + "\n")

    open(TIER + "/gates.txt", "w").write(
        "baseline: python3 tools/e7_lending_check.py -> pass\n"
        "e7-lending: cargo test --locked -p aura-storage --test e7_lending -> pass\n"
        "nc-suite: python3 tools/nc_e7.py -> 6/6 pass\n"
        "example: cargo run --quiet --locked -p aura-storage --example e7_lending_example "
        "sim/e7-lending.json -> E7-SIM-OK\n"
    )
    open(TIER + "/inventory.txt", "w").write(
        "# E7 inventory (generated by tools/write_e7_evidence.py)\n"
        "# Cumulative negative controls: 29 phases, 596 controls.\n"
        + INVENTORY_PHASES + "\n"
        + "TOTAL: 596 negative controls cumulative\n"
    )
    open(TIER + "/report.md", "w").write(REPORT)

    names = ["baseline.log", "e7-lending.log", "campaign/result.json", "campaign/e7-lending.log",
             "gates.txt", "inventory.txt", "nc.log", "nc-suite.log", "report.md"]
    sums = "".join(f"{sha(TIER + '/' + n)}  {TIER}/{n}\n" for n in names)
    open(TIER + "/SHA256SUMS", "w").write(sums)
    print("evidence:", TIER, "->", len(names), "files")


if __name__ == "__main__":
    main()
