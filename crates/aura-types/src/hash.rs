//! The 32-byte [`Hash`] newtype.

use std::fmt;

/// Length of a canonical hash in bytes.
pub const HASH_LEN: usize = 32;

/// Errors produced when decoding a [`Hash`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashError {
    /// A byte slice was not exactly [`HASH_LEN`] bytes long.
    InvalidByteLength(usize),
    /// A hex string was not exactly `2 * HASH_LEN` characters long.
    InvalidHexLength(usize),
    /// A hex string contained a character outside `[0-9a-fA-F]`.
    InvalidHexCharacter,
}

impl fmt::Display for HashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidByteLength(n) => write!(f, "expected {HASH_LEN} bytes, got {n}"),
            Self::InvalidHexLength(n) => write!(f, "expected {} hex chars, got {n}", 2 * HASH_LEN),
            Self::InvalidHexCharacter => f.write_str("invalid hex character"),
        }
    }
}

impl std::error::Error for HashError {}

/// A canonical 32-byte hash value.
///
/// This is a plain newtype: the actual hash function is `aura-crypto` (C3).
/// Keeping the type here lets tx/block/state formats name their hashes in C1
/// without depending on a crypto implementation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Hash([u8; HASH_LEN]);

impl Hash {
    /// The all-zero hash.
    pub const ZERO: Hash = Hash([0u8; HASH_LEN]);

    /// Wrap a fixed-size byte array.
    pub const fn from_bytes(bytes: [u8; HASH_LEN]) -> Self {
        Hash(bytes)
    }

    /// Decode from a byte slice of exactly [`HASH_LEN`] bytes.
    pub fn from_slice(slice: &[u8]) -> Result<Self, HashError> {
        let bytes: [u8; HASH_LEN] = slice
            .try_into()
            .map_err(|_| HashError::InvalidByteLength(slice.len()))?;
        Ok(Hash(bytes))
    }

    /// Borrow the raw bytes.
    pub const fn as_bytes(&self) -> &[u8; HASH_LEN] {
        &self.0
    }

    /// Consume the hash and return the raw bytes.
    pub const fn into_bytes(self) -> [u8; HASH_LEN] {
        self.0
    }

    /// Lowercase hex representation (64 characters).
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(2 * HASH_LEN);
        for byte in &self.0 {
            out.push(char::from_digit((byte >> 4) as u32, 16).expect("nibble < 16"));
            out.push(char::from_digit((byte & 0x0f) as u32, 16).expect("nibble < 16"));
        }
        out
    }

    /// Parse a 64-character hex string (upper or lower case).
    pub fn from_hex(text: &str) -> Result<Self, HashError> {
        if text.len() != 2 * HASH_LEN {
            return Err(HashError::InvalidHexLength(text.len()));
        }
        let mut bytes = [0u8; HASH_LEN];
        for (index, pair) in text.as_bytes().chunks(2).enumerate() {
            bytes[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
        }
        Ok(Hash(bytes))
    }
}

fn hex_nibble(byte: u8) -> Result<u8, HashError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(HashError::InvalidHexCharacter),
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash(0x{})", self.to_hex())
    }
}
