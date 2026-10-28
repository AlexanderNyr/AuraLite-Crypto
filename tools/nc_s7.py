#!/usr/bin/env python3
"""Negative controls for S7 (backup, restore and the chaos gate).

Six tamper tests must each turn the S7 gate red:

  NC-1: change S7_MANIFEST_CONTEXT (manifest checksum domain) -> the golden
        bundle bytes no longer match the independent model.
  NC-2: drop the blob SHA-256 comparison -> a corrupted blob is accepted.
  NC-3: trust the manifest head root instead of replaying -> a re-checksummed
        lie about a record root is accepted.
  NC-4: drop the parent-root chain check -> a broken commit chain is accepted.
  NC-5: allow restore into a directory that already holds a store.
  NC-6: keep the WAL record when reopening (no replay of the fsynced-but-
        unpublished record) -> the chaos gate loses the crash-window record.

Rule (CONTRIBUTING.md §1): a gate that cannot go red is not a gate.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BACKUP_RS = REPO / "crates" / "aura-storage" / "src" / "backup.rs"
STORE_RS = REPO / "crates" / "aura-storage" / "src" / "store.rs"
TEST_RS = REPO / "crates" / "aura-storage" / "tests" / "s7_backup.rs"

CARGO_TEST = ["cargo", "test", "-p", "aura-storage", "--test", "s7_backup", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str, touch_test: bool = False) -> bool:
    original = path.read_text(encoding="utf-8")
    if old not in original:
        print(f"{label}: FAIL — tamper target not found in {path.name}")
        return False
    tampered = original.replace(old, new, 1)
    if tampered == original:
        print(f"{label}: FAIL — tamper was a no-op")
        return False
    try:
        path.write_text(tampered, encoding="utf-8")
        if touch_test:
            os.utime(TEST_RS, None)
        result = run(CARGO_TEST)
        went_red = result.returncode != 0
        snippet = "\n".join((result.stdout + result.stderr).strip().splitlines()[-4:])
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      cargo test -p aura-storage --test s7_backup -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        if touch_test:
            os.utime(TEST_RS, None)


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    print(f"S7 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 manifest checksum context",
            BACKUP_RS,
            'pub const S7_MANIFEST_CONTEXT: &[u8] = b"AuraLite-Crypto/s7-manifest-v1\\x00";',
            'pub const S7_MANIFEST_CONTEXT: &[u8] = b"WRONG-S7-MANIFEST\\x00";',
        ),
        expect_red(
            "NC-2 blob SHA-256 comparison",
            BACKUP_RS,
            "        if sha256_bytes(blob) != manifest.blob_sha256 {\n"
            "            return Err(BackupError::BlobHashMismatch);\n"
            "        }",
            "        if false {\n            return Err(BackupError::BlobHashMismatch);\n        }",
        ),
        expect_red(
            "NC-3 replayed root comparison",
            BACKUP_RS,
            "        if receipt.root != record.state_root {\n"
            "            return Err(BackupError::RootMismatch {\n"
            "                version: record.version,\n"
            "            });\n"
            "        }",
            "        if false {\n"
            "            return Err(BackupError::RootMismatch {\n"
            "                version: record.version,\n"
            "            });\n"
            "        }",
        ),
        expect_red(
            "NC-4 parent-root chain check",
            BACKUP_RS,
            "        if record.parent_root != expected_parent {\n"
            "            return Err(BackupError::ParentRootMismatch);\n"
            "        }",
            "        if false {\n            return Err(BackupError::ParentRootMismatch);\n        }",
        ),
        expect_red(
            "NC-5 restore into a live store",
            BACKUP_RS,
            '    if dir.join("wal.log").exists() || dir.join("head.bin").exists() {\n'
            "        return Err(BackupError::TargetNotEmpty);\n"
            "    }",
            "    if false {\n        return Err(BackupError::TargetNotEmpty);\n    }",
        ),
        expect_red(
            "NC-6 crash-window WAL replay on reopen",
            STORE_RS,
            "        for record in pending {\n",
            "        for record in pending.into_iter().take(0) {\n",
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
