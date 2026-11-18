#!/usr/bin/env python3
"""H7 two-host reproducible build gate (docs/h7-opsec-plan.md D-H7-1).

Builds the pinned artifact twice, from two independent source trees on two
simulated hosts (different absolute paths, separate target directories),
each remapping its own root to the same virtual prefix. The gate is met
when both builds produce the SAME sha256 (H0-S18: "two hosts build to the
same sha256"). `tools/nc_h7.py` NC-2 breaks the remap and the hashes must
then diverge.
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PINNED_TOOLCHAIN = "1.99.1"
ARTIFACT_PACKAGE = "aura-types"
ARTIFACT_GLOB = "libaura_types-*.rlib"
VIRTUAL_PREFIX = "/auralite-build"


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def copy_tree(dst):
    os.makedirs(dst, exist_ok=True)
    proc = subprocess.run(
        ["tar", "-C", ROOT, "--exclude=./target", "--exclude=./fuzz/target",
         "--exclude=./.git", "-cf", "-", "."],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if proc.returncode != 0:
        raise RuntimeError("tree snapshot failed: %s" % proc.stderr.decode())
    extract = subprocess.run(["tar", "-C", dst, "-xf", "-"],
                             input=proc.stdout, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE)
    if extract.returncode != 0:
        raise RuntimeError("tree extract failed: %s" % extract.stderr.decode())


def build_host(host_dir, target_dir):
    env = dict(os.environ)
    env["CARGO_BUILD_JOBS"] = "1"
    env["CARGO_TARGET_DIR"] = target_dir
    remap = "--remap-path-prefix=%s=%s" % (host_dir, VIRTUAL_PREFIX)
    env["RUSTFLAGS"] = remap
    proc = subprocess.run(
        ["cargo", "build", "--release", "--locked", "-p", ARTIFACT_PACKAGE,
         "--lib"],
        cwd=host_dir, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, errors="replace")
    if proc.returncode != 0:
        raise RuntimeError("build failed on %s:\n%s" % (host_dir, proc.stdout))
    deps = os.path.join(target_dir, "release", "deps")
    matches = [os.path.join(deps, name) for name in os.listdir(deps)
               if name.startswith("libaura_types-") and name.endswith(".rlib")]
    if len(matches) != 1:
        raise RuntimeError("expected exactly one artifact, found %r" % matches)
    return matches[0], remap


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(ROOT, "docs", "evidence", "h7",
                                                  "campaign"))
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)

    toolchain = subprocess.run(["rustc", "--version"], stdout=subprocess.PIPE,
                               text=True).stdout.strip()
    results = {"version": "h7-repro-v1", "toolchain": toolchain,
               "artifact": "%s --release --lib" % ARTIFACT_PACKAGE,
               "virtual_prefix": VIRTUAL_PREFIX, "hosts": []}

    with tempfile.TemporaryDirectory(prefix="auralite-h7-hosts-") as tmp:
        hosts = []
        for name in ("host-a", "host-b"):
            host_dir = os.path.join(tmp, name)
            target_dir = os.path.join(tmp, name + "-target")
            print("[h7_repro_build] snapshot %s ..." % name, flush=True)
            copy_tree(host_dir)
            started = time.time()
            artifact, remap = build_host(host_dir, target_dir)
            digest = sha256_file(artifact)
            results["hosts"].append({
                "name": name, "remap": remap, "artifact_sha256": digest,
                "bytes": os.path.getsize(artifact),
                "duration_seconds": round(time.time() - started, 2)})
            print("[h7_repro_build] %s sha256=%s" % (name, digest), flush=True)
            hosts.append(digest)
        shutil.rmtree(tmp, ignore_errors=True)

    same = hosts[0] == hosts[1]
    results["same_sha256"] = same
    with open(os.path.join(args.out, "repro-result.json"), "w",
              encoding="utf-8") as f:
        json.dump(results, f, indent=2)
        f.write("\n")

    if same:
        print("RESULT: H7 two-host reproducible build PASS (%s)" % hosts[0],
              flush=True)
        return 0
    print("H7: two hosts must build to the same sha256 (%s != %s)"
          % (hosts[0], hosts[1]), flush=True)
    print("RESULT: H7 two-host reproducible build FAIL", flush=True)
    return 1


if __name__ == "__main__":
    sys.exit(main())
