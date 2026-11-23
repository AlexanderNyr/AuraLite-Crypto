#!/usr/bin/env python3
"""E8 NFT + DAO UI negative-control mutations (ECOSYSTEM_PLAN E8).

Kept OUTSIDE the repo source (tools-only) so nothing can compile against the
mutated text. Each mutation flips exactly one E8 named message line (six
mutations, one per targeted message; `bad-ui-vote` is the gate mutation for
the "first DAO vote through the UI" DoD), expects exactly one anchored
occurrence in its source file, runs
`cargo test --locked -p aura-storage --test e8_nft` and requires a non-zero
exit, then restores the exact original bytes in a `finally` block.

The suite must fail on every mutation (red) and pass after restore (green).
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
NFT = ROOT / "crates" / "aura-storage" / "src" / "nft.rs"
UI = ROOT / "crates" / "aura-storage" / "src" / "dao_ui.rs"
ANCHORS = [(NFT, "pub struct NftWorld {"), (UI, "pub struct DaoUi {")]
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "e8_nft"]

MUTATIONS = [
    (
        "bad-mint-exists",
        NFT,
        '"E8: token already exists ({id})"',
        '"E8: token already exists"',
    ),
    (
        "bad-transfer-owner",
        NFT,
        '"E8: not token owner ({actor}/{id})"',
        '"E8: not token owner"',
    ),
    (
        "bad-stale-approval",
        NFT,
        '"E8: not approved ({actor}/{id})"',
        '"E8: not approved"',
    ),
    (
        "bad-name-taken",
        NFT,
        '"E8: name already registered ({name})"',
        '"E8: name already registered"',
    ),
    (
        "bad-name-owner",
        NFT,
        '"E8: not name owner ({actor}/{name})"',
        '"E8: not name owner"',
    ),
    (
        "bad-ui-vote",
        NFT,
        '"E8: vote already cast ({proposal}/{voter})"',
        '"E8: vote already cast"',
    ),
]


def main() -> int:
    originals = {}
    texts = {}
    for path, anchor in ANCHORS:
        originals[path] = path.read_bytes()
        text = originals[path].decode()
        texts[path] = text
        if text.count(anchor) != 1:
            print(
                f"NC-BAD anchor count != 1 in {path.name}: {text.count(anchor)}",
                file=sys.stderr,
            )
            return 1

    passed = 0
    try:
        for name, path, old, new in MUTATIONS:
            text = texts[path]
            if text.count(old) != 1:
                print(
                    f"NC-BAD [{name}] anchor count != 1: {text.count(old)}",
                    file=sys.stderr,
                )
                return 1
            path.write_bytes(text.replace(old, new, 1).encode())
            proc = subprocess.run(TEST_CMD, cwd=ROOT, capture_output=True, text=True)
            if proc.returncode == 0:
                print(f"NC-BAD [{name}] mutation survived (test still green)",
                      file=sys.stderr)
                return 1
            passed += 1
            print(f"NC-RED [{name}] ok")
    finally:
        for path, data in originals.items():
            path.write_bytes(data)

    # Restore check: the suite must be green again.
    proc = subprocess.run(TEST_CMD, cwd=ROOT, capture_output=True, text=True)
    if proc.returncode != 0:
        print("NC-BAD restore: suite still red", file=sys.stderr)
        return 1
    print(f"NC-GREEN restore ok ({passed}/6 red as required)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
