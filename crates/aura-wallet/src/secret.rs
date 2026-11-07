//! Redacted, zeroizing secret containers for W1.

use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A 64-byte BIP-39 seed. Never logged, never serialized in plaintext.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretSeed(pub(crate) [u8; 64]);

impl SecretSeed {
    /// Wrap a seed, taking ownership of the bytes.
    pub fn new(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    /// Borrow the seed bytes for local derivation only.
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

impl fmt::Debug for SecretSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretSeed([REDACTED])")
    }
}

impl fmt::Display for SecretSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// A 32-byte Ed25519 signing key.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey32(pub(crate) [u8; 32]);

impl SecretKey32 {
    /// Borrow the key bytes for local signing only.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SecretKey32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey32([REDACTED])")
    }
}

impl fmt::Display for SecretKey32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// A keystore password. Zeroized on drop and redacted in all formatting.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Password(String);

impl Password {
    /// Wrap a password string.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the password for KDF input only.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password([REDACTED])")
    }
}

impl fmt::Display for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}
