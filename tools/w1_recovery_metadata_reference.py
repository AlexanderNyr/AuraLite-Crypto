#!/usr/bin/env python3
"""Independent public W1 recovery-metadata parser/serializer oracle; no secrets.

Models only the candidate non-secret RecoveryMetadata JSON marker:
- UTF-8 input length <= 4096 bytes (otherwise invalid_format)
- strict RFC 8259 JSON object with no duplicate member names after unescaping
- exact single member "normalization_profile"
  * missing member or non-string member value -> missing_profile
  * extra members alongside "normalization_profile" -> invalid_format
- string value must be "bip39-nfkd-unicode-15.1" or "bip39-nfkd-unicode-18.0"
  (otherwise unsupported)
- canonical compact serialization {"normalization_profile":"<id>"}.

Without arguments it verifies the committed public fixture; --write regenerates it.
"""
from __future__ import annotations

import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates/aura-wallet/tests/fixtures/w1_recovery_metadata_reference.json"
PROFILES = ("bip39-nfkd-unicode-15.1", "bip39-nfkd-unicode-18.0")


def _unique_object(pairs):
    obj = {}
    for key, value in pairs:
        if key in obj:
            raise ValueError("duplicate member")
        obj[key] = value
    return obj


def _reject_constant(_):
    raise ValueError("non-JSON constant")


def _valid_unicode(value) -> bool:
    if isinstance(value, str):
        return all(not 0xD800 <= ord(ch) <= 0xDFFF for ch in value)
    if isinstance(value, list):
        return all(_valid_unicode(item) for item in value)
    if isinstance(value, dict):
        return all(_valid_unicode(k) and _valid_unicode(v) for k, v in value.items())
    return True


def evaluate(text: str) -> dict[str, object]:
    if len(text.encode("utf-8")) > 4096:
        return {"accepted": False, "error": "invalid_format", "profile": None, "canonical": None}
    try:
        doc = json.loads(text, object_pairs_hook=_unique_object, parse_constant=_reject_constant)
    except (ValueError, TypeError, RecursionError):
        return {"accepted": False, "error": "invalid_format", "profile": None, "canonical": None}
    if not _valid_unicode(doc) or not isinstance(doc, dict):
        return {"accepted": False, "error": "invalid_format", "profile": None, "canonical": None}
    if "normalization_profile" not in doc:
        return {"accepted": False, "error": "missing_profile", "profile": None, "canonical": None}
    if set(doc) != {"normalization_profile"}:
        return {"accepted": False, "error": "invalid_format", "profile": None, "canonical": None}
    profile = doc["normalization_profile"]
    if not isinstance(profile, str):
        return {"accepted": False, "error": "missing_profile", "profile": None, "canonical": None}
    if profile not in PROFILES:
        return {"accepted": False, "error": "unsupported", "profile": None, "canonical": None}
    canonical = json.dumps({"normalization_profile": profile}, ensure_ascii=True, separators=(",", ":"))
    return {"accepted": True, "error": None, "profile": profile, "canonical": canonical}


