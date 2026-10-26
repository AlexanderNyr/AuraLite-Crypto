#!/usr/bin/env python3
"""Negative controls for C6 (Merkle proofs).

Six tamper tests must each turn the C6 gate red:

  NC-1: change EVENT_LEAF_CONTEXT (domain separation for event leaves).
  NC-2: corrupt one golden root in the fixture.
  NC-3: disable the leaf<->value binding in verify_tx_inclusion().
  NC-4: disable the sibling-count check.
  NC-5: force one sibling direction.
  NC-6: disable the root comparison.

Files are always restored in a `finally` block. `make nc-c6` captures stdout
into docs/evidence/c6/nc.log, and a final green restore check is mandatory.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
PROOF_RS = REPO / "crates" / "aura-block" / "src" / "proof.rs"
TEST_RS = REPO / "crates" / "aura-block" / "tests" / "c6_merkle.rs"
FIXTURE = REPO / "crates" / "aura-block" / "tests" / "fixtures" / "c6_golden.txt"
CARGO_TEST = ["cargo", "test", "-p", "aura-block", "--test", "c6_merkle", "--quiet"]


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
            os.utime(TEST_RS, None)
        result = run(CARGO_TEST)
        went_red = result.returncode != 0
        snippet = "\n".join((result.stdout + result.stderr).strip().splitlines()[-5:])
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      cargo test -p aura-block --test c6_merkle -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        if touch_test:
            os.utime(TEST_RS, None)


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    print(f"C6 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 event leaf context",
            PROOF_RS,
            'pub const EVENT_LEAF_CONTEXT: &[u8] = b"AuraLite-Crypto/event-leaf-v1\\x00";',
            'pub const EVENT_LEAF_CONTEXT: &[u8] = b"WRONG-EVENT-LEAF\\x00";',
        ),
        expect_red(
            "NC-2 independent golden root",
            FIXTURE,
            "root_tx_5=5e490547",
            "root_tx_5=5e490548",
            touch_test=True,
        ),
        expect_red(
            "NC-3 leaf/value binding",
            PROOF_RS,
            "if expected_leaf != proof.leaf {",
            "if false {",
        ),
        expect_red(
            "NC-4 sibling-count check",
            PROOF_RS,
            "if proof.siblings.len() != expected_depth {",
            "if false {",
        ),
        expect_red(
            "NC-5 sibling direction",
            PROOF_RS,
            "if position.is_multiple_of(2) {",
            "if true {",
        ),
        expect_red(
            "NC-6 root comparison",
            PROOF_RS,
            "if computed != *root {",
            "if false {",
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    if passed != len(results):
        return 1

    restored = run(CARGO_TEST)
    print(
        f"restore check: cargo test -p aura-block --test c6_merkle -> rc={restored.returncode} "
        f"({'GREEN as expected' if restored.returncode == 0 else 'STILL RED — FAIL'})"
    )
    return 0 if restored.returncode == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
