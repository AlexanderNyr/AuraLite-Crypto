//! Decoding errors.

use std::fmt;

/// Every way a byte string can fail to be a canonical encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    /// Not enough bytes remain. Tuple is `(needed, available)`.
    UnexpectedEof(usize, usize),
    /// Bytes remain after a complete value was decoded.
    TrailingBytes(usize),
    /// A varint used more bytes than the minimal encoding.
    NonCanonicalVarint,
    /// A varint exceeded 10 bytes or overflowed `u64`.
    VarintOverflow,
    /// A declared byte length exceeded [`crate::MAX_LENGTH`].
    LengthTooLarge(usize),
    /// A declared element count exceeded [`crate::MAX_ITEMS`].
    TooManyItems(usize),
    /// A boolean byte was neither `0x00` nor `0x01`.
    InvalidBool(u8),
    /// An `Option` tag was neither `0x00` nor `0x01`.
    InvalidOptionTag(u8),
    /// A `Domain` tag is not one of the five known tags.
    InvalidDomainTag(u8),
    /// A `Denom` tag is not `0` or `1`.
    InvalidDenomTag(u8),
    /// An enum discriminant is unknown. Tuple is `(type name, tag)`.
    ///
    /// Added in C4 for caller-defined enums (`TxKind`, `SignatureAlgo`).
    /// Additive: no change to the C2 wire format or its golden vectors.
    InvalidEnumTag(&'static str, u8),
    /// Unsupported wire version. Tuple is `(expected, got)`.
    UnsupportedVersion(u8, u8),
    /// A length-prefixed string was not valid UTF-8.
    InvalidUtf8,
    /// A 32-byte hash could not be constructed.
    InvalidHash,
    /// A 20-byte address could not be constructed.
    InvalidAddress,
    /// A decoded chain-id failed `ChainId::new` validation.
    InvalidChainId,
    /// Map keys were not strictly ascending.
    NonCanonicalMapOrder,
    /// A map contained the same key twice.
    DuplicateMapKey,
    /// Re-encoding a decoded value did not reproduce the input bytes.
    NonCanonicalEncoding,
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof(needed, available) => {
                write!(f, "unexpected EOF: need {needed} bytes, have {available}")
            }
            Self::TrailingBytes(n) => write!(f, "{n} trailing byte(s) after decoded value"),
            Self::NonCanonicalVarint => f.write_str("non-minimal varint"),
            Self::VarintOverflow => f.write_str("varint overflow"),
            Self::LengthTooLarge(n) => write!(f, "declared length {n} exceeds MAX_LENGTH"),
            Self::TooManyItems(n) => write!(f, "declared item count {n} exceeds MAX_ITEMS"),
            Self::InvalidBool(v) => write!(f, "invalid bool byte 0x{v:02x}"),
            Self::InvalidOptionTag(v) => write!(f, "invalid Option tag 0x{v:02x}"),
            Self::InvalidDomainTag(v) => write!(f, "invalid Domain tag 0x{v:02x}"),
            Self::InvalidDenomTag(v) => write!(f, "invalid Denom tag 0x{v:02x}"),
            Self::InvalidEnumTag(name, v) => write!(f, "invalid {name} tag 0x{v:02x}"),
            Self::UnsupportedVersion(expected, got) => {
                write!(f, "unsupported codec version {got}, expected {expected}")
            }
            Self::InvalidUtf8 => f.write_str("invalid UTF-8 in length-prefixed string"),
            Self::InvalidHash => f.write_str("could not construct 32-byte Hash"),
            Self::InvalidAddress => f.write_str("could not construct 20-byte Address"),
            Self::InvalidChainId => f.write_str("decoded chain-id failed validation"),
            Self::NonCanonicalMapOrder => f.write_str("map keys are not strictly ascending"),
            Self::DuplicateMapKey => f.write_str("duplicate map key"),
            Self::NonCanonicalEncoding => {
                f.write_str("re-encoding did not reproduce the input bytes")
            }
        }
    }
}

impl std::error::Error for CodecError {}
