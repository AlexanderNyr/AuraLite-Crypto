#!/usr/bin/env python3
"""Shared fail-closed harness for Rust negative controls.

A control passes only when the named unit test is actually run and reported
FAILED after its mutation. Cargo errors, zero-test filters, and unrelated test
failures do not count as a successful negative control.
"""
from __future__ import annotations

import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def focused_test(test: str) -> tuple[bool, str]:
    command = [
        "cargo", "test", "-p", "aura-consensus", "--lib", "--",
        "--exact", test,
    ]
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
    output = result.stdout + result.stderr
    ran_ok = result.returncode == 0 and re.search(
        rf"test\s+{re.escape(test)}\s+\.\.\.\s+ok", output
    ) is not None
    ran_failed = result.returncode != 0 and re.search(
        rf"test\s+{re.escape(test)}\s+\.\.\.\s+FAILED", output
    ) is not None
    return (ran_ok if result.returncode == 0 else ran_failed), output


def run_controls(phase: str, cases: list[tuple[str, str, str, str, str]]) -> int:
    """Run (label, relative source path, old, new, exact test-name) cases."""
    print(f"{phase}: validating focused baseline tests")
    for label, relpath, old, new, test in cases:
        path = ROOT / relpath
        original = path.read_text(encoding="utf-8")
        if old not in original:
            print(f"FAIL {label}: tamper target not found in {relpath}")
            return 1
        ok, output = focused_test(test)
        if not ok:
            print(f"FAIL {label}: target test did not pass before mutation")
            print(output[-5000:])
            return 1

    passed = 0
    for label, relpath, old, new, test in cases:
        path = ROOT / relpath
        original = path.read_text(encoding="utf-8")
        if old not in original:
            print(f"FAIL {label}: tamper target not found in {relpath}")
            return 1
        mutated = original.replace(old, new, 1)
        path.write_text(mutated, encoding="utf-8")
        try:
            red, output = focused_test(test)
        finally:
            path.write_text(original, encoding="utf-8")
        if red:
            passed += 1
            print(f"PASS {label}: {test} failed as expected")
        else:
            print(f"FAIL {label}: expected named test to fail: {test}")
            print(output[-5000:])
    print(f"RESULT: {passed}/{len(cases)} negative controls PASS")
    return 0 if passed == len(cases) else 1
