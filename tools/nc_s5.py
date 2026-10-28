#!/usr/bin/env python3
"""Negative controls for S5 (state sync).

Six tamper tests must each turn the S5 gate red:

  NC-1: change S5_HEAD_CONTEXT (head checksum domain).
  NC-2: change S5_DELTA_CONTEXT (delta checksum domain).
  NC-3: change S5_PACKAGE_CONTEXT (package checksum domain).
  NC-4: corrupt one independent golden package.
  NC-5: disable the quorum comparison (3 heads must agree).
  NC-6: disable the final state-root comparison (substituted diff chunk
        must be rejected).
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SYNC_RS = REPO / "crates" / "aura-state" / "src" / "sync.rs"
TEST_RS = REPO / "crates" / "aura-state" / "tests" / "s5_sync.rs"
FIXTURE = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "s5_golden.txt"

CARGO_TEST = ["cargo", "test", "-p", "aura-state", "--test", "s5_sync", "--quiet"]


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
            f"      cargo test -p aura-state --test s5_sync -> rc={result.returncode} "
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
    print(f"S5 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    fixture = FIXTURE.read_text(encoding="utf-8")
    golden = next(
        line.split("=", 1)[1]
        for line in fixture.splitlines()
        if line.startswith("package_bytes=")
    )
    flipped = ("0" if golden[0] != "0" else "1") + golden[1:]

    results = [
        expect_red(
            "NC-1 head checksum context",
            SYNC_RS,
            'pub const S5_HEAD_CONTEXT: &[u8] = b"AuraLite-Crypto/s5-head-v1\\x00";',
            'pub const S5_HEAD_CONTEXT: &[u8] = b"WRONG-S5-HEAD\\x00";',
        ),
        expect_red(
            "NC-2 delta checksum context",
            SYNC_RS,
            'pub const S5_DELTA_CONTEXT: &[u8] = b"AuraLite-Crypto/s5-delta-v1\\x00";',
            'pub const S5_DELTA_CONTEXT: &[u8] = b"WRONG-S5-DELTA\\x00";',
        ),
        expect_red(
            "NC-3 package checksum context",
            SYNC_RS,
            'pub const S5_PACKAGE_CONTEXT: &[u8] = b"AuraLite-Crypto/s5-package-v1\\x00";',
            'pub const S5_PACKAGE_CONTEXT: &[u8] = b"WRONG-S5-PACKAGE\\x00";',
        ),
        expect_red(
            "NC-4 independent golden package",
            FIXTURE,
            f"package_bytes={golden}",
            f"package_bytes={flipped}",
            touch_test=True,
        ),
        expect_red(
            "NC-5 quorum comparison",
            SYNC_RS,
            "if !heads_agree {",
            "if false {",
        ),
        expect_red(
            "NC-6 final state-root comparison (substituted diff chunk)",
            SYNC_RS,
            "if computed_root != head.state_root {",
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
