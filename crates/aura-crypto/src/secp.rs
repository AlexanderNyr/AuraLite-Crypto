//! secp256k1 deterministic ECDSA (RFC 6979) for compatibility.

use k256::ecdsa::signature::hazmat::{PrehashSigner, PrehashVerifier};
use k256::ecdsa::{Signature, SigningKey, VerifyingKey};

use crate::error::CryptoError;

/// Derive the 33-byte SEC1 compressed public key for a 32-byte seed.
pub fn secp_public_key(seed: &[u8; 32]) -> Result<Vec<u8>, CryptoError> {
    let key = SigningKey::from_bytes(seed.into()).map_err(|_| CryptoError::InvalidSecpSecret)?;
    Ok(key
        .verifying_key()
        .to_encoded_point(true)
        .as_bytes()
        .to_vec())
}

/// Sign a 32-byte prehash with deterministic ECDSA (RFC 6979 / SHA-256).
///
/// `k256` produces low-S signatures, matching the independent pure-Python
/// reference in `tools/gen_c3_vectors.py`.
pub fn sign_prehash_raw(seed: &[u8; 32], prehash: &[u8; 32]) -> Result<[u8; 64], CryptoError> {
    let key = SigningKey::from_bytes(seed.into()).map_err(|_| CryptoError::InvalidSecpSecret)?;
    let signature: Signature = key
        .sign_prehash(prehash)
        .map_err(|_| CryptoError::InvalidSignature)?;
    let bytes = signature.to_bytes();
    let mut out = [0u8; 64];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Verify a deterministic ECDSA signature over a 32-byte prehash.
pub fn verify_prehash_raw(
    public: &[u8],
    prehash: &[u8; 32],
    signature: &[u8; 64],
) -> Result<(), CryptoError> {
    let key = VerifyingKey::from_sec1_bytes(public).map_err(|_| CryptoError::InvalidPublicKey)?;
    let sig = Signature::from_slice(signature).map_err(|_| CryptoError::InvalidSignature)?;
    key.verify_prehash(prehash, &sig)
        .map_err(|_| CryptoError::VerificationFailed)
}
