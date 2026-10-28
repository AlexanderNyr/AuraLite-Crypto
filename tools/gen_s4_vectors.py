#!/usr/bin/env python3
"""Generate S4 snapshot golden vectors from an INDEPENDENT Python model.

No Rust code, no network. Contains pure-Python BLAKE3 (self-checked against
official BLAKE3 vectors), the C7 state-key digest, the S1 sparse Merkle tree and
hashlib SHA-256 (stdlib, independent of the Rust `sha2` crate).

Emits:
  * a small canonical snapshot (7 accounts, 3 chunks) with its manifest bytes,
    data blob, descriptors, chunk_root and state_root;
  * a set of rejection vectors (substituted chunk, bad checksum, bad magic,
    truncated/trailing data, unsorted/duplicate keys, swapped descriptors,
    bad chunk count);
  * a deterministic 1,000,000-key gate: SHA-256 of the concatenated chunk
    payloads (the fixture stays small — the reference computes the digest without
    writing 18 MB of hex).
  * a 1,024-key parallel-hashing reference (chunk digests, in order).

Wire format (all integers fixed-width LE unless marked `v` = minimal LEB128).

    manifest payload =
        "ALS4" || u32 version || v-len chain_id || u64 height ||
        32 state_root || 32 chunk_root || u32 chunk_size || u64 total_keys ||
        u32 chunk_count ||
        (u64 offset || u64 length || 32 sha256(chunk))*count
    manifest bytes = payload || sha256(SNAPSHOT_MANIFEST_CONTEXT || payload)

    chunk payload = v(count) || (v-len key || key || v-len value || value)*count
    chunk hash = sha256(SNAPSHOT_CHUNK_CONTEXT || chunk payload)
    chunk_root = BLAKE3(SNAPSHOT_ROOT_CONTEXT || u32 count || concat(hashes))
    state_root = S1 versioned_root(1, sparse_merkle_root(state_path(key) -> value))
    snapshot data blob = concat(chunk payloads in descriptor order)

Usage:
    python3 tools/gen_s4_vectors.py
    make gen-s4-vectors

Output: crates/aura-state/tests/fixtures/s4_golden.txt
"""

from __future__ import annotations

import hashlib
import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "s4_golden.txt"

SNAPSHOT_MAGIC = b"ALS4"
SNAPSHOT_VERSION = 1
SNAPSHOT_MANIFEST_CONTEXT = b"AuraLite-Crypto/s4-manifest-v1\x00"
SNAPSHOT_CHUNK_CONTEXT = b"AuraLite-Crypto/s4-chunk-v1\x00"
SNAPSHOT_ROOT_CONTEXT = b"AuraLite-Crypto/s4-root-v1\x00"
STATE_KEY_CONTEXT = b"AuraLite-Crypto/state-key-v1\x00"
S1_EMPTY_CONTEXT = b"AuraLite-Crypto/s1-empty-v1\x00"
S1_LEAF_CONTEXT = b"AuraLite-Crypto/s1-leaf-v1\x00"
S1_NODE_CONTEXT = b"AuraLite-Crypto/s1-node-v1\x00"
S1_ROOT_CONTEXT = b"AuraLite-Crypto/s1-root-v1\x00"

CHAIN_ID = "auralite-test-1"
HEIGHT = 1000
CHUNK_SIZE = 3
ACCOUNT_COUNT = 7
MILLION_ENTRIES = 1_000_000
PARALLEL_ENTRIES = 1024

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


def varint(value: int) -> bytes:
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        out.append(byte | (0x80 if value else 0))
        if not value:
            return bytes(out)


def u32(value: int) -> bytes:
    return struct.pack("<I", value)


def u64(value: int) -> bytes:
    return struct.pack("<Q", value)


