#!/usr/bin/env python3
"""Negative controls for S2 (WAL + durable state).

Six tamper tests must each turn the S2 gate red:

  NC-1: change S2_WAL_CONTEXT (WAL checksum domain).
  NC-2: change S2_KEY_CONTEXT (state key -> Merkle path).
  NC-3: change S2_ROOT_CONTEXT (versioned state root).
  NC-4: corrupt one independent golden WAL record.
  NC-5: disable WAL checksum verification.
  NC-6: disable the WAL-before-head invariant check.

Files are restored byte-for-byte in a `finally` block. `make nc-s2`
captures stdout into docs/evidence/s2/nc.log; a final green restore check is
mandatory.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
WAL_RS = REPO / "crates" / "aura-storage" / "src" / "wal.rs"
STORE_RS = REPO / "crates" / "aura-storage" / "src" / "store.rs"
TEST_RS = REPO / "crates" / "aura-storage" / "tests" / "s2_storage.rs"
FIXTURE = REPO / "crates" / "aura-storage" / "tests" / "fixtures" / "s2_golden.txt"

CARGO_TEST = ["cargo", "test", "-p", "aura-storage", "--test", "s2_storage", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(
    label: str, path: Path, old: str, new: str, touch_test: bool = False
) -> bool:
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
            f"      cargo test -p aura-storage --test s2_storage -> rc={result.returncode} "
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
    print(f"S2 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    fixture = FIXTURE.read_text(encoding="utf-8")
    golden_wal = next(
        line.split("=", 1)[1]
        for line in fixture.splitlines()
        if line.startswith("commit_1_wal=")
    )
    flipped = ("0" if golden_wal[0] != "0" else "1") + golden_wal[1:]

    results = [
        expect_red(
            "NC-1 WAL checksum context",
            WAL_RS,
            'pub const S2_WAL_CONTEXT: &[u8] = b"AuraLite-Crypto/s2-wal-v1\\x00";',
            'pub const S2_WAL_CONTEXT: &[u8] = b"WRONG-S2-WAL\\x00";',
        ),
        expect_red(
            "NC-2 state key context",
            WAL_RS,
            'pub const S2_KEY_CONTEXT: &[u8] = b"AuraLite-Crypto/s2-key-v1\\x00";',
            'pub const S2_KEY_CONTEXT: &[u8] = b"WRONG-S2-KEY\\x00";',
        ),
        expect_red(
            "NC-3 state root context",
            WAL_RS,
            'pub const S2_ROOT_CONTEXT: &[u8] = b"AuraLite-Crypto/s2-root-v1\\x00";',
            'pub const S2_ROOT_CONTEXT: &[u8] = b"WRONG-S2-ROOT\\x00";',
        ),
        expect_red(
            "NC-4 independent golden WAL record",
            FIXTURE,
            f"commit_1_wal={golden_wal}",
            f"commit_1_wal={flipped}",
            touch_test=True,
        ),
        expect_red(
            "NC-5 WAL checksum verification",
            WAL_RS,
            "if wal_checksum(payload) != checksum {",
            "if false {",
        ),
        expect_red(
            "NC-6 WAL-before-head invariant",
            STORE_RS,
            "if head.version as usize > wal.len() {",
            "if false {",
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
        run(["python3", "tools/check_deps.py"]),
    ]
    ok = all(result.returncode == 0 for result in checks)
    print(
        "restore check: " + ", ".join(f"rc={r.returncode}" for r in checks)
        + f" ({'GREEN as expected' if ok else 'STILL RED — FAIL'})"
    )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
