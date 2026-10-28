#!/usr/bin/env python3
"""Negative controls for S3 (execution overlay).

Six tamper tests must each turn the S3 gate red:

  NC-1: change S3_DIFF_CONTEXT (diff checksum domain).
  NC-2: change S3_EVENT_CONTEXT (event commitment).
  NC-3: change DIFF_MAGIC.
  NC-4: corrupt one independent golden diff.
  NC-5: disable checksum verification.
  NC-6: remove rollback (DoS NC from the S3 DoD: balance changes after revert).

Files are restored byte-for-byte in a `finally` block. `make nc-s3` captures
stdout into docs/evidence/s3/nc.log; a final green restore check is mandatory.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OVERLAY_RS = REPO / "crates" / "aura-state" / "src" / "overlay.rs"
TEST_RS = REPO / "crates" / "aura-state" / "tests" / "s3_overlay.rs"
FIXTURE = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "s3_golden.txt"

CARGO_TEST = ["cargo", "test", "-p", "aura-state", "--test", "s3_overlay", "--quiet"]


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
            f"      cargo test -p aura-state --test s3_overlay -> rc={result.returncode} "
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
    print(f"S3 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    fixture = FIXTURE.read_text(encoding="utf-8")
    golden = next(
        line.split("=", 1)[1]
        for line in fixture.splitlines()
        if line.startswith("diff_1_bytes=")
    )
    flipped = ("0" if golden[0] != "0" else "1") + golden[1:]

    results = [
        expect_red(
            "NC-1 diff checksum context",
            OVERLAY_RS,
            'pub const S3_DIFF_CONTEXT: &[u8] = b"AuraLite-Crypto/s3-diff-v1\\x00";',
            'pub const S3_DIFF_CONTEXT: &[u8] = b"WRONG-S3-DIFF\\x00";',
        ),
        expect_red(
            "NC-2 event commitment context",
            OVERLAY_RS,
            'pub const S3_EVENT_CONTEXT: &[u8] = b"AuraLite-Crypto/s3-event-v1\\x00";',
            'pub const S3_EVENT_CONTEXT: &[u8] = b"WRONG-S3-EVENT\\x00";',
        ),
        expect_red(
            "NC-3 diff magic",
            OVERLAY_RS,
            'pub const DIFF_MAGIC: &[u8] = b"ALD3";',
            'pub const DIFF_MAGIC: &[u8] = b"XXXX";',
        ),
        expect_red(
            "NC-4 independent golden diff",
            FIXTURE,
            f"diff_1_bytes={golden}",
            f"diff_1_bytes={flipped}",
            touch_test=True,
        ),
        expect_red(
            "NC-5 diff checksum verification",
            OVERLAY_RS,
            "if expected.as_bytes() != checksum_bytes {",
            "if false {",
        ),
        expect_red(
            "NC-6 rollback removal (balance changes after revert)",
            OVERLAY_RS,
            "        self.changes.clear();",
            "        let _ = 0;",
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
