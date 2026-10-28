#!/usr/bin/env python3
"""Negative controls for S0 (naive state baseline).

Six tamper tests must each turn the S0 gate red:

  NC-1: change FINAL_VIEW_CONTEXT.
  NC-2: corrupt an independent golden final-view vector.
  NC-3: pretend a state root exists.
  NC-4: pretend persistence exists.
  NC-5: pretend the baseline is deterministic.
  NC-6: falsify the "no root" measurement in the fixture.

Files are restored byte-for-byte in a `finally` block. `make nc-s0`
captures stdout into docs/evidence/s0/nc.log; a final green restore check is
mandatory.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BASELINE_RS = REPO / "crates" / "aura-state" / "src" / "baseline.rs"
TEST_RS = REPO / "crates" / "aura-state" / "tests" / "s0_baseline.rs"
FIXTURE = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "s0_golden.txt"

CARGO_TEST = ["cargo", "test", "-p", "aura-state", "--test", "s0_baseline", "--quiet"]


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
            f"      cargo test -p aura-state --test s0_baseline -> rc={result.returncode} "
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
    print(f"S0 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print("S0 measures defects; these NCs prove the measurements are real.")
    print()

    fixture = FIXTURE.read_text(encoding="utf-8")
    golden_value = next(
        line.split("=", 1)[1]
        for line in fixture.splitlines()
        if line.startswith("scenario_1_final_a=")
    )
    flipped = ("0" if golden_value[0] != "0" else "1") + golden_value[1:]

    results = [
        expect_red(
            "NC-1 final-view context",
            BASELINE_RS,
            'pub const FINAL_VIEW_CONTEXT: &[u8] = b"AuraLite-Crypto/s0-final-view-v1\\x00";',
            'pub const FINAL_VIEW_CONTEXT: &[u8] = b"WRONG-S0-VIEW\\x00";',
        ),
        expect_red(
            "NC-2 independent golden final view",
            FIXTURE,
            f"scenario_1_final_a={golden_value}",
            f"scenario_1_final_a={flipped}",
            touch_test=True,
        ),
        expect_red(
            "NC-3 fake state root",
            BASELINE_RS,
            "Err(BaselineError::NoRoot)",
            "Ok(Hash::ZERO)",
        ),
        expect_red(
            "NC-4 fake persistence",
            BASELINE_RS,
            "Err(BaselineError::NoPersistence)",
            "Ok(())",
        ),
        expect_red(
            "NC-5 fake determinism",
            BASELINE_RS,
            "deterministic: false,",
            "deterministic: true,",
        ),
        expect_red(
            "NC-6 falsified no-root measurement",
            FIXTURE,
            "no_root=1",
            "no_root=0",
            touch_test=True,
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
