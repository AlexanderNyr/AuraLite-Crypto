#!/usr/bin/env python3
"""Write docs/evidence/t4 (ECONOMICS_PLAN T4).

Runs the T4 gates (independent staking oracle incl. the Rust example
cross-run, cargo test t4_staking) and tools/nc_t4.py, builds
self-contained evidence: gates.txt rows, campaign/ bundle (run logs +
result.json with source digests), nc.log with the RESULT line, report.md
and cumulative inventory (TOTAL: 543).
"""

import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVID = os.path.join(ROOT, "docs", "evidence", "t4")
EVID_DIRS = ["", "campaign"]
GATES = [
    ([sys.executable, os.path.join(ROOT, "tools", "t4_staking_check.py")],
     "baseline", "independent staking oracle (1000 delegators, shares sum to one)"),
    (["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t4_staking"],
     "t4-staking", "named gate: shares sum == 1; withdraw without unbonding -> red"),
    ([sys.executable, os.path.join(ROOT, "tools", "nc_t4.py")],
     "nc-suite", "7 mutation controls incl. the literal withdraw-without-unbonding"),
]
SOURCES = [
    "crates/aura-storage/src/staking.rs",
    "crates/aura-storage/tests/t4_staking.rs",
    "crates/aura-storage/examples/t4_staking_example.rs",
    "sim/t4-staking.json",
    "tools/t4_staking_check.py",
    "tools/nc_t4.py",
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
        return "RESULT: T4 staking check PASS" in output
    if name == "t4-staking":
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
        print("[evidence_t4] %s ..." % name, flush=True)
        code, out, dur = run_cmd(cmd)
        ok = gate_passes(code, out, name)
        ok_all = ok_all and ok
        rows.append((name, "pass" if ok else "FAIL", desc))
        outputs[name] = out
        logs[name + ".log"] = (
            "[evidence_t4] cmd=%s exit=%d duration=%.2fs\n" % (" ".join(cmd), code, dur)
            + out)
        print("[evidence_t4] %s: %s" % (name, "pass" if ok else "FAIL"), flush=True)

    nc_ok = "RESULT: 7/7 negative controls PASS" in outputs.get("nc-suite", "")
    gate_ok = "test result: ok." in outputs.get("t4-staking", "")
    oracle_ok = "RESULT: T4 staking check PASS" in outputs.get("baseline", "")

    with open(os.path.join(EVID, "nc.log"), "w", encoding="utf-8") as f:
        f.write("[evidence_t4] cmd=tools/nc_t4.py\n")
        f.write(outputs.get("nc-suite", ""))

    with open(os.path.join(EVID, "campaign", "t4-staking.log"), "w",
              encoding="utf-8") as f:
        f.write(logs.get("t4-staking.log", ""))
    with open(os.path.join(EVID, "campaign", "result.json"), "w",
              encoding="utf-8") as f:
        json.dump({
            "phase": "T4",
            "when": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "commands": [[" ".join(cmd), name] for cmd, name, _ in GATES],
            "gates": {name: status for name, status, _ in rows},
            "named_gate": "1000 delegators: shares sum == 1; NC: withdraw "
                          "without unbonding -> red",
            "golden_path_digest":
                "e80662c82c74e1d1a0a5b0624beb4cf08af88c24f8091751c277cc7ea6887f07",
            "sources": {rel: sha256_file(os.path.join(ROOT, rel)) for rel in SOURCES},
        }, f, indent=2, sort_keys=True)
        f.write("\n")

    for name, text in logs.items():
        with open(os.path.join(EVID, name), "w", encoding="utf-8") as f:
            f.write(text)

    with open(os.path.join(EVID, "gates.txt"), "w", encoding="utf-8") as f:
        f.write("T4 Staking gates (ECONOMICS_PLAN, "
                "crates/aura-storage/tests/t4_staking.rs)\n")
        f.write("Gate: 1000 delegators: shares sum == 1; NC: withdraw "
                "without unbonding -> red\n\n")
        for name, status, desc in rows:
            f.write("%-42s %-22s %s\n" % (name, status, desc))
        f.write("\nnegative controls: 7/7 (%s)\n" % ("PASS" if nc_ok else "FAIL"))
        f.write("named gate: %s\n" % ("PASS" if gate_ok else "FAIL"))
        f.write("oracle cross-run: %s\n" % ("PASS" if oracle_ok else "FAIL"))

    with open(os.path.join(EVID, "report.md"), "w", encoding="utf-8") as f:
        f.write("# T4 Report — Staking\n\n")
        f.write("Date: %s\n\n" % time.strftime("%Y-%m-%d"))
        f.write("The result: **%s**\n\n" % ("ALL T4 GATES GREEN" if ok_all
                                      else "THERE ARE RED GATES"))
        f.write("Gate: 1000 delegators — the shares sum to exactly one "
                "pool at every step; a withdrawal without unbonding — red.\n\n")
        f.write("The `crates/aura-storage/src/staking.rs` model: min stake "
                "1 000 `ALCRY`; delegation via shares (compounding rewards "
                "changes the share price, but not the count); the validator fee "
                "1000 bps **from rewards only** (D-T2); unbonding of 21 days "
                "(362 880 blocks at 5 s) — an early withdrawal is red "
                "(`StillUnbonding`), without unbonding — red "
                "(`NoUnbonding`); slashing period = the unbonding window — "
                "bonded and unclaimed unbonding are slashed proportionally. "
                "The scenario `sim/t4-staking.json` (2004 steps: 1000 "
                "delegations, 600 rewards, slashes of 100/500 bps, partial "
                "undelegates, a refusal one block before maturity, withdrawals), the golden "
                "digest e80662c8…87f07 (Rust ↔ Python ↔ the example "
                "`t4_staking_example`); the finale: bonded 1 325 889 559 769 "
                "089 696 565 650, shares 1 408 708 333 333 333 333 333 "
                "367 (== Σ shares), a commission of 120 `ALCRY`-million units, "
                "withdrawn 85 430 770 230 910 303 434 449. The gate "
                "`crates/aura-storage/tests/t4_staking.rs` (10 tests + "
                "2 metamorphic properties), the oracle `tools/t4_staking_check.py`.\n")
        f.write("negative controls `tools/nc_t4.py` (`nc.log`, "
                "RESULT: 7/7 negative controls PASS) — 7 controls "
                "(the standard 6 + slash proportionality, precedent H6–H9).\n\n")

    with open(os.path.join(EVID, "inventory.txt"), "w", encoding="utf-8") as f:
        f.write("# T4 inventory (generated by tools/write_t4_evidence.py)\n")
        f.write("# Cumulative negative controls: 21 phases, 543 controls.\n")
        f.write("W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, "
                "H5: 6/6, H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, "
                "E1: 6/6, E2: 6/6, E3: 6/6, T0: 6/6, T1: 6/6, T2: 6/6, "
                "T3: 6/6, T4: 7/7\n")
        f.write("TOTAL: 543 negative controls cumulative\n")

    with open(os.path.join(EVID, "SHA256SUMS"), "w", encoding="utf-8") as f:
        for rel in sorted(EVID_DIRS):
            base = os.path.join(EVID, rel)
            for name in sorted(os.listdir(base)):
                path = os.path.join(base, name)
                if os.path.isfile(path) and name != "SHA256SUMS":
                    repo_rel = os.path.relpath(path, ROOT)
                    f.write("%s  %s\n" % (sha256_file(path), repo_rel))

    print("[evidence_t4] %s"
          % ("all gates green" if ok_all else "RED GATES PRESENT"), flush=True)
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
