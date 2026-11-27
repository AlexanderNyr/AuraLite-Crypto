#!/usr/bin/env python3
"""O4 focused mutation controls; restore the source exactly after each test.

These controls test the tests, not the safety of the public testnet gate.
Never run while another process is building the same checkout. Model:
tools/nc_o3.py.

Control set (docs/o4-testnet-public-plan.md):
  1. criticals check dropped -> gate red (the named O4 control:
     unfixed_criticals_refused — "0 unfixed criticals")
  2. days check dropped -> gate red
  3. uptime check dropped -> gate red
  4. validators check dropped -> gate red
  5. txs check dropped -> gate red
  6. metrics regression check dropped -> gate red
  7. triage open-critical check dropped -> gate red
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
TESTNET = ROOT / "crates/aura-testkit/src/testnet.rs"


def run_test(test: str) -> subprocess.CompletedProcess[str]:
    cmd = [
        "cargo", "test", "-p", "aura-testkit",
        "--test", "o4_testnet_public", test, "--", "--exact",
    ]
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)


def control(label: str, old: str, new: str, test: str, source: Path = TESTNET) -> bool:
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
        "NC: criticals check dropped -> gate red (named: unfixed_criticals_refused)",
        "    if criticals != 0 {",
        "    if false {",
        "unfixed_criticals_refused",
    ))
    checks.append(control(
        "NC: days check dropped -> gate red",
        "    if days < DAYS {",
        "    if false {",
        "days_shortfall_refused",
    ))
    checks.append(control(
        "NC: uptime check dropped -> gate red",
        "    if uptime < UPTIME_PCT {",
        "    if false {",
        "uptime_shortfall_refused",
    ))
    checks.append(control(
        "NC: validators check dropped -> gate red",
        "    if validators < VALIDATORS {",
        "    if false {",
        "validators_shortfall_refused",
    ))
    checks.append(control(
        "NC: txs check dropped -> gate red",
        "    if txs < TXS {",
        "    if false {",
        "txs_shortfall_refused",
    ))
    checks.append(control(
        "NC: metrics regression check dropped -> gate red",
        "            if row.height < last.height || row.txs_total < last.txs_total {",
        "            if false {",
        "metrics_tx_regression_refused",
    ))
    checks.append(control(
        "NC: triage open-critical check dropped -> gate red",
        "        if is_open_critical(row) {",
        "        if false {",
        "triage_open_critical_refused",
    ))

    print(f"RESULT: {sum(checks)}/{len(checks)} negative controls PASS")
    return 0 if all(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
