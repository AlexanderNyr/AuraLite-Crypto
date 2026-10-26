#!/usr/bin/env python3
"""Negative controls for C8 (deterministic genesis).

Six tamper tests must each turn the C8 gate red:

  NC-1: change GENESIS_CONTEXT.
  NC-2: change VALIDATOR_SET_CONTEXT.
  NC-3: corrupt one golden genesis hash in the fixture.
  NC-4: sort validators (would erase the consensus-significant order).
  NC-5: accept unknown JSON fields.
  NC-6: disable the Stage 1 chain-id check.

Files are always restored in a `finally` block. `make nc-c8` captures stdout
into docs/evidence/c8/nc.log, and a final green restore check is mandatory.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
TYPES_RS = REPO / "crates" / "aura-genesis" / "src" / "types.rs"
JSON_RS = REPO / "crates" / "aura-genesis" / "src" / "json.rs"
TEST_RS = REPO / "crates" / "aura-genesis" / "tests" / "c8_genesis.rs"
FIXTURE = REPO / "crates" / "aura-genesis" / "tests" / "fixtures" / "c8_golden.txt"
CARGO_TEST = ["cargo", "test", "-p", "aura-genesis", "--quiet"]


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
            f"      cargo test -p aura-genesis -> rc={result.returncode} "
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
    print(f"C8 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 genesis context",
            TYPES_RS,
            'pub const GENESIS_CONTEXT: &[u8] = b"AuraLite-Crypto/genesis-v1\\x00";',
            'pub const GENESIS_CONTEXT: &[u8] = b"WRONG-GENESIS\\x00";',
        ),
        expect_red(
            "NC-2 validator-set context",
            TYPES_RS,
            'pub const VALIDATOR_SET_CONTEXT: &[u8] = b"AuraLite-Crypto/validator-set-v1\\x00";',
            'pub const VALIDATOR_SET_CONTEXT: &[u8] = b"WRONG-VSET\\x00";',
        ),
        expect_red(
            "NC-3 independent golden genesis hash",
            FIXTURE,
            "genesis_1_hash=af92bd6b",
            "genesis_1_hash=af92bd6c",
            touch_test=True,
        ),
        expect_red(
            "NC-4 validator order (DoD: sorting must not happen)",
            JSON_RS,
            'let validators = parse_validator_array(required(object, "validators")?)?;',
            'let mut validators = parse_validator_array(required(object, "validators")?)?; '
            'validators.sort_by_key(|v| v.address);',
        ),
        expect_red(
            "NC-5 unknown JSON fields",
            JSON_RS,
            "if !ALLOWED_FIELDS.contains(&key.as_str()) {",
            "if false {",
        ),
        expect_red(
            "NC-6 Stage 1 chain-id check",
            JSON_RS,
            "if !is_our_chain(raw_chain) {",
            "if false {",
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    if passed != len(results):
        return 1

    restored = run(CARGO_TEST)
    deps = run(["python3", "tools/check_deps.py"])
    ok = restored.returncode == 0 and deps.returncode == 0
    print(
        f"restore check: cargo test -p aura-genesis -> rc={restored.returncode}; "
        f"check_deps -> rc={deps.returncode} "
        f"({'GREEN as expected' if ok else 'STILL RED — FAIL'})"
    )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
