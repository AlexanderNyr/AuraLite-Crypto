#!/usr/bin/env python3
"""N6 DoS-defence and supplemental pre-auth identity negative controls.

The six historical N6 controls remain the phase receipt. Five supplemental
mutations cover the current N6 pre-auth peer-identity security review. Every
mutation must make its named library unit test fail (not merely fail to compile)
via `cargo test -p aura-p2p --lib --quiet -- --exact <test>`.

Historical N6 controls:
  NC-1 frame-size cap bypassed                 -> oversized frame reaches crypto.
  NC-2 message-rate limit bypassed             -> flood exceeds window cap.
  NC-3 signature budget bypassed              -> signature storm exceeds cap.
  NC-4 invalid signature accepted              -> forged ingress is admitted.
  NC-5 equivocation detection bypassed         -> conflicting vote is accepted.
  NC-6 pre-crypto ban check bypassed           -> banned peer consumes crypto CPU.

Supplemental review controls:
  R-1 identity mismatch bans claimed signer    -> spoofed PeerId is banned.
  R-2 invalid signature bans claimed signer    -> victim is banned, not source.
  R-3 rate bucket keyed by claimed signer      -> spoofed frame poisons victim bucket.
  R-4 signature budget keyed by claimed signer  -> spoofed frame exhausts victim budget.
  R-5 chain mismatch checked before signature  -> invalid frame bans victim.

The supplemental controls do not alter or rewrite historical N6 receipts.

Usage:
    python3 tools/nc_n6.py
    make nc-n6
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DOS_DEFENCE = REPO / "crates" / "aura-p2p" / "src" / "dos_defence.rs"
GATE = ["cargo", "test", "-p", "aura-p2p", "--lib", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, old: str, new: str, test: str) -> bool:
    command = [*GATE, "--", "--exact", test]
    original = DOS_DEFENCE.read_text(encoding="utf-8")
    if original.count(old) != 1:
        print(f"{label}: FAIL — expected one mutation anchor, found {original.count(old)}")
        return False
    tampered = original.replace(old, new, 1)
    if tampered == original:
        print(f"{label}: FAIL — mutation was a no-op")
        return False
    try:
        DOS_DEFENCE.write_text(tampered, encoding="utf-8")
        os.utime(DOS_DEFENCE, None)
        result = run(command)
        output = result.stdout + result.stderr
        named_test_failed = re.search(
            rf"(?:^|\n)(?:test\s+)?{re.escape(test)}\s+(?:\.\.\.|---)\s+FAILED(?:\s|$)",
            output,
        ) is not None
        went_red = result.returncode != 0 and named_test_failed
        tail = "\n".join(line for line in output.strip().splitlines()[-4:] if line.strip())
        tail = re.sub(r"finished in [\d.]+s", "finished", tail)
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      {test} -> rc={result.returncode} "
            f"({'named test RED as expected' if went_red else 'not a named-test failure'})"
        )
        if tail:
            print("      " + tail.replace("\n", "\n      "))
        return went_red
    finally:
        DOS_DEFENCE.write_text(original, encoding="utf-8")
        os.utime(DOS_DEFENCE, None)


def main() -> int:
    print("N6 NEGATIVE CONTROLS (six historical + five supplemental security-review mutations)")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    print("baseline: the untampered aura-p2p library gate")
    baseline = run(GATE)
    print(
        f"      rc={baseline.returncode} "
        f"({'GREEN as expected' if baseline.returncode == 0 else 'RED — fix the tree first'})"
    )
    if baseline.returncode != 0:
        tail = "\n".join((baseline.stdout + baseline.stderr).strip().splitlines()[-10:])
        print(tail)
        print("RESULT: 0/11 controls PASS (baseline is not green)")
        return 1

    print("\nhistorical N6 controls:")
    historical = [
        expect_red(
            "NC-1 frames exceeding max_frame_bytes must be rejected before signature verification",
            "if envelope.raw_size_bytes == 0 || envelope.raw_size_bytes > self.limits.max_frame_bytes {",
            "if (envelope.raw_size_bytes == 0 || envelope.raw_size_bytes > self.limits.max_frame_bytes) && false {",
            "dos_defence::tests::oversized_frame_is_rejected_before_signature_check",
        ),
        expect_red(
            "NC-2 per-peer message rate limit must throttle and ban flooding peers",
            "if current_msgs > max_msgs {",
            "if current_msgs > max_msgs && false {",
            "dos_defence::tests::message_rate_limit_throttles_and_bans_flooding_peer",
        ),
        expect_red(
            "NC-3 per-peer signature verification budget must cap Ed25519 CPU usage",
            "if next_sigs > max_sigs {",
            "if next_sigs > max_sigs && false {",
            "dos_defence::tests::signature_rate_limit_prevents_signature_storm",
        ),
        expect_red(
            "NC-4 forged ingress signatures must be rejected and banned",
            "if verify_raw(&envelope.sender_public_key, &digest, &envelope.signature).is_err() {",
            "if verify_raw(&envelope.sender_public_key, &digest, &envelope.signature).is_err() && false {",
            "dos_defence::tests::forged_ingress_signature_is_rejected_and_banned",
        ),
        expect_red(
            "NC-5 conflicting consensus votes/proposals must record equivocation evidence and ban",
            "if first_hash != block_hash {",
            "if first_hash != block_hash && false {",
            "dos_defence::tests::equivocation_detector_catches_conflicting_vote_and_bans_peer",
        ),
        expect_red(
            "NC-6 banned peers must be dropped before signature verification",
            "if let Err(err) = self.ban_list.check_peer(&authenticated_peer, now_ms) {",
            "if let Err(err) = self.ban_list.check_peer(&authenticated_peer, now_ms) && false {",
            "dos_defence::tests::ten_thousand_msg_per_sec_attacker_is_isolated_while_validators_finalize",
        ),
    ]

    print("\nsupplemental pre-auth identity-review controls:")
    bad_identity_old = '            self.record_strike(authenticated_peer, "dos_identity_mismatch");'
    bad_identity_new = """            self.ban_list.ban_peer(
                signer_peer_id,
                BanReason::IdentityMismatch,
                None,
            );
            self.record_strike(authenticated_peer, "dos_identity_mismatch");"""

    invalid_signature_ban_old = """            self.ban_list.ban_peer(
                authenticated_peer,
                BanReason::ProtocolViolation("invalid_dos_ingress_signature".to_owned()),
                None,
            );"""
    invalid_signature_ban_new = """            self.ban_list.ban_peer(
                signer_peer_id,
                BanReason::ProtocolViolation("invalid_dos_ingress_signature".to_owned()),
                None,
            );"""

    resource_old = """        let state = self.peer_windows.entry(authenticated_peer).or_default();
        if now_ms.saturating_sub(state.window_start_ms) >= window_ms {
            state.window_start_ms = now_ms;
            state.msg_count = 0;
            state.sig_count = 0;
        }
        state.msg_count += 1;
        let current_msgs = state.msg_count;

        // 3. O(1) per-source message rate limit before crypto.
        if current_msgs > max_msgs {
            self.dropped_pre_crypto += 1;
            self.record_strike(authenticated_peer, "dos_message_rate_exceeded");
            return Err(P2pError::DosMessageRateExceeded {
                peer_id: authenticated_peer,
                count: current_msgs,
                max: max_msgs,
            });
        }

        // 4. O(1) per-source signature verification CPU budget before Ed25519.
        let next_sigs = self
            .peer_windows
            .get(&authenticated_peer)
            .unwrap()
            .sig_count
            + 1;
        if next_sigs > max_sigs {
            self.dropped_pre_crypto += 1;
            self.record_strike(authenticated_peer, "dos_signature_budget_exceeded");
            return Err(P2pError::DosSignatureBudgetExceeded {
                peer_id: authenticated_peer,
                count: next_sigs,
                max: max_sigs,
            });
        }
        self.peer_windows
            .get_mut(&authenticated_peer)
            .unwrap()
            .sig_count = next_sigs;"""
    resource_new = (
        resource_old.replace("entry(authenticated_peer)", "entry(signer_peer_id)")
        .replace("get(&authenticated_peer)", "get(&signer_peer_id)")
        .replace("get_mut(&authenticated_peer)", "get_mut(&signer_peer_id)")
    )

    chain_anchor_old = """        // 5. Verify the domain-separated signature before applying identity-based
        // bans for signed protocol violations.
        self.sig_verifications_performed += 1;"""
    chain_anchor_new = """        if envelope.chain_id != self.chain_id {
            self.ban_list.ban_peer(
                signer_peer_id,
                BanReason::ProtocolViolation("dos_chain_id_mismatch".to_owned()),
                None,
            );
            return Err(P2pError::ChainIdMismatch {
                expected: self.chain_id.clone(),
                actual: envelope.chain_id.clone(),
            });
        }

        // 5. Verify the domain-separated signature before applying identity-based
        // bans for signed protocol violations.
        self.sig_verifications_performed += 1;"""

    supplemental = [
        expect_red(
            "R-1 claimed sender ID must not be banned on public-key mismatch",
            bad_identity_old,
            bad_identity_new,
            "dos_defence::tests::unauthenticated_sender_id_mismatch_cannot_ban_victim",
        ),
        expect_red(
            "R-2 invalid signature must ban only the authenticated source",
            invalid_signature_ban_old,
            invalid_signature_ban_new,
            "dos_defence::tests::invalid_signature_bans_authenticated_source_not_spoofed_signer",
        ),
        expect_red(
            "R-3 rate and signature budgets must be keyed by authenticated source",
            resource_old,
            resource_new,
            "dos_defence::tests::unauthenticated_sender_id_mismatch_cannot_ban_victim",
        ),
        expect_red(
            "R-4 signature budgets must be keyed by authenticated source",
            resource_old,
            resource_new,
            "dos_defence::tests::unauthenticated_sender_id_mismatch_cannot_poison_signature_budget",
        ),
        expect_red(
            "R-5 a claimed signer must not be banned before its signature verifies",
            chain_anchor_old,
            chain_anchor_new,
            "dos_defence::tests::wrong_chain_is_checked_only_after_the_claimed_signer_signature",
        ),
    ]

    historical_passed = sum(historical)
    supplemental_passed = sum(supplemental)
    print(
        f"\nRESULT: historical {historical_passed}/{len(historical)}; "
        f"supplemental {supplemental_passed}/{len(supplemental)}"
    )
    restored = run(GATE)
    if restored.returncode != 0:
        print("restore check: aura-p2p library tests remain red")
        return 1
    print("restore check: cargo test -p aura-p2p --lib -> GREEN")
    return 0 if historical_passed == len(historical) and supplemental_passed == len(supplemental) else 1


if __name__ == "__main__":
    raise SystemExit(main())
