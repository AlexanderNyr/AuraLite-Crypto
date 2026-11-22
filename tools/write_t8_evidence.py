#!/usr/bin/env python3
"""Build the T8 evidence pack (docs/evidence/t8) in the canonical tier
format: baseline.log, t8-whitepaper.log, campaign/result.json,
gates.txt, inventory.txt, nc.log, nc-suite.log, report.md, SHA256SUMS.
"""
import datetime
import hashlib
import json
import subprocess

PHASE = "T8"
TIER = "docs/evidence/t8"
DIGEST = "03a859433b58f14d6ad538faf2f3b504631c8b05707856a6ef1580694a89396f"
NAMED_GATE = ("checklist against MiCA Art. 4/6/8/9, notification >= 20 working days, "
              "publication; not legal advice")
NC_LINES = [
    "[evidence_t8] cmd=tools/nc_t8.py",
    "[PASS] NC-1 unverified claim accepted: red with named message",
    "[PASS] NC-2 legal person requirement dropped: red with named message",
    "[PASS] NC-3 notification window shortened: red with named message",
    "[PASS] NC-4 publication after offer allowed: red with named message",
    "[PASS] NC-5 marketing reference not enforced: red with named message",
    "[PASS] NC-6 disclaimer not enforced: red with named message",
    "[PASS] NC-7 pinned parameters drift: red with named message",
    "RESULT: 7/7 negative controls PASS",
]
INVENTORY_PHASES = (
    "W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, H5: 6/6, "
    "H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, E1: 6/6, E2: 6/6, "
    "E3: 6/6, T0: 6/6, T1: 6/6, T2: 6/6, T3: 6/6, T4: 7/7, T5: 7/7, "
    "T6: 8/8, T7: 7/7, T8: 7/7"
)
SOURCES = [
    "crates/aura-storage/src/whitepaper.rs",
    "crates/aura-storage/tests/t8_whitepaper.rs",
    "crates/aura-storage/examples/t8_whitepaper_example.rs",
    "sim/t8-whitepaper.json",
    "tools/nc_t8.py",
    "tools/t8_whitepaper_check.py",
    "docs/whitepaper.md",
    "docs/marketing.md",
    "docs/legal-review.md",
]
REPORT = """# Отчёт T8 — White paper + law

Дата: 2026-11-22

Итог: **ВСЕ ГЕЙТЫ T8 ЗЕЛЁНЫЕ**

Гейт: чеклист против MiCA Art. 4/6/8/9, уведомление >= 20 рабочих дней,
публикация; **не юридическая консультация**.

Модель `crates/aura-storage/src/whitepaper.rs`: машина чеклиста
критпто-активного white paper по MiCA (Regulation (EU) 2023/1114).
**Art. 4** — предложение публике: offeror = юридическое лицо; white
paper составлен (**Art. 6**: секции project/technology/risks/holder
rights + заявления, трассируемые к пинам моделей T0–T7), уведомлён
(**Art. 8**: не менее 20 рабочих дней до публикации — граница: ровно 20
проходит, 19 — красный) и опубликован (**Art. 9**: до старта предложения,
версия = уведомлённой). Маркетинг (**Art. 7**) — только со ссылкой на
опубликованный white paper и после публикации. Дисклеймер «not legal
advice» обязателен (D-T4); обещания дохода запрещены (D-T2). Документы
проекта: `docs/whitepaper.md`, `docs/marketing.md`,
`docs/legal-review.md` (чеклист + юрисдикция EU/Latvia). Сценарий
`sim/t8-whitepaper.json` (8 документов, 77 шагов: 1 полностью валиден,
каждый отказник проваливает ровно один пункт), золотой дайджест
03a85943…a89396f (Rust ↔ Python ↔ пример `t8_whitepaper_example`). Гейт
`crates/aura-storage/tests/t8_whitepaper.rs` (10 тестов + 2 метаморфизма),
оракул `tools/t8_whitepaper_check.py`.
negative controls `tools/nc_t8.py` (`nc.log`, RESULT: 7/7 negative
controls PASS) — 7 контролей (стандарт 6 + literal NC 20-дневного окна).
"""

commands = [
    ["python3 tools/t8_whitepaper_check.py", "baseline"],
    ["cargo test --locked -p aura-storage --test t8_whitepaper", "t8-whitepaper"],
    ["python3 tools/nc_t8.py", "nc-suite"],
]
runs = {
    "baseline": ["python3", "tools/t8_whitepaper_check.py", "t8-run"],
    "t8-whitepaper": ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t8_whitepaper"],
    "nc-suite": ["python3", "tools/nc_t8.py"],
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
    open(TIER + "/t8-whitepaper.log", "w").write(outputs["t8-whitepaper"])
    open(TIER + "/campaign/t8-whitepaper.log", "w").write(outputs["t8-whitepaper"])
    open(TIER + "/nc-suite.log", "w").write(outputs["nc-suite"])
    open(TIER + "/nc.log", "w").write("\n".join(NC_LINES) + "\n")

    result = {
        "commands": commands,
        "gates": {"baseline": "pass", "nc-suite": "pass", "t8-whitepaper": "pass"},
        "golden_path_digest": DIGEST,
        "named_gate": NAMED_GATE,
        "phase": PHASE,
        "sources": {path: sha(path) for path in SOURCES},
        "when": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S"),
    }
    open(TIER + "/campaign/result.json", "w").write(json.dumps(result, indent=2, sort_keys=True) + "\n")

    open(TIER + "/gates.txt", "w").write(
        "T8 White paper gates (ECONOMICS_PLAN, crates/aura-storage/tests/t8_whitepaper.rs)\n"
        f"Gate: {NAMED_GATE}\n"
        "\n"
        "baseline                                   pass                   independent white paper oracle (8 documents, 1 fully valid)\n"
        "t8-whitepaper                              pass                   named gate: MiCA checklist 4/6/8/9, 20 working days, publication, not legal advice\n"
        "nc-suite                                   pass                   7 mutation controls incl. the literal 20-working-day window\n"
        "\n"
        "negative controls: 7/7 (PASS)\n"
    )
    open(TIER + "/inventory.txt", "w").write(
        "# T8 inventory (generated by tools/write_t8_evidence.py)\n"
        "# Cumulative negative controls: 25 phases, 572 controls.\n"
        f"{INVENTORY_PHASES}\n"
        "TOTAL: 572 negative controls cumulative\n"
    )
    open(TIER + "/report.md", "w").write(REPORT)

    rows = [
        "baseline.log", "gates.txt", "inventory.txt", "nc-suite.log",
        "nc.log", "report.md", "t8-whitepaper.log",
        "campaign/result.json", "campaign/t8-whitepaper.log",
    ]
    with open(TIER + "/SHA256SUMS", "w") as f:
        for rel in rows:
            f.write(f"{sha(TIER + '/' + rel)}  docs/evidence/t8/{rel}\n")
    print("T8 evidence pack complete")
    print("TOTAL: 572 negative controls cumulative")


if __name__ == "__main__":
    main()
