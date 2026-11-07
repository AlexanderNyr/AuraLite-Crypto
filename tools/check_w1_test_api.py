#!/usr/bin/env python3
"""Compile-check W1's unsafe test API feature boundary, without executing it."""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
WALLET = ROOT / "crates/aura-wallet"


def run(manifest: Path) -> subprocess.CompletedProcess[str]:
    # The caller controls network/offline config. In a vendored environment,
    # the same CARGO_HOME/target settings used by the workspace are inherited.
    cargo = os.environ.get("CARGO", "cargo")
    return subprocess.run([cargo, "check", "--quiet", "--manifest-path", str(manifest)],
                          cwd=ROOT, capture_output=True, text=True, check=False)


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="w1-feature-boundary-") as directory:
        project = Path(directory)
        (project / "src").mkdir()
        manifest = project / "Cargo.toml"
        source = project / "src/main.rs"
        source.write_text(
            "use aura_wallet::keystore::{KdfParams, Keystore};\n"
            "use aura_wallet::secret::{Password, SecretSeed};\n"
            "fn main() {\n"
            "  let seed = SecretSeed::new([0; 64]);\n"
            "  let password = Password::new(\"PUBLIC-TEST-ONLY\");\n"
            "  let _ = Keystore::encrypt_seed_insecure_for_testing(&seed, &password, KdfParams::testing());\n"
            "  let _ = Keystore::decrypt_seed_insecure_for_testing;\n"
            "}\n", encoding="utf-8")
        header = '[package]\nname = "w1-feature-boundary-probe"\nversion = "0.0.0"\nedition = "2024"\n[workspace]\n[dependencies]\n'
        wallet_path = json.dumps(str(WALLET))
        manifest.write_text(header + f'aura-wallet = {{ path = {wallet_path} }}\n', encoding="utf-8")
        ordinary = run(manifest)
        if ordinary.returncode == 0 or "E0599" not in ordinary.stderr or any(
            name not in ordinary.stderr for name in (
                "encrypt_seed_insecure_for_testing", "decrypt_seed_insecure_for_testing", "testing"
            )
        ):
            raise SystemExit("FAIL: unsafe test API must be absent without its feature\n" + ordinary.stderr[-4000:])
        manifest.write_text(header + f'aura-wallet = {{ path = {wallet_path}, features = ["insecure-test-kdf"] }}\n', encoding="utf-8")
        enabled = run(manifest)
        if enabled.returncode != 0:
            raise SystemExit("FAIL: explicit test feature should compile the test API\n" + enabled.stderr[-4000:])
    print("PASS W1 unsafe test API absent by default and present only with explicit feature")


if __name__ == "__main__":
    main()
