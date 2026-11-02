#!/usr/bin/env python3
"""Negative controls for N8 (Chaos harness: latency/loss/partition/eclipse/Byzantine on 100-node stand).

Six tampers, each turning a named library unit test in `crates/aura-p2p` red
via `cargo test -p aura-p2p --lib --quiet -- --exact <test>`:

  NC-1 light-client verification in stand bypassed -> unverified state_root reported as verified.
  NC-2 partition heal catch-up sync bypassed       -> partitioned nodes fail to converge on head.
  NC-3 eclipse QC verification bypassed            -> eclipsed nodes accept uncertified fake header.
  NC-4 Byzantine equivocation broadcast bypassed   -> Byzantine equivocator never banned by stand.
  NC-5 head divergence detector bypassed           -> mismatched finalized state_root ignored.
  NC-6 chaos matrix divergence counter bypassed    -> matrix reports divergent_runs > 0 on clean run.

Usage:
    python3 tools/nc_n8.py
    make nc-n8
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CHAOS = REPO / "crates" / "aura-p2p" / "src" / "chaos.rs"

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
    print("N8 NEGATIVE CONTROLS (Chaos harness: latency/loss/partition/eclipse/Byzantine on 100-node stand)")
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
            "NC-1 100-node 20% loss + Byzantine stand must verify finalized root via light client",
            CHAOS,
            "let light_client_verified = lc_receipt.block_hash == finalized_block_hash\n            && lc_receipt.state_root == finalized_state_root;",
            "let light_client_verified = false;",
            "chaos::tests::hundred_nodes_with_20pct_loss_and_byzantine_peer_converge_and_light_client_verifies",
        ),
        expect_red(
            "NC-2 partition healing must sync lagging nodes to the canonical finalized head",
            CHAOS,
            "if node.finalized_height < self.config.target_heights {",
            "if node.finalized_height < self.config.target_heights && false {",
            "chaos::tests::network_partition_and_heal_preserves_safety_and_converges_without_divergence",
        ),
        expect_red(
            "NC-3 eclipsed nodes must reject uncertified headers via 2f+1 QC verification",
            CHAOS,
            """.is_err()
                    {
                        node.eclipse_headers_rejected += 1;""",
            """.is_err() && false
                    {
                        node.eclipse_headers_rejected += 1;""",
            "chaos::tests::eclipse_attempt_is_defeated_by_qc_verification_and_outbound_diversity",
        ),
        expect_red(
            "NC-4 honest nodes must detect Byzantine equivocation and ban the Byzantine peer",
            CHAOS,
            "if let Some(byz_idx) = self.config.byzantine_node\n                && height == 1",
            "if let Some(byz_idx) = self.config.byzantine_node\n                && false",
            "chaos::tests::byzantine_equivocator_is_detected_and_banned_across_honest_nodes",
        ),
        expect_red(
            "NC-5 cluster convergence verifier must reject any finalized state_root divergence",
            CHAOS,
            "|| node.finalized_state_root != first.finalized_state_root",
            "|| false",
            "chaos::tests::head_divergence_detector_flags_any_mismatched_finalized_state_root",
        ),
        expect_red(
            "NC-6 20 scenarios × 3 runs matrix must report zero finalized-head divergence",
            CHAOS,
            "if !report.light_client_verified {\n                        divergent_runs += 1;\n                    }",
            "divergent_runs += 1;",
            "chaos::tests::twenty_scenarios_times_three_runs_have_zero_finalized_head_divergence",
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
