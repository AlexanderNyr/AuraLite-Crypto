#!/usr/bin/env python3
"""Generate C7 account/state-key golden vectors from INDEPENDENT references.

No Rust code, no network. Contains pure-Python BLAKE3 (self-checked against
7 official BLAKE3-team vectors) and a Python transcription of the C7 key
layout, key digests and canonical account encoding.

Usage:
    python3 tools/gen_c7_vectors.py
    make gen-c7-vectors

Output: crates/aura-state/tests/fixtures/c7_golden.txt
"""

from __future__ import annotations

import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "c7_golden.txt"

STATE_KEY_CONTEXT = b"AuraLite-Crypto/state-key-v1\x00"
PREFIX_ACCOUNT = 0x01
PREFIX_CODE = 0x02
PREFIX_STORAGE = 0x03
PREFIX_STAKING = 0x04
PREFIX_RECEIPT = 0x05
MAX_STATE_KEY_LEN = 128
ZERO32 = bytes(32)

# ---------------------------------------------------------------- BLAKE3 ---
MASK = 0xFFFFFFFF
IV = [0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
      0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19]
PERM = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8]
CHUNK_START, CHUNK_END, ROOT = 1, 2, 8


def _rotr(x: int, n: int) -> int:
    return ((x >> n) | (x << (32 - n))) & MASK


def _g(s, a, b, c, d, mx, my):
    s[a] = (s[a] + s[b] + mx) & MASK
    s[d] = _rotr(s[d] ^ s[a], 16)
    s[c] = (s[c] + s[d]) & MASK
    s[b] = _rotr(s[b] ^ s[c], 12)
    s[a] = (s[a] + s[b] + my) & MASK
    s[d] = _rotr(s[d] ^ s[a], 8)
    s[c] = (s[c] + s[d]) & MASK
    s[b] = _rotr(s[b] ^ s[c], 7)


def _compress(cv, block, counter, block_len, flags):
    s = list(cv) + list(IV)
    s[12], s[13], s[14], s[15] = counter & MASK, (counter >> 32) & MASK, block_len, flags
    m = list(block)
    for r in range(7):
        _g(s, 0, 4, 8, 12, m[0], m[1]); _g(s, 1, 5, 9, 13, m[2], m[3])
        _g(s, 2, 6, 10, 14, m[4], m[5]); _g(s, 3, 7, 11, 15, m[6], m[7])
        _g(s, 0, 5, 10, 15, m[8], m[9]); _g(s, 1, 6, 11, 12, m[10], m[11])
        _g(s, 2, 7, 8, 13, m[12], m[13]); _g(s, 3, 4, 9, 14, m[14], m[15])
        if r != 6:
            m = [m[PERM[i]] for i in range(16)]
    return [s[i] ^ s[i + 8] for i in range(8)] + [s[i + 8] ^ cv[i] for i in range(8)]


def blake3_short(data: bytes) -> bytes:
    assert len(data) <= 1024, "reference handles a single chunk only"
    blocks = [data[i:i + 64] for i in range(0, len(data), 64)] or [b""]
    cv = IV
    for i, block in enumerate(blocks[:-1]):
        flags = CHUNK_START if i == 0 else 0
        cv = _compress(cv, list(struct.unpack("<16I", block.ljust(64, b"\0"))), 0, 64, flags)[:8]
    last = blocks[-1]
    flags = (CHUNK_START if len(blocks) == 1 else 0) | CHUNK_END
    words = list(struct.unpack("<16I", last.ljust(64, b"\0")))
    return struct.pack("<8I", *_compress(cv, words, 0, len(last), flags | ROOT)[:8])


OFFICIAL_B3 = {
    0: "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
    1: "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213",
    63: "e9bc37a594daad83be9470df7f7b3798297c3d834ce80ba85d6e207627b7db7b",
    64: "4eed7141ea4a5cd4b788606bd23f46e212af9cacebacdc7d1f4c6dc7f2511b98",
    65: "de1e5fa0be70df6d2be8fffd0e99ceaa8eb6e8c93a63f2d8d1c30ecb6b263dee",
    1023: "10108970eeda3eb932baac1428c7a2163b0e924c9a9e25b35bba72b28f70bd11",
    1024: "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7",
}

# ----------------------------------------------------------- C7 key layout ---
def varint(value: int) -> bytes:
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        out.append(byte | (0x80 if value else 0))
        if not value:
            return bytes(out)


def u64(value: int) -> bytes:
    return struct.pack("<Q", value)


def u128(value: int) -> bytes:
    return value.to_bytes(16, "little")


def addr(index: int) -> bytes:
    # First 8 bytes are the index, so the pattern has no short period and
    # 1000 (or 2**64) distinct indices always give distinct addresses.
    return index.to_bytes(8, "little") + bytes(
        (index * 11 + j * 29 + 0x3C) % 256 for j in range(12)
    )


def slot(index: int) -> bytes:
    return index.to_bytes(8, "little") + bytes(
        (index * 3 + j * 17 + 0x77) % 256 for j in range(24)
    )


def txid(index: int) -> bytes:
    return index.to_bytes(8, "little") + bytes(
        (index * 7 + j * 13 + 0xAB) % 256 for j in range(24)
    )


