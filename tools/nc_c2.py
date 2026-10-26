#!/usr/bin/env python3
"""Negative controls for C2 (aura-codec).

Four tamper tests must each turn the C2 gate red:

  NC-1: CODEC_VERSION 1 -> 2 (golden vectors are version 1).
  NC-2: corrupt one hex nibble in the independent golden fixture.
  NC-3: disable Reader::finish() trailing-byte guard.
  NC-4: disable the non-minimal varint predicate.

Files are always restored in a `finally` block. `make nc-c2` captures stdout
into docs/evidence/c2/nc.log, and a final green restore check is mandatory.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
LIB_RS = REPO / "crates" / "aura-codec" / "src" / "lib.rs"
CODEC_RS = REPO / "crates" / "aura-codec" / "src" / "codec.rs"
TEST_RS = REPO / "crates" / "aura-codec" / "tests" / "c2_codec.rs"
FIXTURE = REPO / "crates" / "aura-codec" / "tests" / "fixtures" / "c2_golden.txt"


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str, touch_test: bool = False) -> bool:
    original = path.read_text(encoding="utf-8")
    if old not in original:
        print(f"{label}: FAIL — tamper target not found in {path.name}")
        return False
    tampered = original.replace(old, new, 1)
    if tampered == original:
        print(f"{label}: FAIL — tamper was a no-op")
        return False
    try:
        path.write_text(tampered, encoding="utf-8")
        if touch_test:
            # Force rustc to re-read include_str!(...) after the fixture changes.
            os.utime(TEST_RS, None)
        result = run(["cargo", "test", "-p", "aura-codec", "--quiet"])
        went_red = result.returncode != 0
        snippet = "\n".join((result.stdout + result.stderr).strip().splitlines()[-5:])
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(f"      cargo test -p aura-codec -> rc={result.returncode} "
              f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})")
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        if touch_test:
            os.utime(TEST_RS, None)


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    print(f"C2 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 codec version",
            LIB_RS,
            "pub const CODEC_VERSION: u8 = 1;",
            "pub const CODEC_VERSION: u8 = 2;",
        ),
        expect_red(
            "NC-2 independent golden fixture",
            FIXTURE,
            "record_1=01",
            "record_1=03",
            touch_test=True,
        ),
        expect_red(
            "NC-3 trailing-byte guard",
            CODEC_RS,
            "if !self.remaining.is_empty() {",
            "if false {",
        ),
        expect_red(
            "NC-4 non-minimal varint guard",
            CODEC_RS,
            "last_byte == 0 && encoded_len > 1",
            "false",
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    if passed != len(results):
        return 1

    restored = run(["cargo", "test", "-p", "aura-codec", "--quiet"])
    print(f"restore check: cargo test -p aura-codec -> rc={restored.returncode} "
          f"({'GREEN as expected' if restored.returncode == 0 else 'STILL RED — FAIL'})")
    return 0 if restored.returncode == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
