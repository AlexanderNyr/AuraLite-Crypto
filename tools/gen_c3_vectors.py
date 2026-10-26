#!/usr/bin/env python3
"""Generate C3 golden vectors from INDEPENDENT reference implementations.

No network access, no Rust imports. Sources:

  * BLAKE3 official test vectors (BLAKE3-team/BLAKE3 test_vectors.json),
    first 32 bytes of the extended output; subset embedded below.
  * A pure-Python BLAKE3 (single chunk, validated against those official
    vectors at generation time) used to compute our domain-separated hashes.
  * RFC 8032 section 7.1 Ed25519 TEST 1/2/3 (verbatim).
  * A pure-Python secp256k1 + RFC 6979 deterministic ECDSA implementation;
    its signatures were additionally verified with OpenSSL `pkeyutl -verify`.

Usage:
    python3 tools/gen_c3_vectors.py
    make gen-c3-vectors

Output: crates/aura-crypto/tests/fixtures/c3_golden.txt
"""

from __future__ import annotations

import hashlib
import hmac
import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-crypto" / "tests" / "fixtures" / "c3_golden.txt"

HASH_CONTEXT = b"AuraLite-Crypto/crypto-v1\x00"

# ---------------------------------------------------------------- BLAKE3 ---
MASK = 0xFFFFFFFF
IV = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
    0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
]
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


def _compress(cv: list[int], block: list[int], counter: int, block_len: int, flags: int) -> list[int]:
    s = list(cv) + list(IV)
    s[12], s[13], s[14], s[15] = counter & MASK, (counter >> 32) & MASK, block_len, flags
    m = list(block)
    for r in range(7):
        _g(s, 0, 4, 8, 12, m[0], m[1])
        _g(s, 1, 5, 9, 13, m[2], m[3])
        _g(s, 2, 6, 10, 14, m[4], m[5])
        _g(s, 3, 7, 11, 15, m[6], m[7])
        _g(s, 0, 5, 10, 15, m[8], m[9])
        _g(s, 1, 6, 11, 12, m[10], m[11])
        _g(s, 2, 7, 8, 13, m[12], m[13])
        _g(s, 3, 4, 9, 14, m[14], m[15])
        if r != 6:
            m = [m[PERM[i]] for i in range(16)]
    return [s[i] ^ s[i + 8] for i in range(8)] + [s[i + 8] ^ cv[i] for i in range(8)]


def blake3_short(data: bytes) -> bytes:
    """BLAKE3 for inputs <= 1024 bytes (one chunk). Independent of Rust."""
    assert len(data) <= 1024, "this reference handles a single chunk only"
    blocks = [data[i : i + 64] for i in range(0, len(data), 64)] or [b""]
    cv = IV
    for index, block in enumerate(blocks[:-1]):
        flags = CHUNK_START if index == 0 else 0
        cv = _compress(cv, list(struct.unpack("<16I", block.ljust(64, b"\0"))), 0, 64, flags)[:8]
    last = blocks[-1]
    flags = (CHUNK_START if len(blocks) == 1 else 0) | CHUNK_END
    words = list(struct.unpack("<16I", last.ljust(64, b"\0")))
    out = _compress(cv, words, 0, len(last), flags | ROOT)
    return struct.pack("<8I", *out[:8])


# Subset of https://github.com/BLAKE3-team/BLAKE3/blob/master/test_vectors/test_vectors.json
# Input for length N is bytes(i % 251 for i in range(N)); value = first 32 bytes
# of the official extended hash output.
OFFICIAL_B3 = {
    0: "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
    1: "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213",
    2: "7b7015bb92cf0b318037702a6cdd81dee41224f734684c2c122cd6359cb1ee63",
    3: "e1be4d7a8ab5560aa4199eea339849ba8e293d55ca0a81006726d184519e647f",
    4: "f30f5ab28fe047904037f77b6da4fea1e27241c5d132638d8bedce9d40494f32",
    7: "3f8770f387faad08faa9d8414e9f449ac68e6ff0417f673f602a646a891419fe",
    8: "2351207d04fc16ade43ccab08600939c7c1fa70a5c0aaca76063d04c3228eaeb",
    63: "e9bc37a594daad83be9470df7f7b3798297c3d834ce80ba85d6e207627b7db7b",
    64: "4eed7141ea4a5cd4b788606bd23f46e212af9cacebacdc7d1f4c6dc7f2511b98",
    65: "de1e5fa0be70df6d2be8fffd0e99ceaa8eb6e8c93a63f2d8d1c30ecb6b263dee",
    127: "d81293fda863f008c09e92fc382a81f5a0b4a1251cba1634016a0f86a6bd640d",
    128: "f17e570564b26578c33bb7f44643f539624b05df1a76c81f30acd548c44b45ef",
    129: "683aaae9f3c5ba37eaaf072aed0f9e30bac0865137bae68b1fde4ca2aebdcb12",
    1023: "10108970eeda3eb932baac1428c7a2163b0e924c9a9e25b35bba72b28f70bd11",
    1024: "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7",
}


def official_input(length: int) -> bytes:
    return bytes(i % 251 for i in range(length))


# --------------------------------------------------------------- secp256k1 ---
P = 2**256 - 2**32 - 977
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
G = (
    0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798,
    0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8,
)


def _pt_add(a, b):
    if a is None:
        return b
    if b is None:
        return a
    x1, y1 = a
    x2, y2 = b
    if x1 == x2 and (y1 + y2) % P == 0:
        return None
    if a == b:
        lam = (3 * x1 * x1) * pow(2 * y1, -1, P) % P
    else:
        lam = (y2 - y1) * pow(x2 - x1, -1, P) % P
    x3 = (lam * lam - x1 - x2) % P
    return (x3, (lam * (x1 - x3) - y1) % P)


