#!/usr/bin/env python3
"""Generate S3 golden vectors from an INDEPENDENT Python implementation.

No Rust code, no network. Contains pure-Python BLAKE3 (self-checked against
official BLAKE3 vectors), the C7 state-key digest and the S1 sparse Merkle
tree, plus the S3 canonical diff framing.

Emits:
  * a deterministic base account state and its S1 tree/root hashes;
  * 3 canonical diffs (credit, debit+event, mixed writes) with wire bytes,
    result root, event root and gas used;
  * a set of rejection vectors (bad checksum, trailing bytes, unordered changes,
    duplicate change, non-minimal varint, oversized count, bad magic,
    root mismatch) and the roots that must survive a revert / OutOfGas.

Usage:
    python3 tools/gen_s3_vectors.py
    make gen-s3-vectors

Output: crates/aura-state/tests/fixtures/s3_golden.txt
"""

from __future__ import annotations

import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "s3_golden.txt"

STATE_KEY_CONTEXT = b"AuraLite-Crypto/state-key-v1\x00"
S1_EMPTY_CONTEXT = b"AuraLite-Crypto/s1-empty-v1\x00"
S1_LEAF_CONTEXT = b"AuraLite-Crypto/s1-leaf-v1\x00"
S1_NODE_CONTEXT = b"AuraLite-Crypto/s1-node-v1\x00"
S1_ROOT_CONTEXT = b"AuraLite-Crypto/s1-root-v1\x00"
S3_DIFF_CONTEXT = b"AuraLite-Crypto/s3-diff-v1\x00"
S3_EVENT_CONTEXT = b"AuraLite-Crypto/s3-event-v1\x00"

DIFF_MAGIC = b"ALD3"
MAX_WRITES = 65_536
MAX_EVENTS = 4_096
BASE_VERSION = 7
ACCOUNT_LEN = 88

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


def u16(value: int) -> bytes:
    return struct.pack("<H", value)


def u64(value: int) -> bytes:
    return struct.pack("<Q", value)


def state_path(raw_key: bytes) -> bytes:
    return blake3(STATE_KEY_CONTEXT + u64(len(raw_key)) + raw_key)


def account_bytes(nonce: int, balance: int) -> bytes:
    return u64(nonce) + balance.to_bytes(16, "little") + bytes(32) + bytes(32)


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
    left = build_tree({k: v for k, v in leaves.items() if key_bit(k, depth) == 0}, depth + 1)
    right = build_tree({k: v for k, v in leaves.items() if key_bit(k, depth) == 1}, depth + 1)
    return Node(depth, node_hash(left.hash, right.hash), "branch", left=left, right=right)


def tree_root(leaves: dict[bytes, bytes]) -> bytes:
    return build_tree(leaves).hash


def versioned_root(version: int, root: bytes) -> bytes:
    return blake3(S1_ROOT_CONTEXT + u64(version) + root)


def events_root(events: list[bytes]) -> bytes:
    out = bytearray(S3_EVENT_CONTEXT)
    out += varint(len(events))
    for event in events:
        out += varint(len(event)) + event
    return blake3(bytes(out))


def diff_checksum(payload: bytes) -> bytes:
    return blake3(S3_DIFF_CONTEXT + payload)


def diff_payload(version, base_root, result_root, gas_used, events, changes) -> bytes:
    out = bytearray(DIFF_MAGIC)
    out += u64(version)
    out += base_root
    out += result_root
    out += u64(gas_used)
    out += varint(len(events))
    for event in events:
        out += varint(len(event)) + event
    out += varint(len(changes))
    for path, value in changes:
        out += path
        if value is None:
            out += b"\x00"
        else:
            out += b"\x01" + varint(len(value)) + value
    return bytes(out)


def diff_bytes(version, base_root, result_root, gas_used, events, changes) -> bytes:
    payload = diff_payload(version, base_root, result_root, gas_used, events, changes)
    return payload + diff_checksum(payload)


def apply_changes(leaves: dict[bytes, bytes], changes: list[tuple[bytes, bytes | None]]) -> dict[bytes, bytes]:
    result = dict(leaves)
    for path, value in changes:
        if value is None:
            result.pop(path, None)
        else:
            result[path] = value
    return result


