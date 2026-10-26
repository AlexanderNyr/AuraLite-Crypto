#!/usr/bin/env python3
"""Negative controls for C1.

C1 pins canonical types against an INDEPENDENT bech32 reference. These three
tamper tests require the gate to go red:

  NC-1: flip the BIP-173 checksum constant 1 -> 0.
  NC-2: flip the address version byte 0x00 -> 0x01.
  NC-3: corrupt one golden bech32 vector in the fixture.

Every file is restored in a `finally` block. Output goes to
docs/evidence/c1/nc.log via `make nc-c1`.
"""

from __future__ import annotations

import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
ADDRESS_RS = REPO / "crates" / "aura-types" / "src" / "address.rs"
TEST_RS = REPO / "crates" / "aura-types" / "tests" / "c1_types.rs"
FIXTURE = REPO / "crates" / "aura-types" / "tests" / "fixtures" / "c1_golden.txt"


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
        result = run(["cargo", "test", "-p", "aura-types", "--quiet"])
        went_red = result.returncode != 0
        output = (result.stdout + result.stderr).strip().splitlines()
        snippet = "\n".join(output[-5:])
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(f"      cargo test -p aura-types -> rc={result.returncode} "
              f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})")
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        if touch_test:
            os.utime(TEST_RS, None)


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    print(f"C1 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 bech32 checksum constant",
            ADDRESS_RS,
            "const BECH32_CHECKSUM_CONST: u32 = 1;",
            "const BECH32_CHECKSUM_CONST: u32 = 0;",
        ),
        expect_red(
            "NC-2 address version byte",
            ADDRESS_RS,
            "pub const ADDRESS_VERSION: u8 = 0x00;",
            "pub const ADDRESS_VERSION: u8 = 0x01;",
        ),
        expect_red(
            "NC-3 independent golden vector",
            FIXTURE,
            "address_1_bech32=aura1qqqqzqsrqszsvpcgpy9qkrqdpc83qygjzvp3nn97",
            "address_1_bech32=aura1qqqqzqsrqszsvpcgpy9qkrqdpc83qygjzqp3nn97",
            touch_test=True,
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    if passed != len(results):
        return 1

    restored = run(["cargo", "test", "-p", "aura-types", "--quiet"])
    print(f"restore check: cargo test -p aura-types -> rc={restored.returncode} "
          f"({'GREEN as expected' if restored.returncode == 0 else 'STILL RED — FAIL'})")
    return 0 if restored.returncode == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
