#!/usr/bin/env python3
"""Generate S6 retention golden vectors from an INDEPENDENT Python model.

No Rust code, no network. Contains pure-Python BLAKE3 (self-checked against
official BLAKE3 vectors), the C7 state-key digest, the S2 key path, a
from-scratch rebuild of the S1 sparse Merkle tree, and the S6 wire formats.

What is modelled, and why:

* **Roots** for the first `SCHEDULE_BLOCKS` blocks are computed by REBUILDING
  the tree from the live state at each checkpoint — a different algorithm from
  the Rust incremental `HistoryTree`. If the two disagree, the S6 roots are
  wrong.
* **Byte accounting** for the full 100 000-block run is computed exactly, with
  no hashing, from the canonical encodings: `record bytes = payload ||
  BLAKE3(S6_RECORD_CONTEXT || payload)` and `base bytes = payload ||
  BLAKE3(S6_BASE_CONTEXT || payload)`. The Rust gate must reproduce every
  number.

Wire formats (integers fixed-width LE unless `v` = minimal LEB128):

    record payload = "ALP6" || u64 version || 32 parent_root || 32 state_root
        || v change_count || (v key_len || key || tag || [v value_len ||
        value] || u64 nonce)*
    record bytes   = payload || BLAKE3(S6_RECORD_CONTEXT || payload)

    base payload   = "ALB6" || u64 version || v entry_count ||
        (32 path || v value_len || value)*
    base bytes     = payload || BLAKE3(S6_BASE_CONTEXT || payload)

Retention rule (prune mode), applied after every commit:

    base_version = max(base_version, head - keep_versions)

so a store always keeps exactly `min(head, keep_versions)` deltas plus one
materialised base snapshot.

Usage:
    python3 tools/gen_s6_vectors.py
    make gen-s6-vectors
"""

from __future__ import annotations

import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-storage" / "tests" / "fixtures" / "s6_golden.txt"

STATE_KEY_CONTEXT = b"AuraLite-Crypto/state-key-v1\x00"        # C7
ADDR_CONTEXT = b"AuraLite-Crypto/s1-addr-v1\x00"               # S1
VALUE_CONTEXT = b"AuraLite-Crypto/s1-value-v1\x00"             # S1
EMPTY_CONTEXT = b"AuraLite-Crypto/s1-empty-v1\x00"             # S1
LEAF_CONTEXT = b"AuraLite-Crypto/s1-leaf-v1\x00"               # S1
NODE_CONTEXT = b"AuraLite-Crypto/s1-node-v1\x00"               # S1
ROOT_CONTEXT = b"AuraLite-Crypto/s1-root-v1\x00"               # S1
S2_KEY_CONTEXT = b"AuraLite-Crypto/s2-key-v1\x00"              # S2

RECORD_MAGIC = b"ALP6"
BASE_MAGIC = b"ALB6"
S6_RECORD_CONTEXT = b"AuraLite-Crypto/s6-record-v1\x00"
S6_BASE_CONTEXT = b"AuraLite-Crypto/s6-base-v1\x00"

BLOCKS = 100_000          # the S6 DoD height
SCHEDULE_BLOCKS = 1_000   # prefix for which independent roots are computed
WRITES_PER_BLOCK = 4
LIVE_KEYS = 1_024
KEEP_VERSIONS = 128
CHECKPOINTS = [1, 100, 200, 300, 400, 500, 600, 700, 800, 900, 999, 1000]

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


def varint_len(value: int) -> int:
    return len(varint(value))


def u16(value: int) -> bytes:
    return struct.pack("<H", value)


def u64(value: int) -> bytes:
    return struct.pack("<Q", value)


# ------------------------------------------------------------------- state ---
def account_key(index: int) -> bytes:
    """C7 account state key: 0x01 || address[20]."""
    return b"\x01" + blake3(ADDR_CONTEXT + u64(index))[:20]


def key_path(key: bytes) -> bytes:
    """S2 domain-separated key path (the S6 tree path)."""
    return blake3(S2_KEY_CONTEXT + u64(len(key)) + key)


def value_bytes(seed: int) -> bytes:
    return blake3(VALUE_CONTEXT + u64(seed))


