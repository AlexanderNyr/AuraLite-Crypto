#!/usr/bin/env python3
"""Negative controls for C9 (test rig).

Six tamper tests must each turn a C9 gate red — the point is to prove the
RIG detects bugs, not just that the code is correct today:

  NC-1: corrupt a C9 golden varint vector        -> vector suite red.
  NC-2: raise MIN_VECTORS beyond the inventory  -> vector inventory red.
  NC-3: allow non-minimal varints (real bug)    -> proptest suite red.
  NC-4: disable the trailing-byte guard (bug)   -> proptest suite red.
  NC-5: zero the fuzz iteration counter         -> harness gate red.
  NC-6: unwire the inventory from the Makefile  -> rig wiring gate red.

Files are always restored in a `finally` block. `make nc-c9` captures stdout
into docs/evidence/c9/nc.log, and a final green restore check is mandatory.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CODEC_RS = REPO / "crates" / "aura-codec" / "src" / "codec.rs"
TESTKIT_LIB = REPO / "crates" / "aura-testkit" / "src" / "lib.rs"
VECTORS_RS = REPO / "crates" / "aura-testkit" / "tests" / "vectors.rs"
FIXTURE = REPO / "crates" / "aura-testkit" / "tests" / "fixtures" / "c9_vectors.txt"
COUNTER = REPO / "tools" / "count_vectors.py"
MAKEFILE = REPO / "Makefile"


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(
    label: str,
    path: Path,
    old: str,
    new: str,
    cmd: list[str],
    touch_test: bool = False,
    replace_all: bool = False,
) -> bool:
    original = path.read_text(encoding="utf-8")
    if old not in original:
        print(f"{label}: FAIL — tamper target not found in {path.name}")
        return False
    tampered = original.replace(old, new) if replace_all else original.replace(old, new, 1)
    if tampered == original:
        print(f"{label}: FAIL — tamper was a no-op")
        return False
    try:
        path.write_text(tampered, encoding="utf-8")
        if touch_test:
            os.utime(VECTORS_RS, None)
        result = run(cmd)
        went_red = result.returncode != 0
        snippet = "\n".join((result.stdout + result.stderr).strip().splitlines()[-5:])
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      {' '.join(cmd)} -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        if touch_test:
            os.utime(VECTORS_RS, None)


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    print(f"C9 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print("The rig must catch REAL injected bugs, not only missing files.")
    print()

    vectors_cmd = ["cargo", "test", "-p", "aura-testkit", "--test", "vectors", "--quiet"]
    properties_cmd = ["cargo", "test", "-p", "aura-testkit", "--test", "properties", "--quiet"]
    harness_cmd = ["cargo", "test", "-p", "aura-testkit", "--test", "harness", "--quiet"]

    results = [
        expect_red(
            "NC-1 C9 golden varint vector",
            FIXTURE,
            "varint_ok_0=0000000000000000:00",
            "varint_ok_0=0000000000000000:80",
            vectors_cmd,
            touch_test=True,
        ),
        expect_red(
            "NC-2 vector inventory minimum",
            COUNTER,
            "MIN_VECTORS = 200",
            "MIN_VECTORS = 1000000",
            ["python3", "tools/count_vectors.py"],
        ),
        expect_red(
            "NC-3 injected non-minimal-varint bug",
            CODEC_RS,
            "last_byte == 0 && encoded_len > 1",
            "false",
            properties_cmd,
        ),
        expect_red(
            "NC-4 injected trailing-byte-guard bug",
            CODEC_RS,
            "if !self.remaining.is_empty() {",
            "if false {",
            properties_cmd,
        ),
        expect_red(
            "NC-5 fuzz harness iteration counter",
            TESTKIT_LIB,
            "iterations += 1;",
            "iterations += 0;",
            harness_cmd,
        ),
        expect_red(
            "NC-6 rig wiring in Makefile",
            MAKEFILE,
            "\t@$(PYTHON) tools/count_vectors.py",
            "\t@true",
            ["python3", "tools/check_rig.py"],
            replace_all=True,
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    if passed != len(results):
        return 1

    checks = [
        run(vectors_cmd),
        run(properties_cmd),
        run(harness_cmd),
        run(["python3", "tools/count_vectors.py"]),
        run(["python3", "tools/check_rig.py"]),
    ]
    ok = all(result.returncode == 0 for result in checks)
    print(
        "restore check: " + ", ".join(f"rc={r.returncode}" for r in checks)
        + f" ({'GREEN as expected' if ok else 'STILL RED — FAIL'})"
    )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
