#!/usr/bin/env python3
"""Negative controls for N2 (gossipsub topics, message signatures, dedup, peer scoring).

Six tampers, each turning a named library unit test in `crates/aura-p2p` red
via `cargo test -p aura-p2p --lib --quiet -- --exact <test>`:

  NC-1 gossip signature verification bypassed   -> forged message accepted.
  NC-2 SeenCache deduplication check bypassed   -> duplicate message accepted & re-forwarded.
  NC-3 flood rate-limit check bypassed          -> flooding peer never loses score or gets banned.
  NC-4 peer score ban escalation bypassed       -> negative-score attacker is never banned.
  NC-5 gossip payload size cap bypassed         -> oversized gossip payload accepted.
  NC-6 local topic subscription check bypassed  -> unsubscribed topic message accepted.

Usage:
    python3 tools/nc_n2.py
    make nc-n2
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
GOSSIP = REPO / "crates" / "aura-p2p" / "src" / "gossipsub.rs"

GATE = ["cargo", "test", "-p", "aura-p2p", "--lib", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str, test: str) -> bool:
    cmd = list(GATE) + ["--", "--exact", test]
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
        went_red = result.returncode != 0
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
    print("N2 NEGATIVE CONTROLS (gossipsub topics, signatures, dedup, peer scoring)")
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
            "NC-1 tampered gossip signature must be rejected",
            GOSSIP,
            "verify_raw(&self.sender_public_key, &digest, &self.signature)\n            .map_err(|_| P2pError::InvalidGossipSignature)?;",
            "verify_raw(&self.sender_public_key, &digest, &self.signature)\n            .map_err(|_| P2pError::InvalidGossipSignature)\n            .or::<P2pError>(Ok(()))?;",
            "gossipsub::tests::tampered_gossip_signature_is_rejected_and_penalized",
        ),
        expect_red(
            "NC-2 duplicate gossip message must be dropped by SeenCache",
            GOSSIP,
            "if self.seen.contains(&candidate_id) {",
            "if self.seen.contains(&candidate_id) && false {",
            "gossipsub::tests::duplicate_gossip_message_is_dropped_by_seen_cache",
        ),
        expect_red(
            "NC-3 flooding peer exceeding rate window must lose score and be banned",
            GOSSIP,
            "if count_in_window > self.params.max_messages_per_window {",
            "if count_in_window > self.params.max_messages_per_window && false {",
            "gossipsub::tests::flooding_peer_loses_score_and_is_banned",
        ),
        expect_red(
            "NC-4 peer score reaching ban_threshold must escalate to BanList",
            GOSSIP,
            "let banned = if new_score <= self.params.ban_threshold {",
            "let banned = if new_score <= self.params.ban_threshold && false {",
            "gossipsub::tests::invalid_messages_drive_peer_score_to_ban",
        ),
        expect_red(
            "NC-5 oversized gossip payload must be rejected",
            GOSSIP,
            "if self.payload.len() > max_payload_bytes {",
            "if self.payload.len() > max_payload_bytes && false {",
            "gossipsub::tests::oversized_and_empty_gossip_payloads_are_rejected",
        ),
        expect_red(
            "NC-6 unsubscribed gossip topic must be rejected",
            GOSSIP,
            "if !self.local_topics.contains(&msg.topic) {",
            "if !self.local_topics.contains(&msg.topic) && false {",
            "gossipsub::tests::unsubscribed_or_unknown_topic_is_rejected",
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
