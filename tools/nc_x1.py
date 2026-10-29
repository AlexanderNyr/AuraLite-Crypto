#!/usr/bin/env python3
"""Negative controls for X1 (sandbox).

Six tamper tests must each turn the X1 gate red:

  NC-1 census bypassed            -> banned modules reach the engine.
  NC-2 fuel budget ignored        -> the pinned fuel numbers no longer hold.
  NC-3 memory ceiling removed     -> memory.grow past the ceiling succeeds.
  NC-4 trap misclassified         -> out-of-fuel is reported as `trap_other`.
  NC-5 module-size check removed  -> an oversized module is parsed anyway.
  NC-6 trace cap removed          -> the trace is unbounded.

Rule (CONTRIBUTING.md §1): a gate that cannot go red is not a gate.

Usage:
    python3 tools/nc_x1.py
    make nc-x1
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SANDBOX_RS = REPO / "crates" / "aura-exec" / "src" / "sandbox.rs"
WASMI_RS = REPO / "crates" / "aura-exec" / "src" / "wasmi_engine.rs"

CARGO_TEST = ["cargo", "test", "-p", "aura-exec", "--test", "x1_sandbox", "--quiet"]


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
        snippet = "\n".join((result.stdout + result.stderr).strip().splitlines()[-3:])
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      cargo test -p aura-exec --test x1_sandbox -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        os.utime(path, None)


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    print(f"X1 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 census bypassed (gate always Ok)",
            SANDBOX_RS,
            "        let census_limits = ModuleLimits::restrictive();",
            "        if true { return Ok(()); }\n        let census_limits = ModuleLimits::restrictive();",
        ),
        expect_red(
            "NC-2 fuel budget ignored (4x the published budget)",
            WASMI_RS,
            "        store\n            .set_fuel(limits.fuel)",
            "        store\n            .set_fuel(limits.fuel * 4)",
        ),
        expect_red(
            "NC-3 memory ceiling removed from the store limiter",
            WASMI_RS,
            "                .memory_size(limits.memory_bytes())",
            "                .memory_size(usize::MAX / 2)",
        ),
        expect_red(
            "NC-4 trap misclassified (out-of-fuel -> other)",
            WASMI_RS,
            "        TrapCode::OutOfFuel => TrapKind::OutOfFuel,",
            "        TrapCode::OutOfFuel => TrapKind::Other,",
        ),
        expect_red(
            "NC-5 module-size check removed",
            SANDBOX_RS,
            "        if wasm.len() > self.limits.module_bytes as usize {\n            return Err(SandboxError::Limit(\"module_bytes\"));\n        }",
            "        if false {\n            return Err(SandboxError::Limit(\"module_bytes\"));\n        }",
        ),
        expect_red(
            "NC-6 trace cap removed",
            WASMI_RS,
            "    if state.trace.len() < state.max_trace {",
            "    if true {",
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
