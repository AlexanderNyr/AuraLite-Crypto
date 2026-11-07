#!/usr/bin/env python3
"""Mutation controls for P2P pre-authentication peer-ban regressions.

The transport mutants reintroduce bans before authenticated handshake checks;
the N6 suite adds supplemental mutations for untrusted ingress PeerIds, source
rate-bucket poisoning, and signature-before-chain ordering. Named tests must go
red; compilation errors do not count. Mutated source is restored byte-for-byte.

Run serially: these controls temporarily edit transport.rs and dos_defence.rs
while compiling tests.
"""
from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
TRANSPORT = REPO / "crates/aura-p2p/src/transport.rs"
N6_REVIEW = REPO / "tools/nc_n6.py"
GATE = ["cargo", "test", "-p", "aura-p2p", "--test", "n1_spoofed_peer_ban"]


def run(test: str | None = None) -> subprocess.CompletedProcess[str]:
    command = GATE.copy()
    if test:
        command.extend(["--", "--exact", test])
    return subprocess.run(command, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, old: str, new: str, test: str) -> bool:
    original = TRANSPORT.read_text(encoding="utf-8")
    count = original.count(old)
    if count != 1:
        print(f"{label}: FAIL — expected one mutation anchor, found {count}")
        return False
    tampered = original.replace(old, new, 1)
    if tampered == original:
        print(f"{label}: FAIL — mutation was a no-op")
        return False
    try:
        TRANSPORT.write_text(tampered, encoding="utf-8")
        os.utime(TRANSPORT, None)
        result = run(test)
        output = result.stdout + result.stderr
        went_red = (
            result.returncode != 0
            and re.search(rf"test\s+{re.escape(test)}\s+\.\.\.\s+FAILED", output)
            is not None
        )
        tail = "\n".join(line for line in output.strip().splitlines()[-4:] if line.strip())
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      {test} -> rc={result.returncode} "
            f"({'named test RED as expected' if went_red else 'not a named-test failure'})"
        )
        if tail:
            print("      " + tail.replace("\n", "\n      "))
        return went_red
    finally:
        TRANSPORT.write_text(original, encoding="utf-8")
        os.utime(TRANSPORT, None)


def main() -> int:
    print("P2P SECURITY-REVIEW NEGATIVE CONTROLS (pre-auth peer-ban DoS)")
    baseline = run()
    if baseline.returncode != 0:
        print("baseline: FAIL — n1_spoofed_peer_ban must pass before mutation")
        print("\n".join((baseline.stdout + baseline.stderr).strip().splitlines()[-12:]))
        print("RESULT: 0/4 negative controls PASS")
        return 1
    print("baseline: PASS — five live regression tests green")

    inbound_identity_old = """    validate_remote_hello(
        identity.peer_id(),
        &config.chain_id,
        PROTOCOL_VERSION,
        &init_hello,
    )?;"""
    inbound_identity_new = """    if let Err(err) = validate_remote_hello(
        identity.peer_id(),
        &config.chain_id,
        PROTOCOL_VERSION,
        &init_hello,
    ) {
        bans.lock().unwrap().ban_peer(
            init_hello.claimed_peer_id,
            BanReason::IdentityMismatch,
            None,
        );
        return Err(err);
    }"""

    inbound_signature_old = """    verify_transcript_signature(
        &init_hello.public_key,
        &config.chain_id,
        HandshakeRole::Initiator,
        &transcript_hash,
        &init_sig,
    )?;"""
    inbound_signature_new = """    if let Err(err) = verify_transcript_signature(
        &init_hello.public_key,
        &config.chain_id,
        HandshakeRole::Initiator,
        &transcript_hash,
        &init_sig,
    ) {
        bans.lock().unwrap().ban_peer(
            init_hello.claimed_peer_id,
            BanReason::InvalidHandshakeSignature,
            None,
        );
        return Err(err);
    }"""

    outbound_signature_old = """        verify_transcript_signature(
            &resp_hello.public_key,
            &self.config.chain_id,
            HandshakeRole::Responder,
            &transcript_hash,
            &resp_sig,
        )?;"""
    outbound_signature_new = """        if let Err(err) = verify_transcript_signature(
            &resp_hello.public_key,
            &self.config.chain_id,
            HandshakeRole::Responder,
            &transcript_hash,
            &resp_sig,
        ) {
            self.bans.lock().unwrap().ban_peer(
                resp_hello.claimed_peer_id,
                BanReason::InvalidHandshakeSignature,
                None,
            );
            return Err(err);
        }"""

    substitution_old = "        verify_expected_peer(expected_peer, resp_hello.claimed_peer_id)?;"
    substitution_new = """        if let Err(err) = verify_expected_peer(expected_peer, resp_hello.claimed_peer_id) {
            self.bans.lock().unwrap().ban_peer(
                resp_hello.claimed_peer_id,
                BanReason::IdentitySubstitution,
                None,
            );
            return Err(err);
        }"""

    results = [
        expect_red(
            "NC-1 inbound claimed-ID mismatch must not ban an unauthenticated PeerId",
            inbound_identity_old,
            inbound_identity_new,
            "inbound_claimed_id_mismatch_cannot_ban_victim",
        ),
        expect_red(
            "NC-2 inbound invalid signature must not ban the claimed PeerId",
            inbound_signature_old,
            inbound_signature_new,
            "inbound_invalid_signature_cannot_ban_victim",
        ),
        expect_red(
            "NC-3 outbound invalid signature must not ban the claimed PeerId",
            outbound_signature_old,
            outbound_signature_new,
            "outbound_invalid_signature_cannot_ban_claimed_victim",
        ),
        expect_red(
            "NC-4 expected-identity mismatch must not auto-ban another peer",
            substitution_old,
            substitution_new,
            "outbound_authenticated_substitution_does_not_auto_ban_other_peer",
        ),
    ]
    passed = sum(results)
    print(f"TRANSPORT RESULT: {passed}/{len(results)} negative controls PASS")

    print("\nN6 pre-authentication DoS review controls:")
    n6_review = subprocess.run(
        [sys.executable, str(N6_REVIEW)], cwd=REPO, capture_output=True, text=True, check=False
    )
    if n6_review.stdout:
        print(n6_review.stdout.rstrip())
    if n6_review.stderr:
        print(n6_review.stderr.rstrip())

    restored = run()
    if restored.returncode != 0:
        print("restore check: n1_spoofed_peer_ban remains red")
        return 1
    print("restore check: transport source and live regressions are GREEN")
    if n6_review.returncode != 0:
        print("N6 review controls: FAIL")
        return 1
    print("N6 review controls: PASS (six historical + five supplemental)")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