def generate() -> bytes:
    canonical_18 = '{"normalization_profile":"bip39-nfkd-unicode-18.0"}'
    canonical_15 = '{"normalization_profile":"bip39-nfkd-unicode-15.1"}'
    at_4095 = canonical_18 + (" " * (4095 - len(canonical_18)))
    at_4096 = canonical_18 + (" " * (4096 - len(canonical_18)))
    at_4097 = at_4096 + " "
    multibyte_4096 = '{"normalization_profile":"' + ("\u00e9" * 2034) + '"}'
    multibyte_4097 = '{"normalization_profile":"' + ("\u00e9" * 2034) + 'x"}'

    specs = [
        ("valid-canonical-15.1", canonical_15, True, None, "bip39-nfkd-unicode-15.1"),
        ("valid-canonical-18.0", canonical_18, True, None, "bip39-nfkd-unicode-18.0"),
        (
            "valid-escaped-member-name",
            r'{"normalization_\u0070rofile":"bip39-nfkd-unicode-18.0"}',
            True,
            None,
            "bip39-nfkd-unicode-18.0",
        ),
        (
            "valid-escaped-profile-value",
            r'{"normalization_profile":"bip39-nfkd-unicode-\u0031\u0035.1"}',
            True,
            None,
            "bip39-nfkd-unicode-15.1",
        ),
        (
            "valid-json-whitespace",
            " \n\t{ \"normalization_profile\" : \"bip39-nfkd-unicode-18.0\" }\r",
            True,
            None,
            "bip39-nfkd-unicode-18.0",
        ),
        ("valid-ascii-4095-bytes", at_4095, True, None, "bip39-nfkd-unicode-18.0"),
        ("valid-ascii-4096-bytes", at_4096, True, None, "bip39-nfkd-unicode-18.0"),
        ("reject-ascii-4097-bytes", at_4097, False, "invalid_format", None),
        ("reject-multibyte-utf8-4096-bytes-unsupported", multibyte_4096, False, "unsupported", None),
        ("reject-multibyte-utf8-4097-bytes-cap", multibyte_4097, False, "invalid_format", None),
        ("reject-empty-object", "{}", False, "missing_profile", None),
        ("reject-only-unrelated-field", '{"other":"bip39-nfkd-unicode-18.0"}', False, "missing_profile", None),
        ("reject-top-null", "null", False, "invalid_format", None),
        ("reject-top-array", "[]", False, "invalid_format", None),
        ("reject-top-number", "123", False, "invalid_format", None),
        ("reject-top-bool", "true", False, "invalid_format", None),
        ("reject-top-string", '"bip39-nfkd-unicode-18.0"', False, "invalid_format", None),
        ("reject-null-profile", '{"normalization_profile":null}', False, "missing_profile", None),
        ("reject-bool-profile", '{"normalization_profile":true}', False, "missing_profile", None),
        ("reject-number-profile", '{"normalization_profile":18}', False, "missing_profile", None),
        ("reject-array-profile", '{"normalization_profile":[]}', False, "missing_profile", None),
        ("reject-object-profile", '{"normalization_profile":{}}', False, "missing_profile", None),
        ("reject-empty-profile", '{"normalization_profile":""}', False, "unsupported", None),
        ("reject-future-profile", '{"normalization_profile":"bip39-nfkd-unicode-19.0"}', False, "unsupported", None),
        ("reject-uppercase-profile", '{"normalization_profile":"BIP39-nfkd-unicode-18.0"}', False, "unsupported", None),
        ("reject-leading-space-profile", '{"normalization_profile":" bip39-nfkd-unicode-18.0"}', False, "unsupported", None),
        ("reject-trailing-space-profile", '{"normalization_profile":"bip39-nfkd-unicode-18.0 "}', False, "unsupported", None),
        ("reject-valid-surrogate-pair-profile", r'{"normalization_profile":"\ud83d\udd10"}', False, "unsupported", None),
        (
            "reject-extra-field",
            '{"normalization_profile":"bip39-nfkd-unicode-18.0","extra":null}',
            False,
            "invalid_format",
            None,
        ),
        (
            "reject-duplicate-identical-key",
            '{"normalization_profile":"bip39-nfkd-unicode-18.0","normalization_profile":"bip39-nfkd-unicode-18.0"}',
            False,
            "invalid_format",
            None,
        ),
        (
            "reject-duplicate-escaped-key",
            r'{"normalization_profile":"bip39-nfkd-unicode-15.1","normalization_\u0070rofile":"bip39-nfkd-unicode-18.0"}',
            False,
            "invalid_format",
            None,
        ),
        (
            "reject-duplicate-escaped-key-reversed",
            r'{"normalization_\u0070rofile":"bip39-nfkd-unicode-18.0","normalization_profile":"bip39-nfkd-unicode-15.1"}',
            False,
            "invalid_format",
            None,
        ),
        ("reject-json-nan", '{"normalization_profile":NaN}', False, "invalid_format", None),
        ("reject-trailing-comma", '{"normalization_profile":"bip39-nfkd-unicode-18.0",}', False, "invalid_format", None),
        ("reject-trailing-json", '{"normalization_profile":"bip39-nfkd-unicode-18.0"}{}', False, "invalid_format", None),
        ("reject-unpaired-surrogate", r'{"normalization_profile":"\ud800"}', False, "invalid_format", None),
        ("reject-utf8-bom", "\ufeff" + canonical_18, False, "invalid_format", None),
    ]

    cases = []
    for name, doc, accepted, error, profile in specs:
        actual = evaluate(doc)
        expected_canonical = (
            json.dumps({"normalization_profile": profile}, ensure_ascii=True, separators=(",", ":"))
            if accepted
            else None
        )
        expected = {
            "accepted": accepted,
            "error": error,
            "profile": profile,
            "canonical": expected_canonical,
        }
        if actual != expected:
            raise ValueError(f"independent recovery-metadata oracle disagrees on {name}: {actual} != {expected}")
        cases.append({"name": name, "document": doc, **actual})
    return (json.dumps(cases, ensure_ascii=True, indent=2) + "\n").encode("utf-8")


def main() -> None:
    if sys.argv[1:] == ["--write"]:
        OUT.write_bytes(generate())
        print(f"WROTE public W1 recovery-metadata vectors: {OUT}")
    elif sys.argv[1:]:
        raise SystemExit("usage: python3 tools/w1_recovery_metadata_reference.py [--write]")
    elif not OUT.is_file() or OUT.read_bytes() != generate():
        raise SystemExit("FAIL independent W1 recovery-metadata vectors differ")
    else:
        count = len(json.loads(OUT.read_text(encoding="utf-8")))
        print(f"PASS independent Python W1 recovery-metadata vectors ({count} public cases)")


if __name__ == "__main__":
    main()
