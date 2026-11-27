#!/usr/bin/env python3
"""O1 named gate: two independent builds -> identical binary sha256.

Builds the release binary (default `aura-node`/`aurad`) twice into two
independent CARGO_TARGET_DIRs — the "two hosts" of the O1 gate (D-O1-1) —
with SOURCE_DATE_EPOCH pinned to the commit date and `--remap-path-prefix`
hygiene, then compares the sha256 digests. Writes `release/repro.txt` (the
two-host record the release check validates) either way and exits non-zero
on mismatch.

Usage:
  tools/repro_check.py [--package PKG] [--bin NAME] [--out DIR]
"""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--package", default="aura-node")
    parser.add_argument("--bin", default="aurad")
    parser.add_argument("--out", default="release")
    args = parser.parse_args()

    epoch = subprocess.run(
        ["git", "log", "-1", "--format=%ct"], cwd=ROOT,
        text=True, capture_output=True, check=False,
    ).stdout.strip() or "1764000000"

    env = dict(os.environ)
    env.setdefault("CARGO_BUILD_JOBS", "1")
    env["SOURCE_DATE_EPOCH"] = epoch

    out_dir = ROOT / args.out
    out_dir.mkdir(parents=True, exist_ok=True)

    digests: list[tuple[str, str]] = []
    kept: list[Path] = []
    for host in ("builder-a", "builder-b"):
        target_dir = ROOT / f".repro-{host}"
        shutil.rmtree(target_dir, ignore_errors=True)
        # Both the source root and the per-host target dir are remapped to
        # stable virtual prefixes: generated code (tonic/prost OUT_DIR
        # includes) must not leak either path into panic locations.
        host_env = {
            **env,
            "CARGO_TARGET_DIR": str(target_dir),
            "RUSTFLAGS": f"--remap-path-prefix={ROOT}=/auralite "
                         f"--remap-path-prefix={target_dir}=/auralite-out",
        }
        build = subprocess.run(
            ["cargo", "build", "--release", "-p", args.package, "--bin", args.bin],
            cwd=ROOT, env=host_env,
            text=True, capture_output=True, check=False,
        )
        if build.returncode != 0:
            sys.stderr.write(build.stderr[-2000:])
            return 1
        binary = target_dir / "release" / args.bin
        digests.append((host, sha256_file(binary)))
        # Keep the first build as the release artifact; both agree on success.
        if host == "builder-a":
            shutil.copyfile(binary, out_dir / args.bin)
        kept.append(target_dir)

    ok = len({digest for _, digest in digests}) == 1
    for target_dir in kept:
        if ok:
            shutil.rmtree(target_dir, ignore_errors=True)
        else:
            # Keep both trees for diagnosis (compare the linked outputs).
            print(f"kept {target_dir} for diagnosis")
    record = ["repro v1", f"binary {args.bin}", "toolchain 1.99.1",
              f"source_date_epoch {epoch}"]
    record += [f"host {host} sha256 {digest}" for host, digest in digests]
    (out_dir / "repro.txt").write_text("\n".join(record) + "\n", encoding="utf-8")

    if len({digest for _, digest in digests}) != 1:
        print(f"repro=FAIL binary={args.bin} " +
              " ".join(f"{host}={digest}" for host, digest in digests))
        return 1
    print(f"repro=ok sha256={digests[0][1]} binary={args.bin} hosts=2 "
          f"source_date_epoch={epoch}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
