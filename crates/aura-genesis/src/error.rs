//! Genesis parsing and validation errors.

use std::fmt;

use aura_codec::CodecError;

/// Every reason a genesis document can be rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenesisError {
    /// The input is not valid JSON.
    InvalidJson(String),
    /// An unknown key was found in the top-level object.
    UnknownField(String),
    /// An unknown key was found in a validator/allocation object.
    UnknownNestedField(String),
    /// A required key is missing.
    MissingField(&'static str),
    /// A field has the wrong JSON type.
    WrongType(&'static str),
    /// `chain_id` is not a valid canonical chain identifier.
    InvalidChainId(String),
    /// Stage 1 only accepts the testnet chain-id.
    WrongChain(String),
    /// `protocol_version` is not the current protocol version.
    UnsupportedProtocolVersion(u32),
    /// An address is not valid bech32.
    InvalidAddress(String),
    /// A validator public key is not 32 bytes of hex.
    InvalidPublicKey(String),
    /// A balance is not a decimal `u128` string.
    InvalidBalance(String),
    /// The validator set is empty.
    EmptyValidatorSet,
    /// A validator has zero voting power.
    ZeroValidatorPower,
    /// Two validators share an address.
    DuplicateValidator(String),
    /// Two allocations share an address.
    DuplicateAllocation(String),
    /// An allocation has a zero balance.
    ZeroAllocation,
    /// The sum of all allocations overflows `u128`.
    SupplyOverflow,
    /// Canonical binary decoding failed.
    Decode(CodecError),
}

impl fmt::Display for GenesisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(e) => write!(f, "invalid genesis JSON: {e}"),
            Self::UnknownField(k) => write!(f, "unknown genesis field '{k}'"),
            Self::UnknownNestedField(k) => write!(f, "unknown nested field '{k}'"),
            Self::MissingField(k) => write!(f, "missing genesis field '{k}'"),
            Self::WrongType(k) => write!(f, "wrong type for '{k}'"),
            Self::InvalidChainId(v) => write!(f, "invalid chain-id '{v}'"),
            Self::WrongChain(v) => write!(f, "stage 1 rejects chain-id '{v}'"),
            Self::UnsupportedProtocolVersion(v) => {
                write!(f, "unsupported protocol version {v}")
            }
            Self::InvalidAddress(v) => write!(f, "invalid bech32 address '{v}'"),
            Self::InvalidPublicKey(v) => write!(f, "invalid public key '{v}'"),
            Self::InvalidBalance(v) => write!(f, "invalid balance '{v}'"),
            Self::EmptyValidatorSet => f.write_str("genesis validator set is empty"),
            Self::ZeroValidatorPower => f.write_str("validator has zero power"),
            Self::DuplicateValidator(v) => write!(f, "duplicate validator '{v}'"),
            Self::DuplicateAllocation(v) => write!(f, "duplicate allocation '{v}'"),
            Self::ZeroAllocation => f.write_str("allocation has a zero balance"),
            Self::SupplyOverflow => f.write_str("total allocation supply overflows u128"),
            Self::Decode(e) => write!(f, "canonical decode failed: {e}"),
        }
    }
}

impl std::error::Error for GenesisError {}

impl From<CodecError> for GenesisError {
    fn from(error: CodecError) -> Self {
        GenesisError::Decode(error)
    }
}
