#!/usr/bin/env python3
"""W6 focused mutation controls; restore the source exactly after each test.

These controls test the tests, not the safety of the wallet. Never run while
another process is building the same checkout. Model: tools/nc_w5.py.

Control set (docs/w6-web-wallet-plan.md):
  1. recipient comparison dropped -> gate red (the named W6 control:
     ui_recipient_substitution)
  2. amount comparison dropped -> gate red
  3. homoglyph (non-ASCII host) check dropped -> gate red (the lookalike then
     falls through to `untrusted origin`)
  4. deep-link whitelist dropped -> gate red
  5. blind signing accepted -> gate red
  6. recipient line rendered from the signer, not the bytes -> gate red
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
WEB = ROOT / "crates/aura-wallet/src/web.rs"


def run_test(test: str) -> subprocess.CompletedProcess[str]:
    cmd = [
        "cargo", "test", "-p", "aura-wallet",
        "--features", "insecure-test-kdf",
        "--test", "w6_web", test, "--", "--exact",
    ]
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)


def control(label: str, old: str, new: str, test: str, source: Path = WEB) -> bool:
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
        "NC: recipient comparison dropped -> gate red",
        "        if claim.recipient.is_some_and(|r| r != self.recipient) {",
        "        if false {",
        "ui_recipient_substitution",
    ))
    checks.append(control(
        "NC: amount comparison dropped -> gate red",
        "        if claim.amount.is_some_and(|a| a != self.amount) {",
        "        if false {",
        "ui_amount_substitution",
    ))
    checks.append(control(
        "NC: homoglyph check dropped -> gate red",
        "    if !host.is_ascii() {",
        "    if false {",
        "origin_lookalike_refused",
    ))
    checks.append(control(
        "NC: deep-link whitelist dropped -> gate red",
        "            _ => return Err(WebError::UnknownParameter),",
        "            _ => (),",
        "deep_link_unknown_param",
    ))
    checks.append(control(
        "NC: blind signing accepted -> gate red",
        "    Err(WebError::UiConfirmationSkipped)\n}",
        "    sign_confirmed(&derive_view(_envelope, \"auralite-test-1\")?, _secret)\n}",
        "blind_ui_sign_refused",
    ))
    checks.append(control(
        "NC: recipient line rendered from the signer -> gate red",
        '        format!("ui: to={}", recipient.to_bech32()),',
        '        format!("ui: to={}", unsigned.signer.to_bech32()),',
        "ui_recipient_substitution",
    ))

    passed = sum(checks)
    total = len(checks)
    print(f"RESULT: {passed}/{total} negative controls PASS")
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
