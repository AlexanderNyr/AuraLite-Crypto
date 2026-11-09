#!/usr/bin/env python3
"""Run a bounded, recorded W1 libFuzzer campaign on public test data.

This is NOT a phase receipt or audit. Each target runs sequentially with ASan.
Never enable the weak fuzz feature in production. Never run source-mutating
negative controls or edit sources during a campaign. Output goes to a NEW
scratch directory outside the checkout; existing output is never overwritten.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
TARGETS = (
    ("fuzz_keystore_parse", 8192, False),
    ("fuzz_keystore_decrypt", 128, True),
    ("fuzz_scrypt_differential", 128, False),
    ("fuzz_recovery_metadata", 8192, False),
)
NIGHTLY = "nightly-2026-11-03"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_manifest(root: Path | None = None) -> dict[str, str]:
    """Hash relevant source/fixtures/lockfiles; not a complete system image."""
    base = ROOT if root is None else root.resolve()
    files = [
        base / "Cargo.toml",
        base / "Cargo.lock",
        base / "rust-toolchain.toml",
        base / "tools/w1_fuzz_campaign.py",
        base / "tools/verify_w1_fuzz_evidence.py",
    ]
    for directory in ("crates", "third_party", "fuzz"):
        for path in (base / directory).rglob("*"):
            rel = path.relative_to(base)
            if any(part in {"target", ".git", "artifacts", "__pycache__"} for part in rel.parts):
                continue
            if path.is_file() and not path.is_symlink():
                files.append(path)
    return {path.relative_to(base).as_posix(): digest(path) for path in sorted(files)}


def tree_manifest(directory: Path) -> dict[str, str]:
    """Hash a public corpus; reject symlinks and non-regular filesystem entries."""
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError(f"not a real directory: {directory}")
    entries = {}
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"symlink in corpus: {path}")
        if path.is_file():
            entries[path.relative_to(directory).as_posix()] = digest(path)
        elif not path.is_dir():
            raise ValueError(f"non-regular corpus entry: {path}")
    return entries


def version(command: list[str]) -> str:
    return subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=True).stdout.strip()


def parse_done(output: str) -> tuple[int, int] | None:
    # Require exactly one normal libFuzzer completion line and a nonzero count;
    # a zero exit, build-only invocation, concatenated log or interrupted log fails.
    matches = re.findall(r"(?m)^Done (\d+) runs in (\d+) second\(s\)$", output)
    if len(matches) != 1 or int(matches[0][0]) < 1:
        return None
    return int(matches[0][0]), int(matches[0][1])


def save_json(path: Path, content: object) -> None:
    temp = path.with_suffix(".tmp")
    temp.write_text(json.dumps(content, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.replace(temp, path)


def run(output: Path, seconds: int, seed: int) -> int:
    if seconds < 1 or seconds > 86400:
        raise ValueError("seconds per target must be between 1 and 86400")
    if not 1 <= seed <= 2**32 - len(TARGETS):
        raise ValueError(f"seed must be in 1..={2**32 - len(TARGETS)}")
    output = output.resolve()
    if output == ROOT or ROOT in output.parents:
        raise ValueError("output must be outside the checkout")
    # Refuse any existing directory, even if empty. Never delete user data.
    output.mkdir(parents=True, exist_ok=False)
    initial = source_manifest()
    save_json(output / "source-sha256.json", initial)
    try:
        git_commit = version(["git", "rev-parse", "HEAD"])
        git_status = version(["git", "status", "--short"])
    except (OSError, subprocess.CalledProcessError):
        git_commit, git_status = None, None
    result = {
        "format_version": 2,
        "status": "running",
        "scope": "W1 bounded test-only campaign, not an audit or H3 receipt",
        "seconds_per_target": seconds,
        "seed_base": seed,
        "source_manifest": "source-sha256.json",
        "source_manifest_sha256": digest(output / "source-sha256.json"),
        "git_commit": git_commit,
        "git_status_at_start": git_status,
        "rustc": version(["rustc", "+" + NIGHTLY, "--version"]),
        "cargo_fuzz": version(["cargo", "fuzz", "--version"]),
        "targets": [],
    }
    save_json(output / "result.json", result)
    for index, (target, length, weak) in enumerate(TARGETS):
        if source_manifest() != initial:
            result["status"] = "source_changed"
            save_json(output / "result.json", result)
            return 1
        path = output / target
        path.mkdir()
        input_manifest = tree_manifest(ROOT / "fuzz/corpus" / target)
        if not input_manifest:
            raise ValueError(f"empty seed corpus: {target}")
        shutil.copytree(ROOT / "fuzz/corpus" / target, path / "initial-corpus")
        shutil.copytree(path / "initial-corpus", path / "corpus")
        if tree_manifest(path / "initial-corpus") != input_manifest or tree_manifest(path / "corpus") != input_manifest:
            raise ValueError(f"corpus copy mismatch: {target}")
        save_json(path / "initial-corpus-sha256.json", input_manifest)
        (path / "artifacts").mkdir()
        command = ["cargo", "+" + NIGHTLY, "fuzz", "run", target, "--sanitizer", "address"]
        if weak:
            command += ["--features", "insecure-wallet-fuzz"]
        command += [
            str(path / "corpus"),
            "--",
            f"-max_total_time={seconds}",
            f"-max_len={length}",
            "-timeout=30",
            "-rss_limit_mb=1200",
            f"-seed={seed + index}",
            f"-artifact_prefix={path / 'artifacts'}/",
        ]
        record = {
            "target": target,
            "status": "running",
            "command": command,
            "seed": seed + index,
            "weak_test_only_kdf": weak,
            "initial_corpus_manifest_sha256": digest(path / "initial-corpus-sha256.json"),
        }
        result["targets"].append(record)
        save_json(output / "result.json", result)
        print("RUN", shlex.join(command), flush=True)
        started = time.monotonic()
        with (path / "run.log").open("w", encoding="utf-8") as log:
            proc = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=False)
        content = (path / "run.log").read_text(encoding="utf-8", errors="replace")
        done = parse_done(content)
        artifacts = sorted(
            p.relative_to(path).as_posix()
            for p in (path / "artifacts").rglob("*")
            if p.is_symlink() or not p.is_dir()
        )
        unchanged = source_manifest() == initial
        initial_unchanged = tree_manifest(path / "initial-corpus") == input_manifest
        final_manifest = tree_manifest(path / "corpus")
        save_json(path / "corpus-sha256.json", final_manifest)
        record.update(
            {
                "exit_code": proc.returncode,
                "wall_seconds": round(time.monotonic() - started, 3),
                "log_sha256": digest(path / "run.log"),
                "artifacts": artifacts,
                "source_unchanged": unchanged,
                "initial_corpus_unchanged": initial_unchanged,
                "corpus_manifest_sha256": digest(path / "corpus-sha256.json"),
                "fuzzer_done": list(done) if done else None,
            }
        )
        # The log must reflect a full time budget, not just an early 'Done'.
        passed = (
            proc.returncode == 0
            and done is not None
            and done[1] >= seconds
            and not artifacts
            and unchanged
            and initial_unchanged
            and bool(final_manifest)
        )
        record["status"] = "completed" if passed else "failed_or_incomplete"
        save_json(output / "result.json", result)
        print(f"{target}: {record['status']} exit={proc.returncode} done={done} artifacts={len(artifacts)}", flush=True)
        if not passed:
            result["status"] = "failed_or_incomplete"
            save_json(output / "result.json", result)
            return 1
    result["status"] = "completed"
    save_json(output / "result.json", result)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seconds", type=int, required=True, help="fuzzing seconds PER TARGET")
    parser.add_argument("--seed", type=int, default=20261108)
    args = parser.parse_args()
    try:
        return run(args.output, args.seconds, args.seed)
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        print(f"campaign error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
