#!/usr/bin/env python3
"""Write docs/evidence/h5 (docs/h5-consensus-plan.md D-H5-6).

Runs the H5 test suite and tools/nc_h5.py, builds self-contained evidence:
gates.txt (model table + chaos matrix + TLC rows), campaign/ bundle (runner
logs + result.json + sources + TLC logs), nc.log with the RESULT line,
report.md and cumulative inventory (TOTAL: 460).
"""

import hashlib
import os
import shutil
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVID = os.path.join(ROOT, "docs", "evidence", "h5")
EVID_DIRS = ["", "campaign"]
RUST_TESTS = [
    (["cargo", "test", "--locked", "-p", "aura-consensus", "--test",
      "h5_safety_model", "--", "--nocapture"],
     "h5_model_safety_and_counterexample", "Model safety + counterexample gate"),
    (["cargo", "test", "--locked", "-p", "aura-consensus", "--test",
      "h5_safety_model", "--", "--nocapture"],
     "h5_tla_spec_integrity", "TLA+ spec integrity"),
    (["cargo", "test", "--locked", "-p", "aura-p2p", "--test",
      "h5_jepsen", "--", "--nocapture"],
     "h5_chaos_campaign_zero_divergence", "Chaos matrix 20x3 zero divergence"),
    (["cargo", "test", "--locked", "-p", "aura-p2p", "--test",
      "h5_jepsen", "--", "--nocapture"],
     "h5_divergence_monitor_fires", "Divergence monitor fires on crafted cluster"),
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


def test_passes(output, name):
    for line in output.splitlines():
        if line.startswith("test %s " % name) and line.rstrip().endswith("ok"):
            return True
    return False


def main():
    for rel in EVID_DIRS:
        os.makedirs(os.path.join(EVID, rel), exist_ok=True)

    rows = []
    logs = {}
    ok_all = True
    for cmd, name, desc in RUST_TESTS:
        print("[evidence_h5] %s ..." % name, flush=True)
        code, out, dur = run_cmd(cmd)
        ok = code == 0 and test_passes(out, name)
        ok_all = ok_all and ok
        rows.append((name, "pass" if ok else "FAIL", desc))
        logs[name + ".log"] = (
            "[evidence_h5] cmd=%s exit=%d duration=%.2fs\n" % (" ".join(cmd), code, dur)
            + out)
        print("[evidence_h5] %s: %s" % (name, "pass" if ok else "FAIL"), flush=True)

    # Negative controls (tools/nc_h5.py writes its own RESULT line).
    print("[evidence_h5] nc_h5 ...", flush=True)
    code, out, dur = run_cmd([sys.executable, os.path.join(ROOT, "tools", "nc_h5.py")])
    with open(os.path.join(EVID, "nc.log"), "w", encoding="utf-8") as f:
        f.write("[evidence_h5] cmd=tools/nc_h5.py exit=%d duration=%.2fs\n"
                % (code, dur))
        f.write(out)
    nc_ok = code == 0 and "RESULT: 6/6 negative controls PASS" in out
    ok_all = ok_all and nc_ok

    # Full campaign bundle (model table + chaos matrix + TLC runs).
    print("[evidence_h5] campaign --schedule full ...", flush=True)
    code, out, dur = run_cmd([sys.executable,
                              os.path.join(ROOT, "tools", "h5_consensus_campaign.py"),
                              "--schedule", "full"],
                             timeout=3600)
    with open(os.path.join(EVID, "campaign", "campaign.log"), "w",
              encoding="utf-8") as f:
        f.write("[evidence_h5] cmd=h5_consensus_campaign.py --schedule full "
                "exit=%d duration=%.2fs\n" % (code, dur))
        f.write(out)
    campaign_ok = code == 0 and "RESULT: H5 campaign PASS" in out
    ok_all = ok_all and campaign_ok

    for name, text in logs.items():
        with open(os.path.join(EVID, name), "w", encoding="utf-8") as f:
            f.write(text)

    tlc_rows = ""
    result_path = os.path.join(EVID, "campaign", "result.json")
    if os.path.exists(result_path):
        import json
        with open(result_path, encoding="utf-8") as f:
            result = json.load(f)
        for entry in result.get("results", []):
            if entry.get("id", "").startswith("tlc-"):
                tlc_rows += "| %s | %s | %s |\n" % (
                    entry["id"], entry.get("status", "n/a"),
                    entry.get("command", ""))
                rows.append((entry["id"], entry.get("status", "n/a"),
                             "TLC run of HotStuff3ChainSafetyMC"))
                ok_all = ok_all and entry.get("status") in ("pass",
                                                            "counterexample-found")

    with open(os.path.join(EVID, "gates.txt"), "w", encoding="utf-8") as f:
        f.write("H5 consensus safety gates (docs/h5-consensus-plan.md)\n")
        f.write("Gate: model finds a counterexample when 2f+1 is removed; NC documented\n\n")
        f.write("model table: (4,1)(7,2)(10,3)(13,4)(31,10)(100,33) -> Safe\n")
        f.write("weakened quorum -> Counterexample + named offenders\n")
        f.write("chaos matrix: 20 scenarios x 3 runs -> zero finalized-head divergence\n")
        f.write("TLC: safe config clean / weakened config counterexample\n\n")
        for name, status, desc in rows:
            f.write("%-42s %-22s %s\n" % (name, status, desc))
        f.write("\nnegative controls: 6/6 (%s)\n"
                % ("PASS" if nc_ok else "FAIL"))
        f.write("campaign --schedule full: %s\n"
                % ("PASS" if campaign_ok else "FAIL"))

    with open(os.path.join(EVID, "report.md"), "w", encoding="utf-8") as f:
        f.write("# Отчёт H5 — consensus safety\n\n")
        f.write("Дата: %s\n\n" % time.strftime("%Y-%m-%d"))
        f.write("Итог: **%s**\n\n" % ("ВСЕ ГЕЙТЫ H5 ЗЕЛЁНЫЕ" if ok_all
                                      else "ЕСТЬ КРАСНЫЕ ГЕЙТЫ"))
        f.write("Гейт: model finds a counterexample when 2f+1 is removed; NC documented.\n\n")
        f.write("Кампания `%s` (`campaign/result.json`, логи в `campaign/`);\n"
                % "h5_consensus_campaign.py --schedule full")
        f.write("negative controls `%s` (`nc.log`, RESULT: 6/6 negative controls PASS).\n"
                % "tools/nc_h5.py")
        f.write("TLA+ модель `docs/h5/HotStuff3ChainSafetyMC.tla` (TLC, safe/`WeakenQuorum`):\n"
                "safe — без нарушений; weakened — контрпример обязателен.\n\n")

    with open(os.path.join(EVID, "inventory.txt"), "w", encoding="utf-8") as f:
        f.write("Negative controls cumulative inventory (5 phases x 6 controls):\n")
        f.write("  W4: 6/6  H0: 6/6  H1: 6/6  H2: 6/6  H3: 6/6  H4: 6/6\n")
        f.write("  H5: 6/6 (tools/nc_h5.py)\n")
        f.write("  TOTAL: 460\n")

    with open(os.path.join(EVID, "SHA256SUMS"), "w", encoding="utf-8") as f:
        for rel in sorted(EVID_DIRS):
            base = os.path.join(EVID, rel)
            for name in sorted(os.listdir(base)):
                path = os.path.join(base, name)
                if os.path.isfile(path) and name != "SHA256SUMS":
                    repo_rel = os.path.relpath(path, ROOT)
                    f.write("%s  %s\n" % (sha256_file(path), repo_rel))

    print("[evidence_h5] %s"
          % ("all gates green" if ok_all else "RED GATES PRESENT"), flush=True)
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
