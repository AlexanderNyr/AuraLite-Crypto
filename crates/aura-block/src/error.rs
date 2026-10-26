//! Block validation errors.

use std::fmt;

use aura_codec::CodecError;
use aura_tx::TxError;
use aura_types::Hash;

/// Every reason a block can be rejected before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockError {
    /// Header `version` is not [`aura_types::PROTOCOL_VERSION`]. Tuple is
    /// `(expected, got)`.
    UnsupportedProtocolVersion(u32, u32),
    /// The block chain-id does not match the chain we validate for.
    /// Tuple is `(expected, got)`.
    WrongChain(String, String),
    /// Body has more than [`crate::MAX_BLOCK_TXS`] transactions.
    TooManyTransactions(usize),
    /// The QC blob exceeds [`crate::MAX_QC_BYTES`].
    QcTooLarge(usize),
    /// Two transactions share the same `(signer, nonce)` key.
    DuplicateTxKey,
    /// Body is not sorted by `(signer, nonce)`.
    UnsortedTransactions,
    /// `tx_root` does not match the Merkle root of the body. Tuple is
    /// `(computed, declared)`.
    TxRootMismatch(Hash, Hash),
    /// A transaction in the body failed validation.
    InvalidTransaction(TxError),
    /// Canonical decoding failed (structural).
    Decode(CodecError),
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedProtocolVersion(expected, got) => {
                write!(f, "unsupported protocol version {got}, expected {expected}")
            }
            Self::WrongChain(expected, got) => {
                write!(f, "block chain '{got}' is not '{expected}'")
            }
            Self::TooManyTransactions(n) => write!(f, "body has {n} transactions, over the limit"),
            Self::QcTooLarge(n) => write!(f, "QC blob of {n} bytes exceeds the limit"),
            Self::DuplicateTxKey => f.write_str("duplicate (signer, nonce) in block body"),
            Self::UnsortedTransactions => {
                f.write_str("body is not canonically sorted by (signer, nonce)")
            }
            Self::TxRootMismatch(computed, declared) => {
                write!(
                    f,
                    "tx_root mismatch: computed {}, declared {}",
                    computed, declared
                )
            }
            Self::InvalidTransaction(error) => write!(f, "invalid transaction: {error}"),
            Self::Decode(error) => write!(f, "canonical decode failed: {error}"),
        }
    }
}

impl std::error::Error for BlockError {}

impl From<CodecError> for BlockError {
    fn from(error: CodecError) -> Self {
        BlockError::Decode(error)
    }
}

impl From<TxError> for BlockError {
    fn from(error: TxError) -> Self {
        BlockError::InvalidTransaction(error)
    }
}

/// Errors produced by Merkle proof generation and verification (C6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MerkleError {
    /// Cannot prove or verify anything in an empty tree.
    EmptyTree,
    /// Leaf index is outside the tree. Tuple is `(index, total)`.
    IndexOutOfBounds(u64, u64),
    /// Proof has the wrong number of siblings. Tuple is `(got, expected)`.
    WrongSiblingCount(usize, usize),
    /// The leaf hash does not match the value being proven.
    LeafMismatch,
    /// The reconstructed root does not match the expected root.
    /// Tuple is `(computed, expected)`.
    RootMismatch(Hash, Hash),
    /// Canonical decoding failed (structural).
    Decode(CodecError),
}

impl fmt::Display for MerkleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTree => f.write_str("cannot prove an empty Merkle tree"),
            Self::IndexOutOfBounds(index, total) => {
                write!(f, "leaf index {index} is out of bounds for {total} leaves")
            }
            Self::WrongSiblingCount(got, expected) => {
                write!(f, "proof has {got} siblings, expected {expected}")
            }
            Self::LeafMismatch => f.write_str("proof leaf does not match the supplied value"),
            Self::RootMismatch(computed, expected) => {
                write!(
                    f,
                    "root mismatch: computed {}, expected {}",
                    computed, expected
                )
            }
            Self::Decode(error) => write!(f, "canonical decode failed: {error}"),
        }
    }
}

impl std::error::Error for MerkleError {}

impl From<CodecError> for MerkleError {
    fn from(error: CodecError) -> Self {
        MerkleError::Decode(error)
    }
}
