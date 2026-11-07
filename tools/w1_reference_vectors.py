#!/usr/bin/env python3
"""Independent, test-only D-W1-testnet public-key vectors.

Uses Python stdlib BIP-39 PBKDF2 and SLIP-0010 HMAC, and OpenSSL's Ed25519
implementation. The published mnemonic and passphrase are public test values.
No input is accepted: never run this tool with actual wallet secrets.
"""
from __future__ import annotations

import hashlib
import hmac
import subprocess
import unicodedata

PHRASE = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
PASSPHRASE = "TREZOR"
# RFC 8410 PKCS#8 wrapper for a 32-byte Ed25519 seed; public test data only.
ED25519_PKCS8_PREFIX = bytes.fromhex("302e020100300506032b657004220420")
PUBLIC_SPKI_PREFIX = bytes.fromhex("302a300506032b6570032100")


def main() -> None:
    phrase = unicodedata.normalize("NFKD", PHRASE).encode("utf-8")
    salt = ("mnemonic" + unicodedata.normalize("NFKD", PASSPHRASE)).encode("utf-8")
    seed = hashlib.pbkdf2_hmac("sha512", phrase, salt, 2048, dklen=64)
    for index in (0, 1, 42):
        extended = hmac.new(b"ed25519 seed", seed, hashlib.sha512).digest()
        for component in (44, 1, 0, 0, index):
            hardened = (component | 0x8000_0000).to_bytes(4, "big")
            extended = hmac.new(
                extended[32:], b"\x00" + extended[:32] + hardened, hashlib.sha512
            ).digest()
        result = subprocess.run(
            ["openssl", "pkey", "-inform", "DER", "-pubout", "-outform", "DER"],
            input=ED25519_PKCS8_PREFIX + extended[:32],
            capture_output=True,
            check=True,
        ).stdout
        if len(result) != len(PUBLIC_SPKI_PREFIX) + 32 or not result.startswith(PUBLIC_SPKI_PREFIX):
            raise ValueError("unexpected OpenSSL SubjectPublicKeyInfo encoding")
        print(f"m/44'/1'/0'/0'/{index}' {result[len(PUBLIC_SPKI_PREFIX):].hex()}")


if __name__ == "__main__":
    main()
