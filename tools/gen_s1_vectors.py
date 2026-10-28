#!/usr/bin/env python3
"""Generate S1 golden vectors from an INDEPENDENT Python implementation.

No Rust code, no network. Contains pure-Python BLAKE3 (self-checked against
official BLAKE3 vectors), the C7 state-key digest, and a sparse binary Merkle
tree with 256-bit paths and single-leaf subtree compression.

Emits:
  * 10,000 deterministic operations (insert / remove / lookup);
  * 12 checkpoint roots (versioned state roots);
  * 1,000 inclusion proofs at the final version.

Usage:
    python3 tools/gen_s1_vectors.py
    make gen-s1-vectors

Output: crates/aura-state/tests/fixtures/s1_golden.txt
"""

from __future__ import annotations

import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "s1_golden.txt"

STATE_KEY_CONTEXT = b"AuraLite-Crypto/state-key-v1\x00"        # C7
ADDR_CONTEXT = b"AuraLite-Crypto/s1-addr-v1\x00"
VALUE_CONTEXT = b"AuraLite-Crypto/s1-value-v1\x00"
EMPTY_CONTEXT = b"AuraLite-Crypto/s1-empty-v1\x00"
LEAF_CONTEXT = b"AuraLite-Crypto/s1-leaf-v1\x00"
NODE_CONTEXT = b"AuraLite-Crypto/s1-node-v1\x00"
ROOT_CONTEXT = b"AuraLite-Crypto/s1-root-v1\x00"

OPERATION_COUNT = 10_000
KEY_SPACE = 3_000
PROOF_COUNT = 1_000
CHECKPOINTS = [99, 499, 999, 1999, 2999, 3999, 4999, 5999, 6999, 7999, 8999, 9999]
SEED = 0x51A1C0DE2026

MASK64 = (1 << 64) - 1

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


def blake3(data: bytes) -> bytes:
    """Pure-Python BLAKE3 for inputs <= 1024 bytes (single chunk)."""
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

# ------------------------------------------------------------- wire helpers ---
def varint(value: int) -> bytes:
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        out.append(byte | (0x80 if value else 0))
        if not value:
            return bytes(out)


def u16(value: int) -> bytes:
    return struct.pack("<H", value)


def u64(value: int) -> bytes:
    return struct.pack("<Q", value)


def key_digest(index: int) -> bytes:
    addr = blake3(ADDR_CONTEXT + u64(index))[:20]
    raw = b"\x01" + addr            # C7 KeySpace::Account
    return blake3(STATE_KEY_CONTEXT + u64(len(raw)) + raw)


def value_bytes(seed: int) -> bytes:
    return blake3(VALUE_CONTEXT + u64(seed))


