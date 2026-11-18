#!/usr/bin/env python3
"""H1 focused mutation controls; restore the sources exactly after each test.

These controls test the supply-chain gate, not the safety of the system.
Never run while another process is building the same checkout.
Model: tools/nc_h0.py.

Control set (docs/plans/SECURITY_PLAN.md row H1 + docs/h1-supply-chain-plan.md):
  1. remove deny -> gate red (the named H1 control)
  2. toolchain-pin check removed -> gate red
  3. lockfile digest check removed -> gate red
  4. sbom comparison removed -> gate red
  5. secret rule disabled -> gate red
  6. signature verification skipped -> gate red
"""
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "tools" / "check_supply_chain.py"
SECRETS = ROOT / "tools" / "scan_secrets.py"
SIGN = ROOT / "tools" / "sign_release.py"


def run_selftest() -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, "tools/check_supply_chain.py", "--selftest"],
                          cwd=ROOT, text=True, capture_output=True, check=False)


def mutate(path: Path, pattern: str, repl: str) -> None:
    text = path.read_text(encoding="utf-8")
    n = len(re.findall(pattern, text))
    if n != 1:
        raise SystemExit(f"FAIL: mutation target not unique in {path.name} ({pattern!r})")
    path.write_text(re.sub(pattern, repl, text, count=1), encoding="utf-8")


def control(label: str, target: Path, pattern: str, repl: str,
            expect: str, saved: dict[Path, str]) -> bool:
    if target not in saved:
        saved[target] = target.read_text(encoding="utf-8")
    mutate(target, pattern, repl)
    try:
        result = run_selftest()
    finally:
        for path, text in saved.items():
            path.write_text(text, encoding="utf-8")
    output = result.stdout + result.stderr
    ok = result.returncode != 0 and expect in output
    print(f"{'PASS' if ok else 'FAIL'} {label}: {expect}")
    if not ok:
        print(output[-2500:])
    return ok


def main() -> int:
    saved: dict[Path, str] = {}
    checks: list[bool] = []

    checks.append(control(
        "NC: remove deny -> gate red", CHECK,
        r"        if pkg\[\"name\"\] == adv\[\"crate\"\] and not _patched",
        "        if False and pkg[\"name\"] == adv[\"crate\"] and not _patched",
        "FAIL vulnerable_lock_denied", saved))
    checks.append(control(
        "NC: toolchain-pin check removed -> gate red", CHECK,
        r"    if channel != expected_channel:",
        "    if False and channel != expected_channel:",
        "FAIL toolchain_unpinned_denied", saved))
    checks.append(control(
        "NC: lockfile digest check removed -> gate red", CHECK,
        r"    if digest != vectors\[\"lock\"\]\[\"sha256\"\]:",
        "    if False and digest != vectors[\"lock\"][\"sha256\"]:",
        "FAIL lock_digest_mismatch_denied", saved))
    checks.append(control(
        "NC: sbom comparison removed -> gate red", CHECK,
        r"    if digest != vectors\[\"sbom\"\]\[\"sha256\"\]:",
        "    if False and digest != vectors[\"sbom\"][\"sha256\"]:",
        "FAIL sbom_drift_denied", saved))
    checks.append(control(
        "NC: secret rule disabled -> gate red", SECRETS,
        r"            if rx\.search\(line\):",
        "            if False and rx.search(line):",
        "FAIL planted_secret_detected", saved))
    checks.append(control(
        "NC: signature verification skipped -> gate red", SIGN,
        r"    return sign_message\(PREFIX \+ digest\) == sig",
        "    return True",
        "FAIL forged_signature_denied", saved))

    passed = sum(checks)
    total = len(checks)
    print(f"RESULT: {passed}/{total} negative controls PASS")
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
