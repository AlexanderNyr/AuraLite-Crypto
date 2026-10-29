//! Simple block signature (B1).
//!
//! An Ed25519 signature over a block hash, domain-separated by chain-id and
//! `Domain::Block`. This is PoA-level authentication: the proposer signs the
//! block they produce, and anyone can verify that the signature came from the
//! claimed validator. B2 upgrades this to BLS aggregation into a QC.

use aura_block::Block;
use aura_crypto::{ed25519_public_key, sign, verify};
use aura_types::{Address, Domain, Hash};

use crate::error::ConsensusError;

/// A block signature produced by a validator.
///
/// `signature` is a 64-byte Ed25519 signature over the domain-separated
/// BLAKE3 digest of `block_hash`, under `(chain_id, Domain::Block)`.
/// `public_key` is the 32-byte Ed25519 public key used to verify it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSignature {
    /// The validator who signed (their 20-byte address).
    pub validator: Address,
    /// The Ed25519 public key used to produce and verify the signature.
    pub public_key: [u8; 32],
    /// The hash of the block that was signed.
    pub block_hash: Hash,
    /// The 64-byte Ed25519 signature.
    pub signature: [u8; 64],
}

/// A block together with its proposer signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedBlock {
    /// The block.
    pub block: Block,
    /// The proposer's signature over `block.block_hash()`.
    pub signature: BlockSignature,
}

impl BlockSignature {
    /// Sign `block_hash` as `validator` under `chain_id`.
    ///
    /// `seed` is the 32-byte Ed25519 seed whose derived public key is used
    /// for both signing and verification.
    pub fn sign_block(
        seed: &[u8; 32],
        validator: Address,
        block_hash: Hash,
        chain_id: &str,
    ) -> Self {
        let public_key = ed25519_public_key(seed);
        let payload = block_hash.as_bytes();
        let signature = sign(seed, chain_id, Domain::Block, payload);
        Self {
            validator,
            public_key,
            block_hash,
            signature,
        }
    }

    /// Verify this signature under `chain_id`.
    pub fn verify(&self, chain_id: &str) -> Result<(), ConsensusError> {
        verify(
            &self.public_key,
            chain_id,
            Domain::Block,
            self.block_hash.as_bytes(),
            &self.signature,
        )
        .map_err(|_| ConsensusError::InvalidSignature)
    }
}

impl SignedBlock {
    /// Build a signed block: construct the block, then sign it.
    pub fn sign(
        params: aura_block::HeaderParams,
        txs: Vec<aura_tx::Tx>,
        seed: &[u8; 32],
        validator: Address,
        chain_id: &str,
    ) -> Result<Self, ConsensusError> {
        let block = Block::new(params, txs).map_err(|_| ConsensusError::InvalidRound(0))?;
        let signature = BlockSignature::sign_block(seed, validator, block.block_hash(), chain_id);
        Ok(SignedBlock { block, signature })
    }

    /// Verify the block and its signature.
    pub fn verify(&self, chain_id: &str) -> Result<(), ConsensusError> {
        self.block
            .validate(chain_id)
            .map_err(|_| ConsensusError::InvalidRound(0))?;
        self.signature.verify(chain_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_types::{Address, Amount, Gas};

    fn validator(bytes: [u8; 20]) -> Address {
        Address::from_bytes(bytes)
    }

    fn seed_for(index: u8) -> [u8; 32] {
        let mut s = [0u8; 32];
        s[0] = index;
        s
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let validator = validator([42u8; 20]);
        let seed = seed_for(42);
        let block_hash = Hash::from_bytes([9u8; 32]);
        let sig = BlockSignature::sign_block(&seed, validator, block_hash, "auralite-test-1");
        assert_eq!(sig.validator, validator);
        assert_eq!(sig.block_hash, block_hash);
        assert!(sig.verify("auralite-test-1").is_ok());
    }

    #[test]
    fn wrong_chain_id_fails() {
        let validator = validator([42u8; 20]);
        let seed = seed_for(42);
        let block_hash = Hash::from_bytes([9u8; 32]);
        let sig = BlockSignature::sign_block(&seed, validator, block_hash, "auralite-test-1");
        assert!(sig.verify("other-chain").is_err());
    }

    #[test]
    fn wrong_block_hash_fails() {
        let validator = validator([42u8; 20]);
        let seed = seed_for(42);
        let block_hash = Hash::from_bytes([9u8; 32]);
        let sig = BlockSignature::sign_block(&seed, validator, block_hash, "auralite-test-1");
        let wrong_hash = Hash::from_bytes([8u8; 32]);
        let wrong_sig = BlockSignature {
            validator,
            public_key: sig.public_key,
            block_hash: wrong_hash,
            signature: sig.signature,
        };
        assert!(wrong_sig.verify("auralite-test-1").is_err());
    }

    #[test]
    fn signed_block_roundtrip() {
        let validator = validator([7u8; 20]);
        let seed = seed_for(7);
        let params = aura_block::HeaderParams {
            version: 1,
            chain_id: aura_types::ChainId::stage1(),
            height: aura_types::Height::new(1),
            parent_hash: Hash::ZERO,
            state_root: Hash::ZERO,
            validator_set_hash: Hash::ZERO,
            timestamp_ms: 1_000_000,
            proposer: validator,
            qc: None,
            gas_used: Gas::new(0),
            base_fee: Amount::ZERO,
        };
        let txs = Vec::new();
        let signed = SignedBlock::sign(params, txs, &seed, validator, "auralite-test-1")
            .expect("block construction must succeed");
        assert!(signed.verify("auralite-test-1").is_ok());
    }

    #[test]
    fn tampered_signature_fails() {
        let validator = validator([42u8; 20]);
        let seed = seed_for(42);
        let block_hash = Hash::from_bytes([9u8; 32]);
        let sig = BlockSignature::sign_block(&seed, validator, block_hash, "auralite-test-1");
        let mut tampered = sig.signature;
        tampered[0] ^= 0xff;
        let bad = BlockSignature {
            validator,
            public_key: sig.public_key,
            block_hash,
            signature: tampered,
        };
        assert!(bad.verify("auralite-test-1").is_err());
    }

    #[test]
    fn wrong_public_key_fails() {
        let validator = validator([42u8; 20]);
        let seed = seed_for(42);
        let block_hash = Hash::from_bytes([9u8; 32]);
        let sig = BlockSignature::sign_block(&seed, validator, block_hash, "auralite-test-1");
        let wrong_key = [0u8; 32];
        let bad = BlockSignature {
            validator,
            public_key: wrong_key,
            block_hash,
            signature: sig.signature,
        };
        assert!(bad.verify("auralite-test-1").is_err());
    }
}
