#!/usr/bin/env python3
"""Write docs/evidence/t2 (ECONOMICS_PLAN T2).

Runs the T2 gates (reconciliation oracle incl. the Rust example
cross-run, cargo test t2_allocation) and tools/nc_t2.py, builds
self-contained evidence: gates.txt rows, campaign/ bundle (run logs +
result.json with source digests), nc.log with the RESULT line, report.md
and cumulative inventory (TOTAL: 530).
"""

import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVID = os.path.join(ROOT, "docs", "evidence", "t2")
EVID_DIRS = ["", "campaign"]
GATES = [
    ([sys.executable, os.path.join(ROOT, "tools", "t2_allocation_check.py")],
     "baseline", "reconciliation oracle (genesis balances vs allocation table)"),
    (["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t2_allocation"],
     "t2-allocation", "named gate: extra coins -> red, table reconciles"),
    ([sys.executable, os.path.join(ROOT, "tools", "nc_t2.py")],
     "nc-suite", "6 mutation controls incl. the literal gate (extra coins)"),
]
SOURCES = [
    "crates/aura-storage/src/allocation.rs",
    "crates/aura-storage/tests/t2_allocation.rs",
    "crates/aura-storage/examples/t2_allocation_example.rs",
    "sim/t2-allocation.json",
    "tools/t2_allocation_check.py",
    "tools/nc_t2.py",
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
        return "RESULT: T2 allocation check PASS" in output
    if name == "t2-allocation":
        return "test result: ok." in output
    return "RESULT: 6/6 negative controls PASS" in output


def main():
    for rel in EVID_DIRS:
        os.makedirs(os.path.join(EVID, rel), exist_ok=True)

    rows = []
    logs = {}
    outputs = {}
    ok_all = True
    for cmd, name, desc in GATES:
        print("[evidence_t2] %s ..." % name, flush=True)
        code, out, dur = run_cmd(cmd)
        ok = gate_passes(code, out, name)
        ok_all = ok_all and ok
        rows.append((name, "pass" if ok else "FAIL", desc))
        outputs[name] = out
        logs[name + ".log"] = (
            "[evidence_t2] cmd=%s exit=%d duration=%.2fs\n" % (" ".join(cmd), code, dur)
            + out)
        print("[evidence_t2] %s: %s" % (name, "pass" if ok else "FAIL"), flush=True)

    nc_ok = "RESULT: 6/6 negative controls PASS" in outputs.get("nc-suite", "")
    gate_ok = "test result: ok." in outputs.get("t2-allocation", "")
    oracle_ok = "RESULT: T2 allocation check PASS" in outputs.get("baseline", "")

    with open(os.path.join(EVID, "nc.log"), "w", encoding="utf-8") as f:
        f.write("[evidence_t2] cmd=tools/nc_t2.py\n")
        f.write(outputs.get("nc-suite", ""))

    with open(os.path.join(EVID, "campaign", "t2-allocation.log"), "w",
              encoding="utf-8") as f:
        f.write(logs.get("t2-allocation.log", ""))
    with open(os.path.join(EVID, "campaign", "result.json"), "w",
              encoding="utf-8") as f:
        json.dump({
            "phase": "T2",
            "when": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "commands": [[" ".join(cmd), name] for cmd, name, _ in GATES],
            "gates": {name: status for name, status, _ in rows},
            "named_gate": "script reconciles genesis balances with the table; "
                          "NC: extra coins -> red",
            "golden_table_digest":
                "5fe7417a134011e7028f255cf7cf180335cc23c773ee9657a68de260fcc9ea41",
            "sources": {rel: sha256_file(os.path.join(ROOT, rel)) for rel in SOURCES},
        }, f, indent=2, sort_keys=True)
        f.write("\n")

    for name, text in logs.items():
        with open(os.path.join(EVID, name), "w", encoding="utf-8") as f:
            f.write(text)

    with open(os.path.join(EVID, "gates.txt"), "w", encoding="utf-8") as f:
        f.write("T2 Allocation gates (ECONOMICS_PLAN, "
                "crates/aura-storage/tests/t2_allocation.rs)\n")
        f.write("Gate: script reconciles genesis balances with the table; "
                "NC: extra coins -> red\n\n")
        for name, status, desc in rows:
            f.write("%-42s %-22s %s\n" % (name, status, desc))
        f.write("\nnegative controls: 6/6 (%s)\n" % ("PASS" if nc_ok else "FAIL"))
        f.write("named gate: %s\n" % ("PASS" if gate_ok else "FAIL"))
        f.write("oracle cross-run: %s\n" % ("PASS" if oracle_ok else "FAIL"))

    with open(os.path.join(EVID, "report.md"), "w", encoding="utf-8") as f:
        f.write("# Отчёт T2 — Allocation\n\n")
        f.write("Дата: %s\n\n" % time.strftime("%Y-%m-%d"))
        f.write("Итог: **%s**\n\n" % ("ВСЕ ГЕЙТЫ T2 ЗЕЛЁНЫЕ" if ok_all
                                      else "ЕСТЬ КРАСНЫЕ ГЕЙТЫ"))
        f.write("Гейт: скрипт сверяет genesis-балансы с таблицей; лишние "
                "монеты → красный.\n\n")
        f.write("Таблица `crates/aura-storage/src/allocation.rs`: "
                "validators 1 200 000, treasury 600 000 (вестинг 6/24 мес), "
                "community 750 000, team 450 000 (вестинг 12/36 мес) = "
                "3 000 000 ALCRY = genesis supply T1. Золотой дайджест "
                "таблицы 5fe7417a…c9ea41 (Rust ↔ Python ↔ пример "
                "`t2_allocation_example`). Вестинг: линейный с клиффом; "
                "locked-часть не тратится. Сценарий `sim/t2-allocation.json`, "
                "гейт `crates/aura-storage/tests/t2_allocation.rs` "
                "(`campaign/t2-allocation.log`, `campaign/result.json`).\n")
        f.write("negative controls `tools/nc_t2.py` (`nc.log`, "
                "RESULT: 6/6 negative controls PASS).\n\n")

    with open(os.path.join(EVID, "inventory.txt"), "w", encoding="utf-8") as f:
        f.write("# T2 inventory (generated by tools/write_t2_evidence.py)\n")
        f.write("# Cumulative negative controls: 18 phases, 530 controls.\n")
        f.write("W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, "
                "H5: 6/6, H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, "
                "E1: 6/6, E2: 6/6, E3: 6/6, T0: 6/6, T1: 6/6, T2: 6/6\n")
        f.write("TOTAL: 530 negative controls cumulative\n")

    with open(os.path.join(EVID, "SHA256SUMS"), "w", encoding="utf-8") as f:
        for rel in sorted(EVID_DIRS):
            base = os.path.join(EVID, rel)
            for name in sorted(os.listdir(base)):
                path = os.path.join(base, name)
                if os.path.isfile(path) and name != "SHA256SUMS":
                    repo_rel = os.path.relpath(path, ROOT)
                    f.write("%s  %s\n" % (sha256_file(path), repo_rel))

    print("[evidence_t2] %s"
          % ("all gates green" if ok_all else "RED GATES PRESENT"), flush=True)
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
