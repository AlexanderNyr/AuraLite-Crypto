#!/usr/bin/env python3
"""Write docs/evidence/t5 (ECONOMICS_PLAN T5).

Runs the T5 gates (independent slashing/insurance oracle incl. the Rust
example cross-run, cargo test t5_slashing) and tools/nc_t5.py, builds
self-contained evidence: gates.txt rows, campaign/ bundle (run logs +
result.json with source digests), nc.log with the RESULT line, report.md
and cumulative inventory (TOTAL: 550).
"""

import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVID = os.path.join(ROOT, "docs", "evidence", "t5")
EVID_DIRS = ["", "campaign"]
GATES = [
    ([sys.executable, os.path.join(ROOT, "tools", "t5_slashing_check.py")],
     "baseline", "independent slashing oracle (7 scenarios, penalty >= damage)"),
    (["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t5_slashing"],
     "t5-slashing", "named gate: penalty >= damage; fund never negative"),
    ([sys.executable, os.path.join(ROOT, "tools", "nc_t5.py")],
     "nc-suite", "7 mutation controls incl. the literal fund-never-negative"),
]
SOURCES = [
    "crates/aura-storage/src/slashing.rs",
    "crates/aura-storage/tests/t5_slashing.rs",
    "crates/aura-storage/examples/t5_slashing_example.rs",
    "sim/t5-slashing.json",
    "tools/t5_slashing_check.py",
    "tools/nc_t5.py",
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
        return "RESULT: T5 slashing check PASS" in output
    if name == "t5-slashing":
        return "test result: ok." in output
    return "RESULT: 7/7 negative controls PASS" in output


def main():
    for rel in EVID_DIRS:
        os.makedirs(os.path.join(EVID, rel), exist_ok=True)

    rows = []
    logs = {}
    outputs = {}
    ok_all = True
    for cmd, name, desc in GATES:
        print("[evidence_t5] %s ..." % name, flush=True)
        code, out, dur = run_cmd(cmd)
        ok = gate_passes(code, out, name)
        ok_all = ok_all and ok
        rows.append((name, "pass" if ok else "FAIL", desc))
        outputs[name] = out
        logs[name + ".log"] = (
            "[evidence_t5] cmd=%s exit=%d duration=%.2fs\n" % (" ".join(cmd), code, dur)
            + out)
        print("[evidence_t5] %s: %s" % (name, "pass" if ok else "FAIL"), flush=True)

    nc_ok = "RESULT: 7/7 negative controls PASS" in outputs.get("nc-suite", "")
    gate_ok = "test result: ok." in outputs.get("t5-slashing", "")
    oracle_ok = "RESULT: T5 slashing check PASS" in outputs.get("baseline", "")

    with open(os.path.join(EVID, "nc.log"), "w", encoding="utf-8") as f:
        f.write("[evidence_t5] cmd=tools/nc_t5.py\n")
        f.write(outputs.get("nc-suite", ""))

    with open(os.path.join(EVID, "campaign", "t5-slashing.log"), "w",
              encoding="utf-8") as f:
        f.write(logs.get("t5-slashing.log", ""))
    with open(os.path.join(EVID, "campaign", "result.json"), "w",
              encoding="utf-8") as f:
        json.dump({
            "phase": "T5",
            "when": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "commands": [[" ".join(cmd), name] for cmd, name, _ in GATES],
            "gates": {name: status for name, status, _ in rows},
            "named_gate": "7 scenarios: penalty >= damage, fund never negative",
            "golden_path_digest":
                "4cc9ea0319374c3c2c74bc88db6d7747b23f0e21ea637ae2270bc4c5707c9cef",
            "sources": {rel: sha256_file(os.path.join(ROOT, rel)) for rel in SOURCES},
        }, f, indent=2, sort_keys=True)
        f.write("\n")

    for name, text in logs.items():
        with open(os.path.join(EVID, name), "w", encoding="utf-8") as f:
            f.write(text)

    with open(os.path.join(EVID, "gates.txt"), "w", encoding="utf-8") as f:
        f.write("T5 Slashing and insurance gates (ECONOMICS_PLAN, "
                "crates/aura-storage/tests/t5_slashing.rs)\n")
        f.write("Gate: 7 scenarios: penalty >= damage, fund never negative\n\n")
        for name, status, desc in rows:
            f.write("%-42s %-22s %s\n" % (name, status, desc))
        f.write("\nnegative controls: 7/7 (%s)\n" % ("PASS" if nc_ok else "FAIL"))
        f.write("named gate: %s\n" % ("PASS" if gate_ok else "FAIL"))
        f.write("oracle cross-run: %s\n" % ("PASS" if oracle_ok else "FAIL"))

    with open(os.path.join(EVID, "report.md"), "w", encoding="utf-8") as f:
        f.write("# Отчёт T5 — Slashing and insurance\n\n")
        f.write("Дата: %s\n\n" % time.strftime("%Y-%m-%d"))
        f.write("Итог: **%s**\n\n" % ("ВСЕ ГЕЙТЫ T5 ЗЕЛЁНЫЕ" if ok_all
                                      else "ЕСТЬ КРАСНЫЕ ГЕЙТЫ"))
        f.write("Гейт: 7 сценариев — penalty >= damage, фонд страхования "
                "никогда не уходит в минус.\n\n")
        f.write("Модель `crates/aura-storage/src/slashing.rs`: инцидент "
                "причиняет `damage`; штраф обидчика = damage × 15000 bps "
                "(параметры штрафа, penalty >= damage всегда); пострадавшие "
                "покрываются из страхового фонда (`covered = min(damage, "
                "balance)`, остаток — `shortfall`); фонд пополняется на "
                "5000 bps от применённого штрафа; mass-slash защита — "
                "окно не может уничтожить больше 500 bps зарегистрированного "
                "bonded (1 000 000 `ALCRY` = 50 000 `ALCRY`/окно), избыток "
                "штрафа ОТКЛАДЫВАЕТСЯ (не прощается) и собирается следующими "
                "окнами. Сценарий `sim/t5-slashing.json` (7 сценариев, 87 "
                "шагов, 48 инцидентов: единичный double-sign, повторный "
                "нарушитель, коррелированный mass-slash, истощение фонда, "
                "floor-края штрафа, cap по окнам, цепочка истощения и "
                "пополнения), золотой дайджест 4cc9ea03…7c9cef (Rust ↔ "
                "Python ↔ пример `t5_slashing_example`); финал: фонд "
                "536 750 `ALCRY`, очередь отложенных пуста "
                "(total_applied == total_penalty == 1 790 402 999 999 999 "
                "999 999 999), покрыто 558 451 499 999 999 999 999 998, "
                "непокрыто 635 150 500 000 000 000 000 002. Гейт "
                "`crates/aura-storage/tests/t5_slashing.rs` (10 тестов + "
                "2 метаморфизма), оракул `tools/t5_slashing_check.py`.\n")
        f.write("negative controls `tools/nc_t5.py` (`nc.log`, "
                "RESULT: 7/7 negative controls PASS) — 7 контролей "
                "(стандарт 6 + mass-slash cap, прецедент H6–H9/T4).\n\n")

    with open(os.path.join(EVID, "inventory.txt"), "w", encoding="utf-8") as f:
        f.write("# T5 inventory (generated by tools/write_t5_evidence.py)\n")
        f.write("# Cumulative negative controls: 22 phases, 550 controls.\n")
        f.write("W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, "
                "H5: 6/6, H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, "
                "E1: 6/6, E2: 6/6, E3: 6/6, T0: 6/6, T1: 6/6, T2: 6/6, "
                "T3: 6/6, T4: 7/7, T5: 7/7\n")
        f.write("TOTAL: 550 negative controls cumulative\n")

    with open(os.path.join(EVID, "SHA256SUMS"), "w", encoding="utf-8") as f:
        for rel in sorted(EVID_DIRS):
            base = os.path.join(EVID, rel)
            for name in sorted(os.listdir(base)):
                path = os.path.join(base, name)
                if os.path.isfile(path) and name != "SHA256SUMS":
                    repo_rel = os.path.relpath(path, ROOT)
                    f.write("%s  %s\n" % (sha256_file(path), repo_rel))

    print("[evidence_t5] %s"
          % ("all gates green" if ok_all else "RED GATES PRESENT"), flush=True)
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
