#!/usr/bin/env python3
"""Negative controls for C4 (aura-tx transaction format).

Six tamper tests must each turn the C4 gate red:

  NC-1: change SIGNING_CONTEXT (invalidates every signature/vector).
  NC-2: change TXID_CONTEXT (invalidates every txid).
  NC-3: corrupt one golden signature in the fixture.
  NC-4: disable the chain-id replay check in validate().
  NC-5: disable the signer<->public-key binding check.
  NC-6: bypass Ed25519 verification.

Files are always restored in a `finally` block. `make nc-c4` captures stdout
into docs/evidence/c4/nc.log, and a final green restore check is mandatory.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
TX_RS = REPO / "crates" / "aura-tx" / "src" / "tx.rs"
TEST_RS = REPO / "crates" / "aura-tx" / "tests" / "c4_tx.rs"
FIXTURE = REPO / "crates" / "aura-tx" / "tests" / "fixtures" / "c4_golden.txt"
CARGO_TEST = ["cargo", "test", "-p", "aura-tx", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(
    label: str,
    path: Path,
    old: str,
    new: str,
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
        result = run(CARGO_TEST)
        went_red = result.returncode != 0
        snippet = "\n".join((result.stdout + result.stderr).strip().splitlines()[-5:])
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      cargo test -p aura-tx -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        if touch_test:
            os.utime(TEST_RS, None)


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    print(f"C4 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 signing context",
            TX_RS,
            'pub const SIGNING_CONTEXT: &[u8] = b"AuraLite-Crypto/tx-signing-v1\\x00";',
            'pub const SIGNING_CONTEXT: &[u8] = b"WRONG-SIGNING\\x00";',
        ),
        expect_red(
            "NC-2 txid context",
            TX_RS,
            'pub const TXID_CONTEXT: &[u8] = b"AuraLite-Crypto/txid-v1\\x00";',
            'pub const TXID_CONTEXT: &[u8] = b"WRONG-TXID\\x00";',
        ),
        expect_red(
            "NC-3 independent golden signature",
            FIXTURE,
            "tx1_signature=3f0bd406",
            "tx1_signature=3f0bd407",
            touch_test=True,
        ),
        expect_red(
            "NC-4 replay protection",
            TX_RS,
            "if self.unsigned.chain_id.as_str() != expected_chain {",
            "if false {",
        ),
        expect_red(
            "NC-5 signer/public-key binding",
            TX_RS,
            "if signer_address(&self.sig.public_key) != self.unsigned.signer {",
            "if false {",
        ),
        expect_red(
            "NC-6 Ed25519 verification bypass",
            TX_RS,
            "aura_crypto::verify(pk, chain, Domain::Tx, msg, sig).map_err(|_| TxError::InvalidSignature)",
            "Ok(())",
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    if passed != len(results):
        return 1

    restored = run(CARGO_TEST)
    print(
        f"restore check: cargo test -p aura-tx -> rc={restored.returncode} "
        f"({'GREEN as expected' if restored.returncode == 0 else 'STILL RED — FAIL'})"
    )
    return 0 if restored.returncode == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
