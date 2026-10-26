//! Crypto errors.

use std::fmt;

/// Errors produced by the hashing/signature API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoError {
    /// A secret seed was not exactly 32 bytes. Tuple is the actual length.
    InvalidSecretLength(usize),
    /// A public key could not be parsed.
    InvalidPublicKey,
    /// A signature could not be parsed.
    InvalidSignature,
    /// A prehash was not exactly 32 bytes. Tuple is the actual length.
    InvalidPrehashLength(usize),
    /// A secp256k1 scalar was zero or out of range.
    InvalidSecpSecret,
    /// Signature verification failed.
    VerificationFailed,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSecretLength(n) => write!(f, "secret seed must be 32 bytes, got {n}"),
            Self::InvalidPublicKey => f.write_str("invalid public key encoding"),
            Self::InvalidSignature => f.write_str("invalid signature encoding"),
            Self::InvalidPrehashLength(n) => write!(f, "prehash must be 32 bytes, got {n}"),
            Self::InvalidSecpSecret => f.write_str("invalid secp256k1 secret scalar"),
            Self::VerificationFailed => f.write_str("signature verification failed"),
        }
    }
}

impl std::error::Error for CryptoError {}
