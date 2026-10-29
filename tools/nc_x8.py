#!/usr/bin/env python3
"""Negative controls for X8 (the static verifier).

Six tampers, each one a mistake that looks harmless and destroys the claim "the
verifier refuses what the accepted subset bans". Every one of them turns a *named*
test in `crates/aura-exec/tests/x8_verify.rs` red, and the log records which test
that was — a gate that cannot go red is not a gate (CONTRIBUTING.md §1).

  NC-1 `call` stops reading the callee signature  -> a call is checked as if it took
                                                     no arguments and returned
                                                     nothing, so the stack tracking
                                                     is a count of opcodes again.
  NC-2 an operand-stack pop saturates instead of  -> the most common way a hand-written
       reporting an underflow                        body is wrong stops being a
                                                     refusal.
  NC-3 the float value types are read as `i32`    -> a module that can compute with
                                                     `f32`/`f64` passes, and the X0
                                                     float ban becomes decorative.
  NC-4 any export name counts as `run`            -> a module that exports something,
                                                     anything, deploys without the entry
                                                     point the lifecycle calls.
  NC-5 the `function` and `code` counts may        -> trailing bodies or a missing body
       disagree                                     slip through, and the engine picks
                                                     up the pieces at instantiation.
  NC-6 the module-size ceiling stops being         -> the deploy path accepts a module
       enforced                                      nobody bounded, which is the limit
                                                     the census exists to keep.

Each control runs one focused test rather than the whole file: the tamper must
redden *that* property, and running the covering test alone keeps the loop honest
and fast. A green baseline run of the full gate comes first, so a red caused by a
broken tree can never be mistaken for a successful control.

Usage:
    python3 tools/nc_x8.py
    make nc-x8
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
VERIFY = REPO / "crates" / "aura-exec" / "src" / "verify.rs"

GATE = ["cargo", "test", "-p", "aura-exec", "--test", "x8_verify", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str, test: str | None = None) -> bool:
    cmd = list(GATE)
    if test:
        cmd += ["--", "--exact", test]
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
        shown = test or "the whole gate"
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
    print("X8 NEGATIVE CONTROLS (static verifier gate)")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    print("baseline: the untampered gate")
    baseline = run(GATE)
    print(
        f"      rc={baseline.returncode} "
        f"({'GREEN as expected' if baseline.returncode == 0 else 'RED — fix the tree first'})"
    )
    if baseline.returncode != 0:
        tail = "\n".join((baseline.stdout + baseline.stderr).strip().splitlines()[-10:])
        print(tail)
        print("RESULT: 0/6 negative controls PASS (baseline is not green)")
        return 1
    print()

    results = [
        expect_red(
            "NC-1 `call` stops reading the callee signature",
            VERIFY,
            """                match signature {
                    Some(signature) => {
                        pop!(signature.params.len() as u32);
                        pushv!(signature.results.len() as u32);
                    }
                    None => pop!(0),
                }""",
            """                match signature {
                    Some(_) => pop!(0),
                    None => pop!(0),
                }""",
            "a_call_feeds_the_callee_signature_into_the_stack_check",
        ),
        expect_red(
            "NC-2 a stack underflow no longer reaches the report",
            VERIFY,
            """                if !dead {
                    if depth < count {
                        push(
                            report,
                            Code::StackUnderflow,
                            at,
                            format!("opcode 0x{opcode:02x} pops {count}, depth is {depth}"),
                        );
                        return;
                    }
                    depth -= count;
                }""",
            """                if !dead {
                    depth = depth.saturating_sub(count);
                }""",
            "code_stack_underflow",
        ),
        expect_red(
            "NC-3 the float value types are read as i32",
            VERIFY,
            "            0x7d | 0x7c => Err(Code::FloatType),",
            "            0x7d | 0x7c => Ok(ValType::I32),",
            "code_float_type",
        ),
        expect_red(
            "NC-4 any export name counts as `run`",
            VERIFY,
            """        if name == "run" {
            run = true;
        }""",
            """        {
            let _ = &name;
            run = true;
        }""",
            "code_export_shape_on_a_wrong_name",
        ),
        expect_red(
            "NC-5 the function and code counts may disagree",
            VERIFY,
            "    if !code_bodies.is_empty() && code_bodies.len() != function_types.len() {",
            "    if code_bodies.is_empty() && code_bodies.len() != function_types.len() {",
            "code_function_code_mismatch",
        ),
        expect_red(
            "NC-6 the module-size ceiling stops being enforced",
            VERIFY,
            """    if bytes.len() > limits.max_module_bytes {
        push(
            &mut report,
            Code::ModuleTooLarge,
            0,
            format!("{} bytes > {} limit", bytes.len(), limits.max_module_bytes),
        );
    }""",
            """    if bytes.len() > limits.max_module_bytes {
        let _ = format!("{} bytes > {} limit", bytes.len(), limits.max_module_bytes);
    }""",
            "code_module_too_large",
        ),
    ]

    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
