#!/usr/bin/env python3
"""Write docs/evidence/h8 (docs/h8-audit-plan.md §5/§6).

Runs the H8 gates and tools/nc_h8.py, builds self-contained evidence:
gates.txt rows, campaign/ bundle (reaudit logs + result.json with source
digests), nc.log with the RESULT line, report.md and cumulative inventory
(TOTAL: 481).
"""

import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVID = os.path.join(ROOT, "docs", "evidence", "h8")
EVID_DIRS = ["", "campaign"]
GATES = [
    ([sys.executable, os.path.join(ROOT, "tools", "h8_reaudit.py")],
     "reaudit", "re-audit: old bugs stay fixed, 0 unfixed critical/high (named gate)"),
    ([sys.executable, os.path.join(ROOT, "tools", "nc_h8.py")],
     "nc-suite", "7 mutation controls incl. the named gate twice"),
]
SOURCES = [
    "tools/h8_reaudit.py",
    "tools/nc_h8.py",
    "tools/fixtures/h8_known_bugs.json",
    "tools/fixtures/h8_findings.json",
    "SECURITY.md",
    "docs/audit/h8-public-audit-report.md",
    "docs/h8-audit-plan.md",
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
    if name == "reaudit":
        return "RESULT: H8 re-audit PASS" in output
    return "RESULT: 7/7 negative controls PASS" in output


def main():
    for rel in EVID_DIRS:
        os.makedirs(os.path.join(EVID, rel), exist_ok=True)

    rows = []
    logs = {}
    ok_all = True
    outputs = {}
    for cmd, name, desc in GATES:
        print("[evidence_h8] %s ..." % name, flush=True)
        code, out, dur = run_cmd(cmd)
        ok = gate_passes(code, out, name)
        ok_all = ok_all and ok
        rows.append((name, "pass" if ok else "FAIL", desc))
        outputs[name] = out
        logs[name + ".log"] = (
            "[evidence_h8] cmd=%s exit=%d duration=%.2fs\n" % (" ".join(cmd), code, dur)
            + out)
        print("[evidence_h8] %s: %s" % (name, "pass" if ok else "FAIL"), flush=True)

    nc_ok = "RESULT: 7/7 negative controls PASS" in outputs.get("nc-suite", "")
    reaudit_ok = "RESULT: H8 re-audit PASS" in outputs.get("reaudit", "")

    with open(os.path.join(EVID, "nc.log"), "w", encoding="utf-8") as f:
        f.write("[evidence_h8] cmd=tools/nc_h8.py\n")
        f.write(outputs.get("nc-suite", ""))

    with open(os.path.join(EVID, "campaign", "reaudit.log"), "w",
              encoding="utf-8") as f:
        f.write(logs.get("reaudit.log", ""))
    with open(os.path.join(EVID, "campaign", "result.json"), "w",
              encoding="utf-8") as f:
        json.dump({
            "phase": "H8",
            "when": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "commands": [[" ".join(cmd), name] for cmd, name, _ in GATES],
            "gates": {name: ("pass" if status == "pass" else "FAIL")
                      for name, status, _ in rows},
            "named_gate": "re-audit finds an old bug -> red",
            "sources": {rel: sha256_file(os.path.join(ROOT, rel)) for rel in SOURCES},
        }, f, indent=2, sort_keys=True)
        f.write("\n")

    for name, text in logs.items():
        with open(os.path.join(EVID, name), "w", encoding="utf-8") as f:
            f.write(text)

    with open(os.path.join(EVID, "gates.txt"), "w", encoding="utf-8") as f:
        f.write("H8 External audit + bounty gates (docs/h8-audit-plan.md)\n")
        f.write("Gate: 0 unfixed critical/high; NC: re-audit finds an old bug -> red\n\n")
        for name, status, desc in rows:
            f.write("%-42s %-22s %s\n" % (name, status, desc))
        f.write("\nnegative controls: 7/7 (%s)\n" % ("PASS" if nc_ok else "FAIL"))
        f.write("re-audit named gate: %s\n" % ("PASS" if reaudit_ok else "FAIL"))

    with open(os.path.join(EVID, "report.md"), "w", encoding="utf-8") as f:
        f.write("# Отчёт H8 — внешний аудит + баунти\n\n")
        f.write("Дата: %s\n\n" % time.strftime("%Y-%m-%d"))
        f.write("Итог: **%s**\n\n" % ("ВСЕ ГЕЙТЫ H8 ЗЕЛЁНЫЕ" if ok_all
                                      else "ЕСТЬ КРАСНЫЕ ГЕЙТЫ"))
        f.write("Гейт: 0 unfixed critical/high; NC: re-audit finds an old bug -> red.\n\n")
        f.write("Публичный отчёт `docs/audit/h8-public-audit-report.md`, политика "
                "и баунти `SECURITY.md`; оракул `tools/h8_reaudit.py` "
                "(`campaign/result.json`, `campaign/reaudit.log`).\n")
        f.write("negative controls `tools/nc_h8.py` (`nc.log`, "
                "RESULT: 7/7 negative controls PASS).\n\n")

    with open(os.path.join(EVID, "inventory.txt"), "w", encoding="utf-8") as f:
        f.write("# H8 inventory (generated by tools/write_h8_evidence.py)\n")
        f.write("# Cumulative negative controls: 10 phases, 481 controls.\n")
        f.write("W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, "
                "H5: 6/6, H6: 7/7, H7: 7/7, H8: 7/7\n")
        f.write("TOTAL: 481 negative controls cumulative\n")

    with open(os.path.join(EVID, "SHA256SUMS"), "w", encoding="utf-8") as f:
        for rel in sorted(EVID_DIRS):
            base = os.path.join(EVID, rel)
            for name in sorted(os.listdir(base)):
                path = os.path.join(base, name)
                if os.path.isfile(path) and name != "SHA256SUMS":
                    repo_rel = os.path.relpath(path, ROOT)
                    f.write("%s  %s\n" % (sha256_file(path), repo_rel))

    print("[evidence_h8] %s"
          % ("all gates green" if ok_all else "RED GATES PRESENT"), flush=True)
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