def _pt_mul(k: int, point):
    result = None
    while k:
        if k & 1:
            result = _pt_add(result, point)
        point = _pt_add(point, point)
        k >>= 1
    return result


def secp_pubkey(d: int) -> bytes:
    x, y = _pt_mul(d, G)
    return (b"\x02" if y % 2 == 0 else b"\x03") + x.to_bytes(32, "big")


def secp_sign_rfc6979(d: int, prehash: bytes) -> bytes:
    """Deterministic ECDSA (RFC 6979 / SHA-256), low-S normalised."""
    assert len(prehash) == 32

    def i2o(x: int) -> bytes:
        return x.to_bytes(32, "big")

    def b2i(b: bytes) -> int:
        return int.from_bytes(b, "big")

    b2o = lambda h: (b2i(h) % N).to_bytes(32, "big")  # noqa: E731
    v = b"\x01" * 32
    k = b"\x00" * 32
    k = hmac.new(k, v + b"\x00" + i2o(d) + b2o(prehash), hashlib.sha256).digest()
    v = hmac.new(k, v, hashlib.sha256).digest()
    k = hmac.new(k, v + b"\x01" + i2o(d) + b2o(prehash), hashlib.sha256).digest()
    v = hmac.new(k, v, hashlib.sha256).digest()
    z = b2i(prehash)
    while True:
        v = hmac.new(k, v, hashlib.sha256).digest()
        nonce = b2i(v)
        if 1 <= nonce < N:
            point = _pt_mul(nonce, G)
            r = point[0] % N
            if r:
                s = pow(nonce, -1, N) * (z + r * d) % N
                if s:
                    if s > N // 2:
                        s = N - s
                    return i2o(r) + i2o(s)
        k = hmac.new(k, v + b"\x00", hashlib.sha256).digest()
        v = hmac.new(k, v, hashlib.sha256).digest()


# --------------------------------------------------------------- RFC 8032 ---
# RFC 8032 section 7.1, TEST 1 / TEST 2 / TEST 3 (Ed25519).
RFC8032 = [
    (
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
        "",
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155"
        "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
    ),
    (
        "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
        "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
        "72",
        "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da"
        "085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
    ),
    (
        "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7",
        "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025",
        "af82",
        "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac"
        "18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a",
    ),
]


def domain_message(chain_id: str, tag: str, payload: bytes) -> bytes:
    prefix = chain_id.encode() + b"\x00" + tag.encode() + b"\x00"
    return (
        HASH_CONTEXT
        + struct.pack("<Q", len(prefix))
        + prefix
        + struct.pack("<Q", len(payload))
        + payload
    )


def main() -> int:
    # Self-check: the pure-Python BLAKE3 must reproduce every official vector.
    for length, expected in OFFICIAL_B3.items():
        got = blake3_short(official_input(length)).hex()
        if got != expected:
            raise SystemExit(f"pure-Python BLAKE3 disagrees with official vector len={length}")

    lines = [
        "# AuraLite-Crypto C3 golden vectors",
        "# Sources: official BLAKE3 test_vectors.json, RFC 8032 §7.1,",
        "# and a pure-Python secp256k1/RFC6979 implementation (OpenSSL-verified).",
        "# Generated by tools/gen_c3_vectors.py. Format: key=value. Do not edit.",
        "# BLAKE3 official inputs are bytes(i % 251 for i in range(N)).",
        f"hash_context={HASH_CONTEXT.hex()}",
    ]

    for length, expected in OFFICIAL_B3.items():
        lines.append(f"b3_official_{length}={expected}")

    DOMAIN_CASES = [
        ("auralite-test-1", "tx", b""),
        ("auralite-test-1", "tx", b"transfer:alice->bob:1.5ALCRY"),
        ("auralite-test-1", "vote", b"transfer:alice->bob:1.5ALCRY"),
        ("auralite-1", "tx", b"transfer:alice->bob:1.5ALCRY"),
        ("auralite-test-1", "block", bytes(range(200))),
    ]
    for index, (chain, tag, payload) in enumerate(DOMAIN_CASES, start=1):
        message = domain_message(chain, tag, payload)
        lines.append(f"domain_{index}_chain={chain}")
        lines.append(f"domain_{index}_tag={tag}")
        lines.append(f"domain_{index}_payload={payload.hex()}")
        lines.append(f"domain_{index}_message={message.hex()}")
        lines.append(f"domain_{index}_hash={blake3_short(message).hex()}")

    for index, (secret, public, message, signature) in enumerate(RFC8032, start=1):
        lines.append(f"ed{index}_secret={secret}")
        lines.append(f"ed{index}_public={public}")
        lines.append(f"ed{index}_message={message}")
        lines.append(f"ed{index}_signature={signature}")

    SECP_CASES = [
        (bytes([7] * 32), bytes([3] * 32)),
        (
            hashlib.sha256(b"AuraLite-Crypto C3 second case").digest(),
            hashlib.sha256(b"payload").digest(),
        ),
    ]
    for index, (seed, prehash) in enumerate(SECP_CASES, start=1):
        d = int.from_bytes(seed, "big")
        lines.append(f"secp{index}_secret={seed.hex()}")
        lines.append(f"secp{index}_public={secp_pubkey(d).hex()}")
        lines.append(f"secp{index}_prehash={prehash.hex()}")
        lines.append(f"secp{index}_signature={secp_sign_rfc6979(d, prehash).hex()}")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  official BLAKE3 vectors: {len(OFFICIAL_B3)} (self-checked)")
    print(f"  domain hash vectors:     {len(DOMAIN_CASES)}")
    print(f"  RFC 8032 Ed25519 vectors:{len(RFC8032)}")
    print(f"  secp256k1 RFC6979 vectors:{len(SECP_CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
