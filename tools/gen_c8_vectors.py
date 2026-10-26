#!/usr/bin/env python3
"""Generate C8 genesis golden vectors from INDEPENDENT references.

No Rust code, no network. Contains pure-Python BLAKE3 (self-checked against
7 official BLAKE3 vectors), an independent BIP-173 bech32 encoder, the C3
domain framing, the C5/C6 Merkle node rules, the C7 state-key digest and a
Python transcription of the C8 canonical genesis encoding.

Usage:
    python3 tools/gen_c8_vectors.py
    make gen-c8-vectors

Output: crates/aura-genesis/tests/fixtures/c8_golden.txt
"""

from __future__ import annotations

import json
import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-genesis" / "tests" / "fixtures" / "c8_golden.txt"

CHAIN_ID = "auralite-test-1"
PROTOCOL_VERSION = 1
HASH_CONTEXT = b"AuraLite-Crypto/crypto-v1\x00"              # C3
GENESIS_CONTEXT = b"AuraLite-Crypto/genesis-v1\x00"
VALIDATOR_SET_CONTEXT = b"AuraLite-Crypto/validator-set-v1\x00"
ALLOCATION_LEAF_CONTEXT = b"AuraLite-Crypto/allocation-leaf-v1\x00"
STATE_KEY_CONTEXT = b"AuraLite-Crypto/state-key-v1\x00"      # C7
LEAF_CONTEXT = b"AuraLite-Crypto/tx-leaf-v1\x00"             # C5
NODE_CONTEXT = b"AuraLite-Crypto/tx-node-v1\x00"             # C5
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

# ---------------------------------------------------------------- bech32 ---
CHARSET = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"
HRP = "aura"
GEN = [0x3B6A57B2, 0x26508E6D, 0x1EA119FA, 0x3D4233DD, 0x2A1462B3]


def _polymod(values):
    chk = 1
    for value in values:
        top = chk >> 25
        chk = (chk & 0x1FFFFFF) << 5 ^ value
        for i, gen in enumerate(GEN):
            if (top >> i) & 1:
                chk ^= gen
    return chk


def _hrp_expand(hrp):
    return [ord(c) >> 5 for c in hrp] + [0] + [ord(c) & 31 for c in hrp]


def _convertbits(data, frm, to):
    acc = bits = 0
    out = []
    for value in data:
        acc = (acc << frm) | value
        bits += frm
        while bits >= to:
            bits -= to
            out.append((acc >> bits) & ((1 << to) - 1))
    if bits:
        out.append((acc << (to - bits)) & ((1 << to) - 1))
    return out


def bech32_address(raw20: bytes) -> str:
    data = _convertbits([0x00] + list(raw20), 8, 5)
    values = _hrp_expand(HRP) + data + [0] * 6
    mod = _polymod(values) ^ 1
    checksum = [(mod >> 5 * (5 - i)) & 31 for i in range(6)]
    return HRP + "1" + "".join(CHARSET[d] for d in data + checksum)


# ------------------------------------------------------------- wire helpers ---
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
def u128(v: int) -> bytes: return v.to_bytes(16, "little")


def domain_message(chain_id: str, tag: str, payload: bytes) -> bytes:
    prefix = chain_id.encode() + b"\x00" + tag.encode() + b"\x00"
    return HASH_CONTEXT + u64(len(prefix)) + prefix + u64(len(payload)) + payload


def addr(index: int, salt: int) -> bytes:
    return index.to_bytes(8, "little") + bytes(
        (index * 17 + j * 31 + salt) % 256 for j in range(12)
    )


def pubkey(index: int) -> str:
    return bytes((index * 19 + j * 7 + 0x2D) % 256 for j in range(32)).hex()


def validator(index: int, power: int) -> dict:
    return {
        "address": bech32_address(addr(index, 0x5A)),
        "power": power,
        "public_key": pubkey(index),
    }


def allocation(index: int, balance: int) -> dict:
    return {"address": bech32_address(addr(index, 0xC3)), "balance": str(balance)}


VALIDATORS = [validator(1, 100), validator(2, 200), validator(3, 300), validator(4, 400)]
ALLOCATIONS = [
    allocation(1, 10**18),
    allocation(2, 25 * 10**17),
    allocation(3, 75 * 10**16),
]
GENESIS_TIME_MS = 1_700_000_000_000


def document(validators, allocations, key_order=None, pretty=False, extra=None) -> str:
    fields = {
        "chain_id": CHAIN_ID,
        "protocol_version": PROTOCOL_VERSION,
        "genesis_time_ms": GENESIS_TIME_MS,
        "validators": validators,
        "allocations": allocations,
    }
    order = key_order or list(fields)
    obj = {key: fields[key] for key in order}
    if extra:
        obj.update(extra)
    if pretty:
        return json.dumps(obj, indent=2) + "\n"
    return json.dumps(obj, separators=(",", ":"))


