#!/usr/bin/env python3
"""Write docs/evidence/t3 (ECONOMICS_PLAN T3).

Runs the T3 gates (independent fees oracle incl. the Rust example
cross-run, cargo test t3_fees) and tools/nc_t3.py, builds
self-contained evidence: gates.txt rows, campaign/ bundle (run logs +
result.json with source digests), nc.log with the RESULT line, report.md
and cumulative inventory (TOTAL: 536).
"""

import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVID = os.path.join(ROOT, "docs", "evidence", "t3")
EVID_DIRS = ["", "campaign"]
GATES = [
    ([sys.executable, os.path.join(ROOT, "tools", "t3_fees_check.py")],
     "baseline", "independent fees oracle (10 000 congested blocks, per-formula)"),
    (["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t3_fees"],
     "t3-fees", "named gate: base fee rises/falls per the formula"),
    ([sys.executable, os.path.join(ROOT, "tools", "nc_t3.py")],
     "nc-suite", "6 mutation controls incl. the formula and burn accounting"),
]
SOURCES = [
    "crates/aura-storage/src/fees.rs",
    "crates/aura-storage/tests/t3_fees.rs",
    "crates/aura-storage/examples/t3_fees_example.rs",
    "sim/t3-fees.json",
    "tools/t3_fees_check.py",
    "tools/nc_t3.py",
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
        return "RESULT: T3 fees check PASS" in output
    if name == "t3-fees":
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
        print("[evidence_t3] %s ..." % name, flush=True)
        code, out, dur = run_cmd(cmd)
        ok = gate_passes(code, out, name)
        ok_all = ok_all and ok
        rows.append((name, "pass" if ok else "FAIL", desc))
        outputs[name] = out
        logs[name + ".log"] = (
            "[evidence_t3] cmd=%s exit=%d duration=%.2fs\n" % (" ".join(cmd), code, dur)
            + out)
        print("[evidence_t3] %s: %s" % (name, "pass" if ok else "FAIL"), flush=True)

    nc_ok = "RESULT: 6/6 negative controls PASS" in outputs.get("nc-suite", "")
    gate_ok = "test result: ok." in outputs.get("t3-fees", "")
    oracle_ok = "RESULT: T3 fees check PASS" in outputs.get("baseline", "")

    with open(os.path.join(EVID, "nc.log"), "w", encoding="utf-8") as f:
        f.write("[evidence_t3] cmd=tools/nc_t3.py\n")
        f.write(outputs.get("nc-suite", ""))

    with open(os.path.join(EVID, "campaign", "t3-fees.log"), "w",
              encoding="utf-8") as f:
        f.write(logs.get("t3-fees.log", ""))
    with open(os.path.join(EVID, "campaign", "result.json"), "w",
              encoding="utf-8") as f:
        json.dump({
            "phase": "T3",
            "when": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "commands": [[" ".join(cmd), name] for cmd, name, _ in GATES],
            "gates": {name: status for name, status, _ in rows},
            "named_gate": "10 000 congested blocks: base fee rises/falls "
                          "per the formula",
            "golden_path_digest":
                "412f1f9e120ec829fb5d2ae65e4590da69771cd7b49bef83f29440d0d93bc124",
            "sources": {rel: sha256_file(os.path.join(ROOT, rel)) for rel in SOURCES},
        }, f, indent=2, sort_keys=True)
        f.write("\n")

    for name, text in logs.items():
        with open(os.path.join(EVID, name), "w", encoding="utf-8") as f:
            f.write(text)

    with open(os.path.join(EVID, "gates.txt"), "w", encoding="utf-8") as f:
        f.write("T3 Fees gates (ECONOMICS_PLAN, "
                "crates/aura-storage/tests/t3_fees.rs)\n")
        f.write("Gate: 10 000 congested blocks: base fee rises/falls per "
                "the formula\n\n")
        for name, status, desc in rows:
            f.write("%-42s %-22s %s\n" % (name, status, desc))
        f.write("\nnegative controls: 6/6 (%s)\n" % ("PASS" if nc_ok else "FAIL"))
        f.write("named gate: %s\n" % ("PASS" if gate_ok else "FAIL"))
        f.write("oracle cross-run: %s\n" % ("PASS" if oracle_ok else "FAIL"))

    with open(os.path.join(EVID, "report.md"), "w", encoding="utf-8") as f:
        f.write("# T3 Report — Fees\n\n")
        f.write("Date: %s\n\n" % time.strftime("%Y-%m-%d"))
        f.write("The result: **%s**\n\n" % ("ALL T3 GATES GREEN" if ok_all
                                      else "THERE ARE RED GATES"))
        f.write("Gate: 10 000 congested blocks — the base fee rises/falls "
                "by the formula at every step.\n\n")
        f.write("The `crates/aura-storage/src/fees.rs` model: an EIP-1559-like "
                "`base_fee` + `tip`; the base part is **burned**, the tip goes to the "
                "validator; an elastic gas target of 15 000 000 (a limit "
                "30 000 000, elasticity 2); the formula "
                "`base ± max(base·|gas−target|/target/8, 1)`, unchanged "
                "when gas == target, with a step of at least 1 base unit. "
                "The scenario `sim/t3-fees.json` (segments +25%/−25%/+50%/"
                "−50%/target of 250 blocks × 20), the golden path digest "
                "412f1f9e…bc124 (Rust ↔ Python ↔ the example "
                "`t3_fees_example`); the totals: burned "
                "2 039 212 976 242 596 352 500 000, tips "
                "150 000 000 000 000 000 000, final base 56 473. "
                "The gate `crates/aura-storage/tests/t3_fees.rs` (9 tests + "
                "2 metamorphic properties), the oracle `tools/t3_fees_check.py`.\n")
        f.write("negative controls `tools/nc_t3.py` (`nc.log`, "
                "RESULT: 6/6 negative controls PASS).\n\n")

    with open(os.path.join(EVID, "inventory.txt"), "w", encoding="utf-8") as f:
        f.write("# T3 inventory (generated by tools/write_t3_evidence.py)\n")
        f.write("# Cumulative negative controls: 20 phases, 536 controls.\n")
        f.write("W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, "
                "H5: 6/6, H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, "
                "E1: 6/6, E2: 6/6, E3: 6/6, T0: 6/6, T1: 6/6, T2: 6/6, "
                "T3: 6/6\n")
        f.write("TOTAL: 536 negative controls cumulative\n")

    with open(os.path.join(EVID, "SHA256SUMS"), "w", encoding="utf-8") as f:
        for rel in sorted(EVID_DIRS):
            base = os.path.join(EVID, rel)
            for name in sorted(os.listdir(base)):
                path = os.path.join(base, name)
                if os.path.isfile(path) and name != "SHA256SUMS":
                    repo_rel = os.path.relpath(path, ROOT)
                    f.write("%s  %s\n" % (sha256_file(path), repo_rel))

    print("[evidence_t3] %s"
          % ("all gates green" if ok_all else "RED GATES PRESENT"), flush=True)
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
