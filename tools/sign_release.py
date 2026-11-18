#!/usr/bin/env python3
"""Ed25519 (RFC 8032) release signing/verification for the H1 phase (demo key).

The key is derived deterministically from the pinned phrase
`AuraLite-Crypto/h1-release-demo-key-v1` so the phase bundle is reproducible
across workspace restores (docs/h1-supply-chain-plan.md D-H1-5). This is a
DEMO/testnet release key: production release keys MUST be generated offline
on air-gapped media.

Message = b"AuraLite-Crypto/release-digest-v1\\0" || sha256(artifact).
Verification for the demo key is the deterministic re-signature comparison
(equivalent to full RFC 8032 verification for this pinned key); a production
gate MUST use full RFC 8032 verify.

Usage:
  sign_release.py pubkey                       -> hex public key
  sign_release.py sign   <artifact>            -> writes <artifact>.sig (b64)
  sign_release.py verify <artifact> <sigfile>  -> exit 0/1
"""
from __future__ import annotations

import base64
import hashlib
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from tools import gen_c4_vectors as c4  # noqa: E402

PHRASE = b"AuraLite-Crypto/h1-release-demo-key-v1"
PREFIX = b"AuraLite-Crypto/release-digest-v1\0"


def demo_seed() -> bytes:
    return hashlib.sha256(PHRASE).digest()


def digest_message(artifact: Path) -> bytes:
    return PREFIX + hashlib.sha256(artifact.read_bytes()).digest()


def sign_message(message: bytes) -> bytes:
    return c4.ed25519_sign(message, demo_seed())


def public_key() -> bytes:
    return c4.ed25519_public_key(demo_seed())


def verify_digest(digest: bytes, sig: bytes) -> bool:
    return sign_message(PREFIX + digest) == sig


def verify(artifact: Path, sig: bytes) -> bool:
    return verify_digest(hashlib.sha256(artifact.read_bytes()).digest(), sig)


def main(argv: list[str]) -> int:
    if len(argv) == 2 and argv[1] == "pubkey":
        print(public_key().hex())
        return 0
    if len(argv) == 3 and argv[1] == "sign":
        artifact = Path(argv[2])
        sig = sign_message(digest_message(artifact))
        out = Path(str(artifact) + ".sig")
        out.write_text(base64.b64encode(sig).decode() + "\n", encoding="utf-8")
        print(f"signed {artifact.name} -> {out.name} "
              f"(pubkey {public_key().hex()[:16]}...)")
        return 0
    if len(argv) == 4 and argv[1] == "verify":
        artifact = Path(argv[2])
        sig = base64.b64decode(Path(argv[3]).read_text(encoding="utf-8").strip())
        ok = verify(artifact, sig)
        print(f"{'SIGNATURE OK' if ok else 'error: supply-chain: bad signature'}: "
              f"{artifact.name}")
        return 0 if ok else 1
    print(__doc__)
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv))
