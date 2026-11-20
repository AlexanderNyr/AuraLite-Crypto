#!/usr/bin/env python3
"""Mutation controls for E2 (ECOSYSTEM_PLAN E2).

Each control injects ONE bug (restored after) and requires the named E2
message to appear in the red run. NC-1 is the phase gate literally:
'reproducible contract build = same code_hash' must go red when source
canonicalization is removed.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXPLORER = os.path.join(ROOT, "crates", "aura-storage/src/explorer.rs")

TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage",
            "--test", "e2_explorer"]

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> reproducible build must go red",
        EXPLORER,
        "    let joined = raw.split_whitespace().collect::<Vec<_>>().join(\" \");",
        "    let joined = raw.trim().to_string();",
        "E2: contract build must be reproducible (same code_hash)",
    ),
    (
        "NC-2 verified source check accepts a mismatched source",
        EXPLORER,
        "        Ok((_, hash)) => hash == *expected,",
        "        Ok((_, _hash)) => true,",
        "E2: verified source must match the deployed code_hash",
    ),
    (
        "NC-3 block views stop consulting the indexer",
        EXPLORER,
        "        let hash = self.index.canonical(height)?;",
        "        let hash = Some([7u8; 32])?;",
        "E2: explorer must read blocks from the indexer",
    ),
    (
        "NC-4 orphaned transactions report as live",
        EXPLORER,
        "        if self.index.is_canonical(&tx.block_hash) {",
        "        if self.index.knows(&tx.block_hash) {",
        "E2: tx status must follow the canonical chain",
    ),
    (
        "NC-5 account history forgets the canonical filter",
        EXPLORER,
        "            if !self.tx_is_canonical(tx) {",
        "            if false {",
        "E2: account history must match the chain",
    ),
    (
        "NC-6 search can no longer find contracts",
        EXPLORER,
        "        if self.contracts.contains_key(query) {",
        "        if self.contracts.contains_key(\"__none__\") {",
        "E2: search must find blocks, txs, accounts and contracts",
    ),
]

PASS_MARKERS = ("test result: ok.",)


def run_control(name, path, anchor, replacement, expected):
    with open(path, encoding="utf-8") as f:
        original = f.read()
    if original.count(anchor) != 1:
        return False, "anchor count %d" % original.count(anchor)
    mutated = original.replace(anchor, replacement)
    try:
        with open(path, "w", encoding="utf-8") as f:
            f.write(mutated)
        proc = subprocess.run(TEST_CMD, cwd=ROOT, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True,
                              errors="replace")
        output = proc.stdout or ""
    finally:
        with open(path, "w", encoding="utf-8") as f:
            f.write(original)
    compile_failed = "error[" in output and "test result" not in output
    if compile_failed:
        return False, "mutation broke compilation"
    if any(marker in output for marker in PASS_MARKERS):
        return False, "mutated gate still passes"
    if expected in output:
        return True, "red with named message"
    return False, "red without named message"


def main():
    passed = 0
    for name, path, anchor, replacement, expected in CONTROLS:
        ok, why = run_control(name, path, anchor, replacement, expected)
        print("[%s] %s: %s" % ("PASS" if ok else "FAIL", name, why))
        if ok:
            passed += 1
    total = len(CONTROLS)
    print("RESULT: %d/%d negative controls PASS" % (passed, total))
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
