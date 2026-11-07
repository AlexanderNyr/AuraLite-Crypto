#!/usr/bin/env python3
"""Negative controls for N1 (transport, identity, handshake, ban list).

Six tampers, each turning a named library unit test in `crates/aura-p2p` red
via `cargo test -p aura-p2p --lib --quiet -- --exact <test>`:

  NC-1 PeerId accepts mismatched pubkey hash    -> `peer_id != BLAKE3(pubkey)` accepted.
  NC-2 expected PeerId check bypassed           -> identity substitution accepted.
  NC-3 handshake signature check bypassed       -> tampered Ed25519 transcript sig accepted.
  NC-4 protocol_version check bypassed          -> incompatible protocol version accepted.
  NC-5 BanList peer check bypassed              -> banned PeerId allowed to connect.
  NC-6 SessionCipher frame MAC check bypassed   -> tampered encrypted frame accepted.

Usage:
    python3 tools/nc_n1.py
    make nc-n1
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
IDENTITY = REPO / "crates" / "aura-p2p" / "src" / "identity.rs"
BAN = REPO / "crates" / "aura-p2p" / "src" / "ban.rs"
TRANSPORT = REPO / "crates" / "aura-p2p" / "src" / "transport.rs"

GATE = ["cargo", "test", "-p", "aura-p2p", "--lib", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str, test: str) -> bool:
    cmd = [arg for arg in GATE if arg != "--quiet"] + ["--", "--exact", test]
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
        os.utime(path, None)
        result = run(cmd)
        went_red = result.returncode != 0 and re.search(rf"test\s+{re.escape(test)}\s+\.\.\.\s+FAILED", result.stdout + result.stderr) is not None
        raw = "\n".join(
            line.rstrip()
            for line in (result.stdout + result.stderr).strip().splitlines()
            if line.strip()
        )
        tail = "\n".join(raw.splitlines()[-2:])
        snippet = re.sub(r"finished in [\d.]+s", "finished", tail)
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      {test} -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        if snippet:
            print("      " + snippet.replace("\n", "\n      "))
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        os.utime(path, None)


def main() -> int:
    print("N1 NEGATIVE CONTROLS (P2P transport, identity, handshake, ban list)")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    print("baseline: the untampered gate")
    baseline = run(GATE)
    print(
        f"      rc={baseline.returncode} "
        f"({'GREEN as expected' if baseline.returncode == 0 else 'RED — fix the tree first'})"
    )
    if baseline.returncode != 0:
        tail = "\n".join((baseline.stdout + baseline.stderr).strip().splitlines()[-10:])
        print(tail)
        print("RESULT: 0/6 negative controls PASS (baseline is not green)")
        return 1
    print()

    results = [
        expect_red(
            "NC-1 PeerId must equal BLAKE3(public_key)",
            IDENTITY,
            "if !constant_time_eq(&self.0, &derived.0) {",
            "if constant_time_eq(&self.0, &derived.0) && false {",
            "identity::tests::peer_id_must_match_public_key_hash",
        ),
        expect_red(
            "NC-2 identity substitution against expected PeerId must be rejected",
            TRANSPORT,
            "if let Some(expected_peer) = expected\n        && !constant_time_eq(expected_peer.as_bytes(), actual.as_bytes())",
            "if let Some(expected_peer) = expected\n        && constant_time_eq(expected_peer.as_bytes(), actual.as_bytes()) && false",
            "transport::tests::expected_peer_mismatch_is_rejected_without_an_automatic_ban",
        ),
        expect_red(
            "NC-3 tampered handshake Ed25519 signature must be rejected",
            IDENTITY,
            "verify_raw(public_key, &digest, signature).map_err(|_| P2pError::InvalidHandshakeSignature)",
            "verify_raw(public_key, &digest, signature).map_err(|_| P2pError::InvalidHandshakeSignature).or(Ok(()))",
            "transport::tests::tampered_handshake_signature_is_rejected",
        ),
        expect_red(
            "NC-4 protocol_version mismatch must be rejected",
            TRANSPORT,
            "if remote.protocol_version != local_protocol_version {",
            "if remote.protocol_version != local_protocol_version && false {",
            "transport::tests::protocol_version_and_chain_id_mismatches_are_rejected",
        ),
        expect_red(
            "NC-5 banned PeerId must be rejected by BanList",
            BAN,
            "if let Some(entry) = self.peers.get(peer_id)\n            && entry.is_active(now_ms)",
            "if let Some(entry) = self.peers.get(peer_id)\n            && entry.is_active(now_ms) && false",
            "ban::tests::banned_peer_and_ip_are_rejected_until_expiry",
        ),
        expect_red(
            "NC-6 tampered encrypted session frame must fail MAC verification",
            TRANSPORT,
            """                    msg: &frame[8..],
                    aad: &aad,
                },
            )
            .map_err(|_| P2pError::FrameAuthenticationFailed)?;""",
            """                    msg: &frame[8..],
                    aad: &aad,
                },
            )
            .unwrap_or_default();""",
            "transport::tests::tampered_encrypted_frame_is_rejected",
        ),
    ]

    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    restored = run(GATE)
    if restored.returncode != 0:
        print("restore check: aura-p2p library tests remain red")
        return 1
    print("restore check: cargo test -p aura-p2p --lib -> GREEN")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
