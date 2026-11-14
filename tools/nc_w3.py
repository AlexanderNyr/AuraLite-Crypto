#!/usr/bin/env python3
"""W3 focused mutation controls; restore the source exactly after each test.

These controls test the tests, not the safety of the wallet. Never run while
another process is building the same checkout. Model: tools/nc_w2.py.

Control set (docs/w3-offline-plan.md):
  1. foreign-domain signature accepted -> gate red (the named W3 control)
  2. envelope checksum skipped -> gate red
  3. signer binding dropped in `tx-sign` -> gate red
  4. QR join accepts a missing chunk -> gate red
  5. QR join accepts a duplicate chunk -> gate red
  6. `--against` comparison skipped -> gate red
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OFFLINE = ROOT / "crates/aura-wallet/src/offline.rs"
CLI = ROOT / "crates/aura-wallet/src/cli.rs"


def run_test(test: str) -> subprocess.CompletedProcess[str]:
    cmd = [
        "cargo", "test", "-p", "aura-wallet",
        "--features", "insecure-test-kdf",
        "--test", "w3_offline", test, "--", "--exact",
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
        "NC: foreign-domain signature accepted -> gate red",
        "    // The W3 named control: only the fixed AuraLite `tx` domain verifies.\n"
        "    tx.verify_signature()\n"
        "        .map_err(|_| OfflineError::SignatureInvalid)?;",
        "    let _ = tx.verify_signature();",
        "verify_foreign_domain",
    ))
    checks.append(control(
        "NC: envelope checksum skipped -> gate red",
        "    let digest = blake3_hash(&bin[..header_len + body_len]);\n"
        "    if digest.as_bytes() != &bin[header_len + body_len..] {\n"
        "        return Err(OfflineError::ChecksumMismatch);\n"
        "    }",
        "    let digest = blake3_hash(&bin[..header_len + body_len]);\n"
        "    let _ = digest;",
        "sign_bad_checksum",
    ))
    checks.append(control(
        "NC: signer binding dropped -> gate red",
        "    if unsigned.signer != account.address {",
        "    if false {",
        "sign_signer_mismatch",
        source=CLI,
    ))
    checks.append(control(
        "NC: QR join accepts a missing chunk -> gate red",
        "        text.push_str(slot.as_ref().ok_or(OfflineError::QrMissingChunk)?);",
        "        text.push_str(slot.as_ref().map(String::as_str).unwrap_or_default());",
        "qr_missing_chunk",
    ))
    checks.append(control(
        "NC: QR join accepts a duplicate chunk -> gate red",
        "        if slots[i].is_some() {\n"
        "            return Err(OfflineError::QrDuplicateChunk);\n"
        "        }",
        "        if slots[i].is_some() && std::hint::black_box(false) {\n"
        "            return Err(OfflineError::QrDuplicateChunk);\n"
        "        }",
        "qr_duplicate_chunk",
    ))
    checks.append(control(
        "NC: `--against` comparison skipped -> gate red",
        "        if a_body != tx.unsigned.to_bytes() {\n"
        "            return Err(OfflineError::UnsignedMismatch);\n"
        "        }",
        "        if a_body != tx.unsigned.to_bytes() && std::hint::black_box(false) {\n"
        "            return Err(OfflineError::UnsignedMismatch);\n"
        "        }",
        "verify_against_mismatch",
    ))

    passed = sum(checks)
    total = len(checks)
    print(f"RESULT: {passed}/{total} negative controls PASS")
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
