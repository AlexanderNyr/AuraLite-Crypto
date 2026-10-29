//! Quorum Certificates (B2).
//!
//! A Quorum Certificate (QC) is a threshold certificate: a set of signed
//! votes from 2f+1 distinct validators for the same `(view, block_hash,
//! round)` triple. QCs are the core of the QC / BLS aggregation phase.
//!
//! Each vote carries a `VoteType` (propose, prevote, precommit) and a
//! BLS12-381 signature over the vote payload. The [`QcAggregator`] collects
//! votes, checks the quorum threshold, and produces a [`QuorumCertificate`].

use crate::bls::{BlsPublicKey, BlsSignature, verify};
use aura_types::{Address, Domain, Hash};
use std::collections::BTreeMap;

/// Number of faulty validators the system tolerates.
///
/// With `n` validators, the system tolerates `f = (n - 1) / 3` Byzantine
/// validators and requires `2f + 1` signatures for a quorum.
pub const DEFAULT_FAULT_TOLERANCE: u64 = 1;

/// Type of consensus vote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteType {
    /// Proposal for a new block at this view/round.
    Propose,
    /// Prevote for a block hash.
    Prevote,
    /// Precommit for a block hash.
    Precommit,
}

/// A signed vote from a single validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vote {
    /// View number (epoch / height).
    pub view: u64,
    /// Round number within the view.
    pub round: u64,
    /// Block hash being voted on.
    pub block_hash: Hash,
    /// Validator address.
    pub validator: Address,
    /// Vote type.
    pub vote_type: VoteType,
    /// BLS signature over the vote payload.
    pub signature: BlsSignature,
}

/// A Quorum Certificate: 2f+1 signatures on the same triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuorumCertificate {
    /// View number.
    pub view: u64,
    /// Round number.
    pub round: u64,
    /// Block hash.
    pub block_hash: Hash,
    /// Vote type.
    pub vote_type: VoteType,
    /// Validator public keys (one per signature).
    pub public_keys: Vec<BlsPublicKey>,
    /// Validator signatures (one per public key).
    pub signatures: Vec<BlsSignature>,
}

impl QuorumCertificate {
    /// Number of signatures in this QC.
    pub fn len(&self) -> usize {
        self.public_keys.len()
    }

    /// True if the QC contains no signatures.
    pub fn is_empty(&self) -> bool {
        self.public_keys.is_empty()
    }

    /// Verify all signatures in the QC against the given chain ID and domain.
    ///
    /// Each signature is verified individually against its corresponding
    /// public key. All signatures must be valid for the same
    /// `(view, round, block_hash, vote_type)` payload.
    pub fn verify(&self, chain_id: &str, domain: Domain) -> Result<(), crate::bls::BlsError> {
        let payload = vote_payload(self.view, self.round, self.block_hash, self.vote_type);
        for (pk, sig) in self.public_keys.iter().zip(self.signatures.iter()) {
            verify(pk, chain_id, domain, &payload, sig)?;
        }
        Ok(())
    }
}

/// Computes the quorum threshold: `2f + 1` where `f = (n - 1) / 3`.
pub fn quorum_threshold(total_validators: u64) -> u64 {
    let f = (total_validators - 1) / 3;
    2 * f + 1
}

/// Aggregates votes into a Quorum Certificate.
#[derive(Debug, Default, Clone)]
pub struct QcAggregator {
    /// Votes collected so far, keyed by validator address.
    votes: BTreeMap<Address, (BlsPublicKey, Vote)>,
    /// Target quorum size.
    threshold: u64,
}

impl QcAggregator {
    /// Create a new aggregator for the given quorum threshold.
    pub fn new(threshold: u64) -> Self {
        Self {
            votes: BTreeMap::new(),
            threshold,
        }
    }

    /// Add a vote with the signer's public key. Returns `Some(QC)` once the
    /// quorum threshold is reached.
    ///
    /// Only the first vote from each validator is kept; duplicate votes
    /// from the same address are ignored.
    pub fn add_vote(&mut self, public_key: BlsPublicKey, vote: Vote) -> Option<QuorumCertificate> {
        if self.votes.contains_key(&vote.validator) {
            return None;
        }
        self.votes
            .insert(vote.validator, (public_key, vote.clone()));
        if self.votes.len() as u64 >= self.threshold {
            Some(self.build_qc())
        } else {
            None
        }
    }

    /// Number of votes collected so far.
    pub fn len(&self) -> usize {
        self.votes.len()
    }

    /// True if no votes have been collected.
    pub fn is_empty(&self) -> bool {
        self.votes.is_empty()
    }

    fn build_qc(&self) -> QuorumCertificate {
        let first = self.votes.values().next().expect("aggregator is non-empty");
        let public_keys: Vec<BlsPublicKey> =
            self.votes.values().map(|(pk, _)| pk.clone()).collect();
        let signatures: Vec<BlsSignature> = self
            .votes
            .values()
            .map(|(_, v)| v.signature.clone())
            .collect();
        let first_vote = &first.1;
        QuorumCertificate {
            view: first_vote.view,
            round: first_vote.round,
            block_hash: first_vote.block_hash,
            vote_type: first_vote.vote_type,
            public_keys,
            signatures,
        }
    }
}

