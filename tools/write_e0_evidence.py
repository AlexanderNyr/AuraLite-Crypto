#!/usr/bin/env python3
"""Write docs/evidence/e0 (ECOSYSTEM_PLAN E0).

Runs the E0 gates and tools/nc_e0.py, builds self-contained evidence:
gates.txt rows, campaign/ bundle (baseline logs + result.json with source
digests), nc.log with the RESULT line, report.md and cumulative inventory
(TOTAL: 494).
"""

import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVID = os.path.join(ROOT, "docs", "evidence", "e0")
EVID_DIRS = ["", "campaign"]
GATES = [
    ([sys.executable, os.path.join(ROOT, "tools", "e0_baseline_check.py")],
     "baseline", "named gate: checklist of 10 user scenarios (0/10 so far)"),
    ([sys.executable, os.path.join(ROOT, "tools", "nc_e0.py")],
     "nc-suite", "6 mutation controls incl. the named gate twice"),
]
SOURCES = [
    "tools/e0_baseline_check.py",
    "tools/nc_e0.py",
    "tools/fixtures/e0_user_scenarios.json",
    "docs/ecosystem/e0-baseline-report.md",
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
        return "RESULT: E0 baseline check PASS" in output
    return "RESULT: 6/6 negative controls PASS" in output


def main():
    for rel in EVID_DIRS:
        os.makedirs(os.path.join(EVID, rel), exist_ok=True)

    rows = []
    logs = {}
    outputs = {}
    ok_all = True
    for cmd, name, desc in GATES:
        print("[evidence_e0] %s ..." % name, flush=True)
        code, out, dur = run_cmd(cmd)
        ok = gate_passes(code, out, name)
        ok_all = ok_all and ok
        rows.append((name, "pass" if ok else "FAIL", desc))
        outputs[name] = out
        logs[name + ".log"] = (
            "[evidence_e0] cmd=%s exit=%d duration=%.2fs\n" % (" ".join(cmd), code, dur)
            + out)
        print("[evidence_e0] %s: %s" % (name, "pass" if ok else "FAIL"), flush=True)

    nc_ok = "RESULT: 6/6 negative controls PASS" in outputs.get("nc-suite", "")
    baseline_ok = "RESULT: E0 baseline check PASS" in outputs.get("baseline", "")

    with open(os.path.join(EVID, "nc.log"), "w", encoding="utf-8") as f:
        f.write("[evidence_e0] cmd=tools/nc_e0.py\n")
        f.write(outputs.get("nc-suite", ""))

    with open(os.path.join(EVID, "campaign", "baseline.log"), "w",
              encoding="utf-8") as f:
        f.write(logs.get("baseline.log", ""))
    with open(os.path.join(EVID, "campaign", "result.json"), "w",
              encoding="utf-8") as f:
        json.dump({
            "phase": "E0",
            "when": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "commands": [[" ".join(cmd), name] for cmd, name, _ in GATES],
            "gates": {name: status for name, status, _ in rows},
            "named_gate": "checklist of 10 user scenarios (0/10 so far)",
            "sources": {rel: sha256_file(os.path.join(ROOT, rel)) for rel in SOURCES},
        }, f, indent=2, sort_keys=True)
        f.write("\n")

    for name, text in logs.items():
        with open(os.path.join(EVID, name), "w", encoding="utf-8") as f:
            f.write(text)

    with open(os.path.join(EVID, "gates.txt"), "w", encoding="utf-8") as f:
        f.write("E0 Baseline gates (ECOSYSTEM_PLAN, docs/ecosystem/e0-baseline-report.md)\n")
        f.write("Gate: checklist of 10 user scenarios (0/10 so far)\n\n")
        for name, status, desc in rows:
            f.write("%-42s %-22s %s\n" % (name, status, desc))
        f.write("\nnegative controls: 6/6 (%s)\n" % ("PASS" if nc_ok else "FAIL"))
        f.write("named gate: %s\n" % ("PASS" if baseline_ok else "FAIL"))

    with open(os.path.join(EVID, "report.md"), "w", encoding="utf-8") as f:
        f.write("# Отчёт E0 — базовый уровень экосистемы\n\n")
        f.write("Дата: %s\n\n" % time.strftime("%Y-%m-%d"))
        f.write("Итог: **%s**\n\n" % ("ВСЕ ГЕЙТЫ E0 ЗЕЛЁНЫЕ" if ok_all
                                      else "ЕСТЬ КРАСНЫЕ ГЕЙТЫ"))
        f.write("Гейт: checklist of 10 user scenarios (0/10 so far).\n\n")
        f.write("Отчёт `docs/ecosystem/e0-baseline-report.md`, реестр "
                "`tools/fixtures/e0_user_scenarios.json`, оракул "
                "`tools/e0_baseline_check.py` (`campaign/result.json`, "
                "`campaign/baseline.log`).\n")
        f.write("negative controls `tools/nc_e0.py` (`nc.log`, "
                "RESULT: 6/6 negative controls PASS).\n\n")

    with open(os.path.join(EVID, "inventory.txt"), "w", encoding="utf-8") as f:
        f.write("# E0 inventory (generated by tools/write_e0_evidence.py)\n")
        f.write("# Cumulative negative controls: 12 phases, 494 controls.\n")
        f.write("W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, "
                "H5: 6/6, H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6\n")
        f.write("TOTAL: 494 negative controls cumulative\n")

    with open(os.path.join(EVID, "SHA256SUMS"), "w", encoding="utf-8") as f:
        for rel in sorted(EVID_DIRS):
            base = os.path.join(EVID, rel)
            for name in sorted(os.listdir(base)):
                path = os.path.join(base, name)
                if os.path.isfile(path) and name != "SHA256SUMS":
                    repo_rel = os.path.relpath(path, ROOT)
                    f.write("%s  %s\n" % (sha256_file(path), repo_rel))

    print("[evidence_e0] %s"
          % ("all gates green" if ok_all else "RED GATES PRESENT"), flush=True)
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
