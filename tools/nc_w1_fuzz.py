#!/usr/bin/env python3
"""Mutation-test the structured W1 fuzz oracles (not a security proof).

Temporarily mutates source; never run while another build uses this checkout.
Requires a C/C++ compiler for libfuzzer-sys. Restores source after each control.
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
KEYSTORE = ROOT / "crates/aura-wallet/src/keystore.rs"
SCRYPT_FORK = ROOT / "third_party/scrypt-w1/src/lib.rs"
DECRYPT_TEST = "keystore_decrypt::tests::all_modes_and_boundaries_are_exercised"
PARSE_TEST = "keystore_parse::tests::accepted_documents_pass_strict_metamorphic_rejects"
RECOVERY_TEST = "recovery_metadata::tests::accepted_markers_pass_strict_metamorphic_and_profile_checks"
SCRYPT_TEST = "scrypt_differential::tests::fork_matches_upstream_on_deterministic_inputs"


def run_test(test):
    return subprocess.run(
        ["cargo", "test", "--locked", "--manifest-path", "fuzz/Cargo.toml",
         "--lib", "--features", "insecure-wallet-fuzz", test, "--", "--exact"],
        cwd=ROOT, text=True, capture_output=True, check=False,
    )


def main():
    for test in (DECRYPT_TEST, PARSE_TEST, RECOVERY_TEST, SCRYPT_TEST):
        baseline = run_test(test)
        if baseline.returncode != 0 or f"test {test} ... ok" not in baseline.stdout:
            print(f"FAIL fuzz-driver baseline {test}\n" + baseline.stdout[-3000:] + baseline.stderr[-3000:])
            return 1
    mutations = [
        ("fuzz driver detects wrong-password bypass",
         "        self.decrypt_seed_inner(password, false, true)",
         '        if password.as_str().starts_with("wrong-public-fuzz-password-") { return Ok(SecretSeed::new([0x42; 64])); }\n'
         "        self.decrypt_seed_inner(password, false, true)", DECRYPT_TEST, KEYSTORE),
        ("fuzz driver detects missing AAD ID",
         '        map.remove("ciphertext");',
         '        map.remove("ciphertext");\n        map.remove("id");', DECRYPT_TEST, KEYSTORE),
        ("fuzz driver detects missing AAD seed_derivation",
         '        map.remove("ciphertext");',
         '        map.remove("ciphertext");\n        map.remove("seed_derivation");', DECRYPT_TEST, KEYSTORE),
        ("fuzz driver detects missing AAD kdf",
         '        map.remove("ciphertext");',
         '        map.remove("ciphertext");\n        map.remove("kdf");', DECRYPT_TEST, KEYSTORE),
        ("parse fuzz driver detects extra-field bypass",
         "    if object.len() != fields.len() || !fields.iter().all(|key| object.contains_key(*key)) {",
         "    if !fields.iter().all(|key| object.contains_key(*key)) {", PARSE_TEST, KEYSTORE),
        ("parse fuzz driver detects duplicate-key bypass",
         "                    if !keys.insert(key) {",
         "                    if !keys.insert(key) && false {", PARSE_TEST, KEYSTORE),
        ("parse fuzz driver detects oversize bypass",
         "        if text.len() as u64 > MAX_KEYSTORE_BYTES {",
         "        if false && text.len() as u64 > MAX_KEYSTORE_BYTES {", PARSE_TEST, KEYSTORE),
        ("parse fuzz driver detects profile accessor bypass",
         "            .and_then(Bip39NormalizationProfile::from_id)",
         "            .map(|_| Bip39NormalizationProfile::Unicode15_1)", PARSE_TEST, KEYSTORE),
        ("recovery fuzz driver detects profile accessor bypass",
         "    pub const fn normalization_profile(self) -> Bip39NormalizationProfile {\n        self.normalization_profile\n    }",
         "    pub const fn normalization_profile(self) -> Bip39NormalizationProfile {\n        Bip39NormalizationProfile::Unicode15_1\n    }",
         RECOVERY_TEST, KEYSTORE),
        ("recovery fuzz driver detects size-cap bypass",
         "        if text.len() > 4096 {",
         "        if false && text.len() > 4096 {", RECOVERY_TEST, KEYSTORE),
        ("recovery fuzz driver detects unknown-profile accept",
         "            Bip39NormalizationProfile::from_id(id).ok_or(KeystoreError::Unsupported)?;",
         "            Bip39NormalizationProfile::from_id(id).unwrap_or(Bip39NormalizationProfile::Unicode15_1);",
         RECOVERY_TEST, KEYSTORE),
        ("recovery fuzz driver detects null-profile coercion",
         "            .as_str()\n            .ok_or(KeystoreError::RecoveryProfileMissing)?;",
         "            .as_str()\n            .or(Some(\"bip39-nfkd-unicode-15.1\"))\n            .ok_or(KeystoreError::RecoveryProfileMissing)?;",
         RECOVERY_TEST, KEYSTORE),
        ("recovery fuzz driver detects duplicate-key bypass",
         "                    if !keys.insert(key) {",
         "                    if !keys.insert(key) && false {", RECOVERY_TEST, KEYSTORE),
        ("recovery fuzz driver detects extra-field bypass",
         "    if object.len() != fields.len() || !fields.iter().all(|key| object.contains_key(*key)) {",
         "    if !fields.iter().all(|key| object.contains_key(*key)) {", RECOVERY_TEST, KEYSTORE),
        ("scrypt differential driver detects fork output divergence",
         "    pbkdf2_hmac::<Sha256>(password, &b, 1, output);\n    Ok(())",
         "    pbkdf2_hmac::<Sha256>(password, &b, 1, output);\n    output[0] ^= 1;\n    Ok(())",
         SCRYPT_TEST, SCRYPT_FORK),
    ]
    passed = 0
    for label, old, new, test, source in mutations:
        original = source.read_text(encoding="utf-8")
        if original.count(old) != 1:
            print(f"FAIL {label}: mutation target not unique")
            continue
        try:
            source.write_text(original.replace(old, new, 1), encoding="utf-8")
            result = run_test(test)
        finally:
            source.write_text(original, encoding="utf-8")
        # A compiler error or unrelated test failure is not a valid control.
        ok = result.returncode != 0 and f"test {test} ... FAILED" in result.stdout
        print(f"{'PASS' if ok else 'FAIL'} {label}")
        if ok:
            passed += 1
        else:
            print((result.stdout + result.stderr)[-3000:])
    print(f"RESULT: {passed}/{len(mutations)} fuzz-oracle negative controls PASS")
    return 0 if passed == len(mutations) else 1


if __name__ == "__main__":
    sys.exit(main())
