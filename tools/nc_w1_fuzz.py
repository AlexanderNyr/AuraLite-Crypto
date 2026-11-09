#!/usr/bin/env python3
"""Mutation-test the structured W1 decrypt fuzz oracle (not a security proof).

Temporarily mutates source; never run while another build uses this checkout.
Requires a C/C++ compiler for libfuzzer-sys. Restores source after each control.
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/aura-wallet/src/keystore.rs"
TEST = "keystore_decrypt::tests::all_modes_and_boundaries_are_exercised"


def run_test():
    return subprocess.run(
        ["cargo", "test", "--locked", "--manifest-path", "fuzz/Cargo.toml",
         "--lib", "--features", "insecure-wallet-fuzz", TEST, "--", "--exact"],
        cwd=ROOT, text=True, capture_output=True, check=False,
    )


def main():
    baseline = run_test()
    if baseline.returncode != 0 or f"test {TEST} ... ok" not in baseline.stdout:
        print("FAIL fuzz-driver baseline\n" + baseline.stdout[-3000:] + baseline.stderr[-3000:])
        return 1
    original = SOURCE.read_text(encoding="utf-8")
    mutations = [
        ("fuzz driver detects wrong-password bypass",
         "        self.decrypt_seed_inner(password, false, true)",
         '        if password.as_str().starts_with("wrong-public-fuzz-password-") { return Ok(SecretSeed::new([0x42; 64])); }\n'
         "        self.decrypt_seed_inner(password, false, true)"),
        ("fuzz driver detects missing AAD ID",
         '        map.remove("ciphertext");',
         '        map.remove("ciphertext");\n        map.remove("id");'),
        ("fuzz driver detects missing AAD seed_derivation",
         '        map.remove("ciphertext");',
         '        map.remove("ciphertext");\n        map.remove("seed_derivation");'),
        ("fuzz driver detects missing AAD kdf",
         '        map.remove("ciphertext");',
         '        map.remove("ciphertext");\n        map.remove("kdf");'),
    ]
    passed = 0
    for label, old, new in mutations:
        if original.count(old) != 1:
            print(f"FAIL {label}: mutation target not unique")
            continue
        try:
            SOURCE.write_text(original.replace(old, new, 1), encoding="utf-8")
            result = run_test()
        finally:
            SOURCE.write_text(original, encoding="utf-8")
        # A compiler error or unrelated test failure is not a valid control.
        ok = result.returncode != 0 and f"test {TEST} ... FAILED" in result.stdout
        print(f"{'PASS' if ok else 'FAIL'} {label}")
        if ok:
            passed += 1
        else:
            print((result.stdout + result.stderr)[-3000:])
    print(f"RESULT: {passed}/{len(mutations)} fuzz-oracle negative controls PASS")
    return 0 if passed == len(mutations) else 1


if __name__ == "__main__":
    sys.exit(main())
