#!/usr/bin/env python3
"""H3 fuzz campaign runner (docs/h3-fuzz-plan.md §3, D-H3-3/-4).

Recorded, resumable, append-only. Every run records the exact command,
seed, source manifest (before/after), run.log SHA-256 and exit status into
<out>/result.json; crash artifacts go to <out>/crashes/. The `full`
schedule is the 24-hour budget (86,400 s over 17 runs). libFuzzer/ASan runs
need `cargo fuzz` (nightly-2026-11-03 + cargo-fuzz); without the toolchain
they are recorded as `skipped`, never as passes.
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE_SEED = 20261180
PORTABLE = ["codec", "tx", "state-key", "genesis-json", "block", "merkle-proof"]
LIBFUZZER = [
    "fuzz_codec",
    "fuzz_tx",
    "fuzz_rpc",
    "fuzz_p2p_frame",
    "fuzz_wasm_verify",
    "fuzz_keystore_parse",
    "fuzz_keystore_decrypt",
    "fuzz_state_key",
    "fuzz_genesis_json",
    "fuzz_scrypt_differential",
    "fuzz_recovery_metadata",
]
SCHEDULES = {
    # smoke: session evidence (see plan §3 deviation record).
    "smoke": {"portable": 120, "libfuzzer": 120},
    # full: D-H3-4 — 86,400 s over 17 runs (~5,082 s each).
    "full": {"portable": 5082, "libfuzzer": 5082},
}


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_manifest() -> dict:
    """Digest of every decoder source the campaign fuzzes."""
    entries = {}
    for base in ("crates", "fuzz"):
        for path in sorted((ROOT / base).rglob("*.rs")):
            if "target" in path.parts:
                continue
            entries[str(path.relative_to(ROOT))] = sha256_file(path)
    return entries


def cargo_fuzz_available() -> bool:
    if shutil.which("cargo-fuzz"):
        return True
    probe = subprocess.run(
        ["cargo", "fuzz", "--version"], cwd=ROOT, capture_output=True, text=True
    )
    return probe.returncode == 0


def run_one(row: dict, out: Path) -> dict:
    log_path = out / "logs" / f"{row['harness']}-{row['target']}.run.log"
    before = source_manifest()
    with open(log_path, "wb") as log:
        completed = subprocess.run(
            row["cmd"],
            cwd=ROOT,
            stdout=log,
            stderr=subprocess.STDOUT,
            timeout=row["seconds"] + 300,
        )
    after = source_manifest()
    row["exit"] = completed.returncode
    row["run_log"] = str(log_path.relative_to(out))
    row["run_log_sha256"] = sha256_file(log_path)
    row["manifest_unchanged"] = before == after
    return row


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--schedule", choices=sorted(SCHEDULES), default="smoke")
    parser.add_argument("--out", default="/home/user/h3-campaign")
    args = parser.parse_args()
    budget = SCHEDULES[args.schedule]
    out = Path(args.out)
    (out / "logs").mkdir(parents=True, exist_ok=True)
    (out / "crashes").mkdir(exist_ok=True)

    result_path = out / "result.json"
    state = {"schedule": args.schedule, "runs": []}
    if result_path.exists():
        state = json.loads(result_path.read_text())
    done = {(row["harness"], row["target"]) for row in state["runs"]}

    def save() -> None:
        result_path.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n")

    env = dict(os.environ)
    env.setdefault("CARGO_BUILD_JOBS", "1")

    failures = 0
    for index, target in enumerate(PORTABLE):
        key = ("portable", target)
        if key in done:
            continue
        row = {
            "harness": "portable",
            "target": target,
            "seconds": budget["portable"],
            "seed": BASE_SEED + index,
            "cmd": [
                "cargo", "run", "--locked", "-p", "aura-testkit", "--bin", "aura-fuzz",
                "--", "--seconds", str(budget["portable"]),
                "--seed", str(BASE_SEED + index), "--target", target,
            ],
        }
        row = run_one(row, out)
        if row["exit"] != 0:
            failures += 1
        state["runs"].append(row)
        done.add(key)
        save()
        print(f"portable {target}: exit={row['exit']} seed={row['seed']}")

    fuzz_ok = cargo_fuzz_available()
    for target in LIBFUZZER:
        key = ("libfuzzer", target)
        if key in done:
            continue
        if not fuzz_ok:
            row = {
                "harness": "libfuzzer", "target": target,
                "seconds": budget["libfuzzer"], "status": "skipped",
                "reason": "cargo-fuzz toolchain absent (plan §3 deviation)",
            }
        else:
            crash_dir = out / "crashes" / target
            crash_dir.mkdir(exist_ok=True)
            row = {
                "harness": "libfuzzer",
                "target": target,
                "seconds": budget["libfuzzer"],
                "cmd": [
                    "cargo", "fuzz", "run", target,
                    f"-max_total_time={budget['libfuzzer']}",
                    "-print_coverage=1",
                    f"-artifact_prefix={crash_dir}/",
                ],
            }
            row = run_one(row, out)
            if row["exit"] != 0 or any(crash_dir.iterdir()):
                row["status"] = "crash"
                failures += 1
            else:
                row["status"] = "clean"
        state["runs"].append(row)
        done.add(key)
        save()
        print(f"libfuzzer {target}: {row.get('status', 'exit=' + str(row.get('exit')))}")

    crashes = sorted(str(p) for p in (out / "crashes").rglob("*") if p.is_file())
    state["summary"] = {
        "schedule": args.schedule,
        "runs": len(state["runs"]),
        "failures": failures,
        "crash_artifacts": crashes,
    }
    save()
    print(f"campaign {args.schedule}: runs={len(state['runs'])} failures={failures} "
          f"crash_artifacts={len(crashes)}")
    return 0 if failures == 0 and not crashes else 1


if __name__ == "__main__":
    sys.exit(main())
