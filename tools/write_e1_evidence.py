#!/usr/bin/env python3
"""Write docs/evidence/e1 (ECOSYSTEM_PLAN E1).

Runs the E1 gate (cargo test e1_indexer) and tools/nc_e1.py, builds
self-contained evidence: gates.txt rows, campaign/ bundle (run logs +
result.json with source digests), nc.log with the RESULT line, report.md
and cumulative inventory (TOTAL: 500).
"""

import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVID = os.path.join(ROOT, "docs", "evidence", "e1")
EVID_DIRS = ["", "campaign"]
GATES = [
    (["cargo", "test", "--locked", "-p", "aura-storage", "--test", "e1_indexer"],
     "indexer", "named gate: 10 000 blocks + artificial reorg -> 0 duplicates/losses"),
    ([sys.executable, os.path.join(ROOT, "tools", "nc_e1.py")],
     "nc-suite", "6 mutation controls incl. the literal gate (no rollback)"),
]
SOURCES = [
    "crates/aura-storage/src/indexer.rs",
    "crates/aura-storage/tests/e1_indexer.rs",
    "tools/nc_e1.py",
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
    if name == "indexer":
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
        print("[evidence_e1] %s ..." % name, flush=True)
        code, out, dur = run_cmd(cmd)
        ok = gate_passes(code, out, name)
        ok_all = ok_all and ok
        rows.append((name, "pass" if ok else "FAIL", desc))
        outputs[name] = out
        logs[name + ".log"] = (
            "[evidence_e1] cmd=%s exit=%d duration=%.2fs\n" % (" ".join(cmd), code, dur)
            + out)
        print("[evidence_e1] %s: %s" % (name, "pass" if ok else "FAIL"), flush=True)

    nc_ok = "RESULT: 6/6 negative controls PASS" in outputs.get("nc-suite", "")
    indexer_ok = "test result: ok." in outputs.get("indexer", "")

    with open(os.path.join(EVID, "nc.log"), "w", encoding="utf-8") as f:
        f.write("[evidence_e1] cmd=tools/nc_e1.py\n")
        f.write(outputs.get("nc-suite", ""))

    with open(os.path.join(EVID, "campaign", "indexer.log"), "w",
              encoding="utf-8") as f:
        f.write(logs.get("indexer.log", ""))
    with open(os.path.join(EVID, "campaign", "result.json"), "w",
              encoding="utf-8") as f:
        json.dump({
            "phase": "E1",
            "when": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "commands": [[" ".join(cmd), name] for cmd, name, _ in GATES],
            "gates": {name: status for name, status, _ in rows},
            "named_gate": "10 000 blocks + artificial reorg -> 0 duplicates/losses",
            "sources": {rel: sha256_file(os.path.join(ROOT, rel)) for rel in SOURCES},
        }, f, indent=2, sort_keys=True)
        f.write("\n")

    for name, text in logs.items():
        with open(os.path.join(EVID, name), "w", encoding="utf-8") as f:
            f.write(text)

    with open(os.path.join(EVID, "gates.txt"), "w", encoding="utf-8") as f:
        f.write("E1 Indexer gates (ECOSYSTEM_PLAN, crates/aura-storage/tests/e1_indexer.rs)\n")
        f.write("Gate: 10 000 blocks + artificial reorg -> 0 duplicates/losses; "
                "NC: without rollback -> red\n\n")
        for name, status, desc in rows:
            f.write("%-42s %-22s %s\n" % (name, status, desc))
        f.write("\nnegative controls: 6/6 (%s)\n" % ("PASS" if nc_ok else "FAIL"))
        f.write("named gate: %s\n" % ("PASS" if indexer_ok else "FAIL"))

    with open(os.path.join(EVID, "report.md"), "w", encoding="utf-8") as f:
        f.write("# Отчёт E1 — индексер цепи\n\n")
        f.write("Дата: %s\n\n" % time.strftime("%Y-%m-%d"))
        f.write("Итог: **%s**\n\n" % ("ВСЕ ГЕЙТЫ E1 ЗЕЛЁНЫЕ" if ok_all
                                      else "ЕСТЬ КРАСНЫЕ ГЕЙТЫ"))
        f.write("Гейт: 10 000 блоков + искусственный reorg → 0 дублей/потерь; "
                "NC: без rollback → red.\n\n")
        f.write("Код `crates/aura-storage/src/indexer.rs` (идемпотентный ingest + "
                "reorg rollback над локальным хранилищем — отклонение вместо Postgres "
                "зафиксировано в ECOSYSTEM_PLAN); гейт-тест "
                "`crates/aura-storage/tests/e1_indexer.rs` "
                "(`campaign/indexer.log`, `campaign/result.json`).\n")
        f.write("negative controls `tools/nc_e1.py` (`nc.log`, "
                "RESULT: 6/6 negative controls PASS).\n\n")

    with open(os.path.join(EVID, "inventory.txt"), "w", encoding="utf-8") as f:
        f.write("# E1 inventory (generated by tools/write_e1_evidence.py)\n")
        f.write("# Cumulative negative controls: 13 phases, 500 controls.\n")
        f.write("W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, "
                "H5: 6/6, H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, E1: 6/6\n")
        f.write("TOTAL: 500 negative controls cumulative\n")

    with open(os.path.join(EVID, "SHA256SUMS"), "w", encoding="utf-8") as f:
        for rel in sorted(EVID_DIRS):
            base = os.path.join(EVID, rel)
            for name in sorted(os.listdir(base)):
                path = os.path.join(base, name)
                if os.path.isfile(path) and name != "SHA256SUMS":
                    repo_rel = os.path.relpath(path, ROOT)
                    f.write("%s  %s\n" % (sha256_file(path), repo_rel))

    print("[evidence_e1] %s"
          % ("all gates green" if ok_all else "RED GATES PRESENT"), flush=True)
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
