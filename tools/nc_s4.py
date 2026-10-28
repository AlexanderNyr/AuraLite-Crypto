#!/usr/bin/env python3
"""Negative controls for S4 (snapshots).

Six tamper tests must each turn the S4 gate red:

  NC-1: change SNAPSHOT_MAGIC.
  NC-2: change SNAPSHOT_MANIFEST_CONTEXT.
  NC-3: change SNAPSHOT_CHUNK_CONTEXT.
  NC-4: corrupt one independent golden manifest.
  NC-5: disable manifest checksum verification.
  NC-6: disable chunk digest comparison (substituted chunk must be rejected).

Files are restored byte-for-byte in a `finally` block. `make nc-s4`
captures stdout into docs/evidence/s4/nc.log; a final green restore check is
mandatory.

Usage:
    python3 tools/nc_s4.py
    make nc-s4
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SNAPSHOT_RS = REPO / "crates" / "aura-state" / "src" / "snapshot.rs"
TEST_RS = REPO / "crates" / "aura-state" / "tests" / "s4_snapshot.rs"
FIXTURE = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "s4_golden.txt"

CARGO_TEST = ["cargo", "test", "-p", "aura-state", "--test", "s4_snapshot", "--quiet"]


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
            f"      cargo test -p aura-state --test s4_snapshot -> rc={result.returncode} "
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
    print(f"S4 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    fixture = FIXTURE.read_text(encoding="utf-8")
    golden = next(
        line.split("=", 1)[1]
        for line in fixture.splitlines()
        if line.startswith("manifest_bytes=")
    )
    flipped = ("0" if golden[0] != "0" else "1") + golden[1:]
    results = [
        expect_red(
            "NC-1 snapshot magic",
            SNAPSHOT_RS,
            'pub const SNAPSHOT_MAGIC: &[u8] = b"ALS4";',
            'pub const SNAPSHOT_MAGIC: &[u8] = b"XXXX";',
        ),
        expect_red(
            "NC-2 manifest checksum context",
            SNAPSHOT_RS,
            'pub const SNAPSHOT_MANIFEST_CONTEXT: &[u8] = b"AuraLite-Crypto/s4-manifest-v1\\x00";',
            'pub const SNAPSHOT_MANIFEST_CONTEXT: &[u8] = b"WRONG-S4-MANIFEST\\x00";',
        ),
        expect_red(
            "NC-3 chunk hash context",
            SNAPSHOT_RS,
            'pub const SNAPSHOT_CHUNK_CONTEXT: &[u8] = b"AuraLite-Crypto/s4-chunk-v1\\x00";',
            'pub const SNAPSHOT_CHUNK_CONTEXT: &[u8] = b"WRONG-S4-CHUNK\\x00";',
        ),
        expect_red(
            "NC-4 independent golden manifest",
            FIXTURE,
            f"manifest_bytes={golden}",
            f"manifest_bytes={flipped}",
            touch_test=True,
        ),
        expect_red(
            "NC-5 manifest checksum verification",
            SNAPSHOT_RS,
            "if expected.as_bytes() != checksum {",
            "if false {",
        ),
        expect_red(
            "NC-6 chunk digest comparison (substituted chunk)",
            SNAPSHOT_RS,
            "if !digests_match {",
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
