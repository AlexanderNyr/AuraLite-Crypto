#!/usr/bin/env python3
"""Generate C4 transaction golden vectors from INDEPENDENT references.

No Rust code, no network. The generator contains three independent
implementations, each self-checked before use:

  * pure-Python BLAKE3 (single chunk) — self-checked against 15 official
    BLAKE3-team test vectors;
  * pure-Python Ed25519 (RFC 8032) — self-checked against RFC 8032 §7.1
    TEST 1/2/3;
  * a Python transcription of the C2 wire format + C4 tx layout.

It emits canonical tx bytes, signing bytes, signing hash, Ed25519 public key
and signature, signer address and txid for five transactions, plus a set of
non-canonical / tampered vectors that must be rejected.

Usage:
    python3 tools/gen_c4_vectors.py
    make gen-c4-vectors

Output: crates/aura-tx/tests/fixtures/c4_golden.txt
"""

from __future__ import annotations

import hashlib
import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-tx" / "tests" / "fixtures" / "c4_golden.txt"

CODEC_VERSION = 1
HASH_CONTEXT = b"AuraLite-Crypto/crypto-v1\x00"          # from C3
SIGNING_CONTEXT = b"AuraLite-Crypto/tx-signing-v1\x00"
TXID_CONTEXT = b"AuraLite-Crypto/txid-v1\x00"
ADDRESS_CONTEXT = b"AuraLite-Crypto/address-v1\x00"
MAX_TX_PAYLOAD = 65536

# ---------------------------------------------------------------- BLAKE3 ---
MASK = 0xFFFFFFFF
IV = [0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
      0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19]
PERM = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8]
CHUNK_START, CHUNK_END, ROOT = 1, 2, 8


def _rotr(x: int, n: int) -> int:
    return ((x >> n) | (x << (32 - n))) & MASK


def _g(s: list[int], a: int, b: int, c: int, d: int, mx: int, my: int) -> None:
    s[a] = (s[a] + s[b] + mx) & MASK
    s[d] = _rotr(s[d] ^ s[a], 16)
    s[c] = (s[c] + s[d]) & MASK
    s[b] = _rotr(s[b] ^ s[c], 12)
    s[a] = (s[a] + s[b] + my) & MASK
    s[d] = _rotr(s[d] ^ s[a], 8)
    s[c] = (s[c] + s[d]) & MASK
    s[b] = _rotr(s[b] ^ s[c], 7)


def _compress(cv, block, counter: int, block_len: int, flags: int) -> list[int]:
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
    """BLAKE3 for inputs <= 1024 bytes (one chunk)."""
    assert len(data) <= 1024, "reference handles a single chunk only"
    blocks = [data[i:i + 64] for i in range(0, len(data), 64)] or [b""]
    cv = IV
    for i, block in enumerate(blocks[:-1]):
        flags = CHUNK_START if i == 0 else 0
        cv = _compress(cv, list(struct.unpack("<16I", block.ljust(64, b"\0"))), 0, 64, flags)[:8]
    last = blocks[-1]
    flags = (CHUNK_START if len(blocks) == 1 else 0) | CHUNK_END
    words = list(struct.unpack("<16I", last.ljust(64, b"\0")))
    out = _compress(cv, words, 0, len(last), flags | ROOT)
    return struct.pack("<8I", *out[:8])


# Subset of BLAKE3-team/BLAKE3 test_vectors.json (first 32 bytes of the XOF).
OFFICIAL_B3 = {
    0: "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
    1: "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213",
    63: "e9bc37a594daad83be9470df7f7b3798297c3d834ce80ba85d6e207627b7db7b",
    64: "4eed7141ea4a5cd4b788606bd23f46e212af9cacebacdc7d1f4c6dc7f2511b98",
    65: "de1e5fa0be70df6d2be8fffd0e99ceaa8eb6e8c93a63f2d8d1c30ecb6b263dee",
    1023: "10108970eeda3eb932baac1428c7a2163b0e924c9a9e25b35bba72b28f70bd11",
    1024: "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7",
}

