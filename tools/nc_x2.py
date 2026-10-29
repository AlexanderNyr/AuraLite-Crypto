#!/usr/bin/env python3
"""Negative controls for X2 (gas model + ABI v1).

Seven tamper tests must each turn the X2 gate red:

  NC-1 gas not charged at all         -> host-gas expectations no longer hold.
  NC-2 transfer charges nothing       -> the transfer vectors lose their bill.
  NC-3 max value length removed       -> value_too_long writes 4097 bytes.
  NC-4 rollback removed on a trap     -> a failed call commits a diff.
  NC-5 import census opened           -> the refusal vectors link and run.
  NC-6 out-of-gas guard removed       -> a caught -8 status still commits.
  NC-7 instruction gas never settled  -> the bill stays at the host part.

Rule (CONTRIBUTING.md §1): a gate that cannot go red is not a gate.

Usage:
    python3 tools/nc_x2.py
    make nc-x2
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
ABI_RS = REPO / "crates" / "aura-exec" / "src" / "abi.rs"
CONTRACT_RS = REPO / "crates" / "aura-exec" / "src" / "contract.rs"
WASMI_RS = REPO / "crates" / "aura-exec" / "src" / "wasmi_engine.rs"

CARGO_TEST = ["cargo", "test", "-p", "aura-exec", "--test", "x2_abi", "--quiet"]


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
            f"      cargo test -p aura-exec --test x2_abi -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        os.utime(path, None)


def main() -> int:
    # No timestamp on purpose: this log is checksummed in
    # docs/evidence/x2/SHA256SUMS, so re-running the controls must reproduce
    # the file byte for byte. The date lives in the phase report instead.
    print("X2 NEGATIVE CONTROLS (gas model + ABI v1)")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 gas table zeroed (every call free)",
            ABI_RS,
            """        self.base
            .saturating_add(self.per_byte.saturating_mul(bytes))""",
            """        self.base.saturating_add(0 * bytes)""",
        ),
        expect_red(
            "NC-2 transfer is not charged",
            ABI_RS,
            """            // Charged before the effect, and charged even when it fails: a
            // failed transfer still did the work of reading two accounts.
            charge(host, func, 0)?;""",
            """            charge(host, func, 0)?;
            host.host_gas = 0;""",
        ),
        expect_red(
            "NC-3 value length ceiling removed",
            ABI_RS,
            """            if value.len() > MAX_VALUE_LEN {
                return Err(AbiError::ValueTooLong);
            }""",
            """            if false {
                return Err(AbiError::ValueTooLong);
            }""",
        ),
        expect_red(
            "NC-4 rollback removed on failure",
            CONTRACT_RS,
            """    pub fn rollback(&mut self) {
        self.view.rollback();
    }""",
            """    pub fn rollback(&mut self) {}""",
        ),
        expect_red(
            "NC-5 import census opened (any import links)",
            WASMI_RS,
            """        let ok = func_import
            && ((import.module() == abi::ABI_MODULE && AbiFn::from_name(name).is_some())
                || (import.module() == abi_v2::ABI2_MODULE && Abi2Fn::from_name(name).is_some())
                || (import.module() == abi_v3::ABI3_MODULE && Abi3Fn::from_name(name).is_some()));""",
            """        let ok = true;
        let _ = (func_import, import.module(), name);""",
        ),
        expect_red(
            "NC-6 out-of-gas guard removed (a caught -8 commits)",
            CONTRACT_RS,
            """            Some(Outcome::Returned(value)) if !host.out_of_gas => (""",
            """            Some(Outcome::Returned(value)) => (""",
        ),
        expect_red(
            "NC-7 instruction gas never settled",
            WASMI_RS,
            """    host.settle_instructions(boundary.saturating_sub(current));""",
            """    let _ = (boundary, current);""",
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
