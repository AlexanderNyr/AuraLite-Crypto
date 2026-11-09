#!/usr/bin/env python3
"""Check integrity/consistency of version-2 local W1 fuzz evidence.

Self-consistency only (plus optional live-checkout hash comparison): all hashes
are unkeyed SHA-256, and an attacker who can rewrite both data and manifests
can forge a bundle. This is not an audit, signed receipt, independent timing
measurement, or proof that a campaign really ran.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys

from w1_fuzz_campaign import NIGHTLY, TARGETS, digest, parse_done, source_manifest, tree_manifest


def _unique_object(pairs):
    obj = {}
    for key, value in pairs:
        if key in obj:
            raise ValueError(f"duplicate JSON member: {key}")
        obj[key] = value
    return obj


def _reject_constant(token: str):
    raise ValueError(f"non-JSON constant: {token}")


def read_json(path: Path):
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"missing or unsafe file: {path}")
    return json.loads(
        path.read_text(encoding="utf-8"),
        object_pairs_hook=_unique_object,
        parse_constant=_reject_constant,
    )


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def verify(output: Path, checkout: Path | None = None) -> None:
    require(not output.is_symlink() and output.is_dir(), "invalid evidence directory")
    result = read_json(output / "result.json")
    require(isinstance(result, dict), "result.json must be an object")
    require(type(result.get("format_version")) is int and result["format_version"] == 2,
            "unsupported evidence format (requires version 2)")
    require(result.get("status") == "completed", "campaign incomplete")
    seconds = result.get("seconds_per_target")
    seed = result.get("seed_base")
    require(type(seconds) is int and 1 <= seconds <= 86400, "invalid duration")
    require(type(seed) is int and 1 <= seed <= 2**32 - len(TARGETS), "invalid seed")
    source_file = output / "source-sha256.json"
    source = read_json(source_file)
    require(isinstance(source, dict) and bool(source), "empty source manifest")
    require(
        all(isinstance(k, str) and isinstance(v, str) and re.fullmatch(r"[0-9a-f]{64}", v) for k, v in source.items()),
        "invalid source hash",
    )
    require(result.get("source_manifest") == source_file.name, "unexpected source manifest name")
    require(digest(source_file) == result.get("source_manifest_sha256"), "source manifest digest mismatch")
    if checkout is not None:
        require(not checkout.is_symlink() and checkout.is_dir(), "invalid checkout directory")
        require(source_manifest(checkout) == source, "bundle source manifest differs from live checkout")
    targets = result.get("targets")
    require(isinstance(targets, list) and len(targets) == len(TARGETS), "target count mismatch")
    for index, ((target, max_len, weak), record) in enumerate(zip(TARGETS, targets)):
        require(isinstance(record, dict), "invalid target record")
        require(record.get("target") == target, "target order mismatch")
        require(
            record.get("status") == "completed"
            and type(record.get("exit_code")) is int
            and record["exit_code"] == 0,
            f"{target}: incomplete",
        )
        require(
            type(record.get("seed")) is int
            and record["seed"] == seed + index
            and record.get("weak_test_only_kdf") is weak,
            f"{target}: settings mismatch",
        )
        command = record.get("command")
        require(isinstance(command, list) and all(isinstance(arg, str) for arg in command), f"{target}: invalid command")
        prefix = ["cargo", "+" + NIGHTLY, "fuzz", "run", target, "--sanitizer", "address"]
        if weak:
            prefix += ["--features", "insecure-wallet-fuzz"]
        require(len(command) == len(prefix) + 8 and command[:len(prefix)] == prefix, f"{target}: command prefix mismatch")
        corpus = Path(command[len(prefix)])
        artifact_arg = command[-1]
        require(corpus.is_absolute() and corpus.name == "corpus" and corpus.parent.name == target, f"{target}: corpus path mismatch")
        require(artifact_arg == f"-artifact_prefix={corpus.parent / 'artifacts'}/", f"{target}: artifact path mismatch")
        expected_tail = [
            "--",
            f"-max_total_time={seconds}",
            f"-max_len={max_len}",
            "-timeout=30",
            "-rss_limit_mb=1200",
            f"-seed={seed + index}",
            artifact_arg,
        ]
        require(command[len(prefix) + 1 :] == expected_tail, f"{target}: command options mismatch")
        require(
            record.get("source_unchanged") is True and record.get("initial_corpus_unchanged") is True,
            f"{target}: source or initial corpus drift",
        )
        path = output / target
        require(not path.is_symlink() and path.is_dir(), f"{target}: missing or unsafe directory")
        for name, key, folder in (
            ("initial-corpus-sha256.json", "initial_corpus_manifest_sha256", "initial-corpus"),
            ("corpus-sha256.json", "corpus_manifest_sha256", "corpus"),
        ):
            manifest_path = path / name
            manifest = read_json(manifest_path)
            require(isinstance(manifest, dict) and bool(manifest), f"{target}: empty {name}")
            require(digest(manifest_path) == record.get(key), f"{target}: {name} digest mismatch")
            require(tree_manifest(path / folder) == manifest, f"{target}: {folder} contents mismatch")
            if folder == "initial-corpus":
                source_prefix = f"fuzz/corpus/{target}/"
                expected = {k[len(source_prefix) :]: v for k, v in source.items() if k.startswith(source_prefix)}
                require(manifest == expected, f"{target}: initial corpus does not match source manifest")
        log = path / "run.log"
        require(not log.is_symlink() and log.is_file(), f"{target}: missing log")
        require(digest(log) == record.get("log_sha256"), f"{target}: log digest mismatch")
        done = parse_done(log.read_text(encoding="utf-8", errors="replace"))
        recorded_done = record.get("fuzzer_done")
        require(
            isinstance(recorded_done, list)
            and len(recorded_done) == 2
            and all(type(x) is int for x in recorded_done)
            and done is not None
            and list(done) == recorded_done
            and done[1] >= seconds,
            f"{target}: incomplete log",
        )
        require(record.get("artifacts") == [] and not tree_manifest(path / "artifacts"), f"{target}: artifacts present")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--check-checkout", type=Path, default=None, help="also require source-sha256.json to match this checkout")
    args = parser.parse_args()
    try:
        verify(args.output, checkout=args.check_checkout)
    except (OSError, ValueError, TypeError, AttributeError, json.JSONDecodeError) as exc:
        print(f"EVIDENCE CHECK FAILED: {exc}", file=sys.stderr)
        return 1
    suffix = " + checkout match" if args.check_checkout is not None else ""
    print(f"EVIDENCE CHECK OK: self-consistency{suffix} only; not a signed receipt or audit")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
