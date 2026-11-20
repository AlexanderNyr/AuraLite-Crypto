#!/usr/bin/env python3
"""Mutation controls for E1 (ECOSYSTEM_PLAN E1).

Each control injects ONE bug (restored after) and requires the named E1
gate to go RED with the named message. NC-1 is the phase gate literally:
'10 000 blocks + artificial reorg -> 0 duplicates/losses' must go red
when rollback is removed ("NC: without rollback -> red").
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INDEXER = os.path.join(ROOT, "crates", "aura-storage/src/indexer.rs")

TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage",
            "--test", "e1_indexer"]

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> reorg must go red without rollback",
        INDEXER,
        "                row.canonical = false;",
        "                let _ = row;",
        "E1: indexer must roll back on reorg",
    ),
    (
        "NC-2 duplicate ingest slips past the idempotent no-op",
        INDEXER,
        "        if let Some(row) = self.rows.get(&block.hash) {",
        "        if let Some(row) = self.rows.get(&[0u8; 32]) {",
        "E1: indexer ingest must be idempotent",
    ),
    (
        "NC-3 rollback range swallows canonical rows below the ancestor",
        INDEXER,
        "        for height in (ancestor_height + 1)..=old_tip {",
        "        for height in 0..=old_tip {",
        "E1: reorg must leave 0 duplicates and 0 losses",
    ),
    (
        "NC-4 canonical height map shifts by one",
        INDEXER,
        "        if canonical {\n            self.chain.insert(block.height, block.hash);\n        }",
        "        if canonical {\n            self.chain.insert(block.height + 1, block.hash);\n        }",
        "E1: blocks must be indexed by canonical height",
    ),
    (
        "NC-5 out-of-order ingest no longer refused",
        INDEXER,
        "        if !extends_tip && !grows_fork {",
        "        if false {",
        "E1: blocks must be ingested in canonical order",
    ),
    (
        "NC-6 orphaned blocks start reporting as duplicates",
        INDEXER,
        "            return if row.canonical {\n                IngestOutcome::Duplicate\n            } else {\n                IngestOutcome::Orphaned\n            };",
        "            return IngestOutcome::Duplicate;",
        "E1: orphaned blocks must not count as canonical",
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
