#!/usr/bin/env python3
"""Negative controls for S1 (versioned sparse Merkle tree).

Six tamper tests must each turn the S1 gate red:

  NC-1: change S1_LEAF_CONTEXT (compressed leaf hashing).
  NC-2: change S1_NODE_CONTEXT (branch hashing).
  NC-3: corrupt one independent checkpoint root in the fixture.
  NC-4: disable root comparison during proof verification.
  NC-5: flip MSB-first path-bit ordering.
  NC-6: make empty subtrees depth-independent.

Files are restored byte-for-byte in a `finally` block. `make nc-s1` captures
stdout into docs/evidence/s1/nc.log; a final green restore check is mandatory.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
TREE_RS = REPO / "crates" / "aura-state" / "src" / "tree.rs"
TEST_RS = REPO / "crates" / "aura-state" / "tests" / "s1_tree.rs"
FIXTURE = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "s1_golden.txt"

CARGO_TEST = ["cargo", "test", "-p", "aura-state", "--test", "s1_tree", "--quiet"]


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
        snippet = "\n".join((result.stdout + result.stderr).strip().splitlines()[-4:])
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      cargo test -p aura-state --test s1_tree -> rc={result.returncode} "
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
    print(f"S1 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    fixture = FIXTURE.read_text(encoding="utf-8")
    golden_root = next(
        line.split("=", 1)[1]
        for line in fixture.splitlines()
        if line.startswith("checkpoint_1_root=")
    )
    flipped = ("0" if golden_root[0] != "0" else "1") + golden_root[1:]

    results = [
        expect_red(
            "NC-1 leaf context",
            TREE_RS,
            'pub const S1_LEAF_CONTEXT: &[u8] = b"AuraLite-Crypto/s1-leaf-v1\\x00";',
            'pub const S1_LEAF_CONTEXT: &[u8] = b"WRONG-S1-LEAF\\x00";',
        ),
        expect_red(
            "NC-2 node context",
            TREE_RS,
            'pub const S1_NODE_CONTEXT: &[u8] = b"AuraLite-Crypto/s1-node-v1\\x00";',
            'pub const S1_NODE_CONTEXT: &[u8] = b"WRONG-S1-NODE\\x00";',
        ),
        expect_red(
            "NC-3 independent checkpoint root",
            FIXTURE,
            f"checkpoint_1_root={golden_root}",
            f"checkpoint_1_root={flipped}",
            touch_test=True,
        ),
        expect_red(
            "NC-4 proof root comparison",
            TREE_RS,
            "if versioned_root(proof.version, &hash) != *root {",
            "if false {",
        ),
        expect_red(
            "NC-5 path bit ordering",
            TREE_RS,
            ">> (7 - depth % 8)",
            ">> (depth % 8)",
        ),
        expect_red(
            "NC-6 depth-independent empty subtrees",
            TREE_RS,
            "let hash = empty_hash(depth);",
            "let hash = empty_hash(0);",
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    if passed != len(results):
        return 1

    checks = [
        run(CARGO_TEST),
        run(["python3", "tools/audit_closeout.py"]),
        run(["python3", "tools/count_vectors.py"]),
    ]
    ok = all(result.returncode == 0 for result in checks)
    print(
        "restore check: " + ", ".join(f"rc={r.returncode}" for r in checks)
        + f" ({'GREEN as expected' if ok else 'STILL RED — FAIL'})"
    )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