def sha256(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()


def state_path(raw_key: bytes) -> bytes:
    return blake3(STATE_KEY_CONTEXT + u64(len(raw_key)) + raw_key)


def account_bytes(nonce: int, balance: int) -> bytes:
    return u64(nonce) + balance.to_bytes(16, "little") + bytes(32) + bytes(32)


class Node:
    __slots__ = ("depth", "hash", "kind")

    def __init__(self, depth, hash_, kind):
        self.depth = depth
        self.hash = hash_
        self.kind = kind


def empty_hash(depth: int) -> bytes:
    return blake3(S1_EMPTY_CONTEXT + struct.pack("<H", depth))


def leaf_hash(depth: int, key: bytes, value: bytes) -> bytes:
    return blake3(S1_LEAF_CONTEXT + struct.pack("<H", depth) + key + u64(len(value)) + value)
def key_bit(key: bytes, depth: int) -> int:
    return (key[depth // 8] >> (7 - depth % 8)) & 1


def build_tree(leaves: dict, depth: int = 0) -> Node:
    if not leaves:
        return Node(depth, empty_hash(depth), "empty")
    if len(leaves) == 1:
        key, value = next(iter(leaves.items()))
        return Node(depth, leaf_hash(depth, key, value), "leaf")
    left = build_tree({k: v for k, v in leaves.items() if key_bit(k, depth) == 0}, depth + 1)
    right = build_tree({k: v for k, v in leaves.items() if key_bit(k, depth) == 1}, depth + 1)
    return Node(depth, blake3(S1_NODE_CONTEXT + left.hash + right.hash), "branch")


def tree_root(leaves: dict) -> bytes:
    return build_tree(leaves).hash


def versioned_root(version: int, root: bytes) -> bytes:
    return blake3(S1_ROOT_CONTEXT + u64(version) + root)
def chunk_payload(entries: list[tuple[bytes, bytes]]) -> bytes:
    out = bytearray(varint(len(entries)))
    for key, value in entries:
        out += varint(len(key)) + key + varint(len(value)) + value
    return bytes(out)


def chunk_hash(payload: bytes) -> bytes:
    return sha256(SNAPSHOT_CHUNK_CONTEXT + payload)


def chunk_root(hashes: list[bytes]) -> bytes:
    data = SNAPSHOT_ROOT_CONTEXT + u32(len(hashes))
    for digest in hashes:
        data += digest
    return blake3(data)


def manifest_payload(chain_id, height, state_root, croot, chunk_size, total_keys, descriptors) -> bytes:
    out = bytearray(SNAPSHOT_MAGIC)
    out += u32(SNAPSHOT_VERSION)
    chain = chain_id.encode()
    out += varint(len(chain)) + chain
    out += u64(height)
    out += state_root + croot + u32(chunk_size) + u64(total_keys) + u32(len(descriptors))
    for offset, length, digest in descriptors:
        out += u64(offset) + u64(length) + digest
    return bytes(out)


def manifest_bytes(payload: bytes) -> bytes:
    return payload + sha256(SNAPSHOT_MANIFEST_CONTEXT + payload)
def state_root_of(entries: list[tuple[bytes, bytes]]) -> bytes:
    leaves = {state_path(key): value for key, value in entries}
    return versioned_root(1, tree_root(leaves))


def export(entries, chain_id=CHAIN_ID, height=HEIGHT, chunk_size=CHUNK_SIZE):
    """Return (manifest bytes, data blob, descriptor list, chunk_root)."""
    chunks: list[bytes] = []
    descriptors = []
    offset = 0
    for start in range(0, len(entries), chunk_size):
        payload = chunk_payload(entries[start:start + chunk_size])
        digest = chunk_hash(payload)
        descriptors.append((offset, len(payload), digest))
        chunks.append(payload)
        offset += len(payload)
    croot = chunk_root([d[2] for d in descriptors])
    sroot = state_root_of(entries)
    payload = manifest_payload(chain_id, height, sroot, croot, chunk_size, len(entries), descriptors)
    return manifest_bytes(payload), b"".join(chunks), descriptors, croot, sroot


def framed(payload: bytes) -> bytes:
    return payload + sha256(SNAPSHOT_MANIFEST_CONTEXT + payload)


def bad_count_payload(manifest: bytes) -> bytes:
    payload = bytearray(manifest[:-32])
    # offset of chunk_count: magic(4) + version(4) + varint chain length + chain
    # + height(8) + state_root(32) + chunk_root(32) + chunk_size(4) + total_keys(8)
    chain = CHAIN_ID.encode()
    offset = 4 + 4 + len(varint(len(chain))) + len(chain) + 8 + 32 + 32 + 4 + 8
    payload[offset:offset + 4] = u32(999)
    return bytes(payload)


def bad_root_payload(manifest: bytes) -> bytes:
    payload = bytearray(manifest[:-32])
    chain = CHAIN_ID.encode()
    offset = 4 + 4 + len(varint(len(chain))) + len(chain) + 8 + 32
    payload[offset] ^= 1
    return bytes(payload)


def vector_entries(count: int) -> list[tuple[bytes, bytes]]:
    """Canonical C7 account keys and compact 8-byte values, strictly ascending."""
    entries = []
    for index in range(count):
        key = b"\x01" + index.to_bytes(20, "big")
        value = u64(index)
        entries.append((key, value))
    return entries


def big_snapshot_digest(count: int, chunk_size: int):
    """Streaming reference for the 1M-key gate; returns (data sha256, chunk_root,
    chunk_count, total_data_len)."""
    hasher = hashlib.sha256()
    descriptors = []
    offset = 0
    hashes = []
    for start in range(0, count, chunk_size):
        current = min(chunk_size, count - start)
        out = bytearray(varint(current))
        for index in range(start, start + current):
            key = b"\x01" + index.to_bytes(20, "big")
            value = u64(index)
            out += varint(len(key)) + key + varint(len(value)) + value
        payload = bytes(out)
        digest = chunk_hash(payload)
        descriptors.append((offset, len(payload), digest))
        hashes.append(digest)
        hasher.update(payload)
        offset += len(payload)
    return hasher.hexdigest(), chunk_root(hashes), len(descriptors), offset


def main() -> int:
    for length, expected in OFFICIAL_B3.items():
        if blake3(bytes(i % 251 for i in range(length))).hex() != expected:
            raise SystemExit(f"pure-Python BLAKE3 disagrees with official vector len={length}")

    entries = vector_entries(ACCOUNT_COUNT)
    manifest, data, descriptors, croot, sroot = export(entries)
    lines = [
        "# AuraLite-Crypto S4 snapshot golden vectors",
        "# Generated by tools/gen_s4_vectors.py (independent Python).",
        "# Manifest: payload || sha256(SNAPSHOT_MANIFEST_CONTEXT || payload).",
        "# Chunk hash: sha256(SNAPSHOT_CHUNK_CONTEXT || chunk payload).",
        f"snapshot_magic={SNAPSHOT_MAGIC.hex()}",
        f"snapshot_version={SNAPSHOT_VERSION}",
        f"manifest_context={SNAPSHOT_MANIFEST_CONTEXT.hex()}",
        f"chunk_context={SNAPSHOT_CHUNK_CONTEXT.hex()}",
        f"root_context={SNAPSHOT_ROOT_CONTEXT.hex()}",
        f"state_key_context={STATE_KEY_CONTEXT.hex()}",
        f"chain_id={CHAIN_ID}",
        f"height={HEIGHT}",
        f"chunk_size={CHUNK_SIZE}",
        f"entry_count={ACCOUNT_COUNT}",
        f"chunk_count={len(descriptors)}",
        f"state_root={sroot.hex()}",
        f"chunk_root={croot.hex()}",
        f"manifest_bytes={manifest.hex()}",
        f"data_bytes={data.hex()}",
        f"data_sha256={hashlib.sha256(data).hexdigest()}",
    ]
    for index, (offset, length, digest) in enumerate(descriptors):
        lines.append(f"chunk_{index}_offset={offset}")
        lines.append(f"chunk_{index}_length={length}")
        lines.append(f"chunk_{index}_sha256={digest.hex()}")

    # ---- rejection vectors (data blob + manifest) ----
    lines += [
        f"bad_substituted_data={(data[:-1] + bytes([data[-1] ^ 1])).hex()}",
        f"bad_substituted_manifest={manifest.hex()}",
        f"bad_manifest_checksum={(manifest[:-1] + bytes([manifest[-1] ^ 1])).hex()}",
        f"bad_manifest_magic={framed(b"XXXX" + manifest[4:-32]).hex()}",
        f"bad_chunk_count_manifest={framed(bad_count_payload(manifest)).hex()}",
        f"bad_chunk_root_manifest={framed(bad_root_payload(manifest)).hex()}",
        f"bad_truncated_data={data[:-2].hex()}",
        f"bad_trailing_data={(data + b"\x00").hex()}",
        f"bad_unsorted_data={chunk_payload([entries[1], entries[0], entries[2]]).hex()}",
        f"bad_duplicate_data={chunk_payload([entries[0], entries[0], entries[1]]).hex()}",
        f"bad_empty_data={b"".hex()}",
    ]

    # ---- 1M key gate (small fixture: only digest + root, no 30 MB of hex) ----
    big_data_sha, big_root, big_chunks, big_len = big_snapshot_digest(MILLION_ENTRIES, 65536)
    lines += [
        f"million_entries={MILLION_ENTRIES}",
        f"million_chunk_size=65536",
        f"million_data_len={big_len}",
        f"million_chunk_count={big_chunks}",
        f"million_data_sha256={big_data_sha}",
        f"million_chunk_root={big_root.hex()}",
    ]

    # ---- parallel hashing reference: 1024 keys / 8 chunks of 128 ----
    par_entries = vector_entries(PARALLEL_ENTRIES)
    _, par_data, par_desc, par_croot, _ = export(par_entries, chunk_size=128)
    lines += [
        f"parallel_entries={PARALLEL_ENTRIES}",
        f"parallel_chunk_size=128",
        f"parallel_data_sha256={hashlib.sha256(par_data).hexdigest()}",
        f"parallel_chunk_root={par_croot.hex()}",
    ]
    for index, (offset, length, digest) in enumerate(par_desc):
        lines.append(f"parallel_chunk_{index}_sha256={digest.hex()}")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  canonical snapshot: {ACCOUNT_COUNT} keys in {len(descriptors)} chunks")
    print(f"  1M gate: {MILLION_ENTRIES} keys -> {big_chunks} chunks, {big_len} data bytes")
    print(f"  parallel reference: {len(par_desc)} chunks of 128 keys")
    print("  self-check: official BLAKE3 vectors OK, SHA-256 is stdlib hashlib")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
