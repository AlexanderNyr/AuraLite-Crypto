#!/usr/bin/env python3
"""Golden-vector inventory (C9).

Counts test vectors across every `crates/*/tests/fixtures/*.txt` file and
fails below `MIN_VECTORS`. A "constant" line (context bytes, limits, tags,
versions) is documentation, not a vector; the classification rule is explicit
below so the number is reproducible rather than asserted.

Usage:
    python3 tools/count_vectors.py
    make vector-inventory
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FIXTURE_GLOBS = "crates/*/tests/fixtures/*.txt"

# C9 DoD requires at least 200 vectors.
MIN_VECTORS = 200

CONSTANT_KEYS = {
    "codec_version",
    "protocol_version",
    "chain_id",
    "hash_context",
    "max_length",
    "max_items",
    "max_tx_payload",
    "max_block_txs",
    "max_qc_bytes",
    "max_proof_depth",
    "max_state_key_len",
    "seed",
    "count",
}

CONSTANT_PREFIXES = (
    "max_",
    "len_",
    "prefix_",
    "depth_",
    "validator_count_",
    "allocation_count_",
    "total_supply_",
    "codec_version",
    "protocol_version",
)

CONSTANT_SUFFIXES = ("_context",)


def is_vector(key: str) -> bool:
    if key in CONSTANT_KEYS:
        return False
    if key.startswith(CONSTANT_PREFIXES):
        return False
    if key.endswith(CONSTANT_SUFFIXES):
        return False
    return True


def count_file(path: Path) -> tuple[int, int]:
    raw = 0
    vectors = 0
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        raw += 1
        if is_vector(line.split("=", 1)[0]):
            vectors += 1
    return raw, vectors


def main() -> int:
    files = sorted(REPO.glob(FIXTURE_GLOBS))
    if not files:
        print("VECTOR INVENTORY: FAIL — no fixture files found")
        return 1

    total = 0
    lines = ["VECTOR INVENTORY (C9)", f"  minimum required: {MIN_VECTORS}", ""]
    for path in files:
        raw, vectors = count_file(path)
        total += vectors
        lines.append(f"  {path.relative_to(REPO)}: {vectors:>4} vectors ({raw} data lines)")
    lines += ["", f"  TOTAL: {total} vectors across {len(files)} fixtures"]

    if total < MIN_VECTORS:
        lines.append(f"VECTOR INVENTORY: FAIL — {total} < {MIN_VECTORS}")
        print("\n".join(lines))
        return 1
    lines.append(f"VECTOR INVENTORY: OK — {total} >= {MIN_VECTORS}")
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
