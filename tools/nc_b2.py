#!/usr/bin/env python3
"""Six B2 negative controls for QC aggregation and BLS verification."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from nc_rust import run_controls

QC = "crates/aura-consensus/src/qc.rs"
BLS = "crates/aura-consensus/src/bls.rs"
VERIFY_DIGEST = '''    let digest = hash_domain(chain_id, domain, message);
    let dst = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";'''
CASES = [
    (
        "NC-1 quorum threshold must be n-f",
        QC,
        "total_validators - max_faulty",
        "total_validators.saturating_sub(max_faulty + 1)",
        "qc::tests::quorum_threshold_is_safe_for_all_n",
    ),
    (
        "NC-2 duplicate validators must not add a second vote",
        QC,
        '''        if self.votes.contains_key(&vote.validator) {
            return None;
        }
''',
        """        if self.votes.contains_key(&vote.validator) {
            return Some(self.build_qc());
        }
""",
        "qc::tests::duplicate_validator_cannot_count_twice",
    ),
    (
        "NC-3 an aggregator must form a QC at its threshold",
        QC,
        "self.votes.len() as u64 >= self.threshold",
        "self.votes.len() as u64 > self.threshold",
        "qc::tests::aggregator_reaches_quorum_and_verifies_membership",
    ),
    (
        "NC-4 BLS signatures must be chain separated",
        BLS,
        VERIFY_DIGEST,
        VERIFY_DIGEST.replace(
            "hash_domain(chain_id, domain, message)",
            'hash_domain("auralite-test-1", domain, message)',
        ),
        "bls::tests::wrong_chain_id_fails",
    ),
    (
        "NC-5 BLS signatures must be domain separated",
        BLS,
        VERIFY_DIGEST,
        VERIFY_DIGEST.replace(
            "hash_domain(chain_id, domain, message)",
            "hash_domain(chain_id, Domain::Vote, message)",
        ),
        "bls::tests::wrong_domain_fails",
    ),
    (
        "NC-6 BLS verification must bind the vote message",
        BLS,
        VERIFY_DIGEST,
        VERIFY_DIGEST.replace(
            "hash_domain(chain_id, domain, message)",
            'hash_domain(chain_id, domain, b"original")',
        ),
        "bls::tests::tampered_message_fails",
    ),
]

if __name__ == "__main__":
    raise SystemExit(run_controls("B2", CASES))
