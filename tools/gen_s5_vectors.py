#!/usr/bin/env python3
"""Generate S5 state-sync golden vectors from an INDEPENDENT Python model.

No Rust code, no network. Contains pure-Python BLAKE3 (self-checked against
official BLAKE3 vectors), hashlib SHA-256, the C7 state-key digest, the S1
sparse Merkle tree and the S4 snapshot format (chunks + sha256 manifest +
BLAKE3 chunk_root).

Wire format (integers fixed-width LE unless `v` = minimal LEB128):

    SyncHead payload  = "ALH5" || u32 version || v-len chain_id ||
        u64 height || 32 state_root || u64 snapshot_height || 32 snapshot_root
    head bytes = payload || sha256(S5_HEAD_CONTEXT || payload)

    SyncDelta payload = "ALD5" || u32 version || u64 from_height ||
        u64 to_height || u32 change_count ||
        (v-len key || key || tag(0/1) || [v-len value || value])*
    delta bytes = payload || sha256(S5_DELTA_CONTEXT || payload)

    SyncPackage payload = "ALS5" || u32 version || u32 head_count ||
        (v-len head || head)* || v-len manifest || manifest ||
        v-len data || data || v-len delta || delta
    package bytes = payload || sha256(S5_PACKAGE_CONTEXT || payload)

A new node applies: import S4 snapshot -> apply compacted delta -> recompute
S1 `state_root`. It downloads ONLY the package, not the full event history.

Usage:
    python3 tools/gen_s5_vectors.py
    make gen-s5-vectors
"""
from __future__ import annotations

import hashlib
import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "s5_golden.txt"

HEAD_MAGIC = b"ALH5"
DELTA_MAGIC = b"ALD5"
PACKAGE_MAGIC = b"ALS5"
S5_HEAD_CONTEXT = b"AuraLite-Crypto/s5-head-v1\x00"
S5_DELTA_CONTEXT = b"AuraLite-Crypto/s5-delta-v1\x00"
S5_PACKAGE_CONTEXT = b"AuraLite-Crypto/s5-package-v1\x00"
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
VERSION = 1
STATE_KEYS = 256
CHUNK_SIZE = 64
SNAPSHOT_HEIGHT = 90
TARGET_HEIGHT = 100
HISTORY_HEIGHTS = 100
QUORUM = 3
MAX_SYNC_SECONDS = 60

MASK = 0xFFFFFFFF
IV = [0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
      0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19]
PERM = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8]
CHUNK_START, CHUNK_END, ROOT = 1, 2, 8


def _rotr(x: int, n: int) -> int: return ((x >> n) | (x << (32 - n))) & MASK


def _g(s, a, b, c, d, mx, my):
    s[a] = (s[a] + s[b] + mx) & MASK
    s[d] = _rotr(s[d] ^ s[a], 16); s[c] = (s[c] + s[d]) & MASK
    s[b] = _rotr(s[b] ^ s[c], 12); s[a] = (s[a] + s[b] + my) & MASK
    s[d] = _rotr(s[d] ^ s[a], 8); s[c] = (s[c] + s[d]) & MASK
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


def u32(v: int) -> bytes: return struct.pack("<I", v)
def u64(v: int) -> bytes: return struct.pack("<Q", v)
def sha256(data: bytes) -> bytes: return hashlib.sha256(data).digest()
def state_path(raw: bytes) -> bytes: return blake3(STATE_KEY_CONTEXT + u64(len(raw)) + raw)


class Node:
    __slots__ = ("depth", "hash", "kind")

    def __init__(self, depth, hash_, kind):
        self.depth = depth; self.hash = hash_; self.kind = kind


def empty_hash(d: int) -> bytes: return blake3(S1_EMPTY_CONTEXT + struct.pack("<H", d))
def leaf_hash(d: int, k: bytes, v: bytes) -> bytes:
    return blake3(S1_LEAF_CONTEXT + struct.pack("<H", d) + k + u64(len(v)) + v)
