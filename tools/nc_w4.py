#!/usr/bin/env python3
"""W4 focused mutation controls; restore the source exactly after each test.

These controls test the tests, not the safety of the wallet. Never run while
another process is building the same checkout. Model: tools/nc_w3.py.

Control set (docs/w4-multisig-plan.md):
  1. threshold bypassed in `msig-collect` -> gate red (the named W4 control)
  2. approval signature verification skipped -> gate red
  3. duplicate approvals accepted -> gate red
  4. membership check dropped in `msig-approve` -> gate red
  5. expiry check dropped in `msig-approve` -> gate red
  6. proposal-binding comparison dropped -> gate red
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OFFLINE = ROOT / "crates/aura-wallet/src/multisig.rs"
CLI = ROOT / "crates/aura-wallet/src/cli.rs"


def run_test(test: str) -> subprocess.CompletedProcess[str]:
    cmd = [
        "cargo", "test", "-p", "aura-wallet",
        "--features", "insecure-test-kdf",
        "--test", "w4_multisig", test, "--", "--exact",
    ]
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)


def control(label: str, old: str, new: str, test: str, source: Path = OFFLINE) -> bool:
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
        "NC: threshold bypassed -> gate red",
        "    if collected < usize::from(descriptor.threshold) {",
        "    if false {",
        "collect_2_of_5_rejected",
        source=CLI,
    ))
    checks.append(control(
        "NC: approval signature verification skipped -> gate red",
        "    aura_crypto::verify_raw(&approval.member, &digest, &approval.sig)\n"
        "        .map_err(|_| MsigError::SignatureInvalid)",
        "    let _ = (&approval.member, &digest, &approval.sig);\n"
        "    Ok(())",
        "collect_forged_signature",
    ))
    checks.append(control(
        "NC: duplicate approvals accepted -> gate red",
        "        if seen.contains(&approval.member) {",
        "        if false {",
        "collect_duplicate_member",
        source=CLI,
    ))
    checks.append(control(
        "NC: membership check dropped -> gate red",
        "    if !descriptor.is_member(&account.public_key) {",
        "    if false {",
        "approve_not_a_member",
        source=CLI,
    ))
    checks.append(control(
        "NC: expiry check dropped -> gate red",
        "    let height = parse_u64(args.need(\"height\")?, \"height\")?;\n"
        "    if proposal.expiry < height {\n"
        "        return Err(msig_err(MsigError::ProposalExpired));\n"
        "    }\n"
        "    let password = read_password_file(args.need(\"password-file\")?)?;",
        "    let height = parse_u64(args.need(\"height\")?, \"height\")?;\n"
        "    let _ = height;\n"
        "    let password = read_password_file(args.need(\"password-file\")?)?;",
        "approve_expired",
        source=CLI,
    ))
    checks.append(control(
        "NC: proposal-binding comparison dropped -> gate red",
        "    if approval.proposal != expected {",
        "    if false {",
        "collect_foreign_proposal",
    ))

    passed = sum(checks)
    total = len(checks)
    print(f"RESULT: {passed}/{total} negative controls PASS")
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
