#!/usr/bin/env python3
"""Generate C5 block/header golden vectors from INDEPENDENT references.

No Rust code, no network. The generator contains:

  * pure-Python BLAKE3 (single chunk), self-checked against 7 official
    BLAKE3-team vectors;
  * a Python transcription of the C2 wire format, C3 domain framing and the
    C5 binary Merkle tree;
  * the committed C4 transaction vectors as block bodies (so C5 pins the
    block layer against the already-verified tx layer).

It emits four canonical blocks (2, 3, 0 and 1 transactions), a tx-order
sensitivity vector, and a set of non-canonical / tampered blocks.

Usage:
    python3 tools/gen_c5_vectors.py
    make gen-c5-vectors

Output: crates/aura-block/tests/fixtures/c5_golden.txt
"""

from __future__ import annotations

import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
C4_FIXTURE = REPO / "crates" / "aura-tx" / "tests" / "fixtures" / "c4_golden.txt"
OUT = REPO / "crates" / "aura-block" / "tests" / "fixtures" / "c5_golden.txt"

PROTOCOL_VERSION = 1
CHAIN_ID = "auralite-test-1"
HASH_CONTEXT = b"AuraLite-Crypto/crypto-v1\x00"        # C3
LEAF_CONTEXT = b"AuraLite-Crypto/tx-leaf-v1\x00"
NODE_CONTEXT = b"AuraLite-Crypto/tx-node-v1\x00"
BLOCK_HASH_PAYLOAD_CONTEXT = b"AuraLite-Crypto/block-hash-v1\x00"
MAX_BLOCK_TXS = 65536
MAX_QC_BYTES = 65536
ZERO32 = bytes(32)

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

# ------------------------------------------------------- C2 wire helpers ---
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


def u128(value: int) -> bytes:
    return value.to_bytes(16, "little")


def text(value: str) -> bytes:
    raw = value.encode("utf-8")
    return varint(len(raw)) + raw


def domain_message(chain_id: str, tag: str, payload: bytes) -> bytes:
    prefix = chain_id.encode() + b"\x00" + tag.encode() + b"\x00"
    return HASH_CONTEXT + u64(len(prefix)) + prefix + u64(len(payload)) + payload


def block_hash(chain: str, header_bytes: bytes) -> bytes:
    payload = BLOCK_HASH_PAYLOAD_CONTEXT + header_bytes
    return blake3_short(domain_message(chain, "block", payload))


# ------------------------------------------------------------ Merkle tree ---
def leaf_hash(txid: bytes) -> bytes:
    return blake3_short(LEAF_CONTEXT + txid)


def node_hash(left: bytes, right: bytes) -> bytes:
    return blake3_short(NODE_CONTEXT + left + right)


def tx_root(txids: list[bytes]) -> bytes:
    """Binary Merkle root; odd levels duplicate the last node; empty = zero."""
    if not txids:
        return ZERO32
    level = [leaf_hash(t) for t in txids]
    while len(level) > 1:
        if len(level) % 2:
            level.append(level[-1])
        level = [node_hash(level[i], level[i + 1]) for i in range(0, len(level), 2)]
    return level[0]


# ------------------------------------------------------------- C5 layout ---
def encode_header(p: dict) -> tuple[bytes, dict[str, int]]:
    parts: list[bytes] = []
    offsets: dict[str, int] = {}
    pos = 0

    def add(name: str, blob: bytes) -> None:
        nonlocal pos
        offsets[name] = pos
        parts.append(blob)
        pos += len(blob)

    chain = p["chain"].encode("utf-8")
    add("version", u32(p["version"]))
    add("chain_len", varint(len(chain)))
    add("chain", chain)
    add("height", u64(p["height"]))
    add("parent", p["parent"])
    add("tx_root", p["tx_root"])
    add("state_root", p["state_root"])
    add("validator_set_hash", p["validator_set_hash"])
    add("timestamp_ms", u64(p["timestamp_ms"]))
    add("proposer", p["proposer"])
    if p["qc"] is None:
        add("qc_tag", b"\x00")
    else:
        add("qc_tag", b"\x01")
        add("qc_len", varint(len(p["qc"])))
        add("qc", p["qc"])
    add("gas_used", u64(p["gas_used"]))
    add("base_fee", u128(p["base_fee"]))
    return b"".join(parts), offsets


