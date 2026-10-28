#!/usr/bin/env python3
"""Negative controls for S6 (pruning, archive and compaction).

Six tamper tests must each turn the S6 gate red:

  NC-1: change S6_RECORD_CONTEXT (record checksum domain) -> the canonical
        record bytes no longer match the independent model.
  NC-2: change S6_BASE_CONTEXT (base checksum domain) -> the canonical base
        snapshot no longer matches the independent model.
  NC-3: shift the retention boundary by one version (keep + 1) -> the disk
        accounting no longer matches the independent model.
  NC-4: disable pruning in prune mode -> disk stops being bounded.
  NC-5: let an archive node compact -> the history the archive serves is lost.
  NC-6: return the base state for a pruned version instead of erroring -> a
        pruned version silently answers with the wrong state.

Rule (CONTRIBUTING.md §1): a gate that cannot go red is not a gate.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
PRUNE_RS = REPO / "crates" / "aura-storage" / "src" / "prune.rs"
TEST_RS = REPO / "crates" / "aura-storage" / "tests" / "s6_prune.rs"

CARGO_TEST = ["cargo", "test", "-p", "aura-storage", "--test", "s6_prune", "--quiet"]


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
            f"      cargo test -p aura-storage --test s6_prune -> rc={result.returncode} "
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
    print(f"S6 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red(
            "NC-1 record checksum context",
            PRUNE_RS,
            'pub const S6_RECORD_CONTEXT: &[u8] = b"AuraLite-Crypto/s6-record-v1\\x00";',
            'pub const S6_RECORD_CONTEXT: &[u8] = b"WRONG-S6-RECORD\\x00";',
        ),
        expect_red(
            "NC-2 base checksum context",
            PRUNE_RS,
            'pub const S6_BASE_CONTEXT: &[u8] = b"AuraLite-Crypto/s6-base-v1\\x00";',
            'pub const S6_BASE_CONTEXT: &[u8] = b"WRONG-S6-BASE\\x00";',
        ),
        expect_red(
            "NC-3 retention boundary (keep + 1)",
            PRUNE_RS,
            "let target = self.head.saturating_sub(self.policy.keep_versions);",
            "let target = self\n            .head\n            .saturating_sub(self.policy.keep_versions + 1);",
        ),
        expect_red(
            "NC-4 pruning disabled in prune mode",
            PRUNE_RS,
            "        if self.policy.mode != RetentionMode::Prune {\n            return;\n        }",
            "        if true {\n            return;\n        }",
        ),
        expect_red(
            "NC-5 archive compaction allowed",
            PRUNE_RS,
            "        if self.policy.mode == RetentionMode::Archive {\n"
            "            return Err(PruneError::ArchiveCompactionForbidden);\n"
            "        }",
            "        if false {\n            return Err(PruneError::ArchiveCompactionForbidden);\n        }",
        ),
        expect_red(
            "NC-6 pruned version served instead of rejected",
            PRUNE_RS,
            "        if version < self.base_version {\n"
            "            return Err(PruneError::Pruned {\n"
            "                version,\n"
            "                oldest: self.base_version,\n"
            "            });\n"
            "        }\n"
            "        if version == self.head {\n"
            "            return Ok(self.tree.entries());",
            "        if false {\n"
            "            return Err(PruneError::Pruned {\n"
            "                version,\n"
            "                oldest: self.base_version,\n"
            "            });\n"
            "        }\n"
            "        if version == self.head {\n"
            "            return Ok(self.tree.entries());",
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
