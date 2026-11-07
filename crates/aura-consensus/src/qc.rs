//! Quorum Certificates (B2).
//!
//! A Quorum Certificate (QC) is a threshold certificate: a set of signed
//! votes from 2f+1 distinct validators for the same `(view, block_hash,
//! round)` triple. QCs are the core of the QC / BLS aggregation phase.
//!
//! Each vote carries a `VoteType` (propose, prevote, precommit) and a
//! BLS12-381 signature over the vote payload. The [`QcAggregator`] collects
//! votes, checks the quorum threshold, and produces a [`QuorumCertificate`].

use crate::bls::{BlsPublicKey, BlsSignature, address_from_public_key, verify};
use aura_types::{Address, Domain, Hash};
use std::collections::{BTreeMap, BTreeSet};

/// Number of faulty validators tolerated by the default 4-validator set.
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

/// A quorum certificate: a threshold set of individual BLS signatures on the
/// same `(view, round, block_hash, vote_type)` payload.
///
/// This Stage-1 format intentionally keeps one signature per signer so signer
/// attribution and equivocation evidence remain explicit. It is a threshold
/// certificate, not a compact BLS aggregate signature.
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

    /// Verify structure and every signature. This does not prove quorum or
    /// membership; callers must use [`Self::verify_with_members`] for that.
    pub fn verify(&self, chain_id: &str, domain: Domain) -> Result<(), crate::bls::BlsError> {
        if self.public_keys.len() != self.signatures.len() || self.public_keys.is_empty() {
            return Err(crate::bls::BlsError::VerificationFailed);
        }
        let mut seen = BTreeSet::new();
        for pk in &self.public_keys {
            if !seen.insert(address_from_public_key(pk)) {
                return Err(crate::bls::BlsError::VerificationFailed);
            }
        }
        let payload = vote_payload(self.view, self.round, self.block_hash, self.vote_type);
        for (pk, sig) in self.public_keys.iter().zip(self.signatures.iter()) {
            verify(pk, chain_id, domain, &payload, sig)?;
        }
        Ok(())
    }

    /// Verify signatures, active-set membership, uniqueness and quorum size.
    pub fn verify_with_members(
        &self,
        chain_id: &str,
        domain: Domain,
        active: &BTreeSet<Address>,
    ) -> Result<(), crate::bls::BlsError> {
        self.verify(chain_id, domain)?;
        let required = quorum_threshold(active.len() as u64) as usize;
        if self.len() < required {
            return Err(crate::bls::BlsError::VerificationFailed);
        }
        for pk in &self.public_keys {
            if !active.contains(&address_from_public_key(pk)) {
                return Err(crate::bls::BlsError::VerificationFailed);
            }
        }
        Ok(())
    }
}

/// BFT count quorum for `n` validators: `n - f`, where
/// `f = floor((n - 1) / 3)`. For `n = 3f + 1` this is the familiar `2f + 1`.
/// Returns `0` for an empty validator set; no quorum can be formed from it.
pub fn quorum_threshold(total_validators: u64) -> u64 {
    let Some(max_faulty) = total_validators.checked_sub(1).map(|n| n / 3) else {
        return 0;
    };
    total_validators - max_faulty
}

/// Collects verified votes for one exact payload and one fixed active set.
#[derive(Debug, Clone)]
pub struct QcAggregator {
    chain_id: String,
    active: BTreeSet<Address>,
    votes: BTreeMap<Address, (BlsPublicKey, Vote)>,
    threshold: u64,
    payload: Option<(u64, u64, Hash, VoteType)>,
}

impl QcAggregator {
    /// Create an aggregator for a fixed active validator address set.
    pub fn new(chain_id: impl Into<String>, active: BTreeSet<Address>) -> Self {
        let threshold = quorum_threshold(active.len() as u64);
        Self {
            chain_id: chain_id.into(),
            active,
            votes: BTreeMap::new(),
            threshold,
            payload: None,
        }
    }

    /// Convenience constructor deriving active addresses from public keys.
    pub fn with_signers(chain_id: impl Into<String>, keys: &[BlsPublicKey]) -> Self {
        Self::new(chain_id, keys.iter().map(address_from_public_key).collect())
    }

    /// Chain identifier this aggregator verifies signatures under.
    pub fn chain_id(&self) -> &str {
        &self.chain_id
    }

    /// Active validator set used for membership and quorum size.
    pub fn active(&self) -> &BTreeSet<Address> {
        &self.active
    }

