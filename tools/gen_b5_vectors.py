#!/usr/bin/env python3
"""Write the pinned B5 consensus-baseline fixture from the binary's own measurement.

The fixture is not hand-written: `aura-consensus report --fixture` measures the
workspace and prints the `key=value` lines. `--check` compares the live output
to the committed file without touching it.

Usage:
    python3 tools/gen_b5_vectors.py
    python3 tools/gen_b5_vectors.py --check
    make gen-b5-vectors
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "crates" / "aura-consensus" / "tests" / "fixtures" / "b5_baseline.txt"


def measure() -> str:
    result = subprocess.run(
        ["cargo", "run", "--quiet", "-p", "aura-consensus", "--bin", "aura-consensus", "--", "report", "--fixture"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        sys.stderr.write(f"aura-consensus report --fixture failed: rc={result.returncode}\n")
        sys.stderr.write(result.stderr)
        sys.exit(1)
    return result.stdout


def main() -> int:
    text = measure()
    if "--check" in sys.argv[1:]:
        if not FIXTURE.exists():
            print(f"MISSING: {FIXTURE.relative_to(REPO)}")
            return 1
        committed = FIXTURE.read_text(encoding="utf-8")
        if committed != text:
            print(f"DRIFT in {FIXTURE.relative_to(REPO)} (run `make gen-b5-vectors`)")
            return 1
        print(f"B5 CONSENSUS BASELINE FIXTURE: OK ({FIXTURE.relative_to(REPO)})")
        return 0

    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    FIXTURE.write_text(text, encoding="utf-8")
    lines = len(text.splitlines())
    print(f"wrote {FIXTURE.relative_to(REPO)} ({lines} lines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
