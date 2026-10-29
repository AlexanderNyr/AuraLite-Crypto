#!/usr/bin/env python3
"""Write the D1 skeleton fixture from the binary's own measurement.

The fixture is `crates/aura-node/tests/fixtures/d1_skeleton.txt`; the gate
(`crates/aura-node/tests/d1_skeleton.rs`) compares it with the live survey,
so the pin and the measurement cannot drift apart.

`--check` re-measures and exits non-zero on drift, without writing.

Usage:
    python3 tools/gen_d1_vectors.py
    python3 tools/gen_d1_vectors.py --check
    make gen-d1-vectors
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "crates" / "aura-node" / "tests" / "fixtures" / "d1_skeleton.txt"

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


def build_fixture(fields: dict[str, list[str]]) -> str:
    """Assemble the D1 fixture from the measured survey plus D1 specifics."""
    lines = [
        "# D1 skeleton: what exists, what a node still needs.",
        "# Written by tools/gen_d1_vectors.py; pinned by tests/d1_skeleton.rs.",
        f"phase={fields.get('phase', ['D0'])[0]}",
        f"exit_stub={fields.get('exit_stub', ['2'])[0]}",
        f"exit_usage={fields.get('exit_usage', ['64'])[0]}",
        f"exit_no_workspace={fields.get('exit_no_workspace', ['3'])[0]}",
        f"capabilities={fields.get('capabilities', ['7'])[0]}",
    ]
    # The remaining missing capabilities after D1 (config and lifecycle are done).
    for capability in fields.get("capability", []):
        lines.append(f"capability={capability}")
    lines.append("")
    return "\n".join(lines)


def parse_measured(text: str) -> dict[str, list[str]]:
    fields: dict[str, list[str]] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        fields.setdefault(key, []).append(value)
    return fields


def main() -> int:
    text = measure()
    fields = parse_measured(text)
    fixture_text = build_fixture(fields)

    if "--check" in sys.argv[1:]:
        current = FIXTURE.read_text(encoding="utf-8") if FIXTURE.exists() else ""
        if current == fixture_text:
            print(f"{FIXTURE.relative_to(REPO)}: up to date ({len(fixture_text.splitlines())} lines)")
            return 0
        print(f"DRIFT: {FIXTURE.relative_to(REPO)} does not match a live measurement")
        import difflib

        for line in difflib.unified_diff(
            current.splitlines(), fixture_text.splitlines(), "fixture", "measured", lineterm=""
        ):
            print(line)
        return 1

    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    FIXTURE.write_text(fixture_text, encoding="utf-8")
    print(f"wrote {FIXTURE.relative_to(REPO)} ({len(fixture_text.splitlines())} lines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
