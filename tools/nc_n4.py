#!/usr/bin/env python3
"""Negative controls for N4 (header-first block sync, QC verification, parallel body download).

Six tampers, each turning a named library unit test in `crates/aura-p2p` red
via `cargo test -p aura-p2p --lib --quiet -- --exact <test>`:

  NC-1 header parent_hash check bypassed       -> broken parent link accepted.
  NC-2 header block_hash check bypassed        -> tampered header accepted.
  NC-3 QC quorum stake check bypassed          -> < 2f+1 stake QC accepted.
  NC-4 QC Ed25519 signature check bypassed     -> forged validator signature accepted.
  NC-5 header-first ordering check bypassed    -> body accepted before header+QC verified.
  NC-6 body tx_root verification bypassed      -> corrupted block body accepted without banning peer.

Usage:
    python3 tools/nc_n4.py
    make nc-n4
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BLOCK_SYNC = REPO / "crates" / "aura-p2p" / "src" / "block_sync.rs"

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
    print("N4 NEGATIVE CONTROLS (header-first block sync, QC verification, parallel body download)")
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
            "NC-1 header chain must reject parent_hash mismatch",
            BLOCK_SYNC,
            "if self.parent_hash != *parent_hash {",
            "if self.parent_hash != *parent_hash && false {",
            "block_sync::tests::header_chain_rejects_parent_hash_mismatch",
        ),
        expect_red(
            "NC-2 header verification must reject tampered block_hash",
            BLOCK_SYNC,
            "if self.block_hash != self.compute_block_hash() {",
            "if self.block_hash != self.compute_block_hash() && false {",
            "block_sync::tests::header_rejects_tampered_block_hash",
        ),
        expect_red(
            "NC-3 QC verification must reject stake below 2f+1 quorum threshold",
            BLOCK_SYNC,
            "if signed_stake < required_stake {",
            "if signed_stake < required_stake && false {",
            "block_sync::tests::qc_rejects_below_quorum_stake",
        ),
        expect_red(
            "NC-4 QC verification must reject forged validator Ed25519 signature",
            BLOCK_SYNC,
            "verify_raw(pubkey, &digest, sig)\n                .map_err(|_| P2pError::BlockSyncQcInvalidSignature(*peer_id))?;",
            "verify_raw(pubkey, &digest, sig)\n                .map_err(|_| P2pError::BlockSyncQcInvalidSignature(*peer_id))\n                .or::<P2pError>(Ok(()))?;",
            "block_sync::tests::qc_rejects_forged_validator_signature",
        ),
        expect_red(
            "NC-5 body ingestion must require verified header+QC first (D-N4)",
            BLOCK_SYNC,
            "let Some((header, _)) = self.verified_headers.get(&body.height) else {\n            return Err(P2pError::BlockSyncHeaderMissing(body.height));\n        };",
            "let Some((header, _)) = self.verified_headers.get(&body.height) else {\n            return Ok(());\n        };",
            "block_sync::tests::body_ingestion_requires_verified_header_first",
        ),
        expect_red(
            "NC-6 body verification must reject tx_root mismatch and ban peer",
            BLOCK_SYNC,
            "if actual_root != header.tx_root {",
            "if actual_root != header.tx_root && false {",
            "block_sync::tests::body_rejects_tx_root_mismatch_and_bans_peer",
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
