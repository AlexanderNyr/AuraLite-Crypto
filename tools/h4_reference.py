#!/usr/bin/env python3
"""H4 cross-language reference (docs/h4-property-plan.md §1, D-H4-1).

(a) canonical Tx byte layout — driven through the existing independent C4
    Python encoder (`tools/gen_c4_vectors.py`, self-checked against RFC 8032
    and official BLAKE3 vectors by `tools/w1_tx_reference.py`);
(b) S1 sparse-merkle hash rules — a from-scratch pure-Python implementation
    of the empty/leaf/node layout and the recursive compressed tree.

Both are compared two ways against the Rust side by
`tools/h4_property_campaign.py`. Line format (h4-diff-v1):

    tx <case> <account_index> <nonce> <amount> <bytes hex>
    tree <case> <pathhex:valhex,...> <root hex>

Tree case paths are the 32-byte LE encoding of a case number.

Usage:
    python3 -m tools.h4_reference gen [--out FILE]
    python3 -m tools.h4_reference check FILE
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, "/home/user/aura-python-deps")

from tools import gen_c4_vectors as c4  # noqa: E402

S1_EMPTY = b"AuraLite-Crypto/s1-empty-v1\0"
S1_LEAF = b"AuraLite-Crypto/s1-leaf-v1\0"
S1_NODE = b"AuraLite-Crypto/s1-node-v1\0"


def case_path(i: int) -> bytes:
    return i.to_bytes(32, "little")


def empty_hash(depth: int) -> bytes:
    return c4.blake3_short(S1_EMPTY + depth.to_bytes(2, "little"))


def leaf_hash(depth: int, key: bytes, value: bytes) -> bytes:
    return c4.blake3_short(
        S1_LEAF + depth.to_bytes(2, "little") + key + len(value).to_bytes(8, "little") + value
    )


def node_hash(left: bytes, right: bytes) -> bytes:
    return c4.blake3_short(S1_NODE + left + right)


def key_bit(key: bytes, depth: int) -> int:
    byte = key[depth // 8]
    return (byte >> (7 - depth % 8)) & 1


def tree_root(entries) -> bytes:
    """Naive recursive sparse-merkle root over (path, value) entries."""
    return _root(sorted(entries, key=lambda item: item[0]), 0)


def _root(entries, depth: int) -> bytes:
    if not entries:
        return empty_hash(depth)
    if len(entries) == 1:
        key, value = entries[0]
        return leaf_hash(depth, key, value)
    split = next(
        (i for i, (key, _) in enumerate(entries) if key_bit(key, depth) == 1),
        len(entries),
    )
    return node_hash(_root(entries[:split], depth + 1), _root(entries[split:], depth + 1))


def account_seed(index: int) -> bytes:
    seed = bytearray(32)
    seed[0] = index
    seed[31] = (index + 0x5A) & 0xFF
    seed[15] = 0xC0
    return bytes(seed)


def tx_bytes(account_index: int, nonce: int, amount: int) -> bytes:
    """Canonical bytes of the H4 fixture transaction, built independently."""
    private = account_seed(account_index)
    public = c4.ed25519_public_key(private)
    case = dict(
        chain="h4-chain",
        nonce=nonce,
        kind=0,
        payload=amount.to_bytes(16, "little"),
        gas=1_000,
        max_fee=10,
        tip=1,
        expiry=100,
        address=c4.signer_address(public),
        public_key=public,
    )
    unsigned, _ = c4.encode_unsigned(case)
    signing_bytes = c4.SIGNING_CONTEXT + unsigned
    digest = c4.blake3_short(c4.domain_message("h4-chain", "tx", signing_bytes))
    case["signature"] = c4.ed25519_sign(digest, private)
    return unsigned + c4.encode_signature(case)


def gen(out_path: Path) -> int:
    lines = ["h4-diff-v1"]
    for case in range(64):
        index = case % 16
        lines.append(
            f"tx {case} {index} {case} {case + 1} {tx_bytes(index, case, case + 1).hex()}"
        )
    for case in range(32):
        count = 1 + case % 8
        entries = []
        for j in range(count):
            value_len = (case + j) % 24
            value = bytes((case + j + k) & 0xFF for k in range(value_len))
            entries.append((case_path(case * 32 + j), value))
        rendered = ",".join(f"{path.hex()}:{value.hex()}" for path, value in entries)
        lines.append(f"tree {case} {rendered} {tree_root(entries).hex()}")
    out_path.write_text("\n".join(lines) + "\n")
    print(f"gen: wrote {len(lines) - 1} cases to {out_path}")
    return 0


def check(path: Path) -> int:
    checked = 0
    for lineno, line in enumerate(path.read_text().splitlines(), 1):
        if not line or line.startswith("h4-diff-v1"):
            continue
        fields = line.split(" ")
        if fields[0] == "tx" and len(fields) == 6:
            _, _case, index, nonce, amount, expected = fields
            if tx_bytes(int(index), int(nonce), int(amount)).hex() != expected:
                print(f"H4: canonical roundtrip violated (line {lineno})")
                return 1
            checked += 1
        elif fields[0] == "tree" and len(fields) == 4:
            _, _case, entries, expected = fields
            parsed = [
                (bytes.fromhex(p), bytes.fromhex(v))
                for p, v in (item.split(":") for item in entries.split(","))
            ]
            if tree_root(parsed).hex() != expected:
                print(f"H4: tree differential mismatch (line {lineno})")
                return 1
            checked += 1
        else:
            print(f"line {lineno}: unknown case")
            return 1
    print(f"check: {checked} cases OK")
    return 0


def main() -> int:
    args = sys.argv[1:]
    if args and args[0] == "gen":
        out = Path(args[args.index("--out") + 1]) if "--out" in args else Path("/tmp/h4_expected.txt")
        return gen(out)
    if args and args[0] == "check" and len(args) >= 2:
        return check(Path(args[1]))
    print(__doc__)
    return 64


if __name__ == "__main__":
    sys.exit(main())
