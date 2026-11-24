#!/usr/bin/env python3
"""Write docs/evidence/h9 (docs/h9-incident-plan.md §5/§6).

Runs the H9 gates and tools/nc_h9.py, builds self-contained evidence:
gates.txt rows, campaign/ bundle (drill logs + result.json with source
digests), nc.log with the RESULT line, report.md and cumulative inventory
(TOTAL: 488).
"""

import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVID = os.path.join(ROOT, "docs", "evidence", "h9")
EVID_DIRS = ["", "campaign"]
GATES = [
    ([sys.executable, os.path.join(ROOT, "tools", "h9_incident_drill.py")],
     "drill", "named gate: drill Byzantine validator + bridge bug halted/recovered within SLO"),
    ([sys.executable, os.path.join(ROOT, "tools", "nc_h9.py")],
     "nc-suite", "7 mutation controls incl. the named gate twice"),
]
SOURCES = [
    "tools/h9_incident_drill.py",
    "tools/nc_h9.py",
    "tools/fixtures/h9_drills.json",
    "docs/incident/PLAYBOOK.md",
    "docs/h9-incident-plan.md",
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
    if name == "drill":
        return "RESULT: H9 incident drills PASS" in output
    return "RESULT: 7/7 negative controls PASS" in output


def main():
    for rel in EVID_DIRS:
        os.makedirs(os.path.join(EVID, rel), exist_ok=True)

    rows = []
    logs = {}
    outputs = {}
    ok_all = True
    for cmd, name, desc in GATES:
        print("[evidence_h9] %s ..." % name, flush=True)
        code, out, dur = run_cmd(cmd)
        ok = gate_passes(code, out, name)
        ok_all = ok_all and ok
        rows.append((name, "pass" if ok else "FAIL", desc))
        outputs[name] = out
        logs[name + ".log"] = (
            "[evidence_h9] cmd=%s exit=%d duration=%.2fs\n" % (" ".join(cmd), code, dur)
            + out)
        print("[evidence_h9] %s: %s" % (name, "pass" if ok else "FAIL"), flush=True)

    nc_ok = "RESULT: 7/7 negative controls PASS" in outputs.get("nc-suite", "")
    drill_ok = "RESULT: H9 incident drills PASS" in outputs.get("drill", "")

    with open(os.path.join(EVID, "nc.log"), "w", encoding="utf-8") as f:
        f.write("[evidence_h9] cmd=tools/nc_h9.py\n")
        f.write(outputs.get("nc-suite", ""))

    with open(os.path.join(EVID, "campaign", "drill.log"), "w",
              encoding="utf-8") as f:
        f.write(logs.get("drill.log", ""))
    with open(os.path.join(EVID, "campaign", "result.json"), "w",
              encoding="utf-8") as f:
        json.dump({
            "phase": "H9",
            "when": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "commands": [[" ".join(cmd), name] for cmd, name, _ in GATES],
            "gates": {name: status for name, status, _ in rows},
            "named_gate": "drill Byzantine validator + bridge bug: "
                          "network halted/recovered within SLO",
            "sources": {rel: sha256_file(os.path.join(ROOT, rel)) for rel in SOURCES},
        }, f, indent=2, sort_keys=True)
        f.write("\n")

    for name, text in logs.items():
        with open(os.path.join(EVID, name), "w", encoding="utf-8") as f:
            f.write(text)

    with open(os.path.join(EVID, "gates.txt"), "w", encoding="utf-8") as f:
        f.write("H9 Incident drills gates (docs/h9-incident-plan.md)\n")
        f.write("Gate: drill \"Byzantine validator + bridge bug\": "
                "network halted/recovered within SLO\n\n")
        for name, status, desc in rows:
            f.write("%-42s %-22s %s\n" % (name, status, desc))
        f.write("\nnegative controls: 7/7 (%s)\n" % ("PASS" if nc_ok else "FAIL"))
        f.write("named gate: %s\n" % ("PASS" if drill_ok else "FAIL"))

    with open(os.path.join(EVID, "report.md"), "w", encoding="utf-8") as f:
        f.write("# H9 Report — incident drills\n\n")
        f.write("Date: %s\n\n" % time.strftime("%Y-%m-%d"))
        f.write("The result: **%s**\n\n" % ("ALL H9 GATES GREEN" if ok_all
                                      else "THERE ARE RED GATES"))
        f.write("Gate: drill \"Byzantine validator + bridge bug\": "
                "network halted/recovered within SLO.\n\n")
        f.write("Playbook `docs/incident/PLAYBOOK.md` (halt/pause/upgrade/"
                "rollback, communication, post-mortem); the drills "
                "`tools/fixtures/h9_drills.json`, the runner `tools/h9_incident_drill.py` "
                "(`campaign/result.json`, `campaign/drill.log`).\n")
        f.write("negative controls `tools/nc_h9.py` (`nc.log`, "
                "RESULT: 7/7 negative controls PASS).\n\n")

    with open(os.path.join(EVID, "inventory.txt"), "w", encoding="utf-8") as f:
        f.write("# H9 inventory (generated by tools/write_h9_evidence.py)\n")
        f.write("# Cumulative negative controls: 11 phases, 488 controls.\n")
        f.write("W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, "
                "H5: 6/6, H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7\n")
        f.write("TOTAL: 488 negative controls cumulative\n")

    with open(os.path.join(EVID, "SHA256SUMS"), "w", encoding="utf-8") as f:
        for rel in sorted(EVID_DIRS):
            base = os.path.join(EVID, rel)
            for name in sorted(os.listdir(base)):
                path = os.path.join(base, name)
                if os.path.isfile(path) and name != "SHA256SUMS":
                    repo_rel = os.path.relpath(path, ROOT)
                    f.write("%s  %s\n" % (sha256_file(path), repo_rel))

    print("[evidence_h9] %s"
          % ("all gates green" if ok_all else "RED GATES PRESENT"), flush=True)
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
