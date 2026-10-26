//! Canonical 20-byte [`Address`] with an in-tree bech32 (BIP-173) codec.
//!
//! No external crate is used: the encoder is a direct, auditable transcription
//! of BIP-173. The golden vectors in `tests/fixtures/c1_golden.txt` come from
//! an **independent** Python implementation (`tools/gen_c1_vectors.py`).
//!
//! Note: this is classic bech32 (polymod xor `1`), not bech32m. Version byte
//! `0x00` is reserved in the address payload for a future PQ signature scheme
//! (ROADMAP D7). bech32m is a separate, versioned decision — not a C1 change.

use std::fmt;

use crate::BECH32_PREFIX;

/// Length of an address' pubkey hash in bytes.
pub const ADDRESS_LEN: usize = 20;

/// Current address version byte (`version || pubkey_hash`).
pub const ADDRESS_VERSION: u8 = 0x00;

const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const GEN: [u32; 5] = [
    0x3b6a_57b2,
    0x2650_8e6d,
    0x1ea1_19fa,
    0x3d42_33dd,
    0x2a14_62b3,
];
const CHECKSUM_LEN: usize = 6;

// NC target: tools/nc_c1.py flips this to 0 and requires the golden vectors
// to go red. BIP-173 specifies 1 for classic bech32.
const BECH32_CHECKSUM_CONST: u32 = 1;

/// Errors produced when parsing or constructing an [`Address`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressError {
    /// A byte slice was not exactly [`ADDRESS_LEN`] bytes long.
    InvalidByteLength(usize),
    /// The bech32 string has no `1` separator.
    MissingSeparator,
    /// The human-readable part is empty.
    EmptyPrefix,
    /// The human-readable part is not `aura`.
    InvalidPrefix,
    /// Fewer than 6 characters follow the separator (no checksum).
    TooShort,
    /// A character is outside the bech32 charset.
    InvalidCharacter,
    /// Upper or mixed case is not canonical (only lowercase is accepted).
    MixedCase,
    /// The bech32 checksum does not validate.
    InvalidChecksum,
    /// The 5-bit payload has non-canonical padding.
    NonCanonicalPadding,
    /// The version byte is not [`ADDRESS_VERSION`].
    InvalidVersion(u8),
    /// The decoded payload is not `1 + ADDRESS_LEN` bytes.
    InvalidPayloadLength(usize),
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidByteLength(n) => {
                write!(f, "expected {ADDRESS_LEN} bytes, got {n}")
            }
            Self::MissingSeparator => f.write_str("missing bech32 separator '1'"),
            Self::EmptyPrefix => f.write_str("empty bech32 prefix"),
            Self::InvalidPrefix => write!(f, "expected prefix `{BECH32_PREFIX}`"),
            Self::TooShort => f.write_str("bech32 string has no checksum"),
            Self::InvalidCharacter => f.write_str("invalid bech32 character"),
            Self::MixedCase => f.write_str("only lowercase bech32 is canonical"),
            Self::InvalidChecksum => f.write_str("invalid bech32 checksum"),
            Self::NonCanonicalPadding => f.write_str("non-canonical bech32 padding"),
            Self::InvalidVersion(v) => {
                write!(f, "expected address version {ADDRESS_VERSION}, got {v}")
            }
            Self::InvalidPayloadLength(n) => {
                write!(f, "expected {} payload bytes, got {n}", 1 + ADDRESS_LEN)
            }
        }
    }
}

impl std::error::Error for AddressError {}

/// A canonical 20-byte account address.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Address([u8; ADDRESS_LEN]);

impl Address {
    /// The all-zero address (reserved; not a burn address by protocol rule).
    pub const ZERO: Address = Address([0u8; ADDRESS_LEN]);

    /// Wrap a fixed-size pubkey hash.
    pub const fn from_bytes(bytes: [u8; ADDRESS_LEN]) -> Self {
        Address(bytes)
    }

    /// Decode from a byte slice of exactly [`ADDRESS_LEN`] bytes.
    pub fn from_slice(slice: &[u8]) -> Result<Self, AddressError> {
        let bytes: [u8; ADDRESS_LEN] = slice
            .try_into()
            .map_err(|_| AddressError::InvalidByteLength(slice.len()))?;
        Ok(Address(bytes))
    }

    /// Borrow the raw pubkey hash bytes.
    pub const fn as_bytes(&self) -> &[u8; ADDRESS_LEN] {
        &self.0
    }

    /// Consume the address and return the raw bytes.
    pub const fn into_bytes(self) -> [u8; ADDRESS_LEN] {
        self.0
    }