def key_bit(k: bytes, d: int) -> int: return (k[d // 8] >> (7 - d % 8)) & 1


def build_tree(leaves: dict, d: int = 0) -> Node:
    if not leaves: return Node(d, empty_hash(d), "empty")
    if len(leaves) == 1:
        k, v = next(iter(leaves.items()))
        return Node(d, leaf_hash(d, k, v), "leaf")
    left = build_tree({k: v for k, v in leaves.items() if key_bit(k, d) == 0}, d + 1)
    right = build_tree({k: v for k, v in leaves.items() if key_bit(k, d) == 1}, d + 1)
    return Node(d, blake3(S1_NODE_CONTEXT + left.hash + right.hash), "branch")


def tree_root(leaves: dict) -> bytes: return build_tree(leaves).hash
def versioned_root(version: int, root: bytes) -> bytes: return blake3(S1_ROOT_CONTEXT + u64(version) + root)
def state_root(entries: list[tuple[bytes, bytes]]) -> bytes:
    return versioned_root(1, tree_root({state_path(k): v for k, v in entries}))


def chunk_payload(entries: list[tuple[bytes, bytes]]) -> bytes:
    out = bytearray(varint(len(entries)))
    for k, v in entries:
        out += varint(len(k)) + k + varint(len(v)) + v
    return bytes(out)
def chunk_hash(payload: bytes) -> bytes: return sha256(SNAPSHOT_CHUNK_CONTEXT + payload)
def chunk_root(hashes: list[bytes]) -> bytes:
    data = SNAPSHOT_ROOT_CONTEXT + u32(len(hashes))
    for h in hashes:
        data += h
    return blake3(data)
def manifest_payload(chain, height, sroot, croot, chunk_size, total_keys, descriptors) -> bytes:
    chain = chain.encode()
    out = bytearray(SNAPSHOT_MAGIC + u32(SNAPSHOT_VERSION) + varint(len(chain)) + chain + u64(height))
    out += sroot + croot + u32(chunk_size) + u64(total_keys) + u32(len(descriptors))
    for offset, length, digest in descriptors:
        out += u64(offset) + u64(length) + digest
    return bytes(out)
def framed(payload: bytes, context: bytes) -> bytes:
    return payload + sha256(context + payload)


def snapshot(entries, height: int, chunk_size: int = CHUNK_SIZE):
    chunks, descriptors, offset, hashes = [], [], 0, []
    for start in range(0, len(entries), chunk_size):
        payload = chunk_payload(entries[start:start + chunk_size])
        digest = chunk_hash(payload)
        descriptors.append((offset, len(payload), digest))
        chunks.append(payload); hashes.append(digest); offset += len(payload)
    croot = chunk_root(hashes)
    sroot = state_root(entries)
    manifest = framed(
        manifest_payload(CHAIN_ID, height, sroot, croot, chunk_size, len(entries), descriptors),
        SNAPSHOT_MANIFEST_CONTEXT,
    )
    return manifest, b"".join(chunks), croot, sroot


def state_at(height: int) -> list[tuple[bytes, bytes]]:
    """Deterministic state at `height` over 256 C7 account keys."""
    return [
        (b"\x01" + index.to_bytes(20, "big"), u64(height * 1000 + index))
        for index in range(STATE_KEYS)
    ]


def head_payload(height: int, sroot: bytes, snapshot_height: int, croot: bytes) -> bytes:
    chain = CHAIN_ID.encode()
    return (HEAD_MAGIC + u32(VERSION) + varint(len(chain)) + chain + u64(height) +
            sroot + u64(snapshot_height) + croot)


def head_bytes(height: int, sroot: bytes, snapshot_height: int, croot: bytes) -> bytes:
    return framed(head_payload(height, sroot, snapshot_height, croot), S5_HEAD_CONTEXT)


def delta_payload(from_h: int, to_h: int, changes: list[tuple[bytes, bytes | None]]) -> bytes:
    out = bytearray(DELTA_MAGIC + u32(VERSION) + u64(from_h) + u64(to_h) + u32(len(changes)))
    for key, value in changes:
        out += varint(len(key)) + key
        if value is None:
            out += b"\x00"
        else:
            out += b"\x01" + varint(len(value)) + value
    return bytes(out)


def delta_bytes(from_h: int, to_h: int, changes) -> bytes:
    return framed(delta_payload(from_h, to_h, changes), S5_DELTA_CONTEXT)


def package_payload(heads: list[bytes], manifest: bytes, data: bytes, delta: bytes) -> bytes:
    out = bytearray(PACKAGE_MAGIC + u32(VERSION) + u32(len(heads)))
    for head in heads:
        out += varint(len(head)) + head
    for blob in (manifest, data, delta):
        out += varint(len(blob)) + blob
    return bytes(out)


def package_bytes(heads, manifest: bytes, data: bytes, delta: bytes) -> bytes:
    return framed(package_payload(heads, manifest, data, delta), S5_PACKAGE_CONTEXT)


def history_bytes() -> tuple[int, int]:
    """Size of the FULL event history that a node would download without sync."""
    total = 0
    events = 0
    for height in range(HISTORY_HEIGHTS):
        for index in range(STATE_KEYS):
            key = b"\x01" + index.to_bytes(20, "big")
            value = u64(height * 1000 + index)
            event = varint(len(key)) + key + varint(len(value)) + value + u64(height)
            total += len(event)
            events += 1
    return events, total


def main() -> int:
    for length, expected in OFFICIAL_B3.items():
        if blake3(bytes(i % 251 for i in range(length))).hex() != expected:
            raise SystemExit(f"pure-Python BLAKE3 disagrees with official vector len={length}")

    snap_entries = state_at(SNAPSHOT_HEIGHT)
    manifest, data, croot, sroot = snapshot(snap_entries, SNAPSHOT_HEIGHT)
    target_entries = state_at(TARGET_HEIGHT)
    target_root = state_root(target_entries)
    head = head_bytes(TARGET_HEIGHT, target_root, SNAPSHOT_HEIGHT, croot)
    heads = [head] * QUORUM
    changes = target_entries
    delta = delta_bytes(SNAPSHOT_HEIGHT, TARGET_HEIGHT, changes)
    package = package_bytes(heads, manifest, data, delta)
    events, full_history = history_bytes()

    lines = [
        "# AuraLite-Crypto S5 state-sync golden vectors",
        "# Generated by tools/gen_s5_vectors.py (independent Python).",
        "# New node: import S4 snapshot -> apply compacted delta -> verify S1 root",
        "# against a quorum of 3 identical heads, WITHOUT downloading all history.",
        f"head_magic={HEAD_MAGIC.hex()}",
        f"delta_magic={DELTA_MAGIC.hex()}",
        f"package_magic={PACKAGE_MAGIC.hex()}",
        f"head_context={S5_HEAD_CONTEXT.hex()}",
        f"delta_context={S5_DELTA_CONTEXT.hex()}",
        f"package_context={S5_PACKAGE_CONTEXT.hex()}",
        f"chain_id={CHAIN_ID}",
        f"version={VERSION}",
        f"state_keys={STATE_KEYS}",
        f"chunk_size={CHUNK_SIZE}",
        f"snapshot_height={SNAPSHOT_HEIGHT}",
        f"target_height={TARGET_HEIGHT}",
        f"quorum={QUORUM}",
        f"max_sync_seconds={MAX_SYNC_SECONDS}",
        f"state_root={target_root.hex()}",
        f"snapshot_root={croot.hex()}",
        "head_0_bytes=" + head.hex(),
        "head_1_bytes=" + head.hex(),
        "head_2_bytes=" + head.hex(),
        f"manifest_bytes={manifest.hex()}",
        f"data_bytes={data.hex()}",
        f"delta_bytes={delta.hex()}",
        f"package_bytes={package.hex()}",
        f"package_len={len(package)}",
        f"manifest_len={len(manifest)}",
        f"data_len={len(data)}",
        f"delta_len={len(delta)}",
        f"history_events={events}",
        f"history_bytes={full_history}",
        f"data_sha256={hashlib.sha256(data).hexdigest()}",
        f"package_sha256={hashlib.sha256(package).hexdigest()}",
    ]

    # ---- rejection vectors ----
    bad_package = bytearray(package)
    bad_package[-1] ^= 1
    lines.append(f"bad_package_checksum={bytes(bad_package).hex()}")
    bad_head = bytearray(head)
    bad_head[-1] ^= 1
    lines.append(f"bad_head_checksum={bytes(bad_head).hex()}")
    bad_delta = bytearray(delta)
    bad_delta[-1] ^= 1
    lines.append(f"bad_delta_checksum={bytes(bad_delta).hex()}")
    # substituted diff chunk: one value byte changed, checksum recomputed
    bad_changes = list(changes)
    changed = bytearray(changes[0][1])
    changed[0] ^= 1
    bad_changes[0] = (changes[0][0], bytes(changed))
    lines.append(f"bad_delta_substituted={delta_bytes(SNAPSHOT_HEIGHT, TARGET_HEIGHT, bad_changes).hex()}")
    # one head disagrees (valid checksum, different root)
    disagree = bytearray(target_root)
    disagree[0] ^= 1
    other_head = head_bytes(TARGET_HEIGHT, bytes(disagree), SNAPSHOT_HEIGHT, croot)
    lines.append(f"head_disagree_bytes={other_head.hex()}")
    # quorum with 3 heads where one differs
    lines.append(f"bad_quorum_package_bytes={package_bytes([head, other_head, head], manifest, data, delta).hex()}")
    lines.append(f"bad_substituted_data={(data[:-1] + bytes([data[-1] ^ 1])).hex()}")
    lines.append(f"bad_trailing_package={(package + b'\x00').hex()}")
    lines.append(f"bad_truncated_package={package[:-3].hex()}")
    # swapped heads / delta with bad magic
    bad_delta_magic = framed(b"XXXX" + delta_payload(SNAPSHOT_HEIGHT, TARGET_HEIGHT, changes)[4:], S5_DELTA_CONTEXT)
    lines.append(f"bad_delta_magic={bad_delta_magic.hex()}")
    bad_head_magic = framed(b"XXXX" + head_payload(TARGET_HEIGHT, target_root, SNAPSHOT_HEIGHT, croot)[4:], S5_HEAD_CONTEXT)
    lines.append(f"bad_head_magic={bad_head_magic.hex()}")
    bad_sub_data = data[:-1] + bytes([data[-1] ^ 1])
    lines.append(f"bad_sub_package_bytes={package_bytes(heads, manifest, bad_sub_data, delta).hex()}")
    lines.append(f"bad_sub_delta_package_bytes={package_bytes(heads, manifest, data, delta_bytes(SNAPSHOT_HEIGHT, TARGET_HEIGHT, bad_changes)).hex()}")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  state: {STATE_KEYS} keys, {SNAPSHOT_HEIGHT} -> {TARGET_HEIGHT}")
    print(f"  package: {len(package)} bytes; full history: {full_history} bytes")
    print(f"  ratio: {full_history/len(package):.1f}x smaller, quorum {QUORUM}")
    print("  self-check: official BLAKE3 vectors OK, SHA-256 is stdlib hashlib")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
