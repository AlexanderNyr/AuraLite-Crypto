#!/usr/bin/env python3
"""Write the N7 baseline fixture from the library's own measurement.

The fixture is `crates/aura-p2p/tests/fixtures/n7_baseline.txt`; the gate
(`crates/aura-p2p/src/baseline.rs`) compares it with the live survey, so the
pin and the measurement cannot drift apart.

Usage:
    python3 tools/gen_n7_vectors.py
    python3 tools/gen_n7_vectors.py --check
    make gen-n7-vectors
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "crates" / "aura-p2p" / "tests" / "fixtures" / "n7_baseline.txt"

COMMAND = [
    "cargo",
    "run",
    "--quiet",
    "-p",
    "aura-p2p",
    "--bin",
    "aura-p2p",
    "--",
    "report",
    "--fixture",
]


def measure() -> str:
    result = subprocess.run(COMMAND, cwd=REPO, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise SystemExit(
            f"aura-p2p report --fixture failed: rc={result.returncode}\n{result.stderr}"
        )
    return result.stdout


def main() -> int:
    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    if not FIXTURE.exists():
        FIXTURE.write_text("", encoding="utf-8")
    fixture_text = measure()

    if "--check" in sys.argv[1:]:
        current = FIXTURE.read_text(encoding="utf-8") if FIXTURE.exists() else ""
        if current == fixture_text:
            print(
                f"{FIXTURE.relative_to(REPO)}: up to date ({len(fixture_text.splitlines())} lines)"
            )
            return 0
        print(f"DRIFT: {FIXTURE.relative_to(REPO)} does not match a live measurement")
        import difflib

        for line in difflib.unified_diff(
            current.splitlines(), fixture_text.splitlines(), "fixture", "measured", lineterm=""
        ):
            print(line)
        return 1

    FIXTURE.write_text(fixture_text, encoding="utf-8")
    print(f"wrote {FIXTURE.relative_to(REPO)} ({len(fixture_text.splitlines())} lines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
