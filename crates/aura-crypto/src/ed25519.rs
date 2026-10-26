//! Ed25519 signatures (RFC 8032) and domain-separated signing.

use aura_types::Domain;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::error::CryptoError;
use crate::hash::hash_domain;

/// Derive the 32-byte public key for a 32-byte seed.
pub fn ed25519_public_key(seed: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

/// Sign an arbitrary message with pure Ed25519 (RFC 8032).
///
/// This is the low-level primitive used for the RFC 8032 golden vectors.
/// Protocol code should prefer [`sign`], which adds domain separation.
pub fn sign_raw(seed: &[u8; 32], message: &[u8]) -> [u8; 64] {
    SigningKey::from_bytes(seed).sign(message).to_bytes()
}

/// Verify a pure Ed25519 signature.
///
/// NC target: `tools/nc_c3.py` replaces the verification expression with
/// `Ok(())` and requires the tampered-signature test to go red.
pub fn verify_raw(
    public: &[u8; 32],
    message: &[u8],
    signature: &[u8; 64],
) -> Result<(), CryptoError> {
    let key = VerifyingKey::from_bytes(public).map_err(|_| CryptoError::InvalidPublicKey)?;
    let sig = Signature::from_bytes(signature);
    key.verify(message, &sig)
        .map_err(|_| CryptoError::VerificationFailed)
}

/// Sign `payload` under `(chain_id, domain)`.
///
/// The signed bytes are the 32-byte domain-separated BLAKE3 digest, so the
/// signature covers the chain, the purpose and the exact payload length.
pub fn sign(seed: &[u8; 32], chain_id: &str, domain: Domain, payload: &[u8]) -> [u8; 64] {
    let digest = hash_domain(chain_id, domain, payload);
    sign_raw(seed, digest.as_bytes())
}

/// Verify a domain-separated signature produced by [`sign`].
pub fn verify(
    public: &[u8; 32],
    chain_id: &str,
    domain: Domain,
    payload: &[u8],
    signature: &[u8; 64],
) -> Result<(), CryptoError> {
    let digest = hash_domain(chain_id, domain, payload);
    verify_raw(public, digest.as_bytes(), signature)
}
