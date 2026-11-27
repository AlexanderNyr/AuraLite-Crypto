#!/usr/bin/env python3
"""O3 focused mutation controls; restore the source exactly after each test.

These controls test the tests, not the safety of the closed-testnet ops.
Never run while another process is building the same checkout. Model:
tools/nc_o2.py.

Control set (docs/o3-testnet-ops-plan.md):
  1. restore root-equality check dropped -> gate red (the named O3
     control: restore_mismatch_refused — "0 state losses" starts with
     "the restore restored")
  2. state-losses check dropped -> gate red
  3. uptime-days check dropped -> gate red
  4. restore-drills check dropped -> gate red
  5. restore root-count check dropped -> gate red
  6. unknown-line strictness dropped -> gate red (no commands in receipts)
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OPS = ROOT / "crates/aura-testkit/src/ops.rs"


def run_test(test: str) -> subprocess.CompletedProcess[str]:
    cmd = [
        "cargo", "test", "-p", "aura-testkit",
        "--test", "o3_testnet_ops", test, "--", "--exact",
    ]
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)


def control(label: str, old: str, new: str, test: str, source: Path = OPS) -> bool:
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
        "NC: restore root-equality check dropped -> gate red (named: restore_mismatch_refused)",
        "    if receipt.roots[0] != receipt.roots[1] {",
        "    if false {",
        "restore_mismatch_refused",
    ))
    checks.append(control(
        "NC: state-losses check dropped -> gate red",
        "    if losses != 0 {",
        "    if false {",
        "state_losses_refused",
    ))
    checks.append(control(
        "NC: uptime-days check dropped -> gate red",
        "    if days < UPTIME_DAYS {",
        "    if false {",
        "uptime_days_refused",
    ))
    checks.append(control(
        "NC: restore-drills check dropped -> gate red",
        "    if drills < 1 {",
        "    if false {",
        "restore_drills_refused",
    ))
    checks.append(control(
        "NC: restore root-count check dropped -> gate red",
        "    if receipt.roots.len() != 2 {",
        "    if false {",
        "restore_root_count_refused",
    ))
    checks.append(control(
        "NC: unknown-line strictness dropped -> gate red",
        '            return Err(format!("unknown receipt line: {line}"));',
        "            continue;",
        "foreign_ops_line_refused",
    ))

    print(f"RESULT: {sum(checks)}/{len(checks)} negative controls PASS")
    return 0 if all(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
