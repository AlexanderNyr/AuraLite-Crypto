#!/usr/bin/env python3
"""Public NFKD/PBKDF2 reference, NOT a complete BIP-39 mnemonic validator.

A BIP-39 mnemonic sentence uses one ASCII space between English words after
NFKD. Noncanonical whitespace cases intentionally compute PBKDF2 over the raw
normalized bytes only to demonstrate why silently rejoining is ambiguous;
they are not valid inputs accepted by the W1 API. No external packages needed.
"""
import hashlib
import json
from pathlib import Path
import sys
import unicodedata

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates/aura-wallet/tests/fixtures/w1_mnemonic_reference.json"
PHRASE = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
PASSPHRASE = "TREZOR"


def generate():
    fullwidth = "".join(chr(ord(c) + 0xFEE0) if "a" <= c <= "z" else c for c in PHRASE)
    cases = [
        ("canonical", PHRASE, True),
        ("ideographic-separator-nfkd", PHRASE.replace(" ", "\u3000"), True),
        ("fullwidth-letters-nfkd", fullwidth, True),
        ("double-space", PHRASE.replace(" ", "  ", 1), False),
        ("tab", PHRASE.replace(" ", "\t", 1), False),
        ("newline", PHRASE.replace(" ", "\n", 1), False),
        ("leading-space", " " + PHRASE, False),
        ("trailing-space", PHRASE + " ", False),
    ]
    result = []
    for label, phrase, accepted in cases:
        normalized = unicodedata.normalize("NFKD", phrase).encode("utf-8")
        salt = ("mnemonic" + unicodedata.normalize("NFKD", PASSPHRASE)).encode("utf-8")
        raw = hashlib.pbkdf2_hmac("sha512", normalized, salt, 2048).hex()
        result.append({"name": label, "phrase": phrase, "passphrase": PASSPHRASE,
                       "accepted": accepted, "raw_normalized_pbkdf2_hex": raw})
    canonical = result[0]["raw_normalized_pbkdf2_hex"]
    if any((r["raw_normalized_pbkdf2_hex"] == canonical) != r["accepted"] for r in result):
        raise ValueError("unexpected NFKD/PBKDF2 relation in reference cases")
    return (json.dumps(result, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def main():
    if sys.argv[1:] == ["--write"]:
        OUT.write_bytes(generate())
        print(f"WROTE public mnemonic whitespace reference: {OUT}")
        return 0
    if sys.argv[1:]:
        print("usage: python3 tools/w1_mnemonic_reference.py [--write]", file=sys.stderr)
        return 2
    if not OUT.is_file() or OUT.read_bytes() != generate():
        print("FAIL public mnemonic whitespace reference vectors differ", file=sys.stderr)
        return 1
    print("PASS independent Python NFKD/PBKDF2 mnemonic whitespace reference (8 public cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
