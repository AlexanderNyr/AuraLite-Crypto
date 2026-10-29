#!/usr/bin/env python3
"""Negative controls for X4 (the Rust contract SDK).

Six tampers must each turn an X4 gate red:

  NC-1 `#[entrypoint]` exports the wrong name   -> a rebuilt contract is no longer
                                                  the pinned artifact (and the chain
                                                  would find no `run`).
  NC-2 the SDK ignores the absent-key status    -> a fresh slot reads as an error.
  NC-3 the codec stops length-checking          -> a u32 slot reads as a u64.
  NC-4 the mutable-global allowance is removed  -> every rustc contract is censored.
  NC-5 the element-table rule is switched off   -> an indirect call may reach an import.
  NC-6 the pinned artifact hash is not checked  -> a build that changed is undetected.

The last one is a control on the *tooling* (the pinned-hash gate), because an
artifact fixture that is never verified is decoration.

Rule (CONTRIBUTING.md §1): a gate that cannot go red is not a gate.

Usage:
    python3 tools/nc_x4.py
    make nc-x4
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
MACROS_RS = REPO / "crates" / "aura-sdk-macros" / "src" / "lib.rs"
SDK_ABI_RS = REPO / "crates" / "aura-sdk" / "src" / "abi.rs"
SDK_CODEC_RS = REPO / "crates" / "aura-sdk" / "src" / "codec.rs"
CENSUS_RS = REPO / "crates" / "aura-exec" / "src" / "census.rs"
FIXTURE = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x4_contracts.txt"

SDK_TEST = ["cargo", "test", "-p", "aura-sdk", "--quiet"]
ENGINE_TEST = ["cargo", "test", "-p", "aura-exec", "--test", "x4_contracts", "--quiet"]


def run(cmd: list[str] | str) -> subprocess.CompletedProcess[str]:
    if isinstance(cmd, str):
        return subprocess.run(cmd, shell=True, cwd=REPO, capture_output=True, text=True, check=False)
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(
    label: str, path: Path, old: str, new: str, cmd: list[str] | str
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
        os.utime(path, None)
        result = run(cmd)
        went_red = result.returncode != 0
        shown = cmd if isinstance(cmd, str) else " ".join(cmd)
        raw = "\n".join((result.stdout + result.stderr).strip().splitlines()[-2:])
        snippet = re.sub(r"finished in [\d.]+s", "finished", raw)
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      {shown} -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        os.utime(path, None)


def main() -> int:
    # No timestamp: this log is checksummed in docs/evidence/x4/SHA256SUMS.
    print("X4 NEGATIVE CONTROLS (contract SDK)")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 `#[entrypoint]` exports the wrong name",
            MACROS_RS,
            '            Self::Run => "run",',
            '            Self::Run => "main",',
            "python3 tools/build_contracts.py --check",
        ),
        expect_red(
            "NC-2 the SDK ignores the absent-key status",
            SDK_ABI_RS,
            "        STATUS_ABSENT => Ok(None),",
            "        STATUS_ABSENT => Ok(Some(0)),",
            SDK_TEST,
        ),
        expect_red(
            "NC-3 the codec stops length-checking",
            SDK_CODEC_RS,
            "    if raw.len() == len {\n        Ok(())\n    } else {",
            "    if true {\n        Ok(())\n    } else {",
            SDK_TEST,
        ),
        expect_red(
            "NC-4 the mutable-global allowance is removed",
            CENSUS_RS,
            "            max_mutable_globals: 1,",
            "            max_mutable_globals: 0,",
            ENGINE_TEST,
        ),
        expect_red(
            "NC-5 the element-table rule is switched off",
            CENSUS_RS,
            "            if function < report.imported_functions {",
            "            if false {",
            [
                "cargo",
                "test",
                "-p",
                "aura-exec",
                "--test",
                "x4_census",
                "--quiet",
            ],
        ),
        expect_red(
            "NC-6 the pinned artifact hash is not checked",
            FIXTURE,
            "contract_store_sha256=",
            "contract_store_sha256=0",
            "python3 tools/build_contracts.py --check",
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
