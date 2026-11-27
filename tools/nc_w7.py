#!/usr/bin/env python3
"""W7 focused mutation controls; restore the source exactly after each test.

These controls test the tests, not the safety of the ceremony. Never run while
another process is building the same checkout. Model: tools/nc_w6.py.

Control set (docs/w7-genesis-ceremony-plan.md):
  1. shredding skipped -> gate red (the named W7 control:
     no_private_key_in_outputs — the O5 gate)
  2. threshold check dropped -> gate red (shamir_one_share_refused)
  3. witness chain check dropped -> gate red (witness_log_tamper_detected)
  4. pubkey-set binding dropped -> gate red (pubkey_set_mismatch_refused)
  5. reconstruction corrupted -> gate red (shamir_reconstructs_2_of_3)
  6. media-present check dropped -> gate red (seal_before_shred_refused)
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
CEREMONY = ROOT / "crates/aura-genesis/src/ceremony.rs"


def run_test(test: str) -> subprocess.CompletedProcess[str]:
    cmd = [
        "cargo", "test", "-p", "aura-genesis",
        "--test", "w7_ceremony", test, "--", "--exact",
    ]
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)


def control(label: str, old: str, new: str, test: str, source: Path = CEREMONY) -> bool:
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
        "NC: shredding skipped -> gate red (named: no_private_key_in_outputs)",
        "        shred_file(path).map_err(|_| \"io error\".to_string())?;",
        "        let _ = path;",
        "no_private_key_in_outputs",
    ))
    checks.append(control(
        "NC: threshold check dropped -> gate red",
        "    if (shares.len() as u8) < params.quorum {",
        "    if false {",
        "shamir_one_share_refused",
    ))
    checks.append(control(
        "NC: witness chain check dropped -> gate red",
        "        if hex(&next) != line.head {",
        "        if false {",
        "witness_log_tamper_detected",
    ))
    checks.append(control(
        "NC: pubkey-set binding dropped -> gate red",
        "    if witnessed != manifest.validators {",
        "    if false {",
        "pubkey_set_mismatch_refused",
    ))
    checks.append(control(
        "NC: reconstruction corrupted -> gate red",
        "        secret[byte] = acc;",
        "        secret[byte] = acc ^ 0x5a;",
        "shamir_reconstructs_2_of_3",
    ))
    checks.append(control(
        "NC: media-present check dropped -> gate red",
        "    if !scan_private(&dir).is_empty() {",
        "    if false {",
        "seal_before_shred_refused",
    ))

    print(f"RESULT: {sum(checks)}/{len(checks)} negative controls PASS")
    return 0 if all(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
