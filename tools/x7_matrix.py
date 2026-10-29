#!/usr/bin/env python3
"""Run the X7 scenario under three independent build configurations.

The DoD asks for "8 nodes × 1000 contract transactions × 3 platforms". Three
operating systems cannot be measured from one sandbox, so this tool measures the
three things that *are* different from one machine and that a cross-platform run
would vary anyway:

  1. **profile** — `debug` (opt-level 0, overflow checks on, debug assertions on)
     versus `release` (opt-level 3, no overflow checks). These are different
     machine code, and an arithmetic overflow or an optimizer-dependent iteration
     would show up here first.
  2. **codegen units** — `release` with 16 units instead of 1: a different
     partitioning of the crate, so a different inlining and merging order.
  3. **process** — every configuration is run twice in *separate* processes, so
     Rust's per-process hasher seed and the allocator history differ.

What this tool cannot do is stated in `docs/DETERMINISM.md`: the CI job
`.github/workflows/ci.yml` → `determinism` repeats the same gate on three OSes, and
that job's result is a claim only once it has run.

Usage:
    python3 tools/x7_matrix.py
    python3 tools/x7_matrix.py --nodes 8
    make x7-matrix
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

CONFIGS: list[tuple[str, list[str], dict[str, str]]] = [
    ("debug", [], {}),
    ("release", ["--release"], {}),
    (
        "release-cgu16",
        ["--release"],
        {"CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "16"},
    ),
]


def run_config(name: str, flags: list[str], env_extra: dict[str, str], nodes: int) -> dict:
    command = ["cargo", "run", "-q", "-p", "aura-exec", "--bin", "x7-run"]
    command += flags
    command += ["--", "--node", "0"]
    env = dict(os.environ)
    env.update(env_extra)
    rows = []
    for attempt in range(nodes):
        result = subprocess.run(
            command, cwd=REPO, capture_output=True, text=True, check=False, env=env
        )
        if result.returncode != 0:
            sys.stderr.write(result.stdout + result.stderr)
            raise SystemExit(f"{name}: x7-run failed")
        fields = {}
        for token in result.stdout.split():
            if token.startswith("[") or token.endswith("]"):
                continue
            if "=" in token:
                key, value = token.split("=", 1)
                fields[key] = value
        rows.append(fields)
        print(
            f"  {name:>14} run {attempt + 1}/{nodes}: "
            f"state_root={fields['state_root'][:16]}… gas={fields['gas']}",
            flush=True,
        )
    roots = {row["state_root"] for row in rows}
    gasses = {row["gas"] for row in rows}
    digests = {row["digest"] for row in rows}
    return {
        "name": name,
        "roots": roots,
        "gasses": gasses,
        "digests": digests,
        "first": rows[0],
        "runs": len(rows),
    }


def main() -> int:
    nodes = 2
    args = sys.argv[1:]
    if "--nodes" in args:
        nodes = int(args[args.index("--nodes") + 1])

    print("X7 BUILD MATRIX — the same ordered input under different builds")
    print()
    results = [run_config(name, flags, env, nodes) for name, flags, env in CONFIGS]

    print()
    print(f"{'configuration':>16}  {'runs':>4}  {'state_root':<18}  {'gas':>10}  verdict")
    failures = 0
    for result in results:
        fields = result["first"]
        ok = len(result["roots"]) == 1 and len(result["gasses"]) == 1 and len(result["digests"]) == 1
        verdict = "consistent" if ok else "DIVERGED"
        if not ok:
            failures += 1
        print(
            f"{result['name']:>16}  {result['runs']:>4}  "
            f"{fields['state_root'][:16]}…  {fields['gas']:>10}  {verdict}"
        )

    # Every configuration must also agree with every other one.
    all_roots = {root for result in results for root in result["roots"]}
    all_gasses = {gas for result in results for gas in result["gasses"]}
    all_digests = {digest for result in results for digest in result["digests"]}
    print()
    print(f"distinct state roots across all configurations: {len(all_roots)}")
    print(f"distinct gas totals across all configurations: {len(all_gasses)}")
    print(f"distinct rolling digests across all configurations: {len(all_digests)}")
    if len(all_roots) != 1 or len(all_gasses) != 1 or len(all_digests) != 1:
        print("MATRIX: DIVERGED — this is a NO-GO for mainnet")
        return 1
    print("MATRIX: OK — every configuration and every process agrees")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
