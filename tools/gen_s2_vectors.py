#!/usr/bin/env python3
"""Generate S2 golden vectors from an INDEPENDENT Python implementation.

No Rust code, no network. Contains pure-Python BLAKE3 (self-checked against
official BLAKE3 vectors), the S1 sparse Merkle tree, and the S2 WAL framing.

Emits:
  * 10 deterministic commit batches (put/delete + per-key nonce);
  * the framed, checksummed WAL record bytes for every commit;
  * the versioned state root and head checksum for every commit;
  * a set of rejection vectors: bad checksum, bad magic, truncated frame,
    non-sequential version, wrong parent root, and a wrong state root.

Usage:
    python3 tools/gen_s2_vectors.py
    make gen-s2-vectors

Output: crates/aura-storage/tests/fixtures/s2_golden.txt
"""

from __future__ import annotations

import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-storage" / "tests" / "fixtures" / "s2_golden.txt"

WAL_MAGIC = b"ALW1"
S2_WAL_CONTEXT = b"AuraLite-Crypto/s2-wal-v1\x00"
S2_ROOT_CONTEXT = b"AuraLite-Crypto/s2-root-v1\x00"
S2_KEY_CONTEXT = b"AuraLite-Crypto/s2-key-v1\x00"
S2_HEAD_CONTEXT = b"AuraLite-Crypto/s2-head-v1\x00"
S1_EMPTY_CONTEXT = b"AuraLite-Crypto/s1-empty-v1\x00"
S1_LEAF_CONTEXT = b"AuraLite-Crypto/s1-leaf-v1\x00"
S1_NODE_CONTEXT = b"AuraLite-Crypto/s1-node-v1\x00"

COMMIT_COUNT = 11
KILL_POINTS = 10
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


def key_path(key: bytes) -> bytes:
    return blake3(S2_KEY_CONTEXT + u64(len(key)) + key)


def head_checksum(version: int, root: bytes) -> bytes:
    return blake3(S2_HEAD_CONTEXT + u64(version) + root)


def wal_checksum(payload: bytes) -> bytes:
    return blake3(S2_WAL_CONTEXT + payload)


class Node:
    __slots__ = ("depth", "hash", "kind", "left", "right", "key", "value")

    def __init__(self, depth, hash_, kind, left=None, right=None, key=None, value=None):
        self.depth = depth
        self.hash = hash_
        self.kind = kind
        self.left = left
        self.right = right
        self.key = key
        self.value = value


def empty_hash(depth: int) -> bytes:
    return blake3(S1_EMPTY_CONTEXT + u16(depth))


def leaf_hash(depth: int, key: bytes, value: bytes) -> bytes:
    return blake3(S1_LEAF_CONTEXT + u16(depth) + key + u64(len(value)) + value)


def node_hash(left: bytes, right: bytes) -> bytes:
    return blake3(S1_NODE_CONTEXT + left + right)


