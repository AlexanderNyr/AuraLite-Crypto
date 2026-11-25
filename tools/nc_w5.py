#!/usr/bin/env python3
"""W5 focused mutation controls; restore the source exactly after each test.

These controls test the tests, not the safety of the wallet. Never run while
another process is building the same checkout. Model: tools/nc_w4.py.

Control set (docs/w5-hardware-plan.md):
  1. blind confirmation accepted (`sign_blind` signs) -> gate red
     (the named W5 control: blind_confirmation_rejected)
  2. `Deny` ignored (a denial signs) -> gate red
  3. display binding dropped (host preview trusted) -> gate red
  4. chain check dropped -> gate red
  5. amount render altered -> gate red
  6. payload decode check dropped -> gate red
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
HW = ROOT / "crates/aura-wallet/src/hardware.rs"


def run_test(test: str) -> subprocess.CompletedProcess[str]:
    cmd = [
        "cargo", "test", "-p", "aura-wallet",
        "--features", "insecure-test-kdf",
        "--test", "w5_hardware", test, "--", "--exact",
    ]
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)


def control(label: str, old: str, new: str, test: str, source: Path = HW) -> bool:
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
        "NC: blind confirmation accepted -> gate red",
        "        let _ = body;\n"
        "        Err(HardwareError::BlindConfirmationRejected)",
        "        self.finish_sign(\n"
        "            &UnsignedTx::from_canonical_bytes(body)\n"
        "                .map_err(|_| HardwareError::UndecodablePayload)?,\n"
        "        )",
        "blind_confirmation_rejected",
    ))
    checks.append(control(
        "NC: denial ignored -> gate red",
        "            HumanDecision::Deny => Err(HardwareError::DeniedByUser),",
        "            HumanDecision::Deny => self.finish_sign(&unsigned),",
        "deny_transfer_basic",
    ))
    checks.append(control(
        "NC: display binding dropped -> gate red",
        "            if !claim.matches(&frame) {",
        "            if false {",
        "display_recipient_substitution",
    ))
    checks.append(control(
        "NC: chain check dropped -> gate red",
        "        if unsigned.chain_id.as_str() != self.expected_chain {",
        "        if false {",
        "foreign_chain_refused",
    ))
    checks.append(control(
        "NC: amount render altered -> gate red",
        '            lines.push(format!("display: amount={amount}"));',
        '            lines.push(format!("display: amount={amount} / 100"));',
        "confirm_transfer_high_value",
    ))
    checks.append(control(
        "NC: payload decode check dropped -> gate red",
        "                if unsigned.payload.len() != 28 {",
        "                if false {",
        "undecodable_payload_refused",
    ))

    passed = sum(checks)
    total = len(checks)
    print(f"RESULT: {passed}/{total} negative controls PASS")
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
