#!/usr/bin/env python3
"""Negative controls for C5 (aura-block block/header format).

Six tamper tests must each turn the C5 gate red:

  NC-1: change LEAF_CONTEXT (Merkle leaves).
  NC-2: change NODE_CONTEXT (Merkle internal nodes).
  NC-3: change BLOCK_HASH_PAYLOAD_CONTEXT (block ids).
  NC-4: corrupt one golden tx_root in the fixture.
  NC-5: disable the tx_root recomputation check.
  NC-6: disable the canonical-order check.

Files are always restored in a `finally` block. `make nc-c5` captures stdout
into docs/evidence/c5/nc.log, and a final green restore check is mandatory.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
MERKLE_RS = REPO / "crates" / "aura-block" / "src" / "merkle.rs"
HEADER_RS = REPO / "crates" / "aura-block" / "src" / "header.rs"
BLOCK_RS = REPO / "crates" / "aura-block" / "src" / "block.rs"
TEST_RS = REPO / "crates" / "aura-block" / "tests" / "c5_block.rs"
FIXTURE = REPO / "crates" / "aura-block" / "tests" / "fixtures" / "c5_golden.txt"
CARGO_TEST = ["cargo", "test", "-p", "aura-block", "--quiet"]


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
            f"      cargo test -p aura-block -> rc={result.returncode} "
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
    print(f"C5 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 Merkle leaf context",
            MERKLE_RS,
            'pub const LEAF_CONTEXT: &[u8] = b"AuraLite-Crypto/tx-leaf-v1\\x00";',
            'pub const LEAF_CONTEXT: &[u8] = b"WRONG-LEAF\\x00";',
        ),
        expect_red(
            "NC-2 Merkle node context",
            MERKLE_RS,
            'pub const NODE_CONTEXT: &[u8] = b"AuraLite-Crypto/tx-node-v1\\x00";',
            'pub const NODE_CONTEXT: &[u8] = b"WRONG-NODE\\x00";',
        ),
        expect_red(
            "NC-3 block-hash context",
            HEADER_RS,
            'pub const BLOCK_HASH_PAYLOAD_CONTEXT: &[u8] = b"AuraLite-Crypto/block-hash-v1\\x00";',
            'pub const BLOCK_HASH_PAYLOAD_CONTEXT: &[u8] = b"WRONG-BLOCK-HASH\\x00";',
        ),
        expect_red(
            "NC-4 independent golden tx_root",
            FIXTURE,
            "b1_tx_root=62463b0e",
            "b1_tx_root=62463b0f",
            touch_test=True,
        ),
        expect_red(
            "NC-5 tx_root recomputation check",
            BLOCK_RS,
            "if computed_root != self.header.tx_root {",
            "if false {",
        ),
        expect_red(
            "NC-6 canonical body order check",
            BLOCK_RS,
            "if a > b {",
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
        f"restore check: cargo test -p aura-block -> rc={restored.returncode} "
        f"({'GREEN as expected' if restored.returncode == 0 else 'STILL RED — FAIL'})"
    )
    return 0 if restored.returncode == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
