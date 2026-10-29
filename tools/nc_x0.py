#!/usr/bin/env python3
"""Negative controls for X0 (determinism census).

Six tamper tests must each turn the X0 gate red:

  NC-1 host imports allowed        -> import_wasi_clock is no longer banned.
  NC-2 float opcodes allowed       -> f64_add / f64_nan_const are accepted.
  NC-3 start section allowed       -> start_section is accepted.
  NC-4 memory maximum not required -> memory_no_max is accepted.
  NC-5 shared memory allowed       -> shared_memory is accepted.
  NC-6 unknown opcode allowed      -> the whitelist stops being fail-closed.

Rule (CONTRIBUTING.md §1): a gate that cannot go red is not a gate.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CENSUS_RS = REPO / "crates" / "aura-exec" / "src" / "census.rs"
TEST_RS = REPO / "crates" / "aura-exec" / "tests" / "x0_census.rs"

CARGO_TEST = ["cargo", "test", "-p", "aura-exec", "--test", "x0_census", "--quiet"]


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
        snippet = "\n".join((result.stdout + result.stderr).strip().splitlines()[-4:])
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      cargo test -p aura-exec --test x0_census -> rc={result.returncode} "
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
    print(f"X0 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 host imports allowed",
            CENSUS_RS,
            "        let allowed = limits.allow_abi_imports\n"
            "            && kind == 0x00\n"
            "            && (crate::abi::is_abi_import(&module, &name)\n"
            "                || crate::abi_v2::is_abi2_import(&module, &name)\n"
            "                || crate::abi_v3::is_abi3_import(&module, &name));\n"
            "        if !allowed && refused.is_none() {\n"
            "            refused = Some(Ban::HostImport { module, name });",
            "        let allowed = true;\n"
            "        let _ = (kind, &module, &name);\n"
            "        if !allowed && refused.is_none() {\n"
            "            refused = Some(Ban::HostImport { module, name });",
        ),
        expect_red(
            "NC-2 float policy disabled at its single choke point",
            CENSUS_RS,
            "fn float_ban(opcode: u8) -> Result<(), Ban> {\n    Err(Ban::FloatInstruction(opcode))",
            "fn float_ban(opcode: u8) -> Result<(), Ban> {\n    let _ = opcode; Ok(())",
        ),
        expect_red(
            "NC-3 start section allowed",
            CENSUS_RS,
            "            8 => return Err(Ban::StartSection),",
            "            8 => skip_section(&mut section)?,",
        ),
        expect_red(
            "NC-4 memory maximum not required",
            CENSUS_RS,
            "        let Some(max) = max else {\n"
            "            return Err(Ban::UnboundedMemory);\n"
            "        };",
            "        let Some(max) = max else {\n"
            "            report.memory_pages = Some(min);\n"
            "            continue;\n"
            "        };",
        ),
        expect_red(
            "NC-5 shared memory allowed",
            CENSUS_RS,
            "            0x03 => return Err(Ban::SharedMemory),\n"
            "            _ => return Err(Ban::BadSection(\"memory limits\")),",
            "            0x03 => Some(section.u32()?),\n"
            "            _ => return Err(Ban::BadSection(\"memory limits\")),",
        ),
        expect_red(
            "NC-6 unknown opcode allowed (whitelist not fail-closed)",
            CENSUS_RS,
            "            other => return Err(Ban::UnknownOpcode(other)),",
            "            _other => {}",
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
