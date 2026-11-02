#!/usr/bin/env python3
"""Negative controls for N5 (S4 state sync: chunks, manifest, root verification, rollback).

Six tampers, each turning a named library unit test in `crates/aura-p2p` red
via `cargo test -p aura-p2p --lib --quiet -- --exact <test>`:

  NC-1 manifest state_root check bypassed      -> manifest with wrong state_root accepted.
  NC-2 S4 manifest canonical check bypassed    -> tampered manifest checksum accepted.
  NC-3 manifest-first requirement bypassed     -> chunk accepted before manifest verified.
  NC-4 S4 chunk SHA-256 hash check bypassed    -> corrupt chunk accepted without rollback or ban.
  NC-5 S1 state_root recomputation bypassed    -> mismatched entries committed on finalize.
  NC-6 corrupt-chunk failover loop bypassed    -> corrupt chunk aborts sync instead of re-requesting.

Usage:
    python3 tools/nc_n5.py
    make nc-n5
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
STATE_SYNC = REPO / "crates" / "aura-p2p" / "src" / "state_sync.rs"

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
    print("N5 NEGATIVE CONTROLS (S4 state sync: chunks, manifest, root verification, rollback)")
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
            "NC-1 manifest must reject state_root mismatch against QC-certified header",
            STATE_SYNC,
            "if *parsed.state_root.as_bytes() != self.expected_state_root {",
            "if *parsed.state_root.as_bytes() != self.expected_state_root && false {",
            "state_sync::tests::manifest_rejects_state_root_mismatch_against_certified_header",
        ),
        expect_red(
            "NC-2 manifest must reject tampered S4 checksum or chunk_root and ban peer",
            STATE_SYNC,
            'self.ban_peer(from_peer, format!("invalid_s4_manifest:{err}"));',
            "let _ = &err;",
            "state_sync::tests::manifest_rejects_tampered_s4_checksum_or_chunk_root",
        ),
        expect_red(
            "NC-3 chunk ingestion must require an accepted manifest first",
            STATE_SYNC,
            "let Some(ref manifest) = self.manifest else {\n            return Err(P2pError::StateSyncManifestMissing);\n        };",
            "let Some(ref manifest) = self.manifest else {\n            return Ok(());\n        };",
            "state_sync::tests::chunk_ingestion_requires_accepted_manifest_first",
        ),
        expect_red(
            "NC-4 corrupt chunk must trigger rollback, ban peer, and mark re-request",
            STATE_SYNC,
            "let canonical_ok = length_ok && hash_ok && decode_chunk(&payload).is_ok();",
            "let canonical_ok = (length_ok && hash_ok && decode_chunk(&payload).is_ok()) || true;",
            "state_sync::tests::corrupt_chunk_triggers_rollback_bans_peer_and_marks_rerequest",
        ),
        expect_red(
            "NC-5 finalization must verify recomputed S1 Merkle state_root",
            STATE_SYNC,
            "if *recomputed.as_bytes() != self.expected_state_root {",
            "if *recomputed.as_bytes() != self.expected_state_root && false {",
            "state_sync::tests::finalize_verifies_recomputed_s1_state_root",
        ),
        expect_red(
            "NC-6 sync_from_peers must re-request corrupt chunk from honest fallback peer",
            STATE_SYNC,
            "for offset in 0..peer_sources.len() {",
            "for offset in 0..1usize {",
            "state_sync::tests::failover_rerequests_corrupt_chunk_from_honest_peer",
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
