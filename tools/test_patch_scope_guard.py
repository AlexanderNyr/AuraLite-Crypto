#!/usr/bin/env python3
"""Red/green guard for the secret-scanner patch-archive scope rule.

The immutable `patches/` review archive embeds the demo-fixture lines of the
phases it records (see the post-closeout deviation in
docs/h1-supply-chain-plan.md). The scanner must skip that archive while still
scanning live paths, and this guard must turn red if the scope rule is ever
dropped. The demo token is assembled at runtime so the literal never appears
in the scanned tree.
"""
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import scan_secrets

DEMO_TOKEN = "ghp_" + "demo0" * 7


class PatchScopeGuardTests(unittest.TestCase):
    def test_patch_archive_contents_are_out_of_scope(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "patches").mkdir()
            (root / "patches" / "0001-demo.patch").write_text(
                f'+token = "{DEMO_TOKEN}"\n', encoding="utf-8"
            )
            (root / "README.md").write_text("clean\n", encoding="utf-8")
            self.assertEqual(scan_secrets.scan_tree(root), [])

    def test_live_paths_are_still_scanned(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "docs").mkdir()
            (root / "docs" / "leak.txt").write_text(
                f'token = "{DEMO_TOKEN}"\n', encoding="utf-8"
            )
            findings = scan_secrets.scan_tree(root)
            self.assertEqual(len(findings), 1)
            self.assertEqual(findings[0][0], "github-token")

    def test_dropping_the_scope_rule_turns_the_guard_red(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "patches").mkdir()
            (root / "patches" / "0001-demo.patch").write_text(
                f'+token = "{DEMO_TOKEN}"\n', encoding="utf-8"
            )
            scan_secrets.SKIP_DIRS.discard("patches")
            try:
                self.assertTrue(
                    scan_secrets.scan_tree(root),
                    "mutation (drop 'patches' from SKIP_DIRS) must be visible",
                )
            finally:
                scan_secrets.SKIP_DIRS.add("patches")


if __name__ == "__main__":
    unittest.main()
