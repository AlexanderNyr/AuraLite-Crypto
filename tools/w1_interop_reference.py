#!/usr/bin/env python3
"""Independent W1 keystore interop vectors; PUBLIC TEST DATA ONLY.

Requires optional test dependencies listed in tools/w1-reference-requirements.txt.
The fixed password, seed, salt, nonce, and ID are intentionally public and must
NEVER be used for real wallets. Python's scrypt, argon2-cffi and cryptography's
AESGCM/Ed25519 are independent implementations. Run without arguments to check
fixtures and a fresh Rust-generated keystore in each direction; run --write to
regenerate the public deterministic fixtures.
"""
from __future__ import annotations

import hashlib
import hmac
import json
import subprocess
import sys
import unicodedata
from pathlib import Path

# This oracle deliberately uses assertions for its cryptographic comparisons.
# Python -O/-OO (or PYTHONOPTIMIZE) strips them; fail closed instead of
# presenting an unchecked result as a successful interoperability test.
if not __debug__:
    raise SystemExit("W1 interop checks require Python assertions; disable optimization")

import blake3
from argon2.low_level import Type, hash_secret_raw
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "crates/aura-wallet/tests/fixtures"
PHRASE = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
MNEMONIC_PASSPHRASE = "TREZOR"
PASSWORD = b"public-interop-password"
UNICODE_PASSWORD = "public-café-🔐".encode("utf-8")
UNICODE_PASSWORD_NFD = unicodedata.normalize("NFD", UNICODE_PASSWORD.decode()).encode("utf-8")
UNICODE_MNEMONIC_PASSPHRASE = "pässphraseÅ"
SALT = bytes.fromhex("00112233445566778899aabbccddeeff")
NONCE = bytes.fromhex("0102030405060708090a0b0c")
IDENTIFIER = "102132435465768798a9bacbdcedfe0f"
CHARSET = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"
GEN = (0x3B6A57B2, 0x26508E6D, 0x1EA119FA, 0x3D4233DD, 0x2A1462B3)


def seed_and_account(passphrase: str = MNEMONIC_PASSPHRASE) -> tuple[bytes, str, str]:
    phrase = unicodedata.normalize("NFKD", PHRASE).encode("utf-8")
    salt = ("mnemonic" + unicodedata.normalize("NFKD", passphrase)).encode("utf-8")
    seed = hashlib.pbkdf2_hmac("sha512", phrase, salt, 2048)
    ext = hmac.new(b"ed25519 seed", seed, hashlib.sha512).digest()
    for index in (44, 1, 0, 0, 0):
        ext = hmac.new(ext[32:], b"\0" + ext[:32] + (index | 0x80000000).to_bytes(4, "big"), hashlib.sha512).digest()
    public = Ed25519PrivateKey.from_private_bytes(ext[:32]).public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    if passphrase == MNEMONIC_PASSPHRASE:
        assert public.hex() == "80304784f9593856e1ca54b514ec68397b001f88b534fc3862405b9e368ed8ff"
    raw = blake3.blake3(b"AuraLite-Crypto/address-v1\0" + public).digest()[:20]
    return seed, public.hex(), bech32_address(raw)


def bech32_address(raw: bytes) -> str:
    payload = b"\0" + raw
    acc = bits = 0
    data = []
    for byte in payload:
        acc = (acc << 8) | byte
        bits += 8
        while bits >= 5:
            bits -= 5
            data.append((acc >> bits) & 31)
    if bits:
        data.append((acc << (5 - bits)) & 31)
    hrp = "aura"
    values = [ord(c) >> 5 for c in hrp] + [0] + [ord(c) & 31 for c in hrp] + data + [0] * 6
    polymod = 1
    for v in values:
        top = polymod >> 25
        polymod = ((polymod & 0x1FFFFFF) << 5) ^ v
        for i, g in enumerate(GEN):
            if (top >> i) & 1:
                polymod ^= g
    polymod ^= 1
    checksum = [(polymod >> (5 * (5 - i))) & 31 for i in range(6)]
    return hrp + "1" + "".join(CHARSET[x] for x in data + checksum)


