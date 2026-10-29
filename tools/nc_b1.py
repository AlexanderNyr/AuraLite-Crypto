#!/usr/bin/env python3
"""Six B1 negative controls for proposer rotation and block signatures."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from nc_rust import run_controls

PROPOSER = "crates/aura-consensus/src/proposer.rs"
SIGNATURE = "crates/aura-consensus/src/signature.rs"
CASES = [
    (
        "NC-1 proposer selection must rotate by round",
        PROPOSER,
        "self.validators[round as usize % self.validators.len()]",
        "self.validators[(round as usize + 1) % self.validators.len()]",
        "proposer::tests::round_robin_rotation",
    ),
    (
        "NC-2 proposer identity check must match selected validator",
        PROPOSER,
        "&self.propose(round) == address",
        "true",
        "proposer::tests::is_proposer_matches",
    ),
    (
        "NC-3 block signatures must be chain separated",
        SIGNATURE,
        '            chain_id,\n            Domain::Block,\n            self.block_hash.as_bytes(),',
        '            "auralite-test-1",\n            Domain::Block,\n            self.block_hash.as_bytes(),',
        "signature::tests::wrong_chain_id_fails",
    ),
    (
        "NC-4 block signatures must bind the signed block hash",
        SIGNATURE,
        "self.block_hash.as_bytes(),\n            &self.signature,",
        "&[9u8; 32],\n            &self.signature,",
        "signature::tests::wrong_block_hash_fails",
    ),
    (
        "NC-5 tampered signatures must be rejected",
        SIGNATURE,
        ".map_err(|_| ConsensusError::InvalidSignature)",
        ".map_err(|_| ConsensusError::InvalidSignature)\n        .or(Ok(()))",
        "signature::tests::tampered_signature_fails",
    ),
    (
        "NC-6 verification must use the supplied public key",
        SIGNATURE,
        "            &self.public_key,\n            chain_id,",
        "            &ed25519_public_key(&[42u8; 32]),\n            chain_id,",
        "signature::tests::wrong_public_key_fails",
    ),
]

if __name__ == "__main__":
    raise SystemExit(run_controls("B1", CASES))