/// Serialize a vote payload for signing.
fn vote_payload(view: u64, round: u64, block_hash: Hash, vote_type: VoteType) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&view.to_le_bytes());
    buf.extend_from_slice(&round.to_le_bytes());
    buf.extend_from_slice(block_hash.as_bytes());
    buf.push(vote_type as u8);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bls::BlsSecretKey;
    use aura_types::Domain;

    fn test_chain_id() -> &'static str {
        "auralite-qc-test"
    }

    fn make_vote_with_key(
        sk: &BlsSecretKey,
        validator: Address,
        view: u64,
        round: u64,
        block_hash: Hash,
        vote_type: VoteType,
    ) -> (BlsPublicKey, Vote) {
        let pk = sk.public_key();
        let payload = vote_payload(view, round, block_hash, vote_type);
        let signature = sk.sign(test_chain_id(), Domain::Vote, &payload);
        let vote = Vote {
            view,
            round,
            block_hash,
            validator,
            vote_type,
            signature,
        };
        (pk, vote)
    }

    #[test]
    fn quorum_threshold_basic() {
        assert_eq!(quorum_threshold(4), 3); // f=1, 2*1+1=3
        assert_eq!(quorum_threshold(7), 5); // f=2, 2*2+1=5
        assert_eq!(quorum_threshold(10), 7); // f=3, 2*3+1=7
    }

    #[test]
    fn aggregator_reaches_quorum() {
        let mut agg = QcAggregator::new(3);
        let sks: Vec<_> = (0..5)
            .map(|i| BlsSecretKey::from_seed(&[i as u8; 32]))
            .collect();
        let block_hash = Hash::from_bytes([0x42; 32]);

        let mut qc = None;
        for i in 0..3u64 {
            let addr = Address::from_bytes([i as u8; 20]);
            let (pk, vote) = make_vote_with_key(
                &sks[i as usize],
                addr,
                1,
                1,
                block_hash,
                VoteType::Precommit,
            );
            qc = agg.add_vote(pk, vote);
        }
        assert!(qc.is_some());
        let qc = qc.unwrap();
        assert_eq!(qc.view, 1);
        assert_eq!(qc.round, 1);
        assert_eq!(qc.block_hash, block_hash);
        assert_eq!(qc.vote_type, VoteType::Precommit);
        assert_eq!(qc.len(), 3);
    }

    #[test]
    fn aggregator_ignores_duplicate_validator() {
        let mut agg = QcAggregator::new(3);
        let sk = BlsSecretKey::from_seed(&[7; 32]);
        let addr = Address::from_bytes([7; 20]);
        let block_hash = Hash::from_bytes([0x42; 32]);

        let (pk, vote1) = make_vote_with_key(&sk, addr, 1, 1, block_hash, VoteType::Precommit);
        let (_, vote2) = make_vote_with_key(&sk, addr, 1, 1, block_hash, VoteType::Precommit);
        assert!(agg.add_vote(pk.clone(), vote1).is_none());
        assert!(agg.add_vote(pk, vote2).is_none());
        assert_eq!(agg.len(), 1);
    }

    #[test]
    fn aggregator_below_threshold_returns_none() {
        let mut agg = QcAggregator::new(3);
        let sks: Vec<_> = (0..2)
            .map(|i| BlsSecretKey::from_seed(&[i as u8; 32]))
            .collect();
        let block_hash = Hash::from_bytes([0x42; 32]);

        for i in 0..2u64 {
            let addr = Address::from_bytes([i as u8; 20]);
            let (pk, vote) = make_vote_with_key(
                &sks[i as usize],
                addr,
                1,
                1,
                block_hash,
                VoteType::Precommit,
            );
            assert!(agg.add_vote(pk, vote).is_none());
        }
        assert_eq!(agg.len(), 2);
    }

    #[test]
    fn qc_verify_all_valid() {
        let mut agg = QcAggregator::new(2);
        let sks: Vec<_> = (0..2)
            .map(|i| BlsSecretKey::from_seed(&[i as u8; 32]))
            .collect();
        let block_hash = Hash::from_bytes([0x42; 32]);

        for i in 0..2u64 {
            let addr = Address::from_bytes([i as u8; 20]);
            let (pk, vote) = make_vote_with_key(
                &sks[i as usize],
                addr,
                1,
                1,
                block_hash,
                VoteType::Precommit,
            );
            agg.add_vote(pk, vote);
        }
        let qc = agg.build_qc();
        assert!(qc.verify(test_chain_id(), Domain::Vote).is_ok());
    }

    #[test]
    fn qc_verify_wrong_chain_id_fails() {
        let mut agg = QcAggregator::new(2);
        let sk = BlsSecretKey::from_seed(&[1; 32]);
        let addr = Address::from_bytes([1; 20]);
        let block_hash = Hash::from_bytes([0x42; 32]);

        let (pk, vote) = make_vote_with_key(&sk, addr, 1, 1, block_hash, VoteType::Precommit);
        agg.add_vote(pk.clone(), vote);
        let qc = agg.build_qc();
        assert!(qc.verify("wrong-chain", Domain::Vote).is_err());
    }

    #[test]
    fn quorum_threshold_single_validator() {
        assert_eq!(quorum_threshold(1), 1); // f=0, 2*0+1=1
    }
}
