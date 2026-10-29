//! Fixed-layout value encoding for contract storage (X4).
//!
//! No serde, no `derive`, no tags: a value **is** its little-endian bytes, and
//! its length is the type's length. That makes a storage slot readable by
//! anything — the node's tests, a block explorer, another contract — without
//! sharing a Rust type, and it makes a wrong read a detectable length error
//! instead of silently misaligned fields.
//!
//! | type | bytes | note |
//! |---|---|---|
//! | `u8` | 1 | |
//! | `bool` | 1 | `0` or `1`; any other byte is a decoding error |
//! | `u16` | 2 | |
//! | `u32` | 4 | |
//! | `u64` | 8 | |
//! | `u128` | 16 | the width of an `Amount` |
//! | `i32` | 4 | |
//! | `i64` | 8 | |
//! | `[u8; N]` | N | raw, no length prefix |
//!
//! Extending this set is a **contract-visible** change: a stored value does not
//! say which type wrote it, so a new `Codec` impl must use a new key.

use crate::abi::Error;

/// A value that knows its own fixed size and can round-trip through bytes.
pub trait Codec: Sized + Copy {
    /// Length in bytes — always this many, never variable.
    const LEN: usize;

    /// Write the value into `out`. `out.len()` is at least [`Self::LEN`].
    fn encode_into(self, out: &mut [u8]);

    /// Read the value back. `raw.len()` must be exactly [`Self::LEN`].
    fn decode_from(raw: &[u8]) -> Result<Self, Error>;

    /// Convenience: encode into a caller-owned buffer and return the prefix.
    fn encode(self, out: &mut [u8]) -> Result<&[u8], Error> {
        if out.len() < Self::LEN {
            return Err(Error::ValueTooLong);
        }
        self.encode_into(out);
        Ok(&out[..Self::LEN])
    }
}

fn exact(raw: &[u8], len: usize) -> Result<(), Error> {
    if raw.len() == len {
        Ok(())
    } else {
        // A shorter or longer value means the slot was written by a different
        // layout; refusing is the only honest answer.
        Err(Error::ValueTooLong)
    }
}

macro_rules! integer_codec {
    ($type:ty, $len:expr) => {
        impl Codec for $type {
            const LEN: usize = $len;

            fn encode_into(self, out: &mut [u8]) {
                out[..Self::LEN].copy_from_slice(&self.to_le_bytes());
            }

            fn decode_from(raw: &[u8]) -> Result<Self, Error> {
                exact(raw, Self::LEN)?;
                let mut bytes = [0u8; $len];
                bytes.copy_from_slice(raw);
                Ok(<$type>::from_le_bytes(bytes))
            }
        }
    };
}

integer_codec!(u8, 1);
integer_codec!(u16, 2);
integer_codec!(u32, 4);
integer_codec!(u64, 8);
integer_codec!(u128, 16);
integer_codec!(i32, 4);
integer_codec!(i64, 8);

impl Codec for bool {
    const LEN: usize = 1;

    fn encode_into(self, out: &mut [u8]) {
        out[0] = u8::from(self);
    }

    fn decode_from(raw: &[u8]) -> Result<Self, Error> {
        exact(raw, 1)?;
        match raw[0] {
            0 => Ok(false),
            1 => Ok(true),
            // Not "truthy": a byte that is neither 0 nor 1 was not written by
            // this codec, so it is an error.
            _ => Err(Error::Unknown(-12)),
        }
    }
}

impl<const N: usize> Codec for [u8; N] {
    const LEN: usize = N;

    fn encode_into(self, out: &mut [u8]) {
        out[..N].copy_from_slice(&self);
    }

    fn decode_from(raw: &[u8]) -> Result<Self, Error> {
        exact(raw, N)?;
        let mut bytes = [0u8; N];
        bytes.copy_from_slice(raw);
        Ok(bytes)
    }
}

/// Sequential decoder over a byte slice.
///
/// Used for calldata: the arguments of a method arrive one after another, with
/// no tags, and every read advances. A read past the end, a short field or a
/// leftover byte is [`Error::ValueTooLong`] — the same refusal the storage layer
/// gives for a slot written by a different layout.
pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Start at byte 0.
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// Bytes not yet read.
    pub const fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    /// True when every byte has been consumed.
    pub const fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Decode one value and advance.
    pub fn get<T: Codec>(&mut self) -> Result<T, Error> {
        let end = self.pos + T::LEN;
        let raw = self.bytes.get(self.pos..end).ok_or(Error::ValueTooLong)?;
        let value = T::decode_from(raw)?;
        self.pos = end;
        Ok(value)
    }

    /// Decode `N` raw bytes and advance.
    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let end = self.pos + N;
        let raw = self.bytes.get(self.pos..end).ok_or(Error::ValueTooLong)?;
        let mut out = [0u8; N];
        out.copy_from_slice(raw);
        self.pos = end;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_round_trip() {
        let mut buf = [0u8; 16];
        for value in [0u64, 1, u64::MAX, 0x0123_4567_89ab_cdef] {
            let raw = value.encode(&mut buf).unwrap();
            assert_eq!(u64::decode_from(raw), Ok(value));
        }
        let value = u128::MAX;
        let raw = value.encode(&mut buf).unwrap();
        assert_eq!(u128::decode_from(raw), Ok(value));
    }

    #[test]
    fn length_mismatch_is_an_error() {
        let mut buf = [0u8; 8];
        let raw = 7u32.encode(&mut buf).unwrap();
        assert_eq!(u32::decode_from(raw), Ok(7));
        assert_eq!(u64::decode_from(raw), Err(Error::ValueTooLong));
        assert_eq!(u32::decode_from(&raw[..2]), Err(Error::ValueTooLong));
    }

    #[test]
    fn bool_rejects_non_canonical_bytes() {
        assert_eq!(bool::decode_from(&[1]), Ok(true));
        assert_eq!(bool::decode_from(&[0]), Ok(false));
        assert!(bool::decode_from(&[2]).is_err());
    }

    #[test]
    fn arrays_are_raw() {
        let mut buf = [0u8; 32];
        let raw = [9u8; 32].encode(&mut buf).unwrap();
        assert_eq!(raw.len(), 32);
        assert_eq!(<[u8; 32]>::decode_from(raw), Ok([9u8; 32]));
    }

    #[test]
    fn encode_refuses_a_short_buffer() {
        let mut buf = [0u8; 3];
        assert_eq!(u64::MIN.encode(&mut buf), Err(Error::ValueTooLong));
    }

    #[test]
    fn the_encoding_is_little_endian() {
        let mut buf = [0u8; 8];
        let raw = 0x0102_0304u32.encode(&mut buf).unwrap();
        assert_eq!(raw, &[0x04, 0x03, 0x02, 0x01]);
    }
}