def key_bit(key: bytes, depth: int) -> int:
    return (key[depth // 8] >> (7 - depth % 8)) & 1


def empty_hash(depth: int) -> bytes:
    return blake3(EMPTY_CONTEXT + u16(depth))


def leaf_hash(depth: int, key: bytes, value: bytes) -> bytes:
    return blake3(LEAF_CONTEXT + u16(depth) + key + u64(len(value)) + value)


def node_hash(left: bytes, right: bytes) -> bytes:
    return blake3(NODE_CONTEXT + left + right)


def state_root(version: int, root: bytes) -> bytes:
    return blake3(ROOT_CONTEXT + u64(version) + root)


class Node:
    __slots__ = ("depth", "hash", "kind", "left", "right", "key", "value")

    def __init__(self, depth, hash_, kind, left=None, right=None, key=None, value=None):
        self.depth = depth
        self.hash = hash_
        self.kind = kind  # "empty" | "leaf" | "branch"
        self.left = left
        self.right = right
        self.key = key
        self.value = value


def build_tree(leaves: dict[bytes, bytes], depth: int = 0) -> Node:
    if not leaves:
        return Node(depth, empty_hash(depth), "empty")
    if len(leaves) == 1:
        key, value = next(iter(leaves.items()))
        return Node(depth, leaf_hash(depth, key, value), "leaf", key=key, value=value)
    left_leaves = {k: v for k, v in leaves.items() if key_bit(k, depth) == 0}
    right_leaves = {k: v for k, v in leaves.items() if key_bit(k, depth) == 1}
    left = build_tree(left_leaves, depth + 1)
    right = build_tree(right_leaves, depth + 1)
    return Node(depth, node_hash(left.hash, right.hash), "branch", left=left, right=right)


def proof_bytes(key: bytes, value: bytes, leaf_depth: int, siblings: list[bytes], version: int) -> bytes:
    out = bytearray(u64(version) + key + u16(leaf_depth) + varint(len(siblings)))
    for sib in siblings:
        out += sib
    out += varint(len(value)) + value
    return bytes(out)


def prove(root: Node, key: bytes) -> tuple[bytes, int, list[bytes]] | None:
    node = root
    siblings: list[bytes] = []
    while node.kind == "branch":
        if key_bit(key, node.depth) == 0:
            siblings.append(node.right.hash)
            node = node.left
        else:
            siblings.append(node.left.hash)
            node = node.right
    if node.kind != "leaf" or node.key != key:
        return None
    return node.value, node.depth, list(reversed(siblings))


class Lcg:
    def __init__(self, seed: int) -> None:
        self.state = seed & MASK64

    def next(self) -> int:
        self.state = (self.state * 6364136223846793005 + 1442695040888963407) & MASK64
        return self.state

    def below(self, bound: int) -> int:
        return self.next() % bound


def operations():
    rng = Lcg(SEED)
    for index in range(OPERATION_COUNT):
        kind = rng.below(10)
        key_index = rng.below(KEY_SPACE)
        seed = rng.next()
        yield index, kind, key_index, seed


def main() -> int:
    for length, expected in OFFICIAL_B3.items():
        if blake3(bytes(i % 251 for i in range(length))).hex() != expected:
            raise SystemExit(f"pure-Python BLAKE3 disagrees with official vector len={length}")

    lines = [
        "# AuraLite-Crypto S1 Merkle tree golden vectors",
        "# Generated by tools/gen_s1_vectors.py (independent Python).",
        "# Sparse binary Merkle tree over 256-bit C7 state-key digests with",
        "# single-leaf subtree compression; versioned state root.",
        f"state_key_context={STATE_KEY_CONTEXT.hex()}",
        f"addr_context={ADDR_CONTEXT.hex()}",
        f"value_context={VALUE_CONTEXT.hex()}",
        f"empty_context={EMPTY_CONTEXT.hex()}",
        f"leaf_context={LEAF_CONTEXT.hex()}",
        f"node_context={NODE_CONTEXT.hex()}",
        f"root_context={ROOT_CONTEXT.hex()}",
        f"operation_seed={SEED}",
        f"operation_count={OPERATION_COUNT}",
        f"key_space={KEY_SPACE}",
        f"checkpoint_count={len(CHECKPOINTS)}",
        f"proof_count={PROOF_COUNT}",
        f"empty_root_depth0={empty_hash(0).hex()}",
        f"empty_root_depth256={empty_hash(256).hex()}",
    ]

    leaves: dict[bytes, bytes] = {}
    checkpoint_set = set(CHECKPOINTS)
    checkpoint_rows: list[tuple[int, int, int, bytes]] = []
    for index, kind, key_index, seed in operations():
        key = key_digest(key_index)
        if kind < 7:                       # insert / update
            leaves[key] = value_bytes(seed)
        elif kind == 7:                    # delete
            leaves.pop(key, None)
        # kind 8,9 are lookups — no mutation.
        if index in checkpoint_set:
            version = len(checkpoint_rows) + 1
            root = build_tree(leaves)
            checkpoint_rows.append((index, version, len(leaves), state_root(version, root.hash)))

    for number, (index, version, count, root) in enumerate(checkpoint_rows, 1):
        lines += [
            f"checkpoint_{number}_op={index}",
            f"checkpoint_{number}_version={version}",
            f"checkpoint_{number}_leaves={count}",
            f"checkpoint_{number}_root={root.hex()}",
        ]

    final_tree = build_tree(leaves)
    final_root = state_root(len(checkpoint_rows), final_tree.hash)
    if final_root != checkpoint_rows[-1][3]:
        raise SystemExit("self-check failed: final root != last checkpoint root")

    proof_keys = sorted(leaves)[:PROOF_COUNT]
    if len(proof_keys) != PROOF_COUNT:
        raise SystemExit(f"self-check failed: only {len(proof_keys)} leaves for proofs")
    for number, key in enumerate(proof_keys, 1):
        result = prove(final_tree, key)
        if result is None:
            raise SystemExit(f"self-check failed: no proof for key {number}")
        value, leaf_depth, siblings = result
        lines += [
            f"proof_{number}_key={key.hex()}",
            f"proof_{number}_value={value.hex()}",
            f"proof_{number}_bytes={proof_bytes(key, value, leaf_depth, siblings, len(checkpoint_rows)).hex()}",
        ]

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  operations: {OPERATION_COUNT}, checkpoints: {len(checkpoint_rows)}, proofs: {PROOF_COUNT}")
    print(f"  final leaves: {len(leaves)}")
    print("  self-check: official BLAKE3 vectors OK, final root matches last checkpoint")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
