//! State key and account errors.

use std::fmt;

use aura_codec::CodecError;

use crate::key::KeySpace;

/// Every reason a state key or account operation can be rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// The key prefix is not one of the five protocol namespaces.
    ReservedKeySpace(u8),
    /// The key has no bytes at all.
    EmptyKey,
    /// The key is longer than [`crate::MAX_STATE_KEY_LEN`].
    KeyTooLong(usize),
    /// Known prefix but wrong total length for that namespace.
    InvalidKeyLength {
        /// Namespace decoded from the prefix.
        space: KeySpace,
        /// Required total length, including the prefix byte.
        expected: usize,
        /// Actual length.
        got: usize,
    },
    /// A contract tried to write outside its own storage namespace.
    ForbiddenContractWrite,
    /// A contract tried to write into another contract's storage.
    WrongContractStorage,
    /// Re-encoding a decoded key did not reproduce the input bytes.
    NonCanonicalKeyEncoding,
    /// Canonical decoding failed (structural).
    Decode(CodecError),
    /// Balance addition overflowed `u128`.
    BalanceOverflow,
    /// Balance subtraction would go below zero.
    InsufficientBalance,
    /// Nonce increment overflowed `u64`.
    NonceOverflow,
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReservedKeySpace(prefix) => {
                write!(f, "reserved key space prefix 0x{prefix:02x}")
            }
            Self::EmptyKey => f.write_str("empty state key"),
            Self::KeyTooLong(n) => write!(f, "state key of {n} bytes exceeds the limit"),
            Self::InvalidKeyLength {
                space,
                expected,
                got,
            } => write!(f, "{space:?} key must be {expected} bytes, got {got}"),
            Self::ForbiddenContractWrite => {
                f.write_str("contract may only write to its own storage namespace")
            }
            Self::WrongContractStorage => {
                f.write_str("contract tried to write another contract's storage")
            }
            Self::NonCanonicalKeyEncoding => {
                f.write_str("re-encoding did not reproduce the key bytes")
            }
            Self::Decode(error) => write!(f, "canonical decode failed: {error}"),
            Self::BalanceOverflow => f.write_str("balance overflow"),
            Self::InsufficientBalance => f.write_str("insufficient balance"),
            Self::NonceOverflow => f.write_str("nonce overflow"),
        }
    }
}

impl std::error::Error for StateError {}

impl From<CodecError> for StateError {
    fn from(error: CodecError) -> Self {
        StateError::Decode(error)
    }
}