    /// Add a vote. Invalid signatures, non-members, mismatched public keys,
    /// mixed payloads and duplicate/conflicting votes are rejected (`None`).
    pub fn add_vote(&mut self, public_key: BlsPublicKey, vote: Vote) -> Option<QuorumCertificate> {
        if self.threshold == 0
            || !self.active.contains(&vote.validator)
            || address_from_public_key(&public_key) != vote.validator
        {
            return None;
        }
        let payload = (vote.view, vote.round, vote.block_hash, vote.vote_type);
        if let Some(expected) = self.payload
            && expected != payload
        {
            return None;
        }
        let payload_bytes = vote_payload(vote.view, vote.round, vote.block_hash, vote.vote_type);
        if verify(
            &public_key,
            &self.chain_id,
            Domain::Vote,
            &payload_bytes,
            &vote.signature,
        )
        .is_err()
        {
            return None;
        }
        if self.votes.contains_key(&vote.validator) {
            return None;
        }
        self.payload = Some(payload);
        self.votes.insert(vote.validator, (public_key, vote));
        if self.votes.len() as u64 >= self.threshold {
            Some(self.build_qc())
        } else {
            None
        }
    }

    /// Number of accepted votes.
    pub fn len(&self) -> usize {
        self.votes.len()
    }

    /// True when no vote has been accepted.
    pub fn is_empty(&self) -> bool {
        self.votes.is_empty()
    }

    fn build_qc(&self) -> QuorumCertificate {
        let first = self.votes.values().next().expect("aggregator is non-empty");
        let public_keys = self.votes.values().map(|(pk, _)| pk.clone()).collect();
        let signatures = self
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

    const CHAIN: &str = "auralite-qc-test";

    fn keys(n: usize) -> Vec<BlsSecretKey> {
        (0..n)
            .map(|i| BlsSecretKey::from_seed(&[i as u8 + 1; 32]))
            .collect()
    }

    fn vote(sk: &BlsSecretKey, view: u64, round: u64, hash: Hash) -> Vote {
        let payload = vote_payload(view, round, hash, VoteType::Precommit);
        Vote {
            view,
            round,
            block_hash: hash,
            validator: address_from_public_key(&sk.public_key()),
            vote_type: VoteType::Precommit,
            signature: sk.sign(CHAIN, Domain::Vote, &payload),
        }
    }

    #[test]
    fn quorum_threshold_is_safe_for_all_n() {
        assert_eq!(quorum_threshold(0), 0);
        assert_eq!(quorum_threshold(1), 1);
        assert_eq!(quorum_threshold(4), 3);
        assert_eq!(quorum_threshold(6), 5);
        assert_eq!(quorum_threshold(7), 5);
        assert_eq!(quorum_threshold(10), 7);
    }

    #[test]
    fn aggregator_reaches_quorum_and_verifies_membership() {
        let sks = keys(4);
        let pks: Vec<_> = sks.iter().map(|sk| sk.public_key()).collect();
        let mut agg = QcAggregator::with_signers(CHAIN, &pks);
        let hash = Hash::from_bytes([0x42; 32]);
        let mut qc = None;
        for sk in &sks {
            if let Some(cert) = agg.add_vote(sk.public_key(), vote(sk, 1, 1, hash)) {
                qc = Some(cert);
                break;
            }
        }
        let qc = qc.expect("quorum");
        assert_eq!(qc.signatures.len(), 3);
        assert_eq!(qc.public_keys.len(), 3);
        let active: BTreeSet<_> = pks.iter().map(address_from_public_key).collect();
        assert!(qc.verify_with_members(CHAIN, Domain::Vote, &active).is_ok());
    }

    #[test]
    fn rejects_invalid_signature_nonmember_and_mixed_payload() {
        let sks = keys(4);
        let pks: Vec<_> = sks.iter().map(|sk| sk.public_key()).collect();
        let hash = Hash::from_bytes([0x42; 32]);
        let mut agg = QcAggregator::with_signers(CHAIN, &pks[..3]);
        let mut bad = vote(&sks[0], 1, 1, hash);
        bad.view = 99;
        assert!(agg.add_vote(sks[0].public_key(), bad).is_none());
        assert!(
            agg.add_vote(sks[3].public_key(), vote(&sks[3], 1, 1, hash))
                .is_none()
        );
        assert!(
            agg.add_vote(sks[0].public_key(), vote(&sks[0], 1, 1, hash))
                .is_none()
        );
        assert!(
            agg.add_vote(
                sks[1].public_key(),
                vote(&sks[1], 1, 1, Hash::from_bytes([0x43; 32]))
            )
            .is_none()
        );
        assert_eq!(agg.len(), 1);
    }

    #[test]
    fn duplicate_validator_cannot_count_twice() {
        let sks = keys(4);
        let pks: Vec<_> = sks.iter().map(|sk| sk.public_key()).collect();
        let mut agg = QcAggregator::with_signers(CHAIN, &pks);
        let hash = Hash::from_bytes([7; 32]);
        assert!(
            agg.add_vote(sks[0].public_key(), vote(&sks[0], 1, 1, hash))
                .is_none()
        );
        assert!(
            agg.add_vote(sks[0].public_key(), vote(&sks[0], 1, 1, hash))
                .is_none()
        );
        assert_eq!(agg.len(), 1);
    }
}
