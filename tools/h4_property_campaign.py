#!/usr/bin/env python3
"""H4 property campaign runner (docs/h4-property-plan.md §3, D-H4-5).

Recorded, resumable, append-only (h3 pattern). Runs the 1M-tx property
binary (`h4-properties`) with recorded command/seed/source manifests and
run.log SHA-256, then the cross-language differential in both directions
(`tools/h4_reference.py`). Schedules: `smoke` (200k txs) and `full`
(1,000,000 txs — the gate). Crash/failure artifacts are kept in <out>.
"""
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE_SEED = 20261180
SCHEDULES = {"smoke": {"txs": 200_000}, "full": {"txs": 1_000_000}}


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_manifest() -> dict:
    entries = {}
    for base in ("crates", "tools"):
        for path in sorted((ROOT / base).rglob("*.rs" if base == "crates" else "*.py")):
            if "target" in path.parts:
                continue
            entries[str(path.relative_to(ROOT))] = sha256_file(path)
    return entries


def run_one(row: dict, out: Path) -> dict:
    log_path = out / "logs" / f"{row['step']}.run.log"
    before = source_manifest()
    with open(log_path, "wb") as log:
        completed = subprocess.run(
            row["cmd"],
            cwd=ROOT,
            stdout=log,
            stderr=subprocess.STDOUT,
            timeout=row["seconds"] + 600,
        )
    after = source_manifest()
    row["exit"] = completed.returncode
    row["run_log"] = str(log_path.relative_to(out))
    row["run_log_sha256"] = sha256_file(log_path)
    row["manifest_unchanged"] = before == after
    return row


def main() -> int:
    schedule = "smoke"
    if "--schedule" in sys.argv:
        schedule = sys.argv[sys.argv.index("--schedule") + 1]
    if schedule not in SCHEDULES:
        print("usage: h4_property_campaign.py [--schedule smoke|full] [--out DIR]")
        return 64
    out = Path("/home/user/h4-campaign")
    if "--out" in sys.argv:
        out = Path(sys.argv[sys.argv.index("--out") + 1])
    (out / "logs").mkdir(parents=True, exist_ok=True)
    result_path = out / "result.json"
    state = {"schedule": schedule, "runs": []}
    if result_path.exists():
        state = json.loads(result_path.read_text())
    done = {row["step"] for row in state["runs"]}

    def save() -> None:
        result_path.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n")

    env = dict(os.environ)
    env.setdefault("CARGO_BUILD_JOBS", "1")
    failures = 0
    txs = SCHEDULES[schedule]["txs"]

    steps = [
        {
            "step": "property-1m",
            "seconds": max(txs // 20_000, 60),
            "cmd": [
                "cargo", "run", "--release", "--locked", "-p", "aura-testkit",
                "--bin", "h4-properties", "--", "--txs", str(txs), "--seed", str(BASE_SEED),
            ],
        },
        {
            "step": "rust-dump",
            "seconds": 120,
            "cmd": [
                "cargo", "run", "--release", "--locked", "-p", "aura-testkit",
                "--bin", "h4-properties", "--", "--dump-vectors", str(out / "rust_vectors.txt"),
            ],
        },
        {
            "step": "python-check-rust",
            "seconds": 300,
            "cmd": [sys.executable, "-m", "tools.h4_reference", "check", str(out / "rust_vectors.txt")],
        },
        {
            "step": "python-gen",
            "seconds": 300,
            "cmd": [sys.executable, "-m", "tools.h4_reference", "gen", "--out", str(out / "py_vectors.txt")],
        },
        {
            "step": "rust-check-python",
            "seconds": 120,
            "cmd": [
                "cargo", "run", "--release", "--locked", "-p", "aura-testkit",
                "--bin", "h4-properties", "--", "--differential", str(out / "py_vectors.txt"),
            ],
        },
    ]

    for row in steps:
        if row["step"] in done:
            continue
        row = run_one(row, out)
        if row["exit"] != 0:
            failures += 1
        state["runs"].append(row)
        done.add(row["step"])
        save()
        print(f"{row['step']}: exit={row['exit']}")

    # Extract the measured conservation line for the report.
    summary = {"schedule": schedule, "txs": txs, "failures": failures}
    log = out / "logs" / "property-1m.run.log"
    if log.exists():
        for line in log.read_text().splitlines():
            if line.startswith(("txs=", "committed=", "rejected=", "sum_checks=", "final_total=", "digest=", "RESULT:")):
                summary[line.split("=", 1)[0]] = line.split("=", 1)[-1] if "=" in line else line
    state["summary"] = summary
    save()
    print(f"campaign {schedule}: failures={failures} txs={txs}")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
