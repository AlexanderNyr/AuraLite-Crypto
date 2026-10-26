#!/usr/bin/env python3
"""Generate C6 Merkle-proof golden vectors from INDEPENDENT references.

No Rust code, no network. The generator contains pure-Python BLAKE3
(self-checked against 7 official BLAKE3-team vectors), a Python transcription
of the C5 binary Merkle tree (padded levels, leaf/node domain tags) and the
C6 compact proof encoding.

It emits roots and proofs for trees of 1/2/3/5/8/1000 transaction leaves,
event-leaf roots/proofs, and a set of invalid proofs that must be rejected.

Usage:
    python3 tools/gen_c6_vectors.py
    make gen-c6-vectors

Output: crates/aura-block/tests/fixtures/c6_golden.txt
"""

from __future__ import annotations

import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-block" / "tests" / "fixtures" / "c6_golden.txt"

LEAF_CONTEXT = b"AuraLite-Crypto/tx-leaf-v1\x00"        # C5
NODE_CONTEXT = b"AuraLite-Crypto/tx-node-v1\x00"        # C5
EVENT_LEAF_CONTEXT = b"AuraLite-Crypto/event-leaf-v1\x00"
MAX_PROOF_DEPTH = 64

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

# -------------------------------------------------------------- tree/proof ---
def varint(value: int) -> bytes:
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        out.append(byte | (0x80 if value else 0))
        if not value:
            return bytes(out)


def leaf_hash(txid: bytes) -> bytes:
    return blake3_short(LEAF_CONTEXT + txid)


def node_hash(left: bytes, right: bytes) -> bytes:
    return blake3_short(NODE_CONTEXT + left + right)


def event_leaf_hash(event: bytes) -> bytes:
    return blake3_short(EVENT_LEAF_CONTEXT + struct.pack("<Q", len(event)) + event)


def tree_depth(total: int) -> int:
    depth = 0
    while total > 1:
        total = (total + 1) // 2
        depth += 1
    return depth


def build_levels(leaf_hashes: list[bytes]) -> list[list[bytes]]:
    if not leaf_hashes:
        return []
    levels: list[list[bytes]] = []
    current = list(leaf_hashes)
    while True:
        if len(current) > 1 and len(current) % 2 == 1:
            current.append(current[-1])
        levels.append(list(current))
        if len(current) == 1:
            break
        current = [node_hash(current[i], current[i + 1]) for i in range(0, len(current), 2)]
    return levels


def root_of(levels: list[list[bytes]]) -> bytes:
    return levels[-1][0] if levels else bytes(32)


def prove(levels: list[list[bytes]], total: int, index: int) -> dict:
    siblings = []
    position = index
    for level in range(len(levels) - 1):
        siblings.append(levels[level][position ^ 1])
        position //= 2
    return {"index": index, "total": total, "leaf": levels[0][index], "siblings": siblings}


def encode_proof(proof: dict) -> bytes:
    return (
        varint(proof["index"])
        + varint(proof["total"])
        + proof["leaf"]
        + varint(len(proof["siblings"]))
        + b"".join(proof["siblings"])
    )


def txid(index: int) -> bytes:
    # Trivial deterministic pattern, reproducible in Rust without a hash crate.
    return bytes((index * 7 + j * 13 + 0xAB) % 256 for j in range(32))


def event(index: int) -> bytes:
    return b"event:" + index.to_bytes(4, "little") + bytes([index % 256]) * (index % 17)


