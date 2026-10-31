#!/usr/bin/env python3
"""D3 mutation controls, each guarded by one exact Rust library unit test."""
from __future__ import annotations

import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PIPELINE = ROOT / "crates/aura-node/src/pipeline.rs"
POOL = ROOT / "crates/aura-mempool/src/lib.rs"
CASES = [
    (
        "NC-1 sender nonce must match projected account state",
        PIPELINE,
        "if sender_account.nonce.value() != tx.unsigned.nonce.value() {",
        "if false {",
        "aura-node",
        "pipeline::tests::rejects_nonce_gap",
    ),
    (
        "NC-2 ready sender lanes must use tip priority",
        POOL,
        "Reverse(tx.unsigned.tip.units())",
        "Reverse(0)",
        "aura-mempool",
        "tests::ready_transactions_follow_fee_priority_and_nonce_dependencies",
    ),
    (
        "NC-3 same-nonce replacement must meet the fee bump",
        POOL,
        "if got < required {",
        "if false {",
        "aura-mempool",
        "tests::replacement_requires_fee_bump_and_replaces_same_nonce",
    ),
    (
        "NC-4 full pool must reject equal-priority insertion",
        POOL,
        "if tx.unsigned.tip.units() <= weakest.unsigned.tip.units() {",
        "if false {",
        "aura-mempool",
        "tests::capacity_evicts_only_lower_priority",
    ),
    (
        "NC-5 reorg reinsertion must return validated transactions to the pool",
        POOL,
        "        self.insert(tx)\n    }",
        "        Err(MempoolError::DuplicateTransaction)\n    }",
        "aura-mempool",
        "tests::reorged_transactions_can_be_reinserted_at_new_head",
    ),
    (
        "NC-6 gas limit must cover the native transfer schedule",
        PIPELINE,
        "tx.unsigned.gas_limit.value() < TRANSFER_GAS",
        "false",
        "aura-node",
        "pipeline::tests::rejects_transfer_below_gas_floor",
    ),
]


def run_test(package: str, test: str) -> tuple[bool, str]:
    command = ["cargo", "test", "-p", package, "--lib", "--", "--exact", test]
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
    output = result.stdout + result.stderr
    exact = result.returncode == 0 and re.search(
        rf"test\s+{re.escape(test)}\s+\.\.\.\s+ok", output
    ) is not None
    failed = result.returncode != 0 and re.search(
        rf"test\s+{re.escape(test)}\s+\.\.\.\s+FAILED", output
    ) is not None
    return (exact if result.returncode == 0 else failed), output


def main() -> int:
    originals = {path: path.read_text(encoding="utf-8") for _, path, *_ in CASES}
    print("D3 NEGATIVE CONTROLS (focused Rust library unit tests)")
    for label, path, old, _new, package, test in CASES:
        if old not in originals[path]:
            print(f"{label}: FAIL (mutation target missing)")
            return 1
        green, output = run_test(package, test)
        if not green:
            print(f"{label}: FAIL (exact unit test was not green before mutation)")
            print(output[-4000:])
            return 1

    passed = 0
    for label, path, old, new, package, test in CASES:
        current = path.read_text(encoding="utf-8")
        if old not in current:
            print(f"{label}: FAIL (mutation target disappeared)")
            return 1
        path.write_text(current.replace(old, new, 1), encoding="utf-8")
        try:
            red, output = run_test(package, test)
        finally:
            path.write_text(originals[path], encoding="utf-8")
        if red:
            passed += 1
            print(f"{label}: PASS")
        else:
            print(f"{label}: FAIL (expected {test} to fail)")
            print(output[-4000:])
    print(f"RESULT: {passed}/{len(CASES)} negative controls PASS")
    return 0 if passed == len(CASES) else 1


if __name__ == "__main__":
    raise SystemExit(main())
