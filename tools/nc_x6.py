#!/usr/bin/env python3
"""Negative controls for X6 (call isolation).

Six tampers, each one a mistake a real implementer makes. Every one of them
turns `crates/aura-exec/tests/x6_calls.rs` red — the gate runs the shipped
contracts through the real engine, so these are measured against the same bytes a
deploy would store.

  NC-1 the reentrancy guard is removed      -> an attacker drains a vault that pays
                                               out before it debits.
  NC-2 the depth limit is removed           -> a chain of contracts descends past the
                                               published eight frames.
  NC-3 the callee's diff is not merged      -> the callee ran, its state is gone.
  NC-4 a failed callee's value is not returned -> value sent to a callee that
                                                 reverted is destroyed.
  NC-5 the callee's status is flattened     -> a caller cannot tell a refusal from
                                               success, and handles a failure as one.
  NC-6 the child frame does not inherit the
       parent's stack                       -> the guard checks an empty stack, which
                                               is the same as having no guard.

Rule (CONTRIBUTING.md §1): a gate that cannot go red is not a gate.

Usage:
    python3 tools/nc_x6.py
    make nc-x6
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
ABI_V3 = REPO / "crates" / "aura-exec" / "src" / "abi_v3.rs"
ENGINE_RS = REPO / "crates" / "aura-exec" / "src" / "wasmi_engine.rs"

CALLS_TEST = ["cargo", "test", "-p", "aura-exec", "--test", "x6_calls", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str, cmd: list[str]) -> bool:
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
        shown = " ".join(cmd)
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
    print("X6 NEGATIVE CONTROLS (call isolation)")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()
    results = [
        expect_red(
            "NC-1 the reentrancy guard is removed",
            ABI_V3,
            """    if frames.iter().any(|frame| frame == target) {
        return Err(AbiError::Reentrancy);
    }""",
            """    let _ = (frames, target);""",
            CALLS_TEST,
        ),
        expect_red(
            "NC-2 the depth limit is removed",
            ABI_V3,
            """    if frames.len() as u32 >= MAX_CALL_DEPTH {
        return Err(AbiError::CallDepth);
    }""",
            """    let _ = MAX_CALL_DEPTH;""",
            CALLS_TEST,
        ),
        expect_red(
            "NC-3 the callee's diff is not merged",
            ENGINE_RS,
            """    if let Some(diff) = &receipt.diff {
        host.merge_child(diff);
    }""",
            """    let _ = &receipt.diff;""",
            CALLS_TEST,
        ),
        expect_red(
            "NC-4 a failed callee's value is not returned",
            ENGINE_RS,
            """    if !receipt.status.is_success() {
        if let Some(undo) = undo {
            host.undo_value(undo);
        }""",
            """    if !receipt.status.is_success() {
        let _ = undo;""",
            CALLS_TEST,
        ),
        expect_red(
            "NC-5 the callee's status is flattened to success",
            ENGINE_RS,
            """        crate::contract::ExecStatus::Returned(value) => {
            i32::try_from(value).unwrap_or(AbiError::CalleeFailed.code())
        }""",
            """        crate::contract::ExecStatus::Returned(value) => {
            let _ = value;
            0
        }""",
            CALLS_TEST,
        ),
        expect_red(
            "NC-6 the child frame does not inherit the parent's stack",
            ENGINE_RS,
            """    let mut frames = host.frames.clone();
    frames.push(target);""",
            """    let mut frames = Vec::new();
    frames.push(target);""",
            CALLS_TEST,
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
