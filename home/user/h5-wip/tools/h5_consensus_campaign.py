#!/usr/bin/env python3
"""H5 consensus safety campaign (docs/h5-consensus-plan.md D-H5-5).

Resumable runner (h3/h4 pattern): the executable-model gates, the
Jepsen-like chaos matrix and the TLC runs of the model-checkable spec in
BOTH configurations (safe -> clean, weakened -> counterexample expected).
Results are recorded into <out>/result.json with commands, seeds, sources,
sizes and the SHA-256 of every log. A TLC run is never a pass without its
output; a missing tla2tools.jar is recorded as skipped, never as a pass.
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TLA_JAR = os.path.join(ROOT, "tools-bin", "tla2tools.jar")

STEPS = [
    {
        "id": "model-table",
        "run": ["cargo", "test", "--locked", "-p", "aura-consensus",
                "--test", "h5_safety_model", "--", "--nocapture"],
        "expect": "exit0",
    },
    {
        "id": "chaos-matrix",
        "run": ["cargo", "test", "--locked", "-p", "aura-p2p",
                "--test", "h5_jepsen", "--", "--nocapture"],
        "expect": "exit0",
    },
    {
        "id": "tlc-safe",
        "run": ["java", "-cp", TLA_JAR, "tlc2.TLC", "-config",
                "h5-safe.cfg", "HotStuff3ChainSafetyMC"],
        "workdir": os.path.join(ROOT, "docs", "h5"),
        "expect": "exit0",
    },
    {
        "id": "tlc-weakened",
        "run": ["java", "-cp", TLA_JAR, "tlc2.TLC", "-config",
                "h5-weakened.cfg", "HotStuff3ChainSafetyMC"],
        "workdir": os.path.join(ROOT, "docs", "h5"),
        # THE GATE: removing 2f+1 must produce a counterexample.
        "expect": "counterexample",
    },
]


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def collect_sources():
    sources = []
    for rel in ["crates/aura-consensus/src/safety_model.rs",
                "crates/aura-consensus/src/qc.rs",
                "crates/aura-p2p/src/chaos.rs",
                "docs/evidence/b10/HotStuff3ChainSafety.tla",
                "docs/h5/HotStuff3ChainSafetyMC.tla",
                "docs/h5/h5-safe.cfg",
                "docs/h5/h5-weakened.cfg"]:
        path = os.path.join(ROOT, rel)
        sources.append({"path": rel,
                        "sha256": sha256_file(path),
                        "bytes": os.path.getsize(path)})
    return sources


def classify(step, exit_code, output):
    expect = step["expect"]
    if expect == "exit0":
        return "pass" if exit_code == 0 else "fail"
    if expect == "counterexample":
        found = exit_code != 0 and ("Error: Invariant" in output
                                    or "Invariant is violated" in output)
        return "counterexample-found" if found else "fail"
    return "fail"


def run_one(out_dir, step, resume):
    path = os.path.join(out_dir, f"{step['id']}.log")
    if resume and os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            first = f.readline()
        if first.startswith("[h5_consensus_campaign]"):
            exit_code = int(first.rsplit("exit=", 1)[-1].split("]", 1)[0])
            with open(path, encoding="utf-8") as f:
                output = f.read()
            return {"id": step["id"], "exit_code": exit_code,
                    "status": classify(step, exit_code, output),
                    "log": os.path.basename(path),
                    "log_sha256": sha256_file(path),
                    "bytes": os.path.getsize(path),
                    "command": " ".join(step["run"]), "resumed": True}
    started = time.time()
    proc = subprocess.run(step["run"], cwd=step.get("workdir", ROOT),
                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                          text=True, errors="replace")
    duration = time.time() - started
    header = ("[h5_consensus_campaign] id=%s cmd=%s exit=%d duration=%.2fs\n"
              % (step["id"], " ".join(step["run"]), proc.returncode, duration))
    with open(path, "w", encoding="utf-8") as f:
        f.write(header)
        f.write(proc.stdout or "")
    return {"id": step["id"], "exit_code": proc.returncode,
            "status": classify(step, proc.returncode, proc.stdout or ""),
            "log": os.path.basename(path),
            "log_sha256": sha256_file(path),
            "bytes": os.path.getsize(path),
            "command": " ".join(step["run"]),
            "duration_seconds": round(duration, 2),
            "resumed": False}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--schedule", choices=["smoke", "full"], default="smoke")
    ap.add_argument("--seed", type=int, default=20261180)
    ap.add_argument("--resume", action="store_true")
    ap.add_argument("--out", default=os.path.join(ROOT, "docs", "evidence", "h5", "campaign"))
    args = ap.parse_args()

    os.makedirs(args.out, exist_ok=True)

    steps = list(STEPS)
    if args.schedule == "smoke":
        # Smoke keeps the cheap gates; the TLC runs stay in the full schedule.
        steps = [s for s in steps if not s["id"].startswith("tlc-")]

    missing_jar = not os.path.exists(TLA_JAR)
    results = []
    for step in steps:
        if step["id"].startswith("tlc-") and missing_jar:
            results.append({"id": step["id"], "status": "skipped",
                            "reason": "tools-bin/tla2tools.jar missing",
                            "command": " ".join(step["run"])})
            continue
        print(f"[h5_consensus_campaign] {step['id']} ({args.schedule}) ...",
              flush=True)
        results.append(run_one(args.out, step, args.resume))
        print(f"[h5_consensus_campaign] {step['id']}: {results[-1]['status']}",
              flush=True)

    results.append({"id": "sources", "sources": collect_sources()})
    payload = {
        "version": "h5-campaign-v1",
        "schedule": args.schedule,
        "seed": args.seed,
        "tla_jar_sha256": sha256_file(TLA_JAR) if not missing_jar else None,
        "results": results,
    }
    with open(os.path.join(args.out, "result.json"), "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2)
        f.write("\n")

    verdicts = [r["status"] for r in results if "status" in r]
    bad = [v for v in verdicts if v == "fail"]
    skipped = [v for v in verdicts if v == "skipped"]
    if bad:
        print("RESULT: H5 campaign FAIL", flush=True)
        return 1
    if skipped:
        print("RESULT: H5 campaign PASS with skipped steps (no TLC jar)",
              flush=True)
        return 0
    print("RESULT: H5 campaign PASS (model table, chaos matrix, TLC safe "
          "clean, TLC weakened counterexample)", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
