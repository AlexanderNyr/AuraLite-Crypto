#!/usr/bin/env python3
"""W2 focused mutation controls; restore the source exactly after each test.

These controls test the tests, not the safety of the wallet. Never run while
another process is building the same checkout. Model: tools/nc_w1.py.

Control set (docs/w2-cli-plan.md §5):
  1. foreign chain-id accepted in `sign` -> gate red   (the named W2 control)
  2. ballot chain binding dropped -> gate red
  3. password echoed into stdout -> gate red
  4. broadcast signature verification skipped -> gate red
  5. stake payload encoding order swapped -> gate red
  6. nonce replay accepted -> gate red
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / "crates/aura-wallet/src/cli.rs"


def run_test(test: str) -> subprocess.CompletedProcess[str]:
    cmd = [
        "cargo", "test", "-p", "aura-wallet",
        "--features", "insecure-test-kdf",
        "--test", "w2_cli", test, "--", "--exact",
    ]
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)


def control(label: str, old: str, new: str, test: str, source: Path = CLI) -> bool:
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
        "NC: foreign chain-id accepted -> gate red",
        "    // The named W2 control: a tx built for a foreign chain is refused before signing.\n"
        "    require_chain(params.chain_id.as_str(), chain)?;",
        "    let _ = (params.chain_id.as_str(), chain);",
        "sign_foreign_chain",
    ))
    checks.append(control(
        "NC: ballot chain binding dropped -> gate red",
        "        if self.chain != expected_chain {",
        "        if false {",
        "vote_verify_foreign",
    ))
    checks.append(control(
        "NC: password echoed in output -> gate red",
        '    let _ = writeln!(out, "from={}", account.address.to_bech32());',
        '    let _ = writeln!(out, "password={}", password.as_str());\n'
        '    let _ = writeln!(out, "from={}", account.address.to_bech32());',
        "transfer_happy",
    ))
    checks.append(control(
        "NC: signature verification skipped on broadcast -> gate red",
        '    tx.verify_signature()\n'
        '        .map_err(|_| CliError::Refused("tx signature does not verify".to_string()))?;',
        "    let _ = &tx;",
        "broadcast_tampered_sig",
    ))
    checks.append(control(
        "NC: stake payload encoding order swapped -> gate red",
        "    out.push(action.tag());\n    out.extend_from_slice(validator.as_bytes());",
        "    out.extend_from_slice(validator.as_bytes());\n    out.push(action.tag());",
        "stake_delegate",
    ))
    checks.append(control(
        "NC: nonce replay accepted -> gate red",
        "        if tx.unsigned.nonce != Nonce::new(nonce) {",
        "        if false {",
        "broadcast_replay_refused",
    ))

    passed = sum(checks)
    print(f"RESULT: {passed}/{len(checks)} negative controls PASS")
    return 0 if passed == len(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