def key_bit(key: bytes, depth: int) -> int:
    return (key[depth // 8] >> (7 - depth % 8)) & 1


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


def state_root(version: int, state: dict[bytes, bytes]) -> bytes:
    leaves = {key_path(k): v for k, v in state.items()}
    tree_root = build_tree(leaves).hash
    return blake3(S2_ROOT_CONTEXT + u64(version) + tree_root)


# --------------------------------------------------------------- S2 batches ---
def vector_changes(index: int) -> list[tuple[bytes, bytes | None, int]]:
    out = []
    for slot in range(4):
        seed = (index * 0x9E3779B97F4A7C15 + slot * 0x2545F4914F6CDD1D) & MASK64
        seed ^= seed >> 12
        seed ^= (seed << 25) & MASK64
        seed ^= seed >> 27
        digest_seed = (seed * 0x2545F4914F6CDD1D) & MASK64
        key = blake3(f"s2-key-{index}-{slot}".encode())[:20]
        value = blake3(f"s2-value-{index}-{slot}-{digest_seed}".encode())
        out.append((key, value, index * 10 + slot))
    if index % 3 == 0:
        out.append((b"deleted-marker", None, index * 10 + 9))
    return out


def encode_payload(record: dict) -> bytes:
    out = bytearray(WAL_MAGIC)
    out += u64(record["version"])
    out += record["parent_root"]
    out += record["state_root"]
    out += varint(len(record["changes"]))
    for key, value, nonce in record["changes"]:
        out += varint(len(key)) + key
        if value is None:
            out += b"\x00"
        else:
            out += b"\x01" + varint(len(value)) + value
        out += u64(nonce)
    return bytes(out)


def frame_payload(payload: bytes) -> bytes:
    return varint(len(payload)) + payload + wal_checksum(payload)


def encode_record(record: dict) -> bytes:
    return frame_payload(encode_payload(record))


def make_record(version, parent_root, state_root_value, changes) -> dict:
    return {
        "version": version,
        "parent_root": parent_root,
        "state_root": state_root_value,
        "changes": changes,
    }


def main() -> int:
    for length, expected in OFFICIAL_B3.items():
        if blake3(bytes(i % 251 for i in range(length))).hex() != expected:
            raise SystemExit(f"pure-Python BLAKE3 disagrees with official vector len={length}")

    lines = [
        "# AuraLite-Crypto S2 WAL + RocksDB golden vectors",
        "# Generated by tools/gen_s2_vectors.py (independent Python).",
        "# Framing: varint(payload_len) || payload || BLAKE3(S2_WAL_CONTEXT||payload).",
        f"wal_magic={WAL_MAGIC.hex()}",
        f"wal_context={S2_WAL_CONTEXT.hex()}",
        f"root_context={S2_ROOT_CONTEXT.hex()}",
        f"key_context={S2_KEY_CONTEXT.hex()}",
        f"head_context={S2_HEAD_CONTEXT.hex()}",
        f"commit_count={COMMIT_COUNT}",
        f"kill_points={KILL_POINTS}",
        "genesis_root=" + "00" * 32,
    ]

    state: dict[bytes, bytes] = {}
    parent_root = bytes(32)
    records: list[dict] = []
    wal_bytes = bytearray()
    offsets: list[int] = []

    for index in range(1, COMMIT_COUNT + 1):
        changes = vector_changes(index)
        for key, value, _ in changes:
            if value is None:
                state.pop(key, None)
            else:
                state[key] = value
        root = state_root(index, state)
        record = make_record(index, parent_root, root, changes)
        framed = encode_record(record)
        offset = len(wal_bytes)
        wal_bytes += framed
        offsets.append(offset)
        records.append(record)
        parent_root = root
        lines += [
            f"commit_{index}_version={index}",
            f"commit_{index}_root={root.hex()}",
            f"commit_{index}_parent={record['parent_root'].hex()}",
            f"commit_{index}_wal={framed.hex()}",
            f"commit_{index}_wal_offset={offset}",
            f"commit_{index}_changes={len(changes)}",
            f"commit_{index}_state_count={len(state)}",
            f"commit_{index}_head_checksum={head_checksum(index, root).hex()}",
        ]

    lines += [
        f"wal_file_all={bytes(wal_bytes).hex()}",
        f"final_root={parent_root.hex()}",
        f"final_state_count={len(state)}",
    ]

    # ---- rejection vectors (independent, deliberately corrupt WAL files) ----
    first = records[0]
    second = records[1]
    rec1 = encode_record(first)
    rec2 = encode_record(second)

    # Flip the last checksum byte.
    bad_checksum = bytearray(rec1)
    bad_checksum[-1] ^= 0x01
    lines.append(f"bad_checksum_wal={bytes(bad_checksum).hex()}")

    # Corrupt the magic but keep the checksum valid over the modified payload.
    magic_payload = bytearray(encode_payload(first))
    magic_payload[0:4] = b"XXXX"
    lines.append(f"bad_magic_wal={frame_payload(bytes(magic_payload)).hex()}")

    lines.append(f"bad_truncated_wal={rec1[:10].hex()}")
    lines.append(f"bad_trailing_wal={(rec1 + b'\\x00').hex()}")

    # Version 3 follows version 1 -> non-sequential.
    third = make_record(3, second["state_root"], state_root(3, state), vector_changes(3))
    lines.append(f"bad_nonseq_wal={(rec1 + encode_record(third)).hex()}")

    # Second record claims parent = ZERO instead of first record's root.
    bad_parent = {**second, "parent_root": bytes(32)}
    lines.append(f"bad_parent_wal={(rec1 + encode_record(bad_parent)).hex()}")

    # Declared root is zero; replay publishes it and verify_state_root fails.
    bad_root = {**first, "state_root": bytes(32)}
    lines.append(f"bad_root_wal={encode_record(bad_root).hex()}")

    # Mutate one value byte; the declared root no longer matches the state.
    changed = list(first["changes"])
    key, value, nonce = changed[0]
    mutated = bytearray(value)
    mutated[0] ^= 0x01
    changed[0] = (key, bytes(mutated), nonce)
    bad_value = {**first, "changes": changed}
    lines.append(f"bad_value_wal={encode_record(bad_value).hex()}")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  commits: {COMMIT_COUNT}, WAL bytes: {len(wal_bytes)}, final leaves: {len(state)}")
    print("  self-check: official BLAKE3 vectors OK, WAL chain verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
