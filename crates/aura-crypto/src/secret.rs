//! Zeroizing secret storage and constant-time comparison.

use std::fmt;

use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::error::CryptoError;

/// A 32-byte secret that is zeroed on drop and never printed.
///
/// `Zeroizing<[u8; 32]>` wipes the buffer when this value leaves scope.
/// `Debug` is deliberately redacted so a stray `{:?}` in a log cannot leak a
/// key. This is tested, not assumed.
pub struct SecretBytes(Zeroizing<[u8; 32]>);

impl SecretBytes {
    /// Construct from exactly 32 bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidSecretLength(bytes.len()));
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(bytes);
        Ok(SecretBytes(Zeroizing::new(array)))
    }

    /// Borrow the secret bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes([REDACTED])")
    }
}

/// Constant-time equality of two byte slices.
///
/// Different lengths return `false` immediately (length is not secret);
/// equal-length comparisons use `subtle::ConstantTimeEq`.
pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    bool::from(left.ct_eq(right))
}
