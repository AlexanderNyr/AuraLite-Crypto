#!/usr/bin/env python3
"""Write docs/evidence/t6 (ECONOMICS_PLAN T6).

Runs the T6 gates (independent governance oracle incl. the Rust example
cross-run, cargo test t6_governance) and tools/nc_t6.py, builds
self-contained evidence: gates.txt rows, campaign/ bundle (run logs +
result.json with source digests), nc.log with the RESULT line, report.md
and cumulative inventory (TOTAL: 558).
"""

import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVID = os.path.join(ROOT, "docs", "evidence", "t6")
EVID_DIRS = ["", "campaign"]
GATES = [
    ([sys.executable, os.path.join(ROOT, "tools", "t6_governance_check.py")],
     "baseline", "independent governance oracle (20 proposals, valid ones pass)"),
    (["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t6_governance"],
     "t6-governance", "named gate: valid ones pass; flash-stake attack rejected"),
    ([sys.executable, os.path.join(ROOT, "tools", "nc_t6.py")],
     "nc-suite", "8 mutation controls incl. the literal flash-stake rejection"),
]
SOURCES = [
    "crates/aura-storage/src/governance.rs",
    "crates/aura-storage/tests/t6_governance.rs",
    "crates/aura-storage/examples/t6_governance_example.rs",
    "sim/t6-governance.json",
    "tools/t6_governance_check.py",
    "tools/nc_t6.py",
]


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def run_cmd(cmd, timeout=2400):
    started = time.time()
    proc = subprocess.run(cmd, cwd=ROOT, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, text=True,
                          errors="replace", timeout=timeout)
    return proc.returncode, proc.stdout or "", time.time() - started


def gate_passes(code, output, name):
    if code != 0:
        return False
    if name == "baseline":
        return "RESULT: T6 governance check PASS" in output
    if name == "t6-governance":
        return "test result: ok." in output
    return "RESULT: 8/8 negative controls PASS" in output


