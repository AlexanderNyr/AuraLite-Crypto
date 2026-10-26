#!/usr/bin/env python3
"""Negative controls for C3 (aura-crypto + dependency gate).

Five tamper tests must each turn a gate red:

  NC-1: change HASH_CONTEXT (domain separation).
  NC-2: corrupt a domain-hash golden vector.
  NC-3: corrupt an RFC 8032 Ed25519 golden vector.
  NC-4: bypass Ed25519 verification.
  NC-5: corrupt the external dependency allowlist (proves `make test`
        really runs tools/check_deps.py).

Every file is restored in a `finally` block. `make nc-c3` captures stdout into
docs/evidence/c3/nc.log, and a final green restore check is mandatory.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
HASH_RS = REPO / "crates" / "aura-crypto" / "src" / "hash.rs"
ED_RS = REPO / "crates" / "aura-crypto" / "src" / "ed25519.rs"
TEST_RS = REPO / "crates" / "aura-crypto" / "tests" / "c3_crypto.rs"
FIXTURE = REPO / "crates" / "aura-crypto" / "tests" / "fixtures" / "c3_golden.txt"
ALLOWLIST = REPO / "docs" / "spec" / "c3-dependency-allowlist.txt"

CARGO_TEST = ["cargo", "test", "-p", "aura-crypto", "--quiet"]
MAKE_TEST = ["make", "test"]

VERIFY_EXPR = (
    "    key.verify(message, &sig)\n"
    "        .map_err(|_| CryptoError::VerificationFailed)"
)


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(
    label: str,
    path: Path,
    old: str,
    new: str,
    cmd: list[str],
    touch_test: bool = False,
) -> bool:
    original = path.read_text(encoding="utf-8")
    if old not in original:
        print(f"{label}: FAIL — tamper target not found in {path.name}")
        return False
    tampered = original.replace(old, new, 1)
    if tampered == original:
        print(f"{label}: FAIL — tamper was a no-op")
        return False
    try:
        path.write_text(tampered, encoding="utf-8")
        if touch_test:
            os.utime(TEST_RS, None)
        result = run(cmd)
        went_red = result.returncode != 0
        snippet = "\n".join((result.stdout + result.stderr).strip().splitlines()[-5:])
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(f"      {' '.join(cmd)} -> rc={result.returncode} "
              f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})")
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        if touch_test:
            os.utime(TEST_RS, None)


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    print(f"C3 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 domain separation context",
            HASH_RS,
            'pub const HASH_CONTEXT: &[u8] = b"AuraLite-Crypto/crypto-v1\\x00";',
            'pub const HASH_CONTEXT: &[u8] = b"WRONG-CONTEXT\\x00";',
            CARGO_TEST,
        ),
        expect_red(
            "NC-2 independent domain-hash vector",
            FIXTURE,
            "domain_1_hash=7d4f8a30",
            "domain_1_hash=7d4f8a31",
            CARGO_TEST,
            touch_test=True,
        ),
        expect_red(
            "NC-3 RFC 8032 vector",
            FIXTURE,
            "ed2_signature=92a009a9",
            "ed2_signature=92a009a8",
            CARGO_TEST,
            touch_test=True,
        ),
        expect_red(
            "NC-4 Ed25519 verification bypass",
            ED_RS,
            VERIFY_EXPR,
            "    Ok(())",
            CARGO_TEST,
        ),
        expect_red(
            "NC-5 dependency allowlist / make test gate",
            ALLOWLIST,
            "blake3=1.8.7",
            "blake3=9.9.9",
            MAKE_TEST,
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    if passed != len(results):
        return 1

    restored = run(CARGO_TEST)
    deps = run(["python3", "tools/check_deps.py"])
    ok = restored.returncode == 0 and deps.returncode == 0
    print(f"restore check: cargo test -p aura-crypto -> rc={restored.returncode}; "
          f"check_deps -> rc={deps.returncode} "
          f"({'GREEN as expected' if ok else 'STILL RED — FAIL'})")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
