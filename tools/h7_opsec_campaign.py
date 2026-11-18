#!/usr/bin/env python3
"""H7 OpSec / reproducible-build campaign (docs/h7-opsec-plan.md).

Resumable runner (h3-h6 pattern): the OpSec gates (secrets, CI lint,
toolchain pin), the validator-key hygiene tests and the two-host
reproducible build. Results are recorded into <out>/result.json with
commands, sources, sizes and the SHA-256 of every log. Nothing is a pass
without its output.
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
        "id": "opsec-checks",
        "run": [sys.executable, os.path.join(ROOT, "tools", "check_opsec.py")],
        "expect": "exit0",
    },
    {
        "id": "key-hygiene",
        "run": ["cargo", "test", "--locked", "-p", "aura-wallet",
                "--features", "insecure-test-kdf", "--test", "h7_opsec",
                "--", "--nocapture"],
        "expect": "exit0",
    },
    {
        "id": "two-host-repro",
        "run": [sys.executable, os.path.join(ROOT, "tools", "h7_repro_build.py")],
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
    for rel in ["crates/aura-wallet/src/keystore.rs",
                "crates/aura-wallet/tests/h7_opsec.rs",
                "tools/check_opsec.py",
                "tools/scan_secrets.py",
                "tools/h7_repro_build.py",
                "rust-toolchain.toml",
                ".github/workflows/ci.yml",
                ".github/workflows/w1.yml"]:
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
        if first.startswith("[h7_opsec_campaign]"):
            exit_code = int(first.rsplit("exit=", 1)[-1].split("]", 1)[0])
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
    header = ("[h7_opsec_campaign] id=%s cmd=%s exit=%d duration=%.2fs\n"
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
    ap.add_argument("--out", default=os.path.join(ROOT, "docs", "evidence", "h7", "campaign"))
    args = ap.parse_args()

    os.makedirs(args.out, exist_ok=True)

    results = []
    for step in STEPS:
        print(f"[h7_opsec_campaign] {step['id']} ({args.schedule}) ...", flush=True)
        results.append(run_one(args.out, step, args.resume))
        print(f"[h7_opsec_campaign] {step['id']}: {results[-1]['status']}", flush=True)

    results.append({"id": "sources", "sources": collect_sources()})
    payload = {
        "version": "h7-campaign-v1",
        "schedule": args.schedule,
        "seed": args.seed,
        "results": results,
    }
    with open(os.path.join(args.out, "result.json"), "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2)
        f.write("\n")

    if any(r.get("status") == "fail" for r in results):
        print("RESULT: H7 campaign FAIL", flush=True)
        return 1
    print("RESULT: H7 campaign PASS (opsec checks, key hygiene, two-host repro)",
          flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