def main() -> int:
    for length, expected in OFFICIAL_B3.items():
        if blake3(bytes(i % 251 for i in range(length))).hex() != expected:
            raise SystemExit(f"pure-Python BLAKE3 disagrees with official vector len={length}")

    # ---- deterministic base state: 3 C7 account keys ----
    raw_keys = [b"\x01" + bytes([i + 1]) * 20 for i in range(3)]
    paths = [state_path(key) for key in raw_keys]
    base_values = [
        account_bytes(0, 1_000_000),
        account_bytes(3, 500),
        account_bytes(1, 42),
    ]
    base_leaves = {path: value for path, value in zip(paths, base_values)}
    base_tree_root = tree_root(base_leaves)
    base_root = versioned_root(BASE_VERSION, base_tree_root)

    lines = [
        "# AuraLite-Crypto S3 execution-overlay golden vectors",
        "# Generated by tools/gen_s3_vectors.py (independent Python).",
        "# Diff framing: payload || BLAKE3(S3_DIFF_CONTEXT || payload).",
        f"diff_magic={DIFF_MAGIC.hex()}",
        f"state_key_context={STATE_KEY_CONTEXT.hex()}",
        f"s1_root_context={S1_ROOT_CONTEXT.hex()}",
        f"s3_diff_context={S3_DIFF_CONTEXT.hex()}",
        f"s3_event_context={S3_EVENT_CONTEXT.hex()}",
        f"base_version={BASE_VERSION}",
        f"account_len={ACCOUNT_LEN}",
        f"max_writes={MAX_WRITES}",
        f"max_events={MAX_EVENTS}",
        f"max_value_len={1 << 20}",
        f"base_tree_root={base_tree_root.hex()}",
        f"base_root={base_root.hex()}",
    ]
    for index, (key, path, value) in enumerate(zip(raw_keys, paths, base_values)):
        lines += [
            f"account_{index}_key={key.hex()}",
            f"account_{index}_path={path.hex()}",
            f"account_{index}_value={value.hex()}",
        ]

    # ---- canonical diffs ----
    scenarios = [
        ("diff_1", 1, [], [(paths[0], account_bytes(1, 1_000_500))]),
        ("diff_2", 2, [b"transfer", b"debit"],
         [(paths[0], account_bytes(2, 999_900)),
          (paths[1], account_bytes(4, 600))]),
        ("diff_3", 3, [b"mint", b"burn"],
         [(paths[0], account_bytes(2, 1_000_000)),
          (paths[2], None)]),
    ]
    for name, gas, events, changes in scenarios:
        result_leaves = apply_changes(base_leaves, changes)
        result_root = versioned_root(BASE_VERSION + 1, tree_root(result_leaves))
        canonical = diff_bytes(
            BASE_VERSION + 1, base_root, result_root, gas, events, changes)
        lines += [
            f"{name}_bytes={canonical.hex()}",
            f"{name}_root={result_root.hex()}",
            f"{name}_events_root={events_root(events).hex()}",
            f"{name}_gas={gas}",
            f"{name}_changes={len(changes)}",
        ]
    lines += [
        f"revert_root={base_root.hex()}",
        f"out_of_gas_root={base_root.hex()}",
    ]

    # ---- rejection vectors ----
    diff1 = diff_payload(
        BASE_VERSION + 1, base_root,
        bytes.fromhex(next(l.split("=", 1)[1] for l in lines if l.startswith("diff_1_root="))),
        1, [], [(paths[0], account_bytes(1, 1_000_500))
    ])

    # helper to build a valid-checksum diff from an arbitrary payload
    def framed(payload: bytes) -> bytes:
        return payload + diff_checksum(payload)

    # 1. bad checksum
    bad = bytearray(framed(diff1))
    bad[-1] ^= 0x01
    lines.append(f"bad_checksum_diff={bytes(bad).hex()}")

    # 2. trailing byte
    lines.append(f"bad_trailing_diff={framed(diff1 + b'\x00').hex()}")

    # 3. truncated
    lines.append(f"bad_truncated_diff={framed(diff1)[:20].hex()}")

    # 4. bad magic
    bad_magic = b"XXXX" + diff1[4:]
    lines.append(f"bad_magic_diff={framed(bad_magic).hex()}")

    # 5. result root mismatch (valid checksum)
    bad_root = bytearray(diff1)
    bad_root[4 + 8 + 32] ^= 0x01  # first byte of result_root
    lines.append(f"bad_root_diff={framed(bytes(bad_root)).hex()}")

    # 6. unordered changes (valid checksum): two changes in DESCENDING path order
    two = [(paths[1], account_bytes(4, 600)), (paths[0], account_bytes(2, 999_900))]
    if two[0][0] < two[1][0]:
        two.reverse()
    assert two[0][0] > two[1][0], "unordered vector must actually be unsorted"
    unsorted_payload = diff_payload(
        BASE_VERSION + 1, base_root,
        bytes.fromhex(next(l.split("=", 1)[1] for l in lines if l.startswith("diff_2_root="))),
        2, [], two)
    lines.append(f"bad_unsorted_diff={framed(unsorted_payload).hex()}")

    # 7. duplicate changes
    duplicate = [
        (paths[0], account_bytes(2, 999_900)),
        (paths[0], account_bytes(2, 999_900)),
    ]
    dup_payload = diff_payload(
        BASE_VERSION + 1, base_root,
        bytes.fromhex(next(l.split("=", 1)[1] for l in lines if l.startswith("diff_2_root="))),
        2, [], duplicate)
    lines.append(f"bad_duplicate_diff={framed(dup_payload).hex()}")

    # 8. non-minimal event count varint: 0x80 0x00 with valid checksum
    result_root_1 = bytes.fromhex(next(l.split("=", 1)[1] for l in lines if l.startswith("diff_1_root=")))
    minimal = diff_payload(BASE_VERSION + 1, base_root, result_root_1, 1, [], [(paths[0], account_bytes(1, 1_000_500))
    ])
    # minimal layout: magic(4) version(8) base(32) result(32) gas(8) events_count(1) writes_count(1)
    prefix_len = 4 + 8 + 32 + 32 + 8
    nonminimal = minimal[:prefix_len] + b"\x80\x00" + minimal[prefix_len + 1:]
    lines.append(f"bad_nonminimal_diff={framed(nonminimal).hex()}")

    # 9. oversized writes count
    oversized = minimal[:prefix_len] + b"\x00" + varint(MAX_WRITES + 1) + minimal[prefix_len + 2:]
    lines.append(f"bad_count_diff={framed(oversized).hex()}")

    # 10. bad option tag on first write
    bad_tag = bytearray(minimal)
    bad_tag[-ACCOUNT_LEN - 2] = 2
    lines.append(f"bad_option_tag_diff={framed(bytes(bad_tag)).hex()}")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  base accounts: {len(raw_keys)}, canonical diffs: {len(scenarios)}, "
          f"rejection vectors: 10")
    print("  self-check: official BLAKE3 vectors OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
