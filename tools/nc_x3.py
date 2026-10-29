#!/usr/bin/env python3
"""Negative controls for X3 (contract lifecycle).

Six tamper tests must each turn the X3 gate red:

  NC-1 version check removed        -> a stale upgrade silently overwrites code.
  NC-2 balance not preserved        -> upgrade zeroes the contract's balance.
  NC-3 `migrate` never runs         -> storage is not migrated across upgrade.
  NC-4 code hash not stored         -> the account never points at its code.
  NC-5 `init` never run             -> a constructor is dead code.
  NC-6 census skipped at deploy     -> banned bytecode reaches state.

Rule (CONTRIBUTING.md §1): a gate that cannot go red is not a gate.

Usage:
    python3 tools/nc_x3.py
    make nc-x3
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
LIFECYCLE_RS = REPO / "crates" / "aura-exec" / "src" / "lifecycle.rs"

CARGO_TEST = ["cargo", "test", "-p", "aura-exec", "--test", "x3_lifecycle", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str) -> bool:
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
        os.utime(path, None)
        result = run(CARGO_TEST)
        went_red = result.returncode != 0
        raw = "\n".join((result.stdout + result.stderr).strip().splitlines()[-3:])
        snippet = re.sub(r"finished in [\d.]+s", "finished", raw)
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      cargo test -p aura-exec --test x3_lifecycle -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        os.utime(path, None)


def main() -> int:
    # No timestamp: this log is checksummed in docs/evidence/x3/SHA256SUMS, so
    # re-running the controls must reproduce it byte for byte.
    print("X3 NEGATIVE CONTROLS (contract lifecycle)")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 version check removed (a stale upgrade wins)",
            LIFECYCLE_RS,
            """        if current.code_version != expected_code_version {
            return Err(LifecycleError::VersionMismatch {
                expected: expected_code_version,
                actual: current.code_version,
            });
        }""",
            """        let _ = expected_code_version;""",
        ),
        expect_red(
            "NC-2 balance not preserved (upgrade zeroes the account)",
            LIFECYCLE_RS,
            """        let mut account = account_of(session.view(), address);
        account.code_hash = record.code_hash();""",
            """        let mut account = account_of(session.view(), address);
        account.balance = Amount::ZERO;
        account.code_hash = record.code_hash();""",
        ),
        expect_red(
            "NC-3 `migrate` never runs (storage not migrated)",
            LIFECYCLE_RS,
            """            Some(2) => session
                .run(
                    &code,
                    ENTRY_MIGRATE,
                    &[current.code_version as i32, next as i32],
                )
                .map_err(LifecycleError::Engine)?,""",
            """            Some(2) => {}""",
        ),
        expect_red(
            "NC-4 code hash not stored on the account",
            LIFECYCLE_RS,
            """        account.code_hash = record.code_hash();""",
            """        account.code_hash = Hash::ZERO;""",
        ),
        expect_red(
            "NC-5 `init` never runs",
            LIFECYCLE_RS,
            """        match init_arity {
            None => {}""",
            """        match init_arity {
            None => {}
            Some(_) if true => {}""",
        ),
        expect_red(
            "NC-6 census skipped at deploy",
            LIFECYCLE_RS,
            """        let report = census::census(code, &ModuleLimits::hosted());
        if let Some(ban) = report.ban() {
            return Err(LifecycleError::Refused(SandboxError::Censored(ban.clone())));
        }""",
            """        let report = census::census(code, &ModuleLimits::hosted());
        let _ = &report;""",
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
