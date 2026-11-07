#!/usr/bin/env python3
"""Fail closed on unreviewed changes to the W1 RustCrypto scrypt fork.

Hashes of unmodified source/licenses are from the published 0.11.0 package;
lib.rs is reversed through the exact approved wipe patch and then checked
against the published upstream digest. This is not a cryptographic audit.
"""
from __future__ import annotations

import hashlib
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORK = ROOT / "third_party/scrypt-w1"
HASHES = {
    "Cargo.toml": "2bdc4753a0ff427f6a42feb9cb092d639490649db495e60a7cb5c719e937f065",
    "LICENSE-APACHE": "a9040321c3712d8fd0b09cf52b17445de04a23a10165049ae187cd39e5c86be5",
    "LICENSE-MIT": "c995204cc6bad2ed67dd41f7d89bb9f1a9d48e0edd745732b30640d7912089a4",
    "src/errors.rs": "7d1e04b5eddddd1cc075bb44e47dc0584c710f321f8b6423ad8c6c06cb4ef1e0",
    "src/params.rs": "ad6753bf5005ac890b8e09b4978ebe461de4aa8f4657fd0b80db111f5bdb5c3a",
    "src/romix.rs": "39453e17d356f586ae599b5d297c77834520ebaafb381f37450c00351d82d62d",
    "src/simple.rs": "d2c04d10e6f23a2d5f0e8811b5f3a828725b5e5eb5ea272e73da00a2f9da4114",
}
UPSTREAM_LIB = "b71b86a64f3c214d123ad1433e0c2cdcc3dff9a4085e4d78839ae366ae9bbd62"
PATCH = (
    ("use sha2::Sha256;\n", "use sha2::Sha256;\n// Local W1 patch: wipe password-derived scratch allocations on drop.\nuse zeroize::Zeroizing;\n"),
    ("let mut b = vec![0u8; pr128];", "let mut b = Zeroizing::new(vec![0u8; pr128]);"),
    ("let mut v = vec![0u8; nr128];", "let mut v = Zeroizing::new(vec![0u8; nr128]);"),
    ("let mut t = vec![0u8; r128];", "let mut t = Zeroizing::new(vec![0u8; r128]);"),
)


def check(fork: Path = FORK) -> list[str]:
    errors: list[str] = []
    for name, expected in HASHES.items():
        try:
            actual = hashlib.sha256((fork / name).read_bytes()).hexdigest()
            if actual != expected:
                errors.append(f"{name}: SHA-256 mismatch")
        except OSError as e:
            errors.append(f"{name}: {e}")
    try:
        lib = (fork / "src/lib.rs").read_text(encoding="utf-8")
        for old, new in PATCH:
            if lib.count(new) != 1:
                errors.append(f"src/lib.rs: expected wipe patch is missing or duplicated: {old}")
            else:
                lib = lib.replace(new, old, 1)
        if hashlib.sha256(lib.encode()).hexdigest() != UPSTREAM_LIB:
            errors.append("src/lib.rs: does not reproduce exact upstream after reversing approved patch")
    except (OSError, UnicodeError) as e:
        errors.append(f"src/lib.rs: {e}")
    return errors


def check_wallet_binding(manifest: Path = ROOT / "crates/aura-wallet/Cargo.toml") -> list[str]:
    try:
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
        deps = data.get("dependencies", {})
        if deps.get("scrypt-w1-wipe") != {"path": "../../third_party/scrypt-w1"}:
            return ["wallet runtime must use the guarded local scrypt fork"]
        if "scrypt" in deps:
            return ["unpatched upstream scrypt must not be a direct wallet runtime dependency"]
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as e:
        return [f"wallet manifest: {e}"]
    return []


def main() -> int:
    errors = check() + check_wallet_binding()
    if errors:
        print("SCRYPT FORK GUARD: FAIL")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("SCRYPT FORK GUARD: OK — exact upstream source plus approved B/V/T wipe patch")
    return 0


if __name__ == "__main__":
    sys.exit(main())