def main() -> int:
    for length, expected in OFFICIAL_B3.items():
        if blake3_short(bytes(i % 251 for i in range(length))).hex() != expected:
            raise SystemExit(f"pure-Python BLAKE3 disagrees with official vector len={length}")

    lines = [
        "# AuraLite-Crypto C6 Merkle-proof golden vectors",
        "# Generated by tools/gen_c6_vectors.py (independent Python).",
        "# Self-checked against official BLAKE3 vectors. Format: key=<hex|decimal>.",
        f"leaf_context={LEAF_CONTEXT.hex()}",
        f"node_context={NODE_CONTEXT.hex()}",
        f"event_leaf_context={EVENT_LEAF_CONTEXT.hex()}",
        f"max_proof_depth={MAX_PROOF_DEPTH}",
    ]

    tx_sizes = [1, 2, 3, 5, 8, 1000]
    proofs: dict[tuple[str, int, int], dict] = {}
    roots: dict[tuple[str, int], bytes] = {}

    for size in tx_sizes:
        leaves = [leaf_hash(txid(i)) for i in range(size)]
        levels = build_levels(leaves)
        root = root_of(levels)
        roots[("tx", size)] = root
        lines.append(f"root_tx_{size}={root.hex()}")
        lines.append(f"depth_tx_{size}={tree_depth(size)}")
        indices = range(size) if size <= 8 else [0, 1, 2, 499, 998, 999]
        for index in indices:
            proof = prove(levels, size, index)
            proofs[("tx", size, index)] = proof
            lines.append(f"proof_tx_{size}_{index}={encode_proof(proof).hex()}")

    for size in [1, 3, 5]:
        leaves = [event_leaf_hash(event(i)) for i in range(size)]
        levels = build_levels(leaves)
        root = root_of(levels)
        roots[("event", size)] = root
        lines.append(f"root_event_{size}={root.hex()}")
        for index in range(size):
            proof = prove(levels, size, index)
            proofs[("event", size, index)] = proof
            lines.append(f"proof_event_{size}_{index}={encode_proof(proof).hex()}")

    # A few leaf hashes are pinned explicitly (domain separation).
    lines.append(f"leaf_tx_2={leaf_hash(txid(2)).hex()}")
    lines.append(f"leaf_event_2={event_leaf_hash(event(2)).hex()}")

    base = proofs[("tx", 5, 2)]
    base_bytes = encode_proof(base)

    def replaced(offset: int, value: int) -> bytes:
        out = bytearray(base_bytes)
        out[offset] = value
        return bytes(out)

    bad_leaf = dict(base)
    bad_leaf["leaf"] = bytes([0x9C] * 32)
    bad_sibling = dict(base)
    siblings = list(base["siblings"])
    siblings[0] = bytes([siblings[0][0] ^ 0x01]) + siblings[0][1:]
    bad_sibling["siblings"] = siblings
    bad_index = dict(base)
    bad_index["index"] = 3
    bad_total = dict(base)
    bad_total["total"] = 4
    short = dict(base)
    short["siblings"] = base["siblings"][:-1]
    empty_total = dict(base)
    empty_total["total"] = 0
    oob = dict(base)
    oob["index"] = 5
    deep = dict(base)
    deep["siblings"] = base["siblings"] + [bytes(32)] * (MAX_PROOF_DEPTH + 1 - len(base["siblings"]))

    bad = {
        "bad_substituted_leaf": encode_proof(bad_leaf),
        "bad_wrong_sibling": encode_proof(bad_sibling),
        "bad_wrong_index": encode_proof(bad_index),
        "bad_wrong_total": encode_proof(bad_total),
        "bad_sibling_count": encode_proof(short),
        "bad_empty_total": encode_proof(empty_total),
        "bad_index_oob": encode_proof(oob),
        "bad_depth_overflow": encode_proof(deep),
        "bad_trailing": base_bytes + b"\x00",
        "bad_truncated": base_bytes[:-1],
        # index varint (1 byte for index=2) replaced by a non-minimal 0x80 0x00
        "bad_nonminimal_index": b"\x80\x00" + base_bytes[1:],
    }
    for key, value in bad.items():
        lines.append(f"{key}={value.hex()}")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  tx trees: {tx_sizes}, event trees: [1, 3, 5]")
    print(f"  proofs emitted: {len(proofs)}")
    print(f"  rejection vectors: {len(bad)}")
    print("  self-check: BLAKE3 official vectors OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
