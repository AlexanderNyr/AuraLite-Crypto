//! Small canonical scalar types and domain separation.

use std::collections::BTreeMap;
use std::fmt;

/// The only map type allowed in consensus data structures.
///
/// `BTreeMap` iteration is ordered and therefore deterministic; `HashMap`
/// iteration is not. Rule D3 in `ROADMAP.md`.
pub type CanonicalMap<K, V> = BTreeMap<K, V>;

/// Errors produced by the canonical scalar types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveError {
    /// chain-id length is outside `3..=64`.
    ChainIdLength(usize),
    /// chain-id contains a character outside `[a-z0-9-]`.
    ChainIdCharset,
    /// chain-id starts or ends with `-`.
    ChainIdEdge,
    /// Nonce increment would overflow `u64`.
    NonceOverflow,
    /// Height increment would overflow `u64`.
    HeightOverflow,
    /// Height subtraction would underflow.
    HeightUnderflow,
    /// Gas arithmetic would overflow `u64`.
    GasOverflow,
    /// Gas subtraction would underflow.
    GasUnderflow,
}

impl fmt::Display for PrimitiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChainIdLength(n) => write!(f, "chain-id length {n} outside 3..=64"),
            Self::ChainIdCharset => f.write_str("chain-id must match [a-z0-9-]"),
            Self::ChainIdEdge => f.write_str("chain-id cannot start or end with '-'"),
            Self::NonceOverflow => f.write_str("nonce overflow"),
            Self::HeightOverflow => f.write_str("height overflow"),
            Self::HeightUnderflow => f.write_str("height underflow"),
            Self::GasOverflow => f.write_str("gas overflow"),
            Self::GasUnderflow => f.write_str("gas underflow"),
        }
    }
}

impl std::error::Error for PrimitiveError {}

/// A validated chain identifier, e.g. `auralite-test-1`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainId(String);

impl ChainId {
    /// Validate and construct a chain-id.
    pub fn new(raw: &str) -> Result<Self, PrimitiveError> {
        if !(3..=64).contains(&raw.len()) {
            return Err(PrimitiveError::ChainIdLength(raw.len()));
        }
        if !raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(PrimitiveError::ChainIdCharset);
        }
        if raw.starts_with('-') || raw.ends_with('-') {
            return Err(PrimitiveError::ChainIdEdge);
        }
        Ok(ChainId(raw.to_owned()))
    }

    /// The locked Stage 1 chain-id from C0.
    pub fn stage1() -> Self {
        ChainId(crate::CHAIN_ID.to_owned())
    }

    /// Borrow the identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChainId({})", self.0)
    }
}

/// Account sequence number.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Nonce(u64);

impl Nonce {
    /// Construct from a raw value.
    pub const fn new(value: u64) -> Self {
        Nonce(value)
    }

    /// Read the raw value.
    pub const fn value(self) -> u64 {
        self.0
    }

    /// The next nonce, or `None` on overflow.
    pub fn checked_next(self) -> Result<Self, PrimitiveError> {
        self.0
            .checked_add(1)
            .map(Nonce)
            .ok_or(PrimitiveError::NonceOverflow)
    }
}

impl fmt::Debug for Nonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Nonce({})", self.0)
    }
}

/// Block height.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Height(u64);

impl Height {
    /// Construct from a raw value.
    pub const fn new(value: u64) -> Self {
        Height(value)
    }

    /// Read the raw value.
    pub const fn value(self) -> u64 {
        self.0
    }

    /// The next height, or an overflow error.
    pub fn checked_next(self) -> Result<Self, PrimitiveError> {
        self.0
            .checked_add(1)
            .map(Height)
            .ok_or(PrimitiveError::HeightOverflow)
    }

    /// Checked subtraction of a height delta.
    pub fn checked_sub(self, delta: u64) -> Result<Self, PrimitiveError> {
        self.0
            .checked_sub(delta)
            .map(Height)
            .ok_or(PrimitiveError::HeightUnderflow)
    }
}

impl fmt::Debug for Height {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Height({})", self.0)
    }
}

/// Gas amount. Always checked arithmetic — gas is a consensus value.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Gas(u64);

impl Gas {
    /// Construct from a raw value.
    pub const fn new(value: u64) -> Self {
        Gas(value)
    }

    /// Read the raw value.
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Checked addition.
    pub fn checked_add(self, rhs: u64) -> Result<Self, PrimitiveError> {
        self.0
            .checked_add(rhs)
            .map(Gas)
            .ok_or(PrimitiveError::GasOverflow)
    }

    /// Checked subtraction.
    pub fn checked_sub(self, rhs: u64) -> Result<Self, PrimitiveError> {
        self.0
            .checked_sub(rhs)
            .map(Gas)
            .ok_or(PrimitiveError::GasUnderflow)
    }

    /// Checked multiplication.
    pub fn checked_mul(self, factor: u64) -> Result<Self, PrimitiveError> {
        self.0
            .checked_mul(factor)
            .map(Gas)
            .ok_or(PrimitiveError::GasOverflow)
    }
}

impl fmt::Debug for Gas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Gas({})", self.0)
    }
}

/// Signature domain. Rule D5: `chain-id || domain || payload`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Domain {
    /// A user transaction signature.
    Tx,
    /// A consensus vote signature.
    Vote,
    /// A block header signature.
    Block,
    /// A contract-originated signature.
    Contract,
    /// A genesis / validator-set signature.
    Genesis,
}

impl Domain {
    /// All domains, in a fixed order.
    pub const ALL: [Domain; 5] = [
        Domain::Tx,
        Domain::Vote,
        Domain::Block,
        Domain::Contract,
        Domain::Genesis,
    ];

    /// Stable byte tag for this domain.
    pub const fn tag(&self) -> &'static str {
        match self {
            Domain::Tx => "tx",
            Domain::Vote => "vote",
            Domain::Block => "block",
            Domain::Contract => "contract",
            Domain::Genesis => "genesis",
        }
    }

    /// Build the `chain-id || 0 || domain || 0` prefix for signing.
    ///
    /// C3 concatenates the payload and hashes it. Keeping the byte layout here
    /// means no consumer can silently invent a different separation.
    pub fn prefix(&self, chain_id: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(chain_id.len() + self.tag().len() + 2);
        out.extend_from_slice(chain_id.as_bytes());
        out.push(0);
        out.extend_from_slice(self.tag().as_bytes());
        out.push(0);
        out
    }
}