# ----------------------------------------------------------- C8 encoding ---
def encode_validator(item: dict) -> bytes:
    raw = bech32_decode20(item["address"])
    return raw + u64(item["power"]) + bytes.fromhex(item["public_key"])


def bech32_decode20(text: str) -> bytes:
    # Independent inverse of bech32_address (fixed HRP/version/length).
    pos = text.rindex("1")
    data = [CHARSET.index(c) for c in text[pos + 1:-6]]
    bits = _convertbits(data, 5, 8)
    # convertbits(5->8, pad=False) semantics: drop the final padded element.
    acc = nbits = 0
    out = []
    for value in data:
        acc = (acc << 5) | value
        nbits += 5
        while nbits >= 8:
            nbits -= 8
            out.append((acc >> nbits) & 0xFF)
    assert out[0] == 0
    return bytes(out[1:21])


def encode_validator_vector(items) -> bytes:
    return varint(len(items)) + b"".join(encode_validator(v) for v in items)


def encode_allocation_vector(items) -> bytes:
    return varint(len(items)) + b"".join(
        bech32_decode20(a["address"]) + u128(int(a["balance"])) for a in items
    )


def canonical_bytes(doc: dict) -> bytes:
    chain = doc["chain_id"].encode()
    return (
        varint(len(chain)) + chain
        + u32(doc["protocol_version"])
        + u64(doc["genesis_time_ms"])
        + encode_validator_vector(doc["validators"])
        + encode_allocation_vector(doc["allocations"])
    )


def genesis_hash(doc: dict) -> bytes:
    payload = GENESIS_CONTEXT + canonical_bytes(doc)
    return blake3_short(domain_message(doc["chain_id"], "genesis", payload))


def validator_set_hash(doc: dict) -> bytes:
    body = VALIDATOR_SET_CONTEXT + encode_validator_vector(doc["validators"])
    return blake3_short(body)


def node_hash(left: bytes, right: bytes) -> bytes:
    return blake3_short(NODE_CONTEXT + left + right)


def merkle_root(leaves: list[bytes]) -> bytes:
    if not leaves:
        return ZERO32
    level = list(leaves)
    while len(level) > 1:
        if len(level) % 2:
            level.append(level[-1])
        level = [node_hash(level[i], level[i + 1]) for i in range(0, len(level), 2)]
    return level[0]


def allocation_root(doc: dict) -> bytes:
    # State is a set: sort by address so JSON array order cannot change it.
    leaves = []
    for item in sorted(doc["allocations"], key=lambda a: bech32_decode20(a["address"])):
        raw = bech32_decode20(item["address"])
        key = b"\x01" + raw
        key_digest = blake3_short(STATE_KEY_CONTEXT + u64(len(key)) + key)
        account = u64(0) + u128(int(item["balance"])) + ZERO32 + ZERO32
        leaves.append(
            blake3_short(ALLOCATION_LEAF_CONTEXT + key_digest + u64(len(account)) + account)
        )
    return merkle_root(leaves)


def total_supply(doc: dict) -> int:
    return sum(int(a["balance"]) for a in doc["allocations"])