    /// Encode as lowercase bech32, e.g. `aura1...`.
    pub fn to_bech32(&self) -> String {
        let mut payload = Vec::with_capacity(1 + ADDRESS_LEN);
        payload.push(ADDRESS_VERSION);
        payload.extend_from_slice(&self.0);

        let data = convert_bits(&payload, 8, 5, true).expect("21 bytes always convert 8->5");
        let checksum = create_checksum(BECH32_PREFIX, &data);

        let mut out = String::with_capacity(BECH32_PREFIX.len() + 1 + data.len() + CHECKSUM_LEN);
        out.push_str(BECH32_PREFIX);
        out.push('1');
        for value in data.iter().chain(checksum.iter()) {
            out.push(CHARSET[*value as usize] as char);
        }
        out
    }

    /// Parse a canonical lowercase bech32 address.
    pub fn from_bech32(text: &str) -> Result<Self, AddressError> {
        if text.chars().any(|c| c.is_ascii_uppercase()) {
            return Err(AddressError::MixedCase);
        }
        let separator = text.rfind('1').ok_or(AddressError::MissingSeparator)?;
        let hrp = &text[..separator];
        if hrp.is_empty() {
            return Err(AddressError::EmptyPrefix);
        }
        if hrp != BECH32_PREFIX {
            return Err(AddressError::InvalidPrefix);
        }
        let body = &text[separator + 1..];
        if body.len() < CHECKSUM_LEN {
            return Err(AddressError::TooShort);
        }

        let mut data = Vec::with_capacity(body.len());
        for byte in body.bytes() {
            let value = CHARSET
                .iter()
                .position(|c| *c == byte)
                .ok_or(AddressError::InvalidCharacter)?;
            data.push(value as u8);
        }

        let mut polymod_input = hrp_expand(hrp);
        polymod_input.extend_from_slice(&data);
        if polymod(&polymod_input) != BECH32_CHECKSUM_CONST {
            return Err(AddressError::InvalidChecksum);
        }

        let payload_bits = &data[..data.len() - CHECKSUM_LEN];
        let payload =
            convert_bits(payload_bits, 5, 8, false).ok_or(AddressError::NonCanonicalPadding)?;
        if payload.len() != 1 + ADDRESS_LEN {
            return Err(AddressError::InvalidPayloadLength(payload.len()));
        }
        if payload[0] != ADDRESS_VERSION {
            return Err(AddressError::InvalidVersion(payload[0]));
        }

        Address::from_slice(&payload[1..])
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_bech32())
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.to_bech32())
    }
}

fn polymod(values: &[u8]) -> u32 {
    let mut chk: u32 = 1;
    for &value in values {
        let top = chk >> 25;
        chk = (chk & 0x01ff_ffff) << 5 ^ u32::from(value);
        for (index, generator) in GEN.iter().enumerate() {
            if (top >> (index as u32)) & 1 == 1 {
                chk ^= generator;
            }
        }
    }
    chk
}

fn hrp_expand(hrp: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(hrp.len() * 2 + 1);
    for byte in hrp.bytes() {
        out.push(byte >> 5);
    }
    out.push(0);
    for byte in hrp.bytes() {
        out.push(byte & 31);
    }
    out
}

fn create_checksum(hrp: &str, data: &[u8]) -> [u8; CHECKSUM_LEN] {
    let mut values = hrp_expand(hrp);
    values.extend_from_slice(data);
    values.extend_from_slice(&[0u8; CHECKSUM_LEN]);
    let modulus = polymod(&values) ^ BECH32_CHECKSUM_CONST;
    let mut out = [0u8; CHECKSUM_LEN];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = ((modulus >> (5 * (5 - index))) & 31) as u8;
    }
    out
}

/// Regroup a sequence of `frombits`-wide values into `tobits`-wide values.
fn convert_bits(data: &[u8], frombits: u32, tobits: u32, pad: bool) -> Option<Vec<u8>> {
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    let max_value: u32 = (1u32 << tobits) - 1;
    let mut out = Vec::new();

    for &value in data {
        let widened = u32::from(value);
        if (widened >> frombits) != 0 {
            return None;
        }
        accumulator = (accumulator << frombits) | widened;
        bits += frombits;
        while bits >= tobits {
            bits -= tobits;
            out.push(((accumulator >> bits) & max_value) as u8);
        }
    }

    if pad {
        if bits > 0 {
            out.push(((accumulator << (tobits - bits)) & max_value) as u8);
        }
    } else if bits >= frombits || ((accumulator << (tobits - bits)) & max_value) != 0 {
        return None;
    }
    Some(out)
}