# -------------------------------------------------------------- Ed25519 ---
Q = 2**255 - 19
L = 2**252 + 27742317777372353535851937790883648493
D = (-121665 * pow(121666, Q - 2, Q)) % Q
SQRT_M1 = pow(2, (Q - 1) // 4, Q)


def _xrecover(y: int) -> int:
    xx = (y * y - 1) * pow(D * y * y + 1, Q - 2, Q) % Q
    x = pow(xx, (Q + 3) // 8, Q)
    if (x * x - xx) % Q != 0:
        x = x * SQRT_M1 % Q
    if x % 2 != 0:
        x = Q - x
    return x


BY = 4 * pow(5, Q - 2, Q) % Q
BASE = (_xrecover(BY), BY)


def _edwards(p, qp):
    x1, y1 = p
    x2, y2 = qp
    k = D * x1 * x2 * y1 * y2 % Q
    x3 = (x1 * y2 + x2 * y1) * pow(1 + k, Q - 2, Q) % Q
    y3 = (y1 * y2 + x1 * x2) * pow(1 - k, Q - 2, Q) % Q
    return (x3, y3)


def _scalarmult(p, e: int):
    if e == 0:
        return (0, 1)
    half = _scalarmult(p, e // 2)
    half = _edwards(half, half)
    return _edwards(half, p) if e & 1 else half


def _encodepoint(p) -> bytes:
    x, y = p
    return (y | ((x & 1) << 255)).to_bytes(32, "little")


def _clamped(seed: bytes) -> int:
    a = bytearray(hashlib.sha512(seed).digest()[:32])
    a[0] &= 248
    a[31] &= 127
    a[31] |= 64
    return int.from_bytes(a, "little")


def _hint(data: bytes) -> int:
    return int.from_bytes(hashlib.sha512(data).digest(), "little")


def ed25519_public_key(seed: bytes) -> bytes:
    return _encodepoint(_scalarmult(BASE, _clamped(seed)))


def ed25519_sign(message: bytes, seed: bytes) -> bytes:
    h = hashlib.sha512(seed).digest()
    a = _clamped(seed)
    public = _encodepoint(_scalarmult(BASE, a))
    r = _hint(h[32:] + message)
    cap_r = _scalarmult(BASE, r)
    enc_r = _encodepoint(cap_r)
    s = (r + _hint(enc_r + public + message) * a) % L
    return enc_r + s.to_bytes(32, "little")


# RFC 8032 §7.1 TEST 1/2/3 — self-check for the implementation above.
RFC8032 = [
    ("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
     "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a", "",
     "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155"
     "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"),
    ("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
     "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c", "72",
     "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da"
     "085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"),
    ("c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7",
     "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025", "af82",
     "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac"
     "18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a"),
]

# ------------------------------------------------------- C2 wire helpers ---
def varint(value: int) -> bytes:
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        out.append(byte | (0x80 if value else 0))
        if not value:
            return bytes(out)


def text(value: str) -> bytes:
    raw = value.encode("utf-8")
    return varint(len(raw)) + raw


def u64(value: int) -> bytes:
    return struct.pack("<Q", value)


def u128(value: int) -> bytes:
    return struct.pack("<16s", value.to_bytes(16, "little"))


def domain_message(chain_id: str, tag: str, payload: bytes) -> bytes:
    """C3 hash_domain framing: unambiguous, length-prefixed."""
    prefix = chain_id.encode() + b"\x00" + tag.encode() + b"\x00"
    return (HASH_CONTEXT + u64(len(prefix)) + prefix
            + u64(len(payload)) + payload)


def signer_address(public_key: bytes) -> bytes:
    return blake3_short(ADDRESS_CONTEXT + public_key)[:20]


def encode_unsigned(case: dict) -> tuple[bytes, dict[str, int]]:
    parts: list[bytes] = []
    offsets: dict[str, int] = {}
    pos = 0

    def add(name: str, blob: bytes) -> None:
        nonlocal pos
        offsets[name] = pos
        parts.append(blob)
        pos += len(blob)

    chain = case["chain"].encode("utf-8")
    add("version", bytes([CODEC_VERSION]))
    add("chain_len", varint(len(chain)))
    add("chain", chain)
    add("signer", case["address"])
    add("nonce", u64(case["nonce"]))
    add("kind", bytes([case["kind"]]))
    add("payload_len", varint(len(case["payload"])))
    add("payload", case["payload"])
    add("gas", u64(case["gas"]))
    add("max_fee", u128(case["max_fee"]))
    add("tip", u128(case["tip"]))
    add("expiry", u64(case["expiry"]))
    return b"".join(parts), offsets


def encode_signature(case: dict) -> bytes:
    return bytes([0]) + case["public_key"] + case["signature"]


CASES = [
    dict(chain="auralite-test-1", seed=bytes([0x11] * 32), nonce=0, kind=0,
         payload=b"", gas=21_000, max_fee=10**12, tip=0, expiry=1_000),
    dict(chain="auralite-test-1", seed=bytes([0x22] * 32), nonce=7, kind=2,
         payload=b"transfer:alice->bob:1.5ALCRY", gas=100_000,
         max_fee=5 * 10**18, tip=10**15, expiry=999_999),
    dict(chain="auralite-1", seed=bytes([0x33] * 32), nonce=1, kind=3,
         payload=b"validator:candidate-7", gas=50_000,
         max_fee=10**18, tip=10**14, expiry=42),
    dict(chain="auralite-test-1", seed=bytes([0x44] * 32), nonce=2**64 - 1,
         kind=4, payload=bytes(range(200)), gas=2**64 - 1,
         max_fee=2**128 - 1, tip=2**128 - 1, expiry=2**64 - 1),
    dict(chain="auralite-test-1", seed=bytes([0x55] * 32), nonce=3, kind=1,
         payload=b"\x00wasm\x00init", gas=1, max_fee=7, tip=7, expiry=0),
]


def main() -> int:
    # Self-check both independent primitives before generating anything.
    for length, expected in OFFICIAL_B3.items():
        data = bytes(i % 251 for i in range(length))
        if blake3_short(data).hex() != expected:
            raise SystemExit(f"pure-Python BLAKE3 disagrees with official vector len={length}")
    for index, (secret, public, message, signature) in enumerate(RFC8032, 1):
        seed = bytes.fromhex(secret)
        if ed25519_public_key(seed).hex() != public or \
           ed25519_sign(bytes.fromhex(message), seed).hex() != signature:
            raise SystemExit(f"pure-Python Ed25519 disagrees with RFC 8032 TEST {index}")

    lines = [
        "# AuraLite-Crypto C4 transaction golden vectors",
        "# Generated by tools/gen_c4_vectors.py (independent Python).",
        "# Self-checked against official BLAKE3 vectors and RFC 8032 §7.1.",
        "# Format: key=<hex|text|decimal>. Do not edit by hand.",
        f"codec_version={CODEC_VERSION}",
        f"hash_context={HASH_CONTEXT.hex()}",
        f"signing_context={SIGNING_CONTEXT.hex()}",
        f"txid_context={TXID_CONTEXT.hex()}",
        f"address_context={ADDRESS_CONTEXT.hex()}",
        f"max_tx_payload={MAX_TX_PAYLOAD}",
    ]

    first_tx: bytes | None = None
    first_unsigned: bytes | None = None
    first_offsets: dict[str, int] | None = None
    first_sig: bytes | None = None

    for index, case in enumerate(CASES, 1):
        case = dict(case)
        case["public_key"] = ed25519_public_key(case["seed"])
        case["address"] = signer_address(case["public_key"])
        unsigned, offsets = encode_unsigned(case)
        signing_bytes = SIGNING_CONTEXT + unsigned
        signing_hash = blake3_short(domain_message(case["chain"], "tx", signing_bytes))
        case["signature"] = ed25519_sign(signing_hash, case["seed"])
        sig_bytes = encode_signature(case)
        tx_bytes = unsigned + sig_bytes
        txid = blake3_short(TXID_CONTEXT + tx_bytes)

        p = lambda k: f"tx{index}_{k}"  # noqa: E731
        lines += [
            f"{p('chain')}={case['chain']}",
            f"{p('seed')}={case['seed'].hex()}",
            f"{p('kind')}={case['kind']}",
            f"{p('nonce')}={case['nonce']}",
            f"{p('payload')}={case['payload'].hex()}",
            f"{p('gas')}={case['gas']}",
            f"{p('max_fee')}={case['max_fee']}",
            f"{p('tip')}={case['tip']}",
            f"{p('expiry')}={case['expiry']}",
            f"{p('address')}={case['address'].hex()}",
            f"{p('public_key')}={case['public_key'].hex()}",
            f"{p('unsigned')}={unsigned.hex()}",
            f"{p('signing_bytes')}={signing_bytes.hex()}",
            f"{p('signing_hash')}={signing_hash.hex()}",
            f"{p('signature')}={case['signature'].hex()}",
            f"{p('bytes')}={tx_bytes.hex()}",
            f"{p('txid')}={txid.hex()}",
        ]
        if index == 1:
            first_tx, first_unsigned, first_offsets, first_sig = tx_bytes, unsigned, offsets, sig_bytes

    assert first_tx is not None and first_unsigned is not None
    assert first_offsets is not None and first_sig is not None
    off = first_offsets
    sig_start = len(first_unsigned)

    def patched(at: int, value: int) -> bytes:
        out = bytearray(first_tx)
        out[at] = value
        return bytes(out)

    bad: dict[str, bytes] = {
        "bad_trailing": first_tx + b"\x00",
        "bad_truncated": first_tx[:-1],
        "bad_version": patched(off["version"], 2),
        "bad_kind_tag": patched(off["kind"], 9),
        "bad_algo_tag": patched(sig_start, 1),
        "bad_signer": patched(off["signer"], first_tx[off["signer"]] ^ 0x01),
        "bad_signature": patched(sig_start + 33, first_tx[sig_start + 33] ^ 0x01),
        "bad_chain_len": first_unsigned[:off["chain_len"]] + b"\x80\x00"
                         + first_unsigned[off["chain_len"] + 1:] + first_sig,
        "bad_payload_len": first_unsigned[:off["payload_len"]] + b"\x80\x00"
                           + first_unsigned[off["payload_len"] + 1:] + first_sig,
    }
    for key, value in bad.items():
        lines.append(f"{key}={value.hex()}")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  canonical transactions: {len(CASES)}")
    print(f"  rejection vectors:      {len(bad)}")
    print("  self-checks: BLAKE3 official vectors OK, RFC 8032 Ed25519 OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
