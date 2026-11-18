#!/usr/bin/env python3
"""H6 economic-attack campaign (docs/h6-economic-plan.md).

Resumable runner (h3-h5 pattern): the deterministic attack simulations
(MEV/censorship bounds, stake grinding, 34% emission conservation,
oracle/bridge modelled surfaces, emergency-pause gate). Results are
recorded into <out>/result.json with commands, seeds, sources, sizes and
the SHA-256 of every log. Nothing is a pass without its output.
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

STEPS = [
    {
        "id": "economic-sims",
        "run": ["cargo", "test", "--locked", "-p", "aura-consensus",
                "--test", "h6_economic", "--", "--nocapture"],
        "expect": "exit0",
    },
    {
        "id": "mev-ordering",
        "run": ["cargo", "test", "--locked", "-p", "aura-mempool",
                "--test", "h6_mev", "--", "--nocapture"],
        "expect": "exit0",
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
    for rel in ["crates/aura-consensus/src/governance.rs",
                "crates/aura-consensus/src/rewards.rs",
                "crates/aura-consensus/src/slashing.rs",
                "crates/aura-consensus/src/proposer.rs",
                "crates/aura-consensus/src/vrf_leader.rs",
                "crates/aura-mempool/src/lib.rs",
                "crates/aura-consensus/tests/h6_economic.rs",
                "crates/aura-mempool/tests/h6_mev.rs"]:
        path = os.path.join(ROOT, rel)
        sources.append({"path": rel,
                        "sha256": sha256_file(path),
                        "bytes": os.path.getsize(path)})
    return sources


def run_one(out_dir, step, resume):
    path = os.path.join(out_dir, f"{step['id']}.log")
    if resume and os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            first = f.readline()
        if first.startswith("[h6_economic_campaign]"):
            exit_code = int(first.rsplit("exit=", 1)[-1].split("]", 1)[0])
            with open(path, encoding="utf-8") as f:
                output = f.read()
            return {"id": step["id"], "exit_code": exit_code,
                    "status": "pass" if exit_code == 0 else "fail",
                    "log": os.path.basename(path),
                    "log_sha256": sha256_file(path),
                    "bytes": os.path.getsize(path),
                    "command": " ".join(step["run"]), "resumed": True}
    started = time.time()
    proc = subprocess.run(step["run"], cwd=ROOT,
                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                          text=True, errors="replace")
    duration = time.time() - started
    header = ("[h6_economic_campaign] id=%s cmd=%s exit=%d duration=%.2fs\n"
              % (step["id"], " ".join(step["run"]), proc.returncode, duration))
    with open(path, "w", encoding="utf-8") as f:
        f.write(header)
        f.write(proc.stdout or "")
    return {"id": step["id"], "exit_code": proc.returncode,
            "status": "pass" if proc.returncode == 0 else "fail",
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
    ap.add_argument("--out", default=os.path.join(ROOT, "docs", "evidence", "h6", "campaign"))
    args = ap.parse_args()

    os.makedirs(args.out, exist_ok=True)

    results = []
    for step in STEPS:
        print(f"[h6_economic_campaign] {step['id']} ({args.schedule}) ...",
              flush=True)
        results.append(run_one(args.out, step, args.resume))
        print(f"[h6_economic_campaign] {step['id']}: {results[-1]['status']}",
              flush=True)

    results.append({"id": "sources", "sources": collect_sources()})
    payload = {
        "version": "h6-campaign-v1",
        "schedule": args.schedule,
        "seed": args.seed,
        "results": results,
    }
    with open(os.path.join(args.out, "result.json"), "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2)
        f.write("\n")

    if any(r.get("status") == "fail" for r in results):
        print("RESULT: H6 campaign FAIL", flush=True)
        return 1
    print("RESULT: H6 campaign PASS (economic sims, MEV ordering)", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
