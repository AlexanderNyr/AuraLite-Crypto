#!/usr/bin/env python3
"""Negative controls for N3 (Kademlia DHT, bootstrap list, NAT/UPnP, Circuit Relay v2).

Six tampers, each turning a named library unit test in `crates/aura-p2p` red
via `cargo test -p aura-p2p --lib --quiet -- --exact <test>`:

  NC-1 Kademlia XOR distance metric zeroed       -> bucket index / ordering broken.
  NC-2 PeerId == BLAKE3(public_key) check skipped -> substituted peer key accepted.
  NC-3 Ed25519 peer record signature check skipped -> forged/tampered record accepted.
  NC-4 Kademlia stale sequence check bypassed    -> replayed older record overwrites fresh one.
  NC-5 Kademlia BanList check bypassed           -> banned peer enters k-bucket.
  NC-6 Circuit Relay v2 expiry check bypassed    -> expired relay reservation still routes traffic.

Usage:
    python3 tools/nc_n3.py
    make nc-n3
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DISCOVERY = REPO / "crates" / "aura-p2p" / "src" / "discovery.rs"

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
    print("N3 NEGATIVE CONTROLS (Kademlia DHT, bootstrap list, NAT/UPnP, Circuit Relay v2)")
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
            "NC-1 Kademlia XOR distance metric and bucket index must be exact",
            DISCOVERY,
            "*slot = a.as_bytes()[i] ^ b.as_bytes()[i];",
            "*slot = a.as_bytes()[i] & b.as_bytes()[i];",
            "discovery::tests::xor_distance_metric_and_bucket_index_are_exact",
        ),
        expect_red(
            "NC-2 peer record with substituted public key must be rejected",
            DISCOVERY,
            "self.peer_id.verify_public_key(&self.public_key)?;",
            "let _ = &self.public_key;",
            "discovery::tests::rejects_peer_record_with_substituted_peer_id",
        ),
        expect_red(
            "NC-3 forged or tampered Ed25519 peer record signature must be rejected",
            DISCOVERY,
            "verify_raw(&self.public_key, &digest, &self.signature)\n            .map_err(|_| P2pError::InvalidPeerRecordSignature)",
            "verify_raw(&self.public_key, &digest, &self.signature)\n            .map_err(|_| P2pError::InvalidPeerRecordSignature)\n            .or::<P2pError>(Ok(()))",
            "discovery::tests::rejects_forged_or_tampered_peer_record_signature",
        ),
        expect_red(
            "NC-4 Kademlia routing table must reject stale sequence record replay",
            DISCOVERY,
            "if let Some(existing) = self.records.get(&record.peer_id)\n            && record.seq <= existing.seq",
            "if let Some(existing) = self.records.get(&record.peer_id)\n            && record.seq <= existing.seq\n            && false",
            "discovery::tests::kademlia_rejects_stale_sequence_record_replay",
        ),
        expect_red(
            "NC-5 Kademlia routing table must reject banned peers",
            DISCOVERY,
            "ban_list.check_peer(&record.peer_id, now_ms)?;",
            "let _ = (ban_list, now_ms);",
            "discovery::tests::kademlia_rejects_banned_peer_records",
        ),
        expect_red(
            "NC-6 Circuit Relay v2 must reject expired or unreserved target slots",
            DISCOVERY,
            "if res.expires_at_ms <= now_ms {",
            "if res.expires_at_ms <= now_ms && false {",
            "discovery::tests::relay_rejects_unreserved_or_expired_target",
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