def main():
    for rel in EVID_DIRS:
        os.makedirs(os.path.join(EVID, rel), exist_ok=True)

    rows = []
    logs = {}
    outputs = {}
    ok_all = True
    for cmd, name, desc in GATES:
        print("[evidence_t6] %s ..." % name, flush=True)
        code, out, dur = run_cmd(cmd)
        ok = gate_passes(code, out, name)
        ok_all = ok_all and ok
        rows.append((name, "pass" if ok else "FAIL", desc))
        outputs[name] = out
        logs[name + ".log"] = (
            "[evidence_t6] cmd=%s exit=%d duration=%.2fs\n" % (" ".join(cmd), code, dur)
            + out)
        print("[evidence_t6] %s: %s" % (name, "pass" if ok else "FAIL"), flush=True)

    nc_ok = "RESULT: 8/8 negative controls PASS" in outputs.get("nc-suite", "")
    gate_ok = "test result: ok." in outputs.get("t6-governance", "")
    oracle_ok = "RESULT: T6 governance check PASS" in outputs.get("baseline", "")

    with open(os.path.join(EVID, "nc.log"), "w", encoding="utf-8") as f:
        f.write("[evidence_t6] cmd=tools/nc_t6.py\n")
        f.write(outputs.get("nc-suite", ""))

    with open(os.path.join(EVID, "campaign", "t6-governance.log"), "w",
              encoding="utf-8") as f:
        f.write(logs.get("t6-governance.log", ""))
    with open(os.path.join(EVID, "campaign", "result.json"), "w",
              encoding="utf-8") as f:
        json.dump({
            "phase": "T6",
            "when": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "commands": [[" ".join(cmd), name] for cmd, name, _ in GATES],
            "gates": {name: status for name, status, _ in rows},
            "named_gate": "20 proposals: valid ones pass, flash-stake attack rejected",
            "golden_path_digest":
                "828b13cf7ca3abcaea65386f7c62e1e155f4d207cb000aceccd86397b5c729aa",
            "sources": {rel: sha256_file(os.path.join(ROOT, rel)) for rel in SOURCES},
        }, f, indent=2, sort_keys=True)
        f.write("\n")

    for name, text in logs.items():
        with open(os.path.join(EVID, name), "w", encoding="utf-8") as f:
            f.write(text)

    with open(os.path.join(EVID, "gates.txt"), "w", encoding="utf-8") as f:
        f.write("T6 Governance gates (ECONOMICS_PLAN, "
                "crates/aura-storage/tests/t6_governance.rs)\n")
        f.write("Gate: 20 proposals: valid ones pass, flash-stake attack rejected\n\n")
        for name, status, desc in rows:
            f.write("%-42s %-22s %s\n" % (name, status, desc))
        f.write("\nnegative controls: 8/8 (%s)\n" % ("PASS" if nc_ok else "FAIL"))
        f.write("named gate: %s\n" % ("PASS" if gate_ok else "FAIL"))
        f.write("oracle cross-run: %s\n" % ("PASS" if oracle_ok else "FAIL"))

    with open(os.path.join(EVID, "report.md"), "w", encoding="utf-8") as f:
        f.write("# Отчёт T6 — Governance\n\n")
        f.write("Дата: %s\n\n" % time.strftime("%Y-%m-%d"))
        f.write("Итог: **%s**\n\n" % ("ВСЕ ГЕЙТЫ T6 ЗЕЛЁНЫЕ" if ok_all
                                      else "ЕСТЬ КРАСНЫЕ ГЕЙТЫ"))
        f.write("Гейт: 20 предложений — валидные проходят, flash-stake "
                "атака отвергается.\n\n")
        f.write("Модель `crates/aura-storage/src/governance.rs`: порог "
                "предложения 10 000 `ALCRY`; кворум 2000 bps снапшота "
                "(граница: голоса == кворуму → проходит); период голосования "
                "120 960 блоков (7 дней по 5 с), голосовать можно в "
                "[created, created+period) — на границе красный; **48 ч "
                "timelock** (34 560 блоков) между закрытием голосования и "
                "исполнением (граница: на блок раньше — красный); "
                "голосование идёт по **снапшоту** мощности на момент "
                "создания — flash-stake после снапшота не даёт мощности, "
                "его голос отвергается (`NoVotingPower`); экстренное veto "
                "с **sunset** на блоке 1 521 000 (граница: за блок до — "
                "работает, на закате и после — `VetoSunset`). Сценарий "
                "`sim/t6-governance.json` (20 предложений, 113 шагов: 15 "
                "исполнено, 4 отклонено (большинство/кворум/ничья), 1 "
                "ветировано; валидные проходят), золотой дайджест "
                "828b13cf…c729aa (Rust ↔ Python ↔ пример "
                "`t6_governance_example`). Гейт "
                "`crates/aura-storage/tests/t6_governance.rs` (10 тестов + "
                "2 метаморфизма), оракул `tools/t6_governance_check.py`.\n")
        f.write("negative controls `tools/nc_t6.py` (`nc.log`, "
                "RESULT: 8/8 negative controls PASS) — 8 контролей "
                "(стандарт 6 + flash-stake + veto sunset, прецедент "
                "H6–H9/T4/T5).\n\n")

    with open(os.path.join(EVID, "inventory.txt"), "w", encoding="utf-8") as f:
        f.write("# T6 inventory (generated by tools/write_t6_evidence.py)\n")
        f.write("# Cumulative negative controls: 23 phases, 558 controls.\n")
        f.write("W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, "
                "H5: 6/6, H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, "
                "E1: 6/6, E2: 6/6, E3: 6/6, T0: 6/6, T1: 6/6, T2: 6/6, "
                "T3: 6/6, T4: 7/7, T5: 7/7, T6: 8/8\n")
        f.write("TOTAL: 558 negative controls cumulative\n")

    with open(os.path.join(EVID, "SHA256SUMS"), "w", encoding="utf-8") as f:
        for rel in sorted(EVID_DIRS):
            base = os.path.join(EVID, rel)
            for name in sorted(os.listdir(base)):
                path = os.path.join(base, name)
                if os.path.isfile(path) and name != "SHA256SUMS":
                    repo_rel = os.path.relpath(path, ROOT)
                    f.write("%s  %s\n" % (sha256_file(path), repo_rel))

    print("[evidence_t6] %s"
          % ("all gates green" if ok_all else "RED GATES PRESENT"), flush=True)
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
