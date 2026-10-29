#!/usr/bin/env python3
"""Generate the X7 pinned fixture from a real run of the scenario.

Every line of `crates/aura-exec/tests/fixtures/x7_determinism.txt` is produced by
`cargo run -p aura-exec --bin x7-run -- --fixture`: the same binary the gate spawns
eight times, executing the same 1 004 ordered transactions. Nothing here is typed by
hand, and the file is byte-reproducible — regenerating it must not change it.

Usage:
    python3 tools/gen_x7_vectors.py
    python3 tools/gen_x7_vectors.py --check
    make gen-x7-vectors
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x7_determinism.txt"

HEADER = """\
# X7 determinism scenario — measured, not typed.
# Written by tools/gen_x7_vectors.py from `x7-run --fixture`.
#
# The scenario is a pure function of one constant (`X7_SEED`), so this file is
# the *whole* input of the gate: 4 deploys, then 1 000 transactions mixing 60%
# ABI v1 calls, 10% native transfers, 12% ABI v3 calls with a nested call, 3%
# calls to an address with no code and 2% calls that run out of gas.
#
# `expected_*` is what one node measured. The gate then replays the same input in
# eight separate processes and requires all eight to reproduce these numbers.
"""


def run() -> str:
    result = subprocess.run(
        ["cargo", "run", "-q", "-p", "aura-exec", "--bin", "x7-run", "--", "--fixture"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stdout + result.stderr)
        raise SystemExit("x7-run --fixture failed")
    return result.stdout


def main() -> int:
    check = "--check" in sys.argv[1:]
    body = run()
    text = HEADER + "\n" + body
    if not text.endswith("\n"):
        text += "\n"

    if check:
        current = FIXTURE.read_text(encoding="utf-8") if FIXTURE.exists() else ""
        if current == text:
            print(f"{FIXTURE.relative_to(REPO)}: up to date")
            return 0
        print(f"{FIXTURE.relative_to(REPO)}: DRIFT (run `make gen-x7-vectors`)")
        return 1

    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    FIXTURE.write_text(text, encoding="utf-8")
    lines = body.strip().splitlines()
    print(f"wrote {FIXTURE.relative_to(REPO)} ({len(lines)} measured lines)")
    for line in lines:
        if line.startswith(("scenario_steps", "expected_committed", "expected_gas")):
            print(f"  {line}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