def key_bit(path: bytes, depth: int) -> int:
    return (path[depth // 8] >> (7 - depth % 8)) & 1


_EMPTY_CACHE: dict[int, bytes] = {}


def empty_hash(depth: int) -> bytes:
    """Depth-dependent empty-subtree hash (memoised: only 257 distinct values)."""
    cached = _EMPTY_CACHE.get(depth)
    if cached is None:
        cached = blake3(EMPTY_CONTEXT + u16(depth))
        _EMPTY_CACHE[depth] = cached
    return cached


def leaf_hash(depth: int, path: bytes, value: bytes) -> bytes:
    return blake3(LEAF_CONTEXT + u16(depth) + path + u64(len(value)) + value)


def node_hash(left: bytes, right: bytes) -> bytes:
    return blake3(NODE_CONTEXT + left + right)


def versioned_root(version: int, tree_root: bytes) -> bytes:
    return blake3(ROOT_CONTEXT + u64(version) + tree_root)


def rebuild_root(entries: list[tuple[bytes, bytes]]) -> bytes:
    """Rebuild the S1 tree from scratch (independent of the Rust algorithm)."""
    return _subtree(0, sorted(entries))


def _subtree(depth: int, entries: list[tuple[bytes, bytes]]) -> bytes:
    if not entries:
        return empty_hash(depth)
    if len(entries) == 1:
        path, value = entries[0]
        return leaf_hash(depth, path, value)
    left = [entry for entry in entries if key_bit(entry[0], depth) == 0]
    right = [entry for entry in entries if key_bit(entry[0], depth) == 1]
    return node_hash(_subtree(depth + 1, left), _subtree(depth + 1, right))


# --------------------------------------------------------------- schedule ---
KEYS = [account_key(index) for index in range(LIVE_KEYS)]
PATHS = [key_path(key) for key in KEYS]
# Every value is a 32-byte BLAKE3 digest, so record SIZE depends only on the
# schedule shape; values are hashed only for the blocks whose roots we rebuild.
VALUE_LEN = 32


def schedule(block: int) -> list[int]:
    """Key indices touched by `block` — the same formula as the Rust gate."""
    return [(block * 31 + slot * 17) % LIVE_KEYS for slot in range(WRITES_PER_BLOCK)]


def block_changes(block: int) -> list[tuple[bytes, bytes, int]]:
    """Deterministic `(key, value, nonce)` triples — hashing, first 1 000 blocks."""
    return [
        (KEYS[index], value_bytes(block * WRITES_PER_BLOCK + slot), block)
        for slot, index in enumerate(schedule(block))
    ]


def block_change_lens(block: int) -> list[tuple[int, int]]:
    """`(key_len, value_len)` per change — enough for exact byte accounting."""
    return [(len(KEYS[index]), VALUE_LEN) for index in schedule(block)]


def record_len(version: int, change_lens: list[tuple[int, int]]) -> int:
    payload = len(RECORD_MAGIC) + 8 + 32 + 32 + varint_len(len(change_lens))
    for key_len, value_len in change_lens:
        payload += varint_len(key_len) + key_len + 1     # tag = put
        payload += varint_len(value_len) + value_len
        payload += 8                                     # nonce
    return payload + 32


def record_hex(version: int, parent_root: bytes, state_root: bytes,
               changes: list[tuple[bytes, bytes, int]]) -> str:
    """Canonical `VersionRecord` bytes: payload || BLAKE3(record context)."""
    payload = bytearray()
    payload += RECORD_MAGIC
    payload += u64(version)
    payload += parent_root
    payload += state_root
    payload += varint(len(changes))
    for key, value, nonce in changes:
        payload += varint(len(key)) + key + b"\x01"
        payload += varint(len(value)) + value
        payload += u64(nonce)
    return (bytes(payload) + blake3(S6_RECORD_CONTEXT + bytes(payload))).hex()


def base_hex(version: int, entries: dict[bytes, bytes]) -> str:
    """Canonical base-snapshot bytes: payload || BLAKE3(base context)."""
    payload = bytearray()
    payload += BASE_MAGIC
    payload += u64(version)
    ordered = sorted(entries.items())
    payload += varint(len(ordered))
    for path, value in ordered:
        payload += path
        payload += varint(len(value)) + value
    return (bytes(payload) + blake3(S6_BASE_CONTEXT + bytes(payload))).hex()


def base_len(version: int, entries: dict[bytes, bytes]) -> int:
    payload = len(BASE_MAGIC) + 8 + varint_len(len(entries))
    payload += sum(32 + varint_len(len(value)) + len(value) for value in entries.values())
    return payload + 32


def main() -> int:
    for length, expected in OFFICIAL_B3.items():
        got = blake3(bytes(i % 251 for i in range(length)))
        assert got.hex() == expected, f"BLAKE3 self-check failed for {length} bytes"
    print("BLAKE3 self-check: 7/7 official vectors OK")

    # ---- prefix with independent roots (rebuild tree at each checkpoint) ----
    live: dict[bytes, bytes] = {}
    checkpoints: dict[int, str] = {}
    root_at_checkpoint: dict[int, bytes] = {}
    for block in range(1, SCHEDULE_BLOCKS + 1):
        for key, value, _nonce in block_changes(block):
            live[key_path(key)] = value
        if block in CHECKPOINTS:
            tree_root = rebuild_root(list(live.items()))
            root = versioned_root(block, tree_root)
            root_at_checkpoint[block] = root
            checkpoints[block] = root.hex()
    print(f"independent roots: {len(checkpoints)} checkpoints over {SCHEDULE_BLOCKS} blocks")

    first_len = record_len(1, block_change_lens(1))
    last_len = record_len(SCHEDULE_BLOCKS, block_change_lens(SCHEDULE_BLOCKS))

    # ---- full 100 000-block byte accounting (no hashing needed) ----
    archive_total = base_len(0, {}) + sum(record_len(block, block_change_lens(block))
                                          for block in range(1, BLOCKS + 1))

    base: dict[bytes, bytes] = {}
    base_version = 0
    retained = 0
    retained_bytes = 0
    pending: list[tuple[int, int]] = []      # (version, encoded_len) of retained deltas
    pruned_versions = 0
    pruned_bytes = 0
    peak = 0
    for version in range(1, BLOCKS + 1):
        length = record_len(version, block_change_lens(version))
        pending.append((version, length))
        retained += 1
        retained_bytes += length
        target = max(0, version - KEEP_VERSIONS)
        while base_version < target:
            delta_version, delta_len = pending.pop(0)
            for slot_index in schedule(delta_version):
                base[PATHS[slot_index]] = b"\x00" * VALUE_LEN
            base_version = delta_version
            retained -= 1
            retained_bytes -= delta_len
            pruned_versions += 1
            pruned_bytes += delta_len
        disk = base_len(base_version, base) + retained_bytes
        peak = max(peak, disk)
    prune_final_base = base_len(base_version, base)
    prune_final_disk = prune_final_base + retained_bytes

    lines = [
        "# S6 retention golden vectors — independent Python model.",
        "# Roots: S1 rebuild per checkpoint. Bytes: canonical S6 encodings.",
        f"blocks={BLOCKS}",
        f"schedule_blocks={SCHEDULE_BLOCKS}",
        f"writes_per_block={WRITES_PER_BLOCK}",
        f"live_keys={LIVE_KEYS}",
        f"keep_versions={KEEP_VERSIONS}",
        f"record_len_first={first_len}",
        f"record_len_at_schedule_end={last_len}",
        f"archive_base_bytes={base_len(0, {})}",
        f"archive_total_bytes={archive_total}",
        f"prune_final_base_version={base_version}",
        f"prune_final_base_bytes={prune_final_base}",
        f"prune_final_records_bytes={retained_bytes}",
        f"prune_final_disk_bytes={prune_final_disk}",
        f"prune_peak_disk_bytes={peak}",
        f"prune_retained={retained}",
        f"prune_pruned_versions={pruned_versions}",
        f"prune_pruned_bytes={pruned_bytes}",
    ]
    for version in CHECKPOINTS:
        lines.append(f"checkpoint_root_{version}={checkpoints[version]}")
    lines.append(f"checkpoint_versions={','.join(str(v) for v in CHECKPOINTS)}")

    # ---- canonical bytes the Rust encoder must reproduce exactly ----
    lines.append(
        "record_hex_1="
        + record_hex(1, b"\x00" * 32, root_at_checkpoint[1], block_changes(1))
    )
    lines.append(
        "record_hex_1000="
        + record_hex(1000, root_at_checkpoint[999], root_at_checkpoint[1000],
                     block_changes(1000))
    )
    small: dict[bytes, bytes] = {}
    for block in range(1, 4):
        for key, value, _nonce in block_changes(block):
            small[key_path(key)] = value
    lines.append(f"base_hex_prune2_head5={base_hex(3, small)}")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  archive: {archive_total} bytes over {BLOCKS} blocks")
    print(f"  prune:   {prune_final_disk} bytes final, {peak} bytes peak (keep {KEEP_VERSIONS})")
    print(f"  ratio:   {archive_total / peak:.1f}x smaller at peak")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