def key_for(kdf: dict, password: bytes) -> bytes:
    salt = bytes.fromhex(kdf["salt"])
    if kdf["name"] == "scrypt":
        return hashlib.scrypt(password, salt=salt, n=1 << kdf["log_n"], r=kdf["r"], p=kdf["p"], dklen=32, maxmem=384 * 1024 * 1024)
    if kdf["name"] == "argon2id":
        return hash_secret_raw(password, salt, kdf["t_cost"], kdf["m_cost"], kdf["p_cost"], 32, Type.ID, version=19)
    raise ValueError("unsupported test-vector KDF")


def aad_for(doc: dict) -> bytes:
    header = {k: v for k, v in doc.items() if k != "ciphertext"}
    return json.dumps(header, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def make_fixture(kind: str) -> dict:
    unicode_case = kind.endswith("_unicode")
    passphrase = UNICODE_MNEMONIC_PASSPHRASE if unicode_case else MNEMONIC_PASSPHRASE
    password = UNICODE_PASSWORD if unicode_case else PASSWORD
    seed, public, address = seed_and_account(passphrase)
    salt = (bytes.fromhex("ab112233445566778899aabbccddeeff") if kind == "argon2id_unicode"
            else bytes.fromhex("cd112233445566778899aabbccddeeff") if kind == "scrypt_unicode"
            else SALT)
    nonce = (bytes.fromhex("ab02030405060708090a0b0c") if kind == "argon2id_unicode"
             else bytes.fromhex("cd02030405060708090a0b0c") if kind == "scrypt_unicode"
             else NONCE)
    identifier = ("ab2132435465768798a9bacbdcedfe0f" if kind == "argon2id_unicode"
                  else "cd2132435465768798a9bacbdcedfe0f" if kind == "scrypt_unicode"
                  else IDENTIFIER)
    if kind in ("scrypt", "scrypt_unicode"):
        kdf = {"name": "scrypt", "salt": salt.hex(), "log_n": 17, "r": 8, "p": 1}
    elif kind == "scrypt_legacy":
        # Reproduce the pre-hardening, public weak-scrypt migration fixture only.
        kdf = {"name": "scrypt", "salt": SALT.hex(), "log_n": 14, "r": 8, "p": 1}
    elif kind in ("argon2id", "argon2id_unicode"):
        kdf = {"name": "argon2id", "salt": salt.hex(), "m_cost": 19456, "t_cost": 2, "p_cost": 1}
    else:
        raise ValueError(kind)
    is_legacy = kind == "scrypt_legacy"
    doc = {
        "format": "auralite-keystore", "version": 1 if is_legacy else 2, "id": identifier,
        "kdf": kdf, "cipher": {"name": "aes-256-gcm", "nonce": nonce.hex()},
        **({} if is_legacy else {"seed_derivation": {"normalization_profile": "bip39-nfkd-unicode-18.0"}}),
        "derivation": {"scheme": "slip10-ed25519", "coin_type": 1, "path_template": "m/44'/1'/0'/0'/i'"},
        "accounts": [{"index": 0, "path": "m/44'/1'/0'/0'/0'", "public_key": public, "address": address}],
    }
    doc["ciphertext"] = AESGCM(key_for(kdf, password)).encrypt(nonce, seed, aad_for(doc)).hex()
    return doc


def make_inconsistent_fixture(field: str) -> dict:
    # Public test credentials only. Deliberately build a valid GCM tag for a
    # document whose account metadata does not belong to the encrypted seed.
    doc = make_fixture("argon2id")
    seed, public, address = seed_and_account(MNEMONIC_PASSPHRASE)
    if field == "public_key":
        doc["id"] = "11111111111111111111111111111111"
        doc["kdf"]["salt"] = "112233445566778899aabbccddeeff00"
        doc["cipher"]["nonce"] = "112233445566778899aabbcc"
        doc["accounts"][0]["public_key"] = ("0" if public[0] != "0" else "1") + public[1:]
    elif field == "address":
        doc["id"] = "22222222222222222222222222222222"
        doc["kdf"]["salt"] = "2233445566778899aabbccddeeff0011"
        doc["cipher"]["nonce"] = "2233445566778899aabbccdd"
        other_address = seed_and_account("another-public-passphrase")[2]
        assert other_address != address
        doc["accounts"][0]["address"] = other_address
    else:
        raise ValueError(field)
    doc.pop("ciphertext")
    nonce = bytes.fromhex(doc["cipher"]["nonce"])
    doc["ciphertext"] = AESGCM(key_for(doc["kdf"], PASSWORD)).encrypt(nonce, seed, aad_for(doc)).hex()
    return doc


def verify_inconsistent(doc: dict, field: str) -> None:
    # A successful independent AES-GCM decrypt proves rejection is due to the
    # metadata/seed mismatch, not a wrong password or stale authentication tag.
    seed, public, address = seed_and_account(MNEMONIC_PASSPHRASE)
    plaintext = AESGCM(key_for(doc["kdf"], PASSWORD)).decrypt(
        bytes.fromhex(doc["cipher"]["nonce"]), bytes.fromhex(doc["ciphertext"]), aad_for(doc)
    )
    assert plaintext == seed
    account = doc["accounts"][0]
    assert (account["public_key"] != public) == (field == "public_key")
    assert (account["address"] != address) == (field == "address")


def verify(doc: dict, unicode_case: bool = False) -> None:
    if doc["version"] == 1:
        assert "seed_derivation" not in doc, "legacy v1 profile must remain unknown"
    else:
        assert doc["version"] == 2
        assert doc["seed_derivation"] == {
            "normalization_profile": "bip39-nfkd-unicode-18.0"
        }
    password = UNICODE_PASSWORD if unicode_case else PASSWORD
    seed, public, address = seed_and_account(UNICODE_MNEMONIC_PASSPHRASE if unicode_case else MNEMONIC_PASSPHRASE)
    plaintext = AESGCM(key_for(doc["kdf"], password)).decrypt(bytes.fromhex(doc["cipher"]["nonce"]), bytes.fromhex(doc["ciphertext"]), aad_for(doc))
    assert plaintext == seed, "seed mismatch"
    assert doc["accounts"] == [{"index": 0, "path": "m/44'/1'/0'/0'/0'", "public_key": public, "address": address}]
    try:
        AESGCM(key_for(doc["kdf"], UNICODE_PASSWORD_NFD if unicode_case else b"wrong-public-password")).decrypt(bytes.fromhex(doc["cipher"]["nonce"]), bytes.fromhex(doc["ciphertext"]), aad_for(doc))
    except Exception as exc:
        from cryptography.exceptions import InvalidTag
        if not isinstance(exc, InvalidTag):
            raise
    else:
        raise AssertionError("wrong password unexpectedly authenticated")


def main() -> None:
    write = sys.argv[1:] == ["--write"]
    if sys.argv[1:] and not write:
        raise SystemExit("usage: python3 tools/w1_interop_reference.py [--write]")
    for kind in ("argon2id", "scrypt", "scrypt_legacy", "argon2id_unicode", "scrypt_unicode"):
        path = FIXTURES / f"w1_interop_{kind}.json"
        doc = make_fixture(kind)
        if write:
            path.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        else:
            assert json.loads(path.read_text(encoding="utf-8")) == doc, f"fixture drift: {path}"
            verify(doc, kind.endswith("_unicode"))
            if kind == "scrypt_legacy":
                print("PASS legacy weak-scrypt fixture (Python; Rust migration test is separate)")
                continue
            output = subprocess.run(
                ["cargo", "run", "--quiet", "-p", "aura-wallet", "--example", "w1_interop_fixture", "--", kind],
                cwd=ROOT, text=True, capture_output=True, check=True,
            ).stdout
            verify(json.loads(output), kind.endswith("_unicode"))
            print(f"PASS {kind}: Python-generated fixture and Rust-generated random keystore")
    for field in ("public_key", "address"):
        path = FIXTURES / f"w1_inconsistent_{field}.json"
        doc = make_inconsistent_fixture(field)
        if write:
            path.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        else:
            assert json.loads(path.read_text(encoding="utf-8")) == doc, f"fixture drift: {path}"
            verify_inconsistent(doc, field)
            print(f"PASS inconsistent {field}: valid Python GCM tag and deliberately wrong account metadata")
    print("PUBLIC TEST VECTORS ONLY; no interoperability security certification")


if __name__ == "__main__":
    main()
