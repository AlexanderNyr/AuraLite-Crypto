#!/usr/bin/env python3
"""Independent public W1 parser-conformance matrix; no crypto or secrets.

This is a test oracle, not a production wallet parser. It deliberately models
only the currently documented v1/v2 schema and resource policy. Without
arguments it verifies committed fixtures; --write regenerates public vectors.
"""
from __future__ import annotations

import copy
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
FIX = ROOT / "crates/aura-wallet/tests/fixtures"
OUT = FIX / "w1_schema_reference.json"
PROFILES = {"bip39-nfkd-unicode-15.1", "bip39-nfkd-unicode-18.0"}
PATH = "m/44'/1'/0'/0'/0'"
TEMPLATE = "m/44'/1'/0'/0'/i'"


def unique_object(pairs):
    obj = {}
    for key, value in pairs:
        if key in obj:
            raise ValueError("duplicate member")
        obj[key] = value
    return obj


def reject_constant(_):
    raise ValueError("non-JSON constant")


def object_fields(value, fields):
    return isinstance(value, dict) and set(value) == set(fields)


def number(value, lo, hi):
    return type(value) is int and lo <= value <= hi


def hex_string(value, size):
    return isinstance(value, str) and len(value) == size * 2 and re.fullmatch(r"[0-9a-fA-F]+", value) is not None


def valid_unicode(value):
    if isinstance(value, str):
        return all(not 0xD800 <= ord(char) <= 0xDFFF for char in value)
    if isinstance(value, list):
        return all(valid_unicode(item) for item in value)
    if isinstance(value, dict):
        return all(valid_unicode(key) and valid_unicode(item) for key, item in value.items())
    return True


def accepts(text):
    try:
        if len(text.encode("utf-8")) > 1_048_576:
            return False
        doc = json.loads(text, object_pairs_hook=unique_object, parse_constant=reject_constant)
        if not valid_unicode(doc) or not isinstance(doc, dict) or type(doc.get("version")) is not int:
            return False
        version = doc["version"]
        top = {"format", "version", "id", "kdf", "cipher", "ciphertext", "derivation", "accounts"}
        if version == 2:
            top.add("seed_derivation")
        elif version != 1:
            return False
        if set(doc) != top or doc["format"] != "auralite-keystore" or not hex_string(doc["id"], 16):
            return False
        if version == 2:
            seed = doc["seed_derivation"]
            if not object_fields(seed, {"normalization_profile"}):
                return False
            profile = seed["normalization_profile"]
            if profile is not None and (not isinstance(profile, str) or profile not in PROFILES):
                return False
        cipher = doc["cipher"]
        if not object_fields(cipher, {"name", "nonce"}) or cipher["name"] != "aes-256-gcm" or not hex_string(cipher["nonce"], 12):
            return False
        if not hex_string(doc["ciphertext"], 80):
            return False
        deriv = doc["derivation"]
        if not object_fields(deriv, {"scheme", "coin_type", "path_template"}):
            return False
        if deriv["scheme"] != "slip10-ed25519" or type(deriv["coin_type"]) is not int or deriv["coin_type"] != 1 or deriv["path_template"] != TEMPLATE:
            return False
        accounts = doc["accounts"]
        if not isinstance(accounts, list) or len(accounts) != 1 or not object_fields(accounts[0], {"index", "path", "public_key", "address"}):
            return False
        account = accounts[0]
        if type(account["index"]) is not int or account["index"] != 0 or account["path"] != PATH or not hex_string(account["public_key"], 32):
            return False
        if not isinstance(account["address"], str) or not account["address"]:
            return False
        kdf = doc["kdf"]
        if not isinstance(kdf, dict) or not hex_string(kdf.get("salt"), 16):
            return False
        if kdf.get("name") == "argon2id":
            return (object_fields(kdf, {"name", "salt", "m_cost", "t_cost", "p_cost"})
                    and number(kdf["m_cost"], 19_456, 262_144)
                    and number(kdf["t_cost"], 2, 10)
                    and number(kdf["p_cost"], 1, 4))
        if kdf.get("name") == "scrypt":
            return (object_fields(kdf, {"name", "salt", "log_n", "r", "p"})
                    and number(kdf["log_n"], 14, 18) and number(kdf["r"], 8, 16)
                    and number(kdf["p"], 1, 4)
                    and 128 * (1 << kdf["log_n"]) * kdf["r"] <= 268_435_456)
        return False
    except (ValueError, TypeError, UnicodeError, RecursionError):
        return False


