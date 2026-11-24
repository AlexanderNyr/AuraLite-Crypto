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
/// Rejects non-canonical `S` values first (H2 malleability check): RFC 8032
/// §5.1.7 requires `S < L`, but ed25519-dalek's ZIP215 verification accepts
/// `S >= L`, which malleates `(R, S)` into a second valid twin `(R, S+L)`.
///
/// NC target: `tools/nc_c3.py` replaces the verification expression with
/// `Ok(())` and requires the tampered-signature test to go red. H2 target:
/// `tools/nc_h2.py` NC-2 removes the canonical-`S` guard and requires
/// `H2: high-S signature must be rejected`.
pub fn verify_raw(
    public: &[u8; 32],
    message: &[u8],
    signature: &[u8; 64],
) -> Result<(), CryptoError> {
    if !s_is_canonical(signature) {
        return Err(CryptoError::NonCanonicalSignature);
    }
    let key = VerifyingKey::from_bytes(public).map_err(|_| CryptoError::InvalidPublicKey)?;
    let sig = Signature::from_bytes(signature);
    key.verify(message, &sig)
        .map_err(|_| CryptoError::VerificationFailed)
}

/// Order `L` of the Ed25519 prime-order group, little-endian
/// (`L = 2^252 + 27742317777372353535851937790883648493`).
const GROUP_ORDER_LE: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
];

/// RFC 8032 canonical check: the little-endian scalar `S` must satisfy
/// `S < L`. Byte-wise compare from the most-significant byte (index 31).
fn s_is_canonical(signature: &[u8; 64]) -> bool {
    let s = &signature[32..];
    for i in (0..32).rev() {
        if s[i] < GROUP_ORDER_LE[i] {
            return true;
        }
        if s[i] > GROUP_ORDER_LE[i] {
            return false;
        }
    }
    false // S == L is not canonical either
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

#[cfg(test)]
mod tests {
    use super::*;

    /// H2 review finding, demonstrated: the classic malleation of an Ed25519
    /// signature is the twin `(R, S+L)` — `S' = S + L` still satisfies the
    /// verification equation because `S' ≡ S (mod L)`. ed25519-dalek 2.2.0's
    /// default `check_scalar` (`Scalar::from_canonical_bytes`) rejects it,
    /// but its `legacy_compatibility` feature relaxes the bound to `S < 2^253`
    /// and reopens malleation (dalek's own comment: "permits signature
    /// malleability"). `verify_raw` therefore enforces `S < L` explicitly and
    /// independently, with a distinct `NonCanonicalSignature` error.
    #[test]
    fn high_s_twin_is_rejected_with_non_canonical_signature() {
        let seed = [7u8; 32];
        let message = b"h2 signature malleation demonstration";
        let signature = sign_raw(&seed, message);
        // Build the twin S' = S + L (no wraparound: S < L < 2^252).
        let mut twin = signature;
        let mut carry = 0u16;
        for i in 0..32 {
            let sum = u16::from(signature[32 + i]) + u16::from(GROUP_ORDER_LE[i]) + carry;
            twin[32 + i] = sum as u8;
            carry = sum >> 8;
        }
        assert_eq!(carry, 0, "S+L must fit in 256 bits");
        assert!(s_is_canonical(&signature), "fresh signatures are canonical");
        assert!(!s_is_canonical(&twin), "the S+L twin is not canonical");
        let public = ed25519_public_key(&seed);
        assert!(verify_raw(&public, message, &signature).is_ok());
        assert_eq!(
            verify_raw(&public, message, &twin),
            Err(CryptoError::NonCanonicalSignature),
            "the malleated twin must die on the canonical-S guard"
        );
    }
}

/// One item of a verification batch: public key, message, signature.
#[derive(Debug, Clone, Copy)]
pub struct BatchItem<'a> {
    /// The signer's 32-byte Ed25519 public key.
    pub public: &'a [u8; 32],
    /// The signed message bytes.
    pub message: &'a [u8],
    /// The 64-byte signature over `message`.
    pub signature: &'a [u8; 64],
}

/// Verify a batch of pure Ed25519 signatures (P1 batch verification).
///
/// The batch keeps the strict single-signature semantics of
/// [`verify_raw`] (per-item canonical-`S` guard first — H2 malleability,
/// pinned as this crate's contract regardless of backend behaviour),
/// then evaluates one batch equation over all items
/// (`ed25519_dalek::verify_batch`: Merlin transcript + coefficients,
/// complete and sound up to the standard 2^-128 batch soundness error).
/// On rejection the items are re-checked one by one so the caller
/// learns the first failing index (the batch diagnostic contract).
///
/// NC target (`tools/nc_p1.py`, `bad-batch`): reverting the per-item
/// index in the fallback scan makes the batch blame item 0 while the
/// sequential verifier reports the real culprit — the P1 equivalence
/// gate must go red with `P1: batch verify mismatch (item {n})`.
pub fn verify_batch_raw(items: &[BatchItem<'_>]) -> Result<(), (usize, CryptoError)> {
    if items.is_empty() {
        return Ok(());
    }
    for (i, item) in items.iter().enumerate() {
        if !s_is_canonical(item.signature) {
            return Err((i, CryptoError::NonCanonicalSignature));
        }
        if VerifyingKey::from_bytes(item.public).is_err() {
            return Err((i, CryptoError::InvalidPublicKey));
        }
    }
    let messages: Vec<&[u8]> = items.iter().map(|i| i.message).collect();
    let signatures: Vec<Signature> = items
        .iter()
        .map(|i| Signature::from_bytes(i.signature))
        .collect();
    let keys: Vec<VerifyingKey> = items
        .iter()
        .map(|i| VerifyingKey::from_bytes(i.public).expect("pre-checked above"))
        .collect();
    if ed25519_dalek::verify_batch(&messages, &signatures, &keys).is_ok() {
        return Ok(());
    }
    for (i, item) in items.iter().enumerate() {
        if let Err(e) = verify_raw(item.public, item.message, item.signature) {
            return Err((i, e));
        }
    }
    // The batch equation rejected a fully valid batch: report the first
    // item (unreachable except for the 2^-128 batch soundness error).
    Err((0, CryptoError::VerificationFailed))
}
