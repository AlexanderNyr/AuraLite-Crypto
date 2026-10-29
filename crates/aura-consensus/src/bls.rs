//! BLS12-381 signatures (B2).
//!
//! Threshold signatures over the BLS12-381 curve using the [`blst`]
//! bindings. A validator signs a vote message; signatures for the same
//! message can be verified against a set of public keys, and a Quorum
//! Certificate (QC) is formed from 2f+1 distinct validators' signatures.
//!
//! All signing is domain-separated by `(chain_id, domain, message)` via
//! [`aura_crypto::hash_domain`], so a BLS signature for one chain or
//! purpose cannot be reused elsewhere.

use aura_crypto::hash_domain;
use aura_types::{Address, Domain};

use blst::min_pk::{PublicKey, SecretKey, Signature};
use std::fmt;

/// A BLS12-381 secret key, derived from a 32-byte seed.
#[derive(Debug, Clone)]
pub struct BlsSecretKey(SecretKey);

impl BlsSecretKey {
    /// Derive a secret key from a 32-byte seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let sk = SecretKey::key_gen(seed, &[]).expect("BLS key generation must succeed");
        Self(sk)
    }

    /// The corresponding public key.
    pub fn public_key(&self) -> BlsPublicKey {
        BlsPublicKey(self.0.sk_to_pk())
    }

    /// Sign `message` under `(chain_id, domain)`.
    pub fn sign(&self, chain_id: &str, domain: Domain, message: &[u8]) -> BlsSignature {
        let digest = hash_domain(chain_id, domain, message);
        let sig = self.0.sign(
            digest.as_bytes(),
            b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_",
            &[],
        );
        BlsSignature(sig)
    }
}

/// A BLS12-381 public key (compressed, 48 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlsPublicKey(PublicKey);

impl BlsPublicKey {
    /// The compressed 48-byte encoding.
    pub fn to_bytes(&self) -> [u8; 48] {
        self.0.to_bytes()
    }

    /// Decode from compressed bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BlsError> {
        if bytes.len() != 48 {
            return Err(BlsError::InvalidPublicKey);
        }
        PublicKey::from_bytes(bytes)
            .map(Self)
            .map_err(|_| BlsError::InvalidPublicKey)
    }
}

impl fmt::Display for BlsPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.to_bytes();
        write!(f, "BlsPublicKey(")?;
        for b in &bytes {
            write!(f, "{b:02x}")?;
        }
        write!(f, ")")
    }
}

/// A BLS12-381 signature (compressed, 96 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlsSignature(Signature);

impl BlsSignature {
    /// The compressed 96-byte encoding.
    pub fn to_bytes(&self) -> [u8; 96] {
        self.0.to_bytes()
    }

    /// Decode from compressed bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BlsError> {
        if bytes.len() != 96 {
            return Err(BlsError::InvalidSignature);
        }
        Signature::from_bytes(bytes)
            .map(Self)
            .map_err(|_| BlsError::InvalidSignature)
    }
}

/// Errors produced by BLS operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlsError {
    /// A public key was not a valid BLS12-381 point.
    InvalidPublicKey,
    /// A signature was not a valid BLS12-381 signature.
    InvalidSignature,
    /// Signature verification failed.
    VerificationFailed,
}

impl fmt::Display for BlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPublicKey => f.write_str("invalid BLS public key"),
            Self::InvalidSignature => f.write_str("invalid BLS signature"),
            Self::VerificationFailed => f.write_str("BLS signature verification failed"),
        }
    }
}

impl std::error::Error for BlsError {}

/// Verify a BLS signature under `(chain_id, domain)`.
pub fn verify(
    public_key: &BlsPublicKey,
    chain_id: &str,
    domain: Domain,
    message: &[u8],
    signature: &BlsSignature,
) -> Result<(), BlsError> {
    let digest = hash_domain(chain_id, domain, message);
    let dst = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";
    let err = signature
        .0
        .verify(true, digest.as_bytes(), dst, &[], &public_key.0, true);
    if err == blst::BLST_ERROR::BLST_SUCCESS {
        Ok(())
    } else {
        Err(BlsError::VerificationFailed)
    }
}

/// Derive a 20-byte validator address from a BLS public key.
pub fn address_from_public_key(public_key: &BlsPublicKey) -> Address {
    let digest = hash_domain(
        "address-from-pubkey",
        Domain::Genesis,
        &public_key.to_bytes(),
    );
    let bytes = digest.as_bytes();
    Address::from_bytes(bytes[0..20].try_into().unwrap_or([0u8; 20]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_types::Address;

    fn seed_for(index: u8) -> [u8; 32] {
        let mut s = [0u8; 32];
        s[0] = index;
        s
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let sk = BlsSecretKey::from_seed(&seed_for(1));
        let pk = sk.public_key();
        let message = b"hello BLS";
        let sig = sk.sign("auralite-test-1", Domain::Vote, message);
        assert!(verify(&pk, "auralite-test-1", Domain::Vote, message, &sig).is_ok());
    }

    #[test]
    fn wrong_chain_id_fails() {
        let sk = BlsSecretKey::from_seed(&seed_for(1));
        let pk = sk.public_key();
        let message = b"hello BLS";
        let sig = sk.sign("auralite-test-1", Domain::Vote, message);
        assert!(verify(&pk, "other-chain", Domain::Vote, message, &sig).is_err());
    }

    #[test]
    fn wrong_domain_fails() {
        let sk = BlsSecretKey::from_seed(&seed_for(1));
        let pk = sk.public_key();
        let message = b"hello BLS";
        let sig = sk.sign("auralite-test-1", Domain::Vote, message);
        assert!(verify(&pk, "auralite-test-1", Domain::Block, message, &sig).is_err());
    }

    #[test]
    fn tampered_message_fails() {
        let sk = BlsSecretKey::from_seed(&seed_for(1));
        let pk = sk.public_key();
        let sig = sk.sign("auralite-test-1", Domain::Vote, b"original");
        assert!(verify(&pk, "auralite-test-1", Domain::Vote, b"tampered", &sig).is_err());
    }

    #[test]
    fn wrong_public_key_fails() {
        let sk = BlsSecretKey::from_seed(&seed_for(1));
        let message = b"hello BLS";
        let sig = sk.sign("auralite-test-1", Domain::Vote, message);
        let wrong_pk = BlsSecretKey::from_seed(&seed_for(2)).public_key();
        assert!(verify(&wrong_pk, "auralite-test-1", Domain::Vote, message, &sig).is_err());
    }

    #[test]
    fn address_from_public_key_is_deterministic() {
        let sk = BlsSecretKey::from_seed(&seed_for(42));
        let pk = sk.public_key();
        let addr1 = address_from_public_key(&pk);
        let addr2 = address_from_public_key(&pk);
        assert_eq!(addr1, addr2);
        assert_ne!(addr1, Address::ZERO);
    }

    #[test]
    fn different_seeds_different_addresses() {
        let pk1 = BlsSecretKey::from_seed(&seed_for(1)).public_key();
        let pk2 = BlsSecretKey::from_seed(&seed_for(2)).public_key();
        assert_ne!(address_from_public_key(&pk1), address_from_public_key(&pk2));
    }
}