def generate():
    base = json.loads((FIX / "w1_interop_argon2id.json").read_text(encoding="utf-8"))
    legacy = json.loads((FIX / "w1_interop_scrypt_legacy.json").read_text(encoding="utf-8"))
    cases = []

    def add_raw(label, text, expected):
        actual = accepts(text)
        if actual != expected:
            raise ValueError(f"independent oracle disagrees with expected {label}: {actual}")
        cases.append({"name": label, "document": text, "accepted": expected})

    def altered(label, path, value, expected, source=base):
        doc = copy.deepcopy(source)
        target = doc
        for part in path[:-1]:
            target = target[part]
        if value is MISSING:
            del target[path[-1]]
        else:
            target[path[-1]] = value
        add_raw(label, json.dumps(doc, ensure_ascii=True, separators=(",", ":")), expected)

    class Missing:
        pass
    MISSING = Missing()
    raw = json.dumps(base, ensure_ascii=True, separators=(",", ":"))
    add_raw("valid-v2-argon", raw, True)
    add_raw("valid-v1-legacy-scrypt-parse-only", json.dumps(legacy), True)
    for name in ("argon2id_profile_null", "argon2id_profile_15_1", "scrypt", "scrypt_profile_null", "scrypt_profile_15_1"):
        add_raw("valid-" + name, (FIX / f"w1_interop_{name}.json").read_text(encoding="utf-8"), True)
    add_raw("duplicate-top-escaped-key", raw.replace('"format":', '"\\u0066ormat":"auralite-keystore","format":', 1), False)
    add_raw("duplicate-nested-escaped-key", raw.replace('"nonce":', '"\\u006eonce":"0102030405060708090a0b0c","nonce":', 1), False)
    add_raw("trailing-garbage", raw + "x", False)
    add_raw("truncated-json", raw[:-1], False)
    add_raw("json-nan", raw.replace('"version":2', '"version":NaN', 1), False)
    add_raw("json-surrogate", raw.replace('"format":"auralite-keystore"', '"format":"\\ud800"', 1), False)
    add_raw("json-surrogate-address", raw.replace(json.dumps(base["accounts"][0]["address"]), '"\\ud800"', 1), False)
    add_raw("json-valid-surrogate-pair-address", raw.replace(json.dumps(base["accounts"][0]["address"]), '"\\ud83d\\udd10"', 1), True)
    altered("missing-format", ["format"], MISSING, False)
    altered("unknown-top", ["unexpected"], 1, False)
    altered("version-zero", ["version"], 0, False)
    altered("version-three", ["version"], 3, False)
    altered("version-float", ["version"], 2.0, False)
    altered("version-bool", ["version"], True, False)
    altered("format-unknown", ["format"], "other", False)
    altered("id-short", ["id"], "0" * 30, False)
    altered("id-upper-hex", ["id"], "AB" * 16, True)
    altered("id-not-hex", ["id"], "g" * 32, False)
    altered("cipher-extra", ["cipher", "extra"], 1, False)
    altered("cipher-unknown", ["cipher", "name"], "aes-128-gcm", False)
    altered("nonce-short", ["cipher", "nonce"], "00" * 11, False)
    altered("ciphertext-short", ["ciphertext"], "00" * 79, False)
    altered("ciphertext-upper-hex", ["ciphertext"], "AB" * 80, True)
    altered("missing-profile", ["seed_derivation", "normalization_profile"], MISSING, False)
    altered("extra-profile-field", ["seed_derivation", "extra"], 1, False)
    altered("unknown-profile", ["seed_derivation", "normalization_profile"], "unknown", False)
    altered("profile-bool", ["seed_derivation", "normalization_profile"], True, False)
    altered("profile-null", ["seed_derivation", "normalization_profile"], None, True)
    altered("v1-with-v2-marker", ["seed_derivation"], {"normalization_profile": None}, False, legacy)
    altered("coin-type-bool", ["derivation", "coin_type"], True, False)
    altered("coin-type-wrong", ["derivation", "coin_type"], 2, False)
    altered("path-template-wrong", ["derivation", "path_template"], "m/44'/1'/0'/0'/i", False)
    altered("accounts-empty", ["accounts"], [], False)
    altered("accounts-extra", ["accounts"], [base["accounts"][0], base["accounts"][0]], False)
    altered("account-index-bool", ["accounts", 0, "index"], False, False)
    altered("account-index-one", ["accounts", 0, "index"], 1, False)
    altered("account-path-wrong", ["accounts", 0, "path"], "m/44'/1'/0'/0'/1'", False)
    altered("account-key-short", ["accounts", 0, "public_key"], "00" * 31, False)
    altered("account-key-uppercase-parse-only", ["accounts", 0, "public_key"], "AB" * 32, True)
    altered("account-address-empty", ["accounts", 0, "address"], "", False)
    altered("account-address-untrusted", ["accounts", 0, "address"], "not-a-valid-address", True)
    altered("kdf-extra", ["kdf", "extra"], 1, False)
    altered("salt-short", ["kdf", "salt"], "00" * 15, False)
    altered("argon-memory-below", ["kdf", "m_cost"], 19_455, False)
    altered("argon-memory-min", ["kdf", "m_cost"], 19_456, True)
    altered("argon-memory-max", ["kdf", "m_cost"], 262_144, True)
    altered("argon-memory-above", ["kdf", "m_cost"], 262_145, False)
    altered("argon-memory-overflow", ["kdf", "m_cost"], 2**32, False)
    altered("argon-iterations-below", ["kdf", "t_cost"], 1, False)
    altered("argon-iterations-max", ["kdf", "t_cost"], 10, True)
    altered("argon-iterations-above", ["kdf", "t_cost"], 11, False)
    altered("argon-parallelism-bool", ["kdf", "p_cost"], True, False)
    altered("argon-parallelism-above", ["kdf", "p_cost"], 5, False)
    altered("scrypt-logn-below", ["kdf", "log_n"], 13, False, legacy)
    altered("scrypt-logn-min-legacy", ["kdf", "log_n"], 14, True, legacy)
    altered("scrypt-logn-17", ["kdf", "log_n"], 17, True, legacy)
    altered("scrypt-logn-18", ["kdf", "log_n"], 18, True, legacy)
    altered("scrypt-logn-above", ["kdf", "log_n"], 19, False, legacy)
    altered("scrypt-logn-u8-overflow", ["kdf", "log_n"], 256, False, legacy)
    altered("scrypt-r-below", ["kdf", "r"], 7, False, legacy)
    altered("scrypt-r-above", ["kdf", "r"], 17, False, legacy)
    altered("scrypt-p-below", ["kdf", "p"], 0, False, legacy)
    altered("scrypt-p-above", ["kdf", "p"], 5, False, legacy)
    high = copy.deepcopy(legacy)
    high["kdf"]["log_n"] = 18
    altered("scrypt-memory-over-cap", ["kdf", "r"], 9, False, high)
    return (json.dumps(cases, ensure_ascii=True, indent=2) + "\n").encode("utf-8")


def main():
    if sys.argv[1:] == ["--write"]:
        OUT.write_bytes(generate())
        print(f"WROTE public W1 schema vectors: {OUT}")
    elif sys.argv[1:]:
        raise SystemExit("usage: python3 tools/w1_schema_reference.py [--write]")
    elif not OUT.is_file() or OUT.read_bytes() != generate():
        raise SystemExit("FAIL independent W1 schema vectors differ")
    else:
        print(f"PASS independent Python W1 schema vectors ({len(json.loads(OUT.read_text()))} public cases)")


if __name__ == "__main__":
    main()
