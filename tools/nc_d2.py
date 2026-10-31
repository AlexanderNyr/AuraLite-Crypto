#!/usr/bin/env python3
"""D2 negative controls; every mutation must turn its exact library unit test red."""
from __future__ import annotations

import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/aura-node/src/pipeline.rs"
CASES = [
    (
        "NC-1 chain replay must be rejected",
        "tx.validate(&self.chain_id, height)?;",
        "tx.verify_signature()?;",
        "rejects_replay_from_another_chain",
    ),
    (
        "NC-2 sender nonce must match projected state",
        "sender_account.nonce.value() != tx.unsigned.nonce.value()",
        "false",
        "rejects_nonce_gap",
    ),
    (
        "NC-3 transfer value and fee must be debited atomically",
        '''    if sender_account.balance < total_debit {
        return Err(PipelineError::InsufficientBalance);
    }
    sender_account
        .debit(total_debit)
        .map_err(|_| PipelineError::InsufficientBalance)?;''',
        '''    if false {
        return Err(PipelineError::InsufficientBalance);
    }
    sender_account
        .debit(Amount::ZERO)
        .map_err(|_| PipelineError::InsufficientBalance)?;''',
        "rejects_insufficient_balance_for_value_and_fee",
    ),
    (
        "NC-4 gas limit must cover the transfer schedule",
        "tx.unsigned.gas_limit.value() < TRANSFER_GAS",
        "false",
        "rejects_transfer_below_gas_floor",
    ),
    (
        "NC-5 fee cap must cover base fee plus tip",
        "tx.unsigned.max_fee < effective_price",
        "false",
        "rejects_fee_cap_below_base_fee_plus_tip",
    ),
    (
        "NC-6 receipt must be durable before event publication",
        "value: Some(receipt.to_bytes()),",
        "value: None,",
        "receipt_is_durable_before_commit_event",
    ),
]


def run_test(test: str) -> tuple[bool, str]:
    exact = f"pipeline::tests::{test}"
    command = [
        "cargo", "test", "-p", "aura-node", "--lib", "--", "--exact", exact,
    ]
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
    output = result.stdout + result.stderr
    ran_ok = result.returncode == 0 and re.search(
        rf"test\s+{re.escape(exact)}\s+\.\.\.\s+ok", output
    ) is not None
    ran_failed = result.returncode != 0 and re.search(
        rf"test\s+{re.escape(exact)}\s+\.\.\.\s+FAILED", output
    ) is not None
    return (ran_ok if result.returncode == 0 else ran_failed), output


def main() -> int:
    original = SOURCE.read_text(encoding="utf-8")
    print("D2 NEGATIVE CONTROLS (focused aura-node library unit tests)")

    # Establish that each exact unit test is green before applying its mutation.
    for label, old, _new, test in CASES:
        if old not in original:
            print(f"{label}: FAIL (tamper target missing)")
            return 1
        ok, output = run_test(test)
        if not ok:
            print(f"{label}: FAIL (exact unit test did not pass before mutation)")
            print(output[-5000:])
            return 1

    passed = 0
    for label, old, new, test in CASES:
        current = SOURCE.read_text(encoding="utf-8")
        if old not in current:
            print(f"{label}: FAIL (tamper target missing)")
            return 1
        SOURCE.write_text(current.replace(old, new, 1), encoding="utf-8")
        try:
            went_red, output = run_test(test)
        finally:
            SOURCE.write_text(original, encoding="utf-8")
        if went_red:
            passed += 1
            print(f"{label}: PASS")
        else:
            print(f"{label}: FAIL (expected exact unit test to fail: pipeline::tests::{test})")
            print(output[-5000:])
    print(f"RESULT: {passed}/{len(CASES)} negative controls PASS")
    return 0 if passed == len(CASES) else 1


if __name__ == "__main__":
    raise SystemExit(main())
