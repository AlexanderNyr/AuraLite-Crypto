#!/usr/bin/env python3
"""Negative controls for N7 (Light client: header+QC proof, checkpoint check, simplified DAS).

Six tampers, each turning a named library unit test in `crates/aura-p2p` red
via `cargo test -p aura-p2p --lib --quiet -- --exact <test>`:

  NC-1 header self-hash check bypassed         -> forged header state_root accepted by light client.
  NC-2 QC 2f+1 verification bypassed           -> invalid QC signature accepted by light client.
  NC-3 finalized checkpoint anchor bypassed    -> wrong checkpoint hash accepted by light client.
  NC-4 proof size & gas bounds bypassed        -> oversized/over-gas proof accepted pre-crypto.
  NC-5 DAS minimum sample count bypassed       -> duplicate/insufficient DAS samples accepted.
  NC-6 DAS Merkle inclusion proof bypassed     -> corrupted DAS chunk accepted against tx_root.

Usage:
    python3 tools/nc_n7.py
    make nc-n7
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
LIGHT_CLIENT = REPO / "crates" / "aura-p2p" / "src" / "light_client.rs"

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
    print("N7 NEGATIVE CONTROLS (Light client: header+QC proof, checkpoint check, simplified DAS)")
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
            "NC-1 forged header or tampered state_root must be rejected and ban the peer",
            LIGHT_CLIENT,
            "if proof.header.block_hash != proof.header.compute_block_hash() {",
            "if proof.header.block_hash != proof.header.compute_block_hash() && false {",
            "light_client::tests::forged_header_or_tampered_state_root_is_rejected_and_peer_banned",
        ),
        expect_red(
            "NC-2 QC with forged signature or insufficient quorum must be rejected",
            LIGHT_CLIENT,
            """        if let Err(err) = proof.qc.verify(
            &self.checkpoint.chain_id,
            proof.header.height,
            &proof.header.block_hash,
            &self.validator_set,
        ) {""",
            """        if let Err(err) = proof.qc.verify(
            &self.checkpoint.chain_id,
            proof.header.height,
            &proof.header.block_hash,
            &self.validator_set,
        ) && false {""",
            "light_client::tests::qc_below_quorum_or_forged_signature_is_rejected",
        ),
        expect_red(
            "NC-3 finalized checkpoint anchor mismatch must be rejected",
            LIGHT_CLIENT,
            """        if proof.checkpoint_height != self.checkpoint.height
            || proof.checkpoint_hash != self.checkpoint.block_hash
        {""",
            """        if (proof.checkpoint_height != self.checkpoint.height
            || proof.checkpoint_hash != self.checkpoint.block_hash)
            && false
        {""",
            "light_client::tests::checkpoint_mismatch_or_height_regression_is_rejected",
        ),
        expect_red(
            "NC-4 proofs exceeding max_proof_bytes or max_verification_gas must be rejected",
            LIGHT_CLIENT,
            "if proof_bytes > self.bounds.max_proof_bytes {",
            "if proof_bytes > self.bounds.max_proof_bytes && false {",
            "light_client::tests::proof_exceeding_size_or_gas_bound_is_rejected_before_crypto",
        ),
        expect_red(
            "NC-5 DAS verification must require min_das_samples distinct chunk indices",
            LIGHT_CLIENT,
            "if distinct_indices.len() < required_samples {",
            "if distinct_indices.len() < required_samples && false {",
            "light_client::tests::das_sampling_rejects_insufficient_or_duplicate_chunk_samples",
        ),
        expect_red(
            "NC-6 DAS chunk sample Merkle branch must verify against header.tx_root",
            LIGHT_CLIENT,
            "&current == expected_tx_root",
            "(&current == expected_tx_root) || true",
            "light_client::tests::das_sampling_rejects_corrupted_chunk_merkle_proof_and_bans_peer",
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