def encode_body(tx_blobs: list[bytes]) -> bytes:
    return varint(len(tx_blobs)) + b"".join(tx_blobs)


def read_c4() -> dict[int, dict]:
    data: dict[str, str] = {}
    for line in C4_FIXTURE.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        data[key] = value
    out = {}
    for index in range(1, 6):
        out[index] = {
            "bytes": bytes.fromhex(data[f"tx{index}_bytes"]),
            "txid": bytes.fromhex(data[f"tx{index}_txid"]),
            "address": bytes.fromhex(data[f"tx{index}_address"]),
            "nonce": int(data[f"tx{index}_nonce"]),
            "chain": data[f"tx{index}_chain"],
        }
    return out


def sort_key(tx: dict) -> tuple[bytes, int]:
    return (tx["address"], tx["nonce"])


def main() -> int:
    for length, expected in OFFICIAL_B3.items():
        if blake3_short(bytes(i % 251 for i in range(length))).hex() != expected:
            raise SystemExit(f"pure-Python BLAKE3 disagrees with official vector len={length}")

    c4 = read_c4()
    # Only testnet txs may appear in a Stage 1 block.
    usable = {k: v for k, v in c4.items() if v["chain"] == CHAIN_ID}

    def sorted_ids(indices: list[int]) -> list[int]:
        return sorted(indices, key=lambda i: sort_key(usable[i]))

    base = dict(
        version=PROTOCOL_VERSION, chain=CHAIN_ID, height=100,
        parent=bytes([0x11] * 32), state_root=bytes([0x22] * 32),
        validator_set_hash=bytes([0x33] * 32), timestamp_ms=1_700_000_000_000,
        proposer=bytes([0x44] * 20), qc=None, gas_used=21_000, base_fee=10**9,
    )
    cases = [
        dict(base, tx_ids=sorted_ids([1, 2])),
        dict(base, height=101, timestamp_ms=1_700_000_001_000,
             tx_ids=sorted_ids([1, 2, 5])),
        dict(base, height=0, parent=ZERO32, state_root=ZERO32,
             validator_set_hash=ZERO32, timestamp_ms=0, proposer=ZERO32[:20],
             gas_used=0, base_fee=0, qc=None, tx_ids=[]),
        dict(base, height=2**64 - 1, parent=bytes([0xFF] * 32),
             state_root=bytes([0xEE] * 32), validator_set_hash=bytes([0xDD] * 32),
             timestamp_ms=2**64 - 1, proposer=bytes([0xCC] * 20),
             qc=bytes([0xAA] * 8), gas_used=2**64 - 1, base_fee=2**128 - 1,
             tx_ids=[4]),
    ]

    lines = [
        "# AuraLite-Crypto C5 block/header golden vectors",
        "# Generated by tools/gen_c5_vectors.py (independent Python).",
        "# Self-checked against official BLAKE3 vectors; bodies reuse the",
        "# committed C4 tx vectors. Format: key=<hex|text|decimal>.",
        f"protocol_version={PROTOCOL_VERSION}",
        f"chain_id={CHAIN_ID}",
        f"hash_context={HASH_CONTEXT.hex()}",
        f"leaf_context={LEAF_CONTEXT.hex()}",
        f"node_context={NODE_CONTEXT.hex()}",
        f"block_hash_payload_context={BLOCK_HASH_PAYLOAD_CONTEXT.hex()}",
        f"max_block_txs={MAX_BLOCK_TXS}",
        f"max_qc_bytes={MAX_QC_BYTES}",
    ]

    blocks = []
    previous_hash = ZERO32
    for index, case in enumerate(cases, 1):
        case = dict(case)
        if index == 2:
            case["parent"] = previous_hash  # real chain link block1 -> block2
        txs = [usable[i] for i in case["tx_ids"]]
        root = tx_root([t["txid"] for t in txs])
        case["tx_root"] = root
        header, offsets = encode_header(case)
        body = encode_body([t["bytes"] for t in txs])
        block = header + body
        digest = block_hash(case["chain"], header)
        previous_hash = digest
        blocks.append(dict(case=case, header=header, offsets=offsets, body=body,
                           block=block, digest=digest, txs=txs))

        p = lambda k: f"b{index}_{k}"  # noqa: E731
        lines += [
            f"{p('version')}={case['version']}",
            f"{p('chain')}={case['chain']}",
            f"{p('height')}={case['height']}",
            f"{p('parent')}={case['parent'].hex()}",
            f"{p('state_root')}={case['state_root'].hex()}",
            f"{p('validator_set_hash')}={case['validator_set_hash'].hex()}",
            f"{p('timestamp_ms')}={case['timestamp_ms']}",
            f"{p('proposer')}={case['proposer'].hex()}",
            f"{p('qc_present')}={0 if case['qc'] is None else 1}",
            f"{p('qc')}={'' if case['qc'] is None else case['qc'].hex()}",
            f"{p('gas_used')}={case['gas_used']}",
            f"{p('base_fee')}={case['base_fee']}",
            f"{p('tx_ids')}={','.join(str(i) for i in case['tx_ids'])}",
            f"{p('tx_root')}={root.hex()}",
            f"{p('header')}={header.hex()}",
            f"{p('body')}={body.hex()}",
            f"{p('bytes')}={block.hex()}",
            f"{p('block_hash')}={digest.hex()}",
        ]

    # DoD vector: the same two txids in opposite order give a different root.
    ids = blocks[0]["case"]["tx_ids"]
    forward = [usable[i]["txid"] for i in ids]
    backward = [usable[i]["txid"] for i in reversed(ids)]
    if forward == backward:
        raise SystemExit("order-sensitivity vector needs two distinct txids")
    lines.append(f"tx_root_forward={tx_root(forward).hex()}")
    lines.append(f"tx_root_backward={tx_root(backward).hex()}")

    first = blocks[0]
    first_header, first_body = first["header"], first["body"]
    first_block = first["block"]
    off = first["offsets"]
    first_txs = [t["bytes"] for t in first["txs"]]
    reversed_txs = list(reversed(first_txs))
    reversed_root = tx_root([t["txid"] for t in reversed(first["txs"])])

    def patched(at: int, value: int) -> bytes:
        out = bytearray(first_block)
        out[at] = value
        return bytes(out)

    # Header whose root was recomputed for the REVERSED body: root check alone
    # would pass, so only the canonical-order check can reject this block.
    unsorted_case = dict(first["case"])
    unsorted_case["tx_root"] = reversed_root
    unsorted_header, _ = encode_header(unsorted_case)

    bad = {
        "bad_trailing": first_block + b"\x00",
        "bad_truncated": first_block[:-1],
        "bad_version": patched(off["version"], 2),
        "bad_root": patched(off["tx_root"], first_block[off["tx_root"]] ^ 0x01),
        "bad_reordered": first_header + encode_body(reversed_txs),
        "bad_unsorted_with_root": unsorted_header + encode_body(reversed_txs),
        "bad_duplicate": first_header + encode_body([first_txs[0], first_txs[0]]),
        "bad_qc_tag": patched(off["qc_tag"], 2),
        "bad_nonminimal_count": (first_header + b"\x80\x00"
                                 + first_body[1:]),
    }
    for key, value in bad.items():
        lines.append(f"{key}={value.hex()}")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  canonical blocks: {len(cases)} (2/3/0/1 transactions)")
    print(f"  rejection vectors: {len(bad)}")
    print("  self-check: BLAKE3 official vectors OK, C4 tx bodies reused")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
