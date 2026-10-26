//! Transaction validation errors.

use std::fmt;

use aura_codec::CodecError;

/// Every reason a transaction can be rejected before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxError {
    /// `codec_version` is not [`aura_codec::CODEC_VERSION`]. Tuple is
    /// `(expected, got)`.
    UnsupportedCodecVersion(u8, u8),
    /// The tx chain-id does not match the chain we are validating for.
    /// Tuple is `(expected, got)`.
    WrongChain(String, String),
    /// Payload exceeds [`crate::MAX_TX_PAYLOAD`].
    PayloadTooLarge(usize),
    /// `gas_limit` is zero.
    ZeroGasLimit,
    /// `tip` is greater than `max_fee`.
    TipExceedsMaxFee,
    /// The tx has expired. Tuple is `(expiry_height, current_height)`.
    Expired(u64, u64),
    /// `signer` does not match the address derived from the public key.
    SignerMismatch,
    /// Ed25519 verification failed.
    InvalidSignature,
    /// Canonical decoding failed (structural).
    Decode(CodecError),
}

impl fmt::Display for TxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCodecVersion(expected, got) => {
                write!(f, "unsupported tx codec version {got}, expected {expected}")
            }
            Self::WrongChain(expected, got) => {
                write!(f, "replay rejected: chain '{got}' is not '{expected}'")
            }
            Self::PayloadTooLarge(n) => write!(f, "payload of {n} bytes exceeds the limit"),
            Self::ZeroGasLimit => f.write_str("gas_limit must be non-zero"),
            Self::TipExceedsMaxFee => f.write_str("tip exceeds max_fee"),
            Self::Expired(expiry, height) => {
                write!(f, "tx expired at height {expiry}, current height {height}")
            }
            Self::SignerMismatch => f.write_str("signer does not match the public key"),
            Self::InvalidSignature => f.write_str("invalid transaction signature"),
            Self::Decode(error) => write!(f, "canonical decode failed: {error}"),
        }
    }
}

impl std::error::Error for TxError {}

impl From<CodecError> for TxError {
    fn from(error: CodecError) -> Self {
        TxError::Decode(error)
    }
}
