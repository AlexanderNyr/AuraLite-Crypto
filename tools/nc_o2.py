#!/usr/bin/env python3
"""O2 focused mutation controls; restore the source exactly after each test.

These controls test the tests, not the safety of the milestones. Never run
while another process is building the same checkout. Model: tools/nc_o1.py.

Control set (docs/o2-m0-m1-plan.md):
  1. root-equality check dropped -> gate red (the named O2 control:
     m0_root_drift_refused — "byte-identical state root on 2 runs")
  2. validator-count check dropped -> gate red
  3. desync check dropped -> gate red
  4. height check dropped -> gate red
  5. rpc-balance check dropped -> gate red
  6. unknown-line strictness dropped -> gate red (no commands in receipts)
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
MILESTONE = ROOT / "crates/aura-testkit/src/milestone.rs"


def run_test(test: str) -> subprocess.CompletedProcess[str]:
    cmd = [
        "cargo", "test", "-p", "aura-testkit",
        "--test", "o2_milestones", test, "--", "--exact",
    ]
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)


def control(label: str, old: str, new: str, test: str, source: Path = MILESTONE) -> bool:
    original = source.read_text(encoding="utf-8")
    if original.count(old) != 1:
        print(f"FAIL {label}: mutation target not unique")
        return False
    try:
        source.write_text(original.replace(old, new, 1), encoding="utf-8")
        result = run_test(test)
    finally:
        source.write_text(original, encoding="utf-8")
    # Compilation failure is NOT a valid red test.
    ok = result.returncode != 0 and f"test {test} ... FAILED" in result.stdout
    print(f"{'PASS' if ok else 'FAIL'} {label}: {test}")
    if not ok:
        print((result.stdout + result.stderr)[-2500:])
    return ok


def main() -> int:
    checks: list[bool] = []

    checks.append(control(
        "NC: root-equality check dropped -> gate red (named: m0_root_drift_refused)",
        "    if receipt.roots[0] != receipt.roots[1] {",
        "    if false {",
        "m0_root_drift_refused",
    ))
    checks.append(control(
        "NC: validator-count check dropped -> gate red",
        "    if validators != M1_VALIDATORS {",
        "    if false {",
        "m1_validator_count_refused",
    ))
    checks.append(control(
        "NC: desync check dropped -> gate red",
        "    if desync != 0 {",
        "    if false {",
        "m1_desync_refused",
    ))
    checks.append(control(
        "NC: height check dropped -> gate red",
        "    if height < M1_MIN_HEIGHT {",
        "    if false {",
        "m1_height_refused",
    ))
    checks.append(control(
        "NC: rpc-balance check dropped -> gate red",
        "    if rpc_balance <= 0 {",
        "    if false {",
        "m1_rpc_balance_refused",
    ))
    checks.append(control(
        "NC: unknown-line strictness dropped -> gate red",
        '            return Err(format!("unknown receipt line: {line}"));',
        "            continue;",
        "foreign_receipt_line_refused",
    ))

    print(f"RESULT: {sum(checks)}/{len(checks)} negative controls PASS")
    return 0 if all(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
