#!/usr/bin/env python3
"""Generate S7 backup/restore golden vectors from an INDEPENDENT Python model.

No Rust code, no network. The S2 model (`tools/gen_s2_vectors.py`) is reused as
the single source of truth for the deterministic `vector_changes` batches, the
S1 sparse Merkle tree and the S2 WAL framing; this script adds the S7 bundle:

    bundle         = u32le(manifest_len) || manifest_bytes || blob
    manifest bytes = payload || BLAKE3(S7_MANIFEST_CONTEXT || payload)
    payload        = "ALM7" || u32 format_version || v-len chain_id ||
                     u64 head_version || 32 head_root || u64 record_count ||
                     u64 blob_len || 32 blob_sha256
    blob           = the framed S2 WAL records 1..=head (each with its own
                     BLAKE3 checksum)
    blob_sha256    = SHA-256(blob)          # hashlib, not BLAKE3

Emits:
  * the S2 state root for every version the chaos gate can reach;
  * the canonical bundle, manifest and blob digest for a 3-commit store;
  * the pinned constants (magic, context, format version, chain-id).

Usage:
    python3 tools/gen_s7_vectors.py
    make gen-s7-vectors
"""

from __future__ import annotations

import hashlib
import struct
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-storage" / "tests" / "fixtures" / "s7_golden.txt"

sys.path.insert(0, str(Path(__file__).resolve().parent))
# Reuse the INDEPENDENT S2 reference: same batches, same tree, same framing.
from gen_s2_vectors import (  # noqa: E402
    OFFICIAL_B3,
    blake3,
    encode_record,
    make_record,
    state_root,
    u64,
    varint,
    vector_changes,
)

MANIFEST_MAGIC = b"ALM7"
S7_MANIFEST_CONTEXT = b"AuraLite-Crypto/s7-manifest-v1\x00"
BACKUP_FORMAT_VERSION = 1
CHAIN_ID = "auralite-test-1"

CHAOS_CYCLES = 50
MAX_COMMITS = 12        # chaos cycles commit 1..=MAX_COMMITS records
GOLDEN_COMMITS = 3      # canonical bundle is emitted for a 3-record store


def u32(value: int) -> bytes:
    return struct.pack("<I", value)


def manifest_payload(head_version: int, head_root: bytes, record_count: int,
                     blob: bytes) -> bytes:
    blob_digest = hashlib.sha256(blob).digest()
    payload = bytearray(MANIFEST_MAGIC)
    payload += u32(BACKUP_FORMAT_VERSION)
    payload += varint(len(CHAIN_ID)) + CHAIN_ID.encode()
    payload += u64(head_version)
    payload += head_root
    payload += u64(record_count)
    payload += u64(len(blob))
    payload += blob_digest
    return bytes(payload)


def manifest_bytes(head_version: int, head_root: bytes, record_count: int,
                   blob: bytes) -> bytes:
    payload = manifest_payload(head_version, head_root, record_count, blob)
    return payload + blake3(S7_MANIFEST_CONTEXT + payload)


def bundle_bytes(head_version: int, head_root: bytes, record_count: int,
                 records: list[dict]) -> tuple[bytes, bytes, bytes]:
    blob = b"".join(encode_record(record) for record in records)
    manifest = manifest_bytes(head_version, head_root, record_count, blob)
    bundle = u32(len(manifest)) + manifest + blob
    return bundle, manifest, blob


def replay(commits: int) -> list[dict]:
    """The exact record chain a store with `commits` records must hold."""
    state: dict[bytes, bytes] = {}
    records: list[dict] = []
    parent = b"\x00" * 32
    for index in range(1, commits + 1):
        changes = vector_changes(index)
        for key, value, _nonce in changes:
            if value is None:
                state.pop(key, None)
            else:
                state[key] = value
        root = state_root(index, state)
        records.append(make_record(index, parent, root, changes))
        parent = root
    return records


def main() -> int:
    for length, expected in OFFICIAL_B3.items():
        if blake3(bytes(i % 251 for i in range(length))).hex() != expected:
            raise SystemExit(f"pure-Python BLAKE3 disagrees with official vector len={length}")

    # Roots for every version the chaos gate can reach (with the WAL-only
    # crash window the head is one record ahead of the requested count).
    roots = replay(MAX_COMMITS + 1)
    golden_records = replay(GOLDEN_COMMITS)
    golden_head = golden_records[-1]
    bundle, manifest, blob = bundle_bytes(
        GOLDEN_COMMITS, golden_head["state_root"], GOLDEN_COMMITS, golden_records
    )

    lines = [
        "# S7 backup/restore golden vectors — independent Python model.",
        "# Reuses the independent S2 reference for batches, tree and framing.",
        f"chain_id={CHAIN_ID}",
        f"format_version={BACKUP_FORMAT_VERSION}",
        f"manifest_magic={MANIFEST_MAGIC.hex()}",
        f"manifest_context={S7_MANIFEST_CONTEXT.hex()}",
        f"chaos_cycles={CHAOS_CYCLES}",
        f"max_commits={MAX_COMMITS}",
        f"golden_commits={GOLDEN_COMMITS}",
        f"golden_blob_len={len(blob)}",
        f"golden_blob_sha256={hashlib.sha256(blob).hexdigest()}",
        f"golden_manifest_len={len(manifest)}",
        f"golden_bundle_len={len(bundle)}",
        f"golden_manifest_hex={manifest.hex()}",
        f"golden_bundle_hex={bundle.hex()}",
    ]
    for record in roots:
        lines.append(f"root_at_{record['version']}={record['state_root'].hex()}")
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  chaos: {CHAOS_CYCLES} cycles, up to {MAX_COMMITS} (+1) commits each")
    print(f"  golden bundle: {len(bundle)} bytes, blob {len(blob)} bytes, "
          f"head v{GOLDEN_COMMITS}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
