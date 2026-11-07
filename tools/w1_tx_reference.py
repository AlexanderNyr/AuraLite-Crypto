#!/usr/bin/env python3
"""Independent PUBLIC TEST-ONLY W1 account -> C4 transaction vector.

The mnemonic, password, derived key and transaction are public. Never use this
fixture for funds. Python PBKDF2/HMAC implements BIP-39 and SLIP-0010; the
existing independent C4 Python encoder supplies wire framing; OpenSSL via
cryptography and a separate pure-Python implementation cross-check Ed25519.
No Rust production code is invoked. This is not a wallet signing API or audit.
"""
from __future__ import annotations

import hashlib
import hmac
import json
import sys
from pathlib import Path

from tools import gen_c4_vectors as c4
from tools.w1_interop_reference import (
    MNEMONIC_PASSPHRASE, PHRASE, Ed25519PrivateKey, Encoding, PublicFormat,
    bech32_address, blake3, seed_and_account,
)

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "crates/aura-wallet/tests/fixtures/w1_tx_reference.json"


def reference_vector() -> dict:
    # Self-check both reference primitives against published vectors.
    for length, expected in c4.OFFICIAL_B3.items():
        if c4.blake3_short(bytes(i % 251 for i in range(length))).hex() != expected:
            raise ValueError("pure-Python BLAKE3 reference failed official vector")
    for secret, public, message, signature in c4.RFC8032:
        key = bytes.fromhex(secret)
        if c4.ed25519_public_key(key).hex() != public or c4.ed25519_sign(bytes.fromhex(message), key).hex() != signature:
            raise ValueError("pure-Python Ed25519 reference failed RFC 8032")
    seed, expected_public, expected_address = seed_and_account(MNEMONIC_PASSPHRASE)
    standard_seed = hashlib.pbkdf2_hmac("sha512", PHRASE.encode(), ("mnemonic" + MNEMONIC_PASSPHRASE).encode(), 2048)
    if standard_seed != seed:
        raise ValueError("BIP-39 test seed mismatch")
    extended = hmac.new(b"ed25519 seed", seed, hashlib.sha512).digest()
    for index in (44, 1, 0, 0, 0):
        extended = hmac.new(extended[32:], b"\x00" + extended[:32] + (index | 0x80000000).to_bytes(4, "big"), hashlib.sha512).digest()
    # Public test-only signing key, never included in the output fixture.
    private = extended[:32]
    key = Ed25519PrivateKey.from_private_bytes(private)
    public = key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    if public.hex() != expected_public or c4.ed25519_public_key(private) != public:
        raise ValueError("independent Ed25519 derivations disagree")
    raw_address = c4.signer_address(public)
    if bech32_address(raw_address) != expected_address:
        raise ValueError("account address mismatch")
    case = dict(chain="auralite-test-1", nonce=7, kind=0, payload=b"public-w1-transfer-test", gas=21000, max_fee=123456789, tip=1000, expiry=1000, address=raw_address, public_key=public)
    unsigned, _ = c4.encode_unsigned(case)
    signing_bytes = c4.SIGNING_CONTEXT + unsigned
    digest_input = c4.domain_message(case["chain"], "tx", signing_bytes)
    digest = c4.blake3_short(digest_input)
    if digest != blake3.blake3(digest_input).digest():
        raise ValueError("BLAKE3 implementations disagree")
    signature = key.sign(digest)
    if signature != c4.ed25519_sign(digest, private):
        raise ValueError("Ed25519 implementations disagree")
    case["signature"] = signature
    tx = unsigned + c4.encode_signature(case)
    txid = c4.blake3_short(c4.TXID_CONTEXT + tx)
    if txid != blake3.blake3(c4.TXID_CONTEXT + tx).digest():
        raise ValueError("transaction ID implementations disagree")
    return {
        "notice": "PUBLIC TEST DATA ONLY - DO NOT USE FOR FUNDS",
        "chain": case["chain"], "index": 0, "path": "m/44'/1'/0'/0'/0'",
        "nonce": case["nonce"], "kind": case["kind"], "payload": case["payload"].hex(),
        "gas": case["gas"], "max_fee": case["max_fee"], "tip": case["tip"], "expiry": case["expiry"],
        "public_key": public.hex(), "address": expected_address,
        "unsigned": unsigned.hex(), "signing_bytes": signing_bytes.hex(),
        "signing_hash": digest.hex(), "signature": signature.hex(), "tx": tx.hex(), "txid": txid.hex(),
    }


def main() -> None:
    args = sys.argv[1:]
    if args not in ([], ["--write"]):
        raise SystemExit("usage: python3 -m tools.w1_tx_reference [--write]")
    result = reference_vector()
    if args:
        FIXTURE.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    else:
        if json.loads(FIXTURE.read_text(encoding="utf-8")) != result:
            raise SystemExit("W1 transaction reference fixture drift")
        print("PASS public W1 account to C4 transaction independent reference")


if __name__ == "__main__":
    main()
