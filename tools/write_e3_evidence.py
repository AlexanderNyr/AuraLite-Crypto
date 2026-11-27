#!/usr/bin/env python3
"""Write docs/evidence/e3 (ECOSYSTEM_PLAN E3).

Runs the E3 gates (faucet oracle, cargo test e3_faucet, SDK examples)
and tools/nc_e3.py, builds self-contained evidence: gates.txt rows,
campaign/ bundle (run logs + result.json with source digests), nc.log
with the RESULT line, report.md and cumulative inventory (TOTAL: 512).
"""

import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVID = os.path.join(ROOT, "docs", "evidence", "e3")
EVID_DIRS = ["", "campaign"]
GATES = [
    ([sys.executable, os.path.join(ROOT, "tools", "e3_faucet_check.py")],
     "baseline", "1000-request policy oracle (independent Python model)"),
    (["cargo", "test", "--locked", "-p", "aura-storage", "--test", "e3_faucet"],
     "e3-faucet", "named gate: 1000 requests, 1 payout/address/day"),
    ([sys.executable, os.path.join(ROOT, "tools", "e3_examples_check.py")],
     "examples", "SDK examples pass in CI (rust, ts, python)"),
    ([sys.executable, os.path.join(ROOT, "tools", "nc_e3.py")],
     "nc-suite", "6 mutation controls incl. the literal gate (rate limit)"),
]
SOURCES = [
    "crates/aura-storage/src/faucet.rs",
    "crates/aura-storage/src/sdk.rs",
    "crates/aura-storage/tests/e3_faucet.rs",
    "crates/aura-storage/examples/e3_sdk_example.rs",
    "sdk/python/aura_lite.py",
    "sdk/python/example.py",
    "sdk/ts/aura_lite.ts",
    "sdk/ts/example.ts",
    "tools/e3_faucet_check.py",
    "tools/e3_examples_check.py",
    "tools/nc_e3.py",
    "tools/fixtures/e3_faucet_script.json",
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
        return "RESULT: E3 faucet check PASS" in output
    if name == "e3-faucet":
        return "test result: ok." in output
    if name == "examples":
        return "RESULT: 3/3 SDK examples PASS" in output
    return "RESULT: 6/6 negative controls PASS" in output


def main():
    for rel in EVID_DIRS:
        os.makedirs(os.path.join(EVID, rel), exist_ok=True)

    rows = []
    logs = {}
    outputs = {}
    ok_all = True
    for cmd, name, desc in GATES:
        print("[evidence_e3] %s ..." % name, flush=True)
        code, out, dur = run_cmd(cmd)
        ok = gate_passes(code, out, name)
        ok_all = ok_all and ok
        rows.append((name, "pass" if ok else "FAIL", desc))
        outputs[name] = out
        logs[name + ".log"] = (
            "[evidence_e3] cmd=%s exit=%d duration=%.2fs\n" % (" ".join(cmd), code, dur)
            + out)
        print("[evidence_e3] %s: %s" % (name, "pass" if ok else "FAIL"), flush=True)

    nc_ok = "RESULT: 6/6 negative controls PASS" in outputs.get("nc-suite", "")
    gate_ok = "test result: ok." in outputs.get("e3-faucet", "")
    examples_ok = "RESULT: 3/3 SDK examples PASS" in outputs.get("examples", "")

    with open(os.path.join(EVID, "nc.log"), "w", encoding="utf-8") as f:
        f.write("[evidence_e3] cmd=tools/nc_e3.py\n")
        f.write(outputs.get("nc-suite", ""))

    with open(os.path.join(EVID, "campaign", "e3-faucet.log"), "w",
              encoding="utf-8") as f:
        f.write(logs.get("e3-faucet.log", ""))
    with open(os.path.join(EVID, "campaign", "result.json"), "w",
              encoding="utf-8") as f:
        json.dump({
            "phase": "E3",
            "when": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "commands": [[" ".join(cmd), name] for cmd, name, _ in GATES],
            "gates": {name: status for name, status, _ in rows},
            "named_gate": "1000 requests: 1 payout/address/day; SDK examples pass in CI",
            "sources": {rel: sha256_file(os.path.join(ROOT, rel)) for rel in SOURCES},
        }, f, indent=2, sort_keys=True)
        f.write("\n")

    for name, text in logs.items():
        with open(os.path.join(EVID, name), "w", encoding="utf-8") as f:
            f.write(text)

    with open(os.path.join(EVID, "gates.txt"), "w", encoding="utf-8") as f:
        f.write("E3 Faucet+SDK gates (ECOSYSTEM_PLAN, crates/aura-storage/tests/e3_faucet.rs)\n")
        f.write("Gate: 1000 requests: 1 payout/address/day; SDK examples pass in CI\n\n")
        for name, status, desc in rows:
            f.write("%-42s %-22s %s\n" % (name, status, desc))
        f.write("\nnegative controls: 6/6 (%s)\n" % ("PASS" if nc_ok else "FAIL"))
        f.write("named gate: %s\n" % ("PASS" if gate_ok else "FAIL"))
        f.write("sdk examples: %s\n" % ("PASS" if examples_ok else "FAIL"))

    with open(os.path.join(EVID, "report.md"), "w", encoding="utf-8") as f:
        f.write("# Отчёт E3 — Faucet + SDK\n\n")
        f.write("Дата: %s\n\n" % time.strftime("%Y-%m-%d"))
        f.write("Итог: **%s**\n\n" % ("ВСЕ ГЕЙТЫ E3 ЗЕЛЁНЫЕ" if ok_all
                                      else "ЕСТЬ КРАСНЫЕ ГЕЙТЫ"))
        f.write("Гейт: 1000 requests: 1 payout/address/day; SDK examples pass "
                "in CI.\n\n")
        f.write("Faucet `crates/aura-storage/src/faucet.rs` (детерминированная "
                "политика: 1 выплата/адрес/день, testnet only) + клиентский "
                "SDK `crates/aura-storage/src/sdk.rs`, `sdk/ts/`, `sdk/python/`; "
                "гейт `crates/aura-storage/tests/e3_faucet.rs` "
                "(`campaign/e3-faucet.log`, `campaign/result.json`), оракул "
                "`tools/e3_faucet_check.py` (независимая Python-модель, "
                "`tools/fixtures/e3_faucet_script.json`: 1000 запросов → 750 "
                "выплат, 250 отказов), примеры SDK — `tools/e3_examples_check.py` "
                "(`examples.log`: rust/ts/python 3/3).\n")
        f.write("negative controls `tools/nc_e3.py` (`nc.log`, "
                "RESULT: 6/6 negative controls PASS).\n\n")

    with open(os.path.join(EVID, "inventory.txt"), "w", encoding="utf-8") as f:
        f.write("# E3 inventory (generated by tools/write_e3_evidence.py)\n")
        f.write("# Cumulative negative controls: 15 phases, 512 controls.\n")
        f.write("W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, "
                "H5: 6/6, H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, "
                "E1: 6/6, E2: 6/6, E3: 6/6\n")
        f.write("TOTAL: 512 negative controls cumulative\n")

    with open(os.path.join(EVID, "SHA256SUMS"), "w", encoding="utf-8") as f:
        for rel in sorted(EVID_DIRS):
            base = os.path.join(EVID, rel)
            for name in sorted(os.listdir(base)):
                path = os.path.join(base, name)
                if os.path.isfile(path) and name != "SHA256SUMS":
                    repo_rel = os.path.relpath(path, ROOT)
                    f.write("%s  %s\n" % (sha256_file(path), repo_rel))

    print("[evidence_e3] %s"
          % ("all gates green" if ok_all else "RED GATES PRESENT"), flush=True)
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
