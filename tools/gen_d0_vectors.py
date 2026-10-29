#!/usr/bin/env python3
"""Write the D0 baseline fixture from the binary's own measurement.

The fixture is `crates/aura-node/tests/fixtures/d0_baseline.txt`; the gate
(`crates/aura-node/tests/d0_baseline.rs`) compares it byte-for-byte with
`aurad report --fixture`, so the pin and the measurement cannot drift apart.

`--check` re-measures and exits non-zero on drift, without writing.

Usage:
    python3 tools/gen_d0_vectors.py
    python3 tools/gen_d0_vectors.py --check
    make gen-d0-vectors
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "crates" / "aura-node" / "tests" / "fixtures" / "d0_baseline.txt"

COMMAND = [
    "cargo",
    "run",
    "--quiet",
    "-p",
    "aura-node",
    "--bin",
    "aurad",
    "--",
    "report",
    "--fixture",
]


def measure() -> str:
    result = subprocess.run(COMMAND, cwd=REPO, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise SystemExit(f"aurad report --fixture failed: rc={result.returncode}\n{result.stderr}")
    return result.stdout


def main() -> int:
    text = measure()
    if "--check" in sys.argv[1:]:
        current = FIXTURE.read_text(encoding="utf-8") if FIXTURE.exists() else ""
        if current == text:
            print(f"{FIXTURE.relative_to(REPO)}: up to date ({len(text.splitlines())} lines)")
            return 0
        print(f"DRIFT: {FIXTURE.relative_to(REPO)} does not match a live measurement")
        import difflib

        for line in difflib.unified_diff(
            current.splitlines(), text.splitlines(), "fixture", "measured", lineterm=""
        ):
            print(line)
        return 1
    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    FIXTURE.write_text(text, encoding="utf-8")
    print(f"wrote {FIXTURE.relative_to(REPO)} ({len(text.splitlines())} lines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
