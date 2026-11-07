#!/usr/bin/env python3
"""Six D7 devnet mutation controls, each targeting an exact aura-node library unit test."""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DEVNET = REPO / "crates/aura-node/src/devnet.rs"

CASES = [
    (
        "NC-1 substituted peer identity must be rejected",
        DEVNET,
        "    if claimed_peer_id.verify_public_key(public_key).is_err() {\n        return Err(DevnetError::SubstitutedPeerIdentity);\n    }",
        "    let _ = (claimed_peer_id, public_key);",
        "devnet::tests::rejects_substituted_peer_identity",
    ),
    (
        "NC-2 malformed or unsigned gossip must be rejected",
        DEVNET,
        "        GossipOutcome::Rejected { error, .. } => {\n            Err(DevnetError::MalformedOrUnsignedGossip(error.to_string()))\n        }",
        "        GossipOutcome::Rejected { .. } => Ok(Some(decoded)),",
        "devnet::tests::rejects_malformed_or_unsigned_gossip",
    ),
    (
        "NC-3 duplicate gossip messages must be deduplicated",
        DEVNET,
        "        GossipOutcome::Duplicate { .. } => Ok(None),",
        "        GossipOutcome::Duplicate { .. } => Ok(Some(decoded)),",
        "devnet::tests::deduplicates_repeated_gossip_messages",
    ),
    (
        "NC-4 four-validator quorum threshold must be three",
        DEVNET,
        "    aura_consensus::qc::quorum_threshold(validator_count)",
        "    validator_count / 2",
        "devnet::tests::four_validator_quorum_threshold_is_three",
    ),
    (
        "NC-5 quorum must use the registered committee, not the voters alone",
        DEVNET,
        "    let required = devnet_quorum_threshold(validator_keys.len() as u64);\n    let mut agg = QcAggregator::with_signers(chain_id, validator_keys);",
        "    let voter_keys: Vec<_> = votes.iter().map(|(pk, _)| pk.clone()).collect();\n    let required = devnet_quorum_threshold(voter_keys.len() as u64);\n    let mut agg = QcAggregator::with_signers(chain_id, &voter_keys);",
        "devnet::tests::two_votes_of_four_never_commit",
    ),
    (
        "NC-6 devnet must continue finalizing with one of four validators unavailable",
        DEVNET,
        "        let required = devnet_quorum_threshold(DEVNET_VALIDATOR_COUNT as u64);\n        if online_count < required {",
        "        let required = DEVNET_VALIDATOR_COUNT as u64;\n        if online_count < required {",
        "devnet::tests::continues_finalization_with_one_of_four_validators_unavailable",
    ),
]


def focused_test(test: str, expect_failure: bool = False) -> tuple[bool, str]:
    command = [
        "cargo",
        "test",
        "-p",
        "aura-node",
        "--lib",
        "--",
        "--exact",
        test,
    ]
    result = subprocess.run(command, cwd=REPO, text=True, capture_output=True)
    output = result.stdout + result.stderr
    ran_ok = result.returncode == 0 and re.search(
        rf"test\s+{re.escape(test)}\s+\.\.\.\s+ok", output
    ) is not None
    ran_failed = result.returncode != 0 and re.search(
        rf"test\s+{re.escape(test)}\s+\.\.\.\s+FAILED", output
    ) is not None
    return (ran_failed if expect_failure else ran_ok), output


def main() -> int:
    print("D7: validating focused baseline tests")
    for label, path, old, _new, test in CASES:
        original = path.read_text(encoding="utf-8")
        if old not in original:
            print(f"FAIL {label}: tamper target not found in {path.relative_to(REPO)}")
            return 1
        ok, output = focused_test(test)
        if not ok:
            print(f"FAIL {label}: target test did not pass before mutation")
            print(output[-5000:])
            return 1

    passed = 0
    for label, path, old, new, test in CASES:
        original = path.read_text(encoding="utf-8")
        mutated = original.replace(old, new, 1)
        path.write_text(mutated, encoding="utf-8")
        try:
            red, output = focused_test(test, expect_failure=True)
        finally:
            path.write_text(original, encoding="utf-8")
        if red:
            passed += 1
            print(f"PASS {label}: {test} failed as expected")
        else:
            print(f"FAIL {label}: expected named test to fail: {test}")
            print(output[-5000:])

    print(f"RESULT: {passed}/{len(CASES)} negative controls PASS")
    return 0 if passed == len(CASES) else 1


if __name__ == "__main__":
    sys.exit(main())
