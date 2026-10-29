//! Consensus error types.
//!
//! B1 introduces the first real consensus code: a deterministic round-robin
//! proposer and a simple block signature. These can fail in well-defined
//! ways — wrong validator, bad signature, empty validator set — and each
//! failure is a distinct error so callers can distinguish them.

use std::fmt;

/// Errors produced by the B1 consensus primitives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsensusError {
    /// The validator set is empty — no proposer can be selected.
    EmptyValidatorSet,
    /// The requested round is not valid for this validator set.
    InvalidRound(u64),
    /// The address is not a member of the validator set.
    NotAValidator,
    /// The signature does not verify under the validator's public key.
    InvalidSignature,
    /// The block hash does not match the signature payload.
    BlockHashMismatch,
    /// The signature was produced by a different validator.
    WrongSigner,
}

impl fmt::Display for ConsensusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyValidatorSet => f.write_str("empty validator set"),
            Self::InvalidRound(round) => write!(f, "invalid round {round}"),
            Self::NotAValidator => f.write_str("address is not a validator"),
            Self::InvalidSignature => f.write_str("signature verification failed"),
            Self::BlockHashMismatch => f.write_str("block hash does not match signature payload"),
            Self::WrongSigner => f.write_str("signature was produced by a different validator"),
        }
    }
}

impl std::error::Error for ConsensusError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_is_stable() {
        assert_eq!(
            ConsensusError::EmptyValidatorSet.to_string(),
            "empty validator set"
        );
        assert_eq!(
            ConsensusError::InvalidRound(42).to_string(),
            "invalid round 42"
        );
        assert_eq!(
            ConsensusError::NotAValidator.to_string(),
            "address is not a validator"
        );
        assert_eq!(
            ConsensusError::InvalidSignature.to_string(),
            "signature verification failed"
        );
        assert_eq!(
            ConsensusError::BlockHashMismatch.to_string(),
            "block hash does not match signature payload"
        );
        assert_eq!(
            ConsensusError::WrongSigner.to_string(),
            "signature was produced by a different validator"
        );
    }
}
