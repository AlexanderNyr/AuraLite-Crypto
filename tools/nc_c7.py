#!/usr/bin/env python3
"""Negative controls for C7 (account model and state key layout).

Six tamper tests must each turn the C7 gate red:

  NC-1: change STATE_KEY_CONTEXT (key digests).
  NC-2: corrupt one golden key digest in the fixture.
  NC-3: accept reserved key-space prefixes.
  NC-4: disable the namespace-length check.
  NC-5: disable the contract namespace check.
  NC-6: disable the contract-address check.

Files are always restored in a `finally` block. `make nc-c7` captures stdout
into docs/evidence/c7/nc.log, and a final green restore check is mandatory.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
KEY_RS = REPO / "crates" / "aura-state" / "src" / "key.rs"
TEST_RS = REPO / "crates" / "aura-state" / "tests" / "c7_state.rs"
FIXTURE = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "c7_golden.txt"
CARGO_TEST = ["cargo", "test", "-p", "aura-state", "--quiet"]


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
            f"      cargo test -p aura-state -> rc={result.returncode} "
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
    print(f"C7 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 state key digest context",
            KEY_RS,
            'pub const STATE_KEY_CONTEXT: &[u8] = b"AuraLite-Crypto/state-key-v1\\x00";',
            'pub const STATE_KEY_CONTEXT: &[u8] = b"WRONG-STATE-KEY\\x00";',
        ),
        expect_red(
            "NC-2 independent golden key digest",
            FIXTURE,
            "digest_account_0=25d3c745",
            "digest_account_0=25d3c746",
            touch_test=True,
        ),
        expect_red(
            "NC-3 reserved key-space prefixes",
            KEY_RS,
            "return Err(StateError::ReservedKeySpace(prefix));",
            "return Ok(StateKey(raw.to_vec()));",
        ),
        expect_red(
            "NC-4 namespace length check",
            KEY_RS,
            "if raw.len() != space.expected_len() {",
            "if false {",
        ),
        expect_red(
            "NC-5 contract namespace check",
            KEY_RS,
            "if key.space() != KeySpace::ContractStorage {",
            "if false {",
        ),
        expect_red(
            "NC-6 contract address check",
            KEY_RS,
            "if &key.as_bytes()[1..21] != contract.as_bytes().as_slice() {",
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
        f"restore check: cargo test -p aura-state -> rc={restored.returncode} "
        f"({'GREEN as expected' if restored.returncode == 0 else 'STILL RED — FAIL'})"
    )
    return 0 if restored.returncode == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