def key_digest(raw: bytes) -> bytes:
    return blake3_short(STATE_KEY_CONTEXT + u64(len(raw)) + raw)


def encode_key(raw: bytes) -> bytes:
    return varint(len(raw)) + raw


def key_account(index: int) -> bytes:
    return bytes([PREFIX_ACCOUNT]) + addr(index)


def key_code(index: int) -> bytes:
    return bytes([PREFIX_CODE]) + addr(index)


def key_storage(contract: int, slot_index: int) -> bytes:
    return bytes([PREFIX_STORAGE]) + addr(contract) + slot(slot_index)


def key_staking(index: int) -> bytes:
    return bytes([PREFIX_STAKING]) + addr(index)


def key_receipt(index: int) -> bytes:
    return bytes([PREFIX_RECEIPT]) + txid(index)


def encode_account(nonce: int, balance: int, code_hash: bytes, storage_root: bytes) -> bytes:
    return u64(nonce) + u128(balance) + code_hash + storage_root


ACCOUNTS = [
    (0, 0, ZERO32, ZERO32),
    (7, 5 * 10**18, bytes([0xAA] * 32), bytes([0xBB] * 32)),
    (2**64 - 1, 2**128 - 1, bytes([0xFF] * 32), bytes([0xEE] * 32)),
]


def main() -> int:
    for length, expected in OFFICIAL_B3.items():
        if blake3_short(bytes(i % 251 for i in range(length))).hex() != expected:
            raise SystemExit(f"pure-Python BLAKE3 disagrees with official vector len={length}")

    lines = [
        "# AuraLite-Crypto C7 account/state-key golden vectors",
        "# Generated by tools/gen_c7_vectors.py (independent Python).",
        "# Self-checked against official BLAKE3 vectors.",
        "# Format: key=<hex|decimal>. Do not edit by hand.",
        f"state_key_context={STATE_KEY_CONTEXT.hex()}",
        f"prefix_account={PREFIX_ACCOUNT}",
        f"prefix_code={PREFIX_CODE}",
        f"prefix_storage={PREFIX_STORAGE}",
        f"prefix_staking={PREFIX_STAKING}",
        f"prefix_receipt={PREFIX_RECEIPT}",
        f"len_account=21",
        f"len_code=21",
        f"len_storage=53",
        f"len_staking=21",
        f"len_receipt=33",
        f"max_state_key_len={MAX_STATE_KEY_LEN}",
    ]

    all_digests: list[bytes] = []
    for index in range(4):
        variants = {
            "account": key_account(index),
            "code": key_code(index),
            "storage": key_storage(index, index),
            "staking": key_staking(index),
            "receipt": key_receipt(index),
        }
        for name, raw in variants.items():
            digest = key_digest(raw)
            all_digests.append(digest)
            lines.append(f"key_{name}_{index}={raw.hex()}")
            lines.append(f"encoded_{name}_{index}={encode_key(raw).hex()}")
            lines.append(f"digest_{name}_{index}={digest.hex()}")
        lines.append(f"addr_{index}={addr(index).hex()}")
        lines.append(f"slot_{index}={slot(index).hex()}")
        lines.append(f"txid_{index}={txid(index).hex()}")

    if len(set(all_digests)) != len(all_digests):
        raise SystemExit("independent self-check failed: two state keys share a digest")

    for index, account in enumerate(ACCOUNTS, 1):
        lines.append(f"account_{index}_bytes={encode_account(*account).hex()}")

    # Contract-write isolation: contract A may write only its own storage key.
    lines.append(f"contract_a={addr(0).hex()}")
    lines.append(f"contract_b={addr(1).hex()}")
    lines.append(f"storage_key_a={key_storage(0, 0).hex()}")
    lines.append(f"storage_key_b={key_storage(1, 0).hex()}")

    valid = key_account(0)
    bad = {
        # Reserved prefixes can never be decoded into a StateKey.
        "bad_reserved_00": encode_key(bytes([0x00]) + bytes(20)),
        "bad_reserved_06": encode_key(bytes([0x06]) + bytes(20)),
        "bad_reserved_7f": encode_key(bytes([0x7F]) + bytes(20)),
        "bad_reserved_80": encode_key(bytes([0x80]) + bytes(20)),
        "bad_reserved_ff": encode_key(bytes([0xFF]) + bytes(20)),
        # Known prefix but wrong length.
        "bad_short_account": encode_key(bytes([PREFIX_ACCOUNT]) + bytes(19)),
        "bad_long_account": encode_key(bytes([PREFIX_ACCOUNT]) + bytes(21)),
        "bad_short_storage": encode_key(bytes([PREFIX_STORAGE]) + bytes(51)),
        # Encoding-level canonicality.
        "bad_empty": encode_key(b""),
        "bad_nonminimal_len": b"\x80\x00" + valid,
        "bad_trailing": encode_key(valid) + b"\x00",
        "bad_truncated": encode_key(valid)[:-1],
    }
    for key, value in bad.items():
        lines.append(f"{key}={value.hex()}")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  key variants: 5 namespaces x 4 indices = {len(all_digests)} distinct digests")
    print(f"  account vectors: {len(ACCOUNTS)}, rejection vectors: {len(bad)}")
    print("  self-check: BLAKE3 official vectors OK, no key collisions")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