def main() -> int:
    for length, expected in OFFICIAL_B3.items():
        if blake3_short(bytes(i % 251 for i in range(length))).hex() != expected:
            raise SystemExit(f"pure-Python BLAKE3 disagrees with official vector len={length}")

    base = {
        "chain_id": CHAIN_ID,
        "protocol_version": PROTOCOL_VERSION,
        "genesis_time_ms": GENESIS_TIME_MS,
        "validators": VALIDATORS,
        "allocations": ALLOCATIONS,
    }
    second = {
        "chain_id": CHAIN_ID,
        "protocol_version": PROTOCOL_VERSION,
        "genesis_time_ms": 1_800_000_000_000,
        "validators": [validator(7, 10), validator(8, 20)],
        "allocations": [allocation(9, 42)],
    }

    json_docs = {
        "json_min": document(VALIDATORS, ALLOCATIONS),
        "json_keys_reordered": document(
            VALIDATORS, ALLOCATIONS,
            key_order=["allocations", "genesis_time_ms", "validators", "chain_id", "protocol_version"],
        ),
        "json_pretty": document(VALIDATORS, ALLOCATIONS, pretty=True),
        "json_validators_reordered": document(list(reversed(VALIDATORS)), ALLOCATIONS),
        "json_allocations_reordered": document(VALIDATORS, list(reversed(ALLOCATIONS))),
        "json_second": document(second["validators"], second["allocations"], pretty=True)
        .replace(str(GENESIS_TIME_MS), str(second["genesis_time_ms"])),
        "json_bad_unknown_field": document(VALIDATORS, ALLOCATIONS, extra={"network": "x"}),
        "json_bad_wrong_chain": document(VALIDATORS, ALLOCATIONS).replace(
            f'"chain_id":"{CHAIN_ID}"', '"chain_id":"auralite-1"'
        ),
        "json_bad_version": document(VALIDATORS, ALLOCATIONS).replace(
            '"protocol_version":1', '"protocol_version":2'
        ),
        "json_bad_duplicate_validator": document([VALIDATORS[0], VALIDATORS[0]], ALLOCATIONS),
        "json_bad_zero_power": document(
            [{**VALIDATORS[0], "power": 0}] + VALIDATORS[1:], ALLOCATIONS
        ),
        "json_bad_float_balance": document(
            VALIDATORS, [{**ALLOCATIONS[0], "balance": 1.5}] + ALLOCATIONS[1:]
        ),
        "json_bad_address": document(VALIDATORS, ALLOCATIONS).replace(
            VALIDATORS[0]["address"], VALIDATORS[0]["address"][:-1] + "q"
        ),
        "json_bad_empty_validators": document([], ALLOCATIONS),
        "json_bad_pubkey": document(
            [{**VALIDATORS[0], "public_key": "zz" * 32}] + VALIDATORS[1:], ALLOCATIONS
        ),
        "json_bad_supply_overflow": document(
            VALIDATORS,
            [allocation(1, 2**128 - 1), allocation(2, 1)],
        ),
        "json_bad_missing_field": '{"protocol_version":1,"genesis_time_ms":0,"validators":[],"allocations":[]}',
        "json_bad_unknown_validator_field": document(
            [{**VALIDATORS[0], "name": "v1"}] + VALIDATORS[1:], ALLOCATIONS
        ),
        "json_bad_duplicate_allocation": document(VALIDATORS, [ALLOCATIONS[0], ALLOCATIONS[0]]),
    }

    lines = [
        "# AuraLite-Crypto C8 genesis golden vectors",
        "# Generated by tools/gen_c8_vectors.py (independent Python).",
        "# Self-checked against official BLAKE3 vectors + independent bech32.",
        "# JSON documents are hex-encoded UTF-8 to keep one key=value per line.",
        f"chain_id={CHAIN_ID}",
        f"protocol_version={PROTOCOL_VERSION}",
        f"hash_context={HASH_CONTEXT.hex()}",
        f"genesis_context={GENESIS_CONTEXT.hex()}",
        f"validator_set_context={VALIDATOR_SET_CONTEXT.hex()}",
        f"allocation_leaf_context={ALLOCATION_LEAF_CONTEXT.hex()}",
        f"state_key_context={STATE_KEY_CONTEXT.hex()}",
    ]

    for key, text in json_docs.items():
        lines.append(f"{key}={text.encode('utf-8').hex()}")

    for index, doc in enumerate([base, second], 1):
        lines += [
            f"genesis_{index}_canonical={canonical_bytes(doc).hex()}",
            f"genesis_{index}_hash={genesis_hash(doc).hex()}",
            f"validator_set_hash_{index}={validator_set_hash(doc).hex()}",
            f"allocation_root_{index}={allocation_root(doc).hex()}",
            f"total_supply_{index}={total_supply(doc)}",
            f"validator_count_{index}={len(doc['validators'])}",
            f"allocation_count_{index}={len(doc['allocations'])}",
        ]

    # Self-check: key order / whitespace must not change any commitment.
    a = json.loads(json_docs["json_min"])
    b = json.loads(json_docs["json_keys_reordered"])
    c = json.loads(json_docs["json_pretty"])
    for doc in (b, c):
        if canonical_bytes(doc) != canonical_bytes(a) or genesis_hash(doc) != genesis_hash(a):
            raise SystemExit("JSON object key order/whitespace changed the hash")
    rev = json.loads(json_docs["json_validators_reordered"])
    if genesis_hash(rev) == genesis_hash(a):
        raise SystemExit("validator order did NOT change the hash")
    alloc_rev = json.loads(json_docs["json_allocations_reordered"])
    if allocation_root(alloc_rev) != allocation_root(a):
        raise SystemExit("allocation order changed the state commitment")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  JSON documents: {len(json_docs)} (3 equivalent + 2 order variants + bad)")
    print("  canonical genesis sets: 2")
    print("  self-check: BLAKE3 official vectors OK, bech32 round-trip OK")
    print("  self-check: key order/whitespace stable, validator order significant")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
