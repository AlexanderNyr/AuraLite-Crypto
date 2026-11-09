#!/usr/bin/env python3
"""Test the local Unicode 15.1 fork provenance guard without mutating the checkout."""
from pathlib import Path
import shutil
import tempfile
import unittest

import check_unicode_15_1_fork as guard


class Unicode151ForkGuardTests(unittest.TestCase):
    def test_table_drift_patch_tampering_extra_file_and_symlink_are_detected(self):
        self.assertEqual(guard.check(), [])
        with tempfile.TemporaryDirectory() as tmp:
            fork = Path(tmp) / "fork"
            shutil.copytree(guard.FORK, fork)

            tables = fork / "src/tables.rs"
            original_tables = tables.read_text(encoding="utf-8")
            tables.write_text(original_tables + "\n// drift\n", encoding="utf-8")
            self.assertTrue(guard.check(fork))
            tables.write_text(original_tables, encoding="utf-8")
            self.assertEqual(guard.check(fork), [])

            lib = fork / "src/lib.rs"
            original_lib = lib.read_text(encoding="utf-8")
            for old, new in guard.LIB_PATCH:
                with self.subTest(patch=old[:24]):
                    lib.write_text(original_lib.replace(new, old, 1), encoding="utf-8")
                    self.assertTrue(guard.check(fork))
            lib.write_text(original_lib, encoding="utf-8")

            extra = fork / "src/unexpected.rs"
            extra.write_text("// extra\n", encoding="utf-8")
            self.assertTrue(guard.check(fork))
            extra.unlink()

            link = fork / "src/link.rs"
            link.symlink_to(tables)
            self.assertTrue(guard.check(fork))
            link.unlink()
            self.assertEqual(guard.check(fork), [])

    def test_workspace_and_wallet_bindings_must_remain_pinned(self):
        self.assertEqual(guard.check_bindings(), [])
        ws_orig = (guard.ROOT / "Cargo.toml").read_text(encoding="utf-8")
        wallet_orig = (guard.ROOT / "crates/aura-wallet/Cargo.toml").read_text(encoding="utf-8")
        with tempfile.TemporaryDirectory() as tmp:
            ws = Path(tmp) / "ws.toml"
            wallet = Path(tmp) / "wallet.toml"
            ws.write_text(ws_orig.replace('"=0.1.26"', '"0.1.26"', 1), encoding="utf-8")
            wallet.write_text(wallet_orig, encoding="utf-8")
            self.assertTrue(guard.check_bindings(ws, wallet))

            ws.write_text(ws_orig, encoding="utf-8")
            wallet.write_text(wallet_orig.replace("unicode-normalization-15-1 = { workspace = true }\n", "", 1), encoding="utf-8")
            self.assertTrue(guard.check_bindings(ws, wallet))


if __name__ == "__main__":
    unittest.main()
