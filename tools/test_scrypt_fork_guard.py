#!/usr/bin/env python3
"""Test the local scrypt provenance guard without mutating the checkout."""
import shutil
import tempfile
import unittest
from pathlib import Path
import check_scrypt_fork as guard


class ForkGuardTests(unittest.TestCase):
    def test_each_wipe_bypass_and_upstream_code_drift_is_detected(self):
        self.assertEqual(guard.check(), [])
        with tempfile.TemporaryDirectory() as tmp:
            fork = Path(tmp) / "fork"
            shutil.copytree(guard.FORK, fork)
            lib = fork / "src/lib.rs"
            original = lib.read_text(encoding="utf-8")
            for old, new in guard.PATCH[1:]:
                with self.subTest(wipe=old):
                    lib.write_text(original.replace(new, old), encoding="utf-8")
                    self.assertTrue(guard.check(fork))
            lib.write_text(original, encoding="utf-8")
            source = fork / "src/romix.rs"
            source.write_text(source.read_text(encoding="utf-8") + "\n// unexpected drift\n", encoding="utf-8")
            self.assertTrue(guard.check(fork))


    def test_runtime_must_bind_the_guarded_fork(self):
        self.assertEqual(guard.check_wallet_binding(), [])
        original = (guard.ROOT / "crates/aura-wallet/Cargo.toml").read_text(encoding="utf-8")
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "Cargo.toml"
            manifest.write_text(original.replace('scrypt-w1-wipe = { path = "../../third_party/scrypt-w1" }', 'scrypt = { workspace = true }'), encoding="utf-8")
            self.assertTrue(guard.check_wallet_binding(manifest))


if __name__ == "__main__":
    unittest.main()
