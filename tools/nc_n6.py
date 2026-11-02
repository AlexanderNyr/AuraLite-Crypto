#!/usr/bin/env python3
"""Negative controls for N6 (DoS defence: size/rate limits, signature budget, equivocation).

Six tampers, each turning a named library unit test in `crates/aura-p2p` red
via `cargo test -p aura-p2p --lib --quiet -- --exact <test>`:

  NC-1 frame size cap check bypassed           -> oversized frame admitted past pre-crypto guard.
  NC-2 per-peer message rate limit bypassed    -> message flood exceeds window cap without ban.
  NC-3 per-peer signature CPU budget bypassed  -> signature flood exceeds per-peer crypto budget.
  NC-4 invalid signature verification bypassed -> forged ingress signature accepted without ban.
  NC-5 equivocation detector bypassed          -> conflicting vote at same (height, round) accepted.
  NC-6 pre-crypto ban check bypassed           -> banned 10k msg/s peer consumes Ed25519 CPU.

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
    print("N6 NEGATIVE CONTROLS (DoS defence: size/rate limits, signature budget, equivocation)")
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
            "NC-1 frames exceeding max_frame_bytes must be rejected before signature verification",
            DOS_DEFENCE,
            "if envelope.raw_size_bytes == 0 || envelope.raw_size_bytes > self.limits.max_frame_bytes {",
            "if (envelope.raw_size_bytes == 0 || envelope.raw_size_bytes > self.limits.max_frame_bytes) && false {",
            "dos_defence::tests::oversized_frame_is_rejected_before_signature_check",
        ),
        expect_red(
            "NC-2 per-peer message rate limit must throttle and ban flooding peers",
            DOS_DEFENCE,
            "if current_msgs > max_msgs {",
            "if current_msgs > max_msgs && false {",
            "dos_defence::tests::message_rate_limit_throttles_and_bans_flooding_peer",
        ),
        expect_red(
            "NC-3 per-peer signature verification budget must cap Ed25519 CPU usage",
            DOS_DEFENCE,
            "if next_sigs > max_sigs {",
            "if next_sigs > max_sigs && false {",
            "dos_defence::tests::signature_rate_limit_prevents_signature_storm",
        ),
        expect_red(
            "NC-4 forged ingress signatures must be rejected and ban the peer",
            DOS_DEFENCE,
            "if verify_raw(&envelope.sender_public_key, &digest, &envelope.signature).is_err() {",
            "if verify_raw(&envelope.sender_public_key, &digest, &envelope.signature).is_err() && false {",
            "dos_defence::tests::forged_ingress_signature_is_rejected_and_banned",
        ),
        expect_red(
            "NC-5 conflicting consensus votes/proposals must record equivocation evidence and ban",
            DOS_DEFENCE,
            "if first_hash != block_hash {",
            "if first_hash != block_hash && false {",
            "dos_defence::tests::equivocation_detector_catches_conflicting_vote_and_bans_peer",
        ),
        expect_red(
            "NC-6 banned 10k msg/s peers must be dropped before signature verification",
            DOS_DEFENCE,
            "if let Err(err) = self.ban_list.check_peer(&peer_id, now_ms) {",
            "if let Err(err) = self.ban_list.check_peer(&peer_id, now_ms) && false {",
            "dos_defence::tests::ten_thousand_msg_per_sec_attacker_is_isolated_while_validators_finalize",
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
