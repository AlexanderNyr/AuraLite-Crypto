//! Traits, byte reader/writer and canonical varint.

use crate::error::CodecError;
use crate::{MAX_ITEMS, MAX_LENGTH};

/// Append-only byte writer used by [`Encode`].
#[derive(Debug, Default)]
pub struct Writer {
    buffer: Vec<u8>,
}

impl Writer {
    /// Create an empty writer.
    pub fn new() -> Self {
        Writer { buffer: Vec::new() }
    }

    /// Append one byte.
    pub fn push_byte(&mut self, byte: u8) {
        self.buffer.push(byte);
    }

    /// Append a byte slice verbatim.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// Borrow the bytes written so far.
    pub fn as_slice(&self) -> &[u8] {
        &self.buffer
    }

    /// Consume the writer and return the bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }
}

/// Bounded byte reader used by [`Decode`]. Tracks the unconsumed tail.
#[derive(Debug, Clone, Copy)]
pub struct Reader<'a> {
    remaining: &'a [u8],
}

impl<'a> Reader<'a> {
    /// Create a reader over `bytes`.
    pub fn new(bytes: &'a [u8]) -> Self {
        Reader { remaining: bytes }
    }

    /// Number of bytes not yet consumed.
    pub fn remaining_len(&self) -> usize {
        self.remaining.len()
    }

    /// Consume one byte.
    pub fn read_byte(&mut self) -> Result<u8, CodecError> {
        let bytes = self.read_exact(1)?;
        Ok(bytes[0])
    }

    /// Consume exactly `len` bytes and return them.
    pub fn read_exact(&mut self, len: usize) -> Result<&'a [u8], CodecError> {
        if len > self.remaining.len() {
            return Err(CodecError::UnexpectedEof(len, self.remaining.len()));
        }
        let (head, tail) = self.remaining.split_at(len);
        self.remaining = tail;
        Ok(head)
    }

    /// Require that the input was fully consumed.
    ///
    /// NC target: `tools/nc_c2.py` disables this guard and requires the
    /// trailing-byte test to go red.
    pub fn finish(&self) -> Result<(), CodecError> {
        if !self.remaining.is_empty() {
            return Err(CodecError::TrailingBytes(self.remaining.len()));
        }
        Ok(())
    }
}

/// Canonical encoding of a value into a [`Writer`].
///
/// Implementations must write fields in a fixed order and never branch on
/// platform-dependent values. Producers must respect [`MAX_LENGTH`] and
/// [`MAX_ITEMS`]; decoders enforce those limits.
pub trait Encode {
    /// Append the canonical encoding of `self`.
    fn encode(&self, writer: &mut Writer);
}

/// Canonical decoding of a value from a [`Reader`].
pub trait Decode: Sized {
    /// Read one value, rejecting any non-canonical representation.
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError>;
}

/// Encode a value to an owned byte vector.
pub fn encode_to_bytes<T: Encode>(value: &T) -> Vec<u8> {
    let mut writer = Writer::new();
    value.encode(&mut writer);
    writer.into_bytes()
}

/// Decode exactly one value and require that no trailing bytes remain.
pub fn decode_from_slice<T: Decode>(bytes: &[u8]) -> Result<T, CodecError> {
    let mut reader = Reader::new(bytes);
    let value = T::decode(&mut reader)?;
    reader.finish()?;
    Ok(value)
}

/// Decode one value and require that re-encoding reproduces the input exactly.
///
/// This is the structural backstop: even if a specific check is missed in
/// `Decode`, a non-canonical input cannot survive the byte comparison.
pub fn decode_canonical<T>(bytes: &[u8]) -> Result<T, CodecError>
where
    T: Decode + Encode,
{
    let value = decode_from_slice::<T>(bytes)?;
    let reencoded = encode_to_bytes(&value);
    if reencoded.as_slice() != bytes {
        return Err(CodecError::NonCanonicalEncoding);
    }
    Ok(value)
}

/// Write a minimal unsigned LEB128 varint.
pub fn write_varint(writer: &mut Writer, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            writer.push_byte(byte);
            return;
        }
        writer.push_byte(byte | 0x80);
    }
}

/// Read a minimal unsigned LEB128 varint.
///
/// Rejects non-minimal encodings, more than 10 bytes and `u64` overflow.
pub fn read_varint(reader: &mut Reader<'_>) -> Result<u64, CodecError> {
    let mut value: u64 = 0;
    let mut count: usize = 0;
    loop {
        if count == 10 {
            return Err(CodecError::VarintOverflow);
        }
        let byte = reader.read_byte()?;
        count += 1;
        if count == 10 && (byte & 0x80 != 0 || byte > 1) {
            return Err(CodecError::VarintOverflow);
        }
        value |= u64::from(byte & 0x7f) << (7 * (count - 1));
        if byte & 0x80 == 0 {
            if varint_is_nonminimal(count, byte) {
                return Err(CodecError::NonCanonicalVarint);
            }
            return Ok(value);
        }
    }
}

/// True when a terminated varint used extra bytes to encode zero.
///
/// NC target: `tools/nc_c2.py` replaces the predicate with `false` and
/// requires the direct varint test to go red.
fn varint_is_nonminimal(encoded_len: usize, last_byte: u8) -> bool {
    last_byte == 0 && encoded_len > 1
}

/// Decode a length-prefixed byte string with the shared length limit.
pub(crate) fn read_length(reader: &mut Reader<'_>) -> Result<usize, CodecError> {
    let length = read_varint(reader)?;
    if length > MAX_LENGTH as u64 {
        return Err(CodecError::LengthTooLarge(length as usize));
    }
    Ok(length as usize)
}

/// Decode an element count with the shared item limit.
pub(crate) fn read_count(reader: &mut Reader<'_>) -> Result<usize, CodecError> {
    let count = read_varint(reader)?;
    if count > MAX_ITEMS as u64 {
        return Err(CodecError::TooManyItems(count as usize));
    }
    Ok(count as usize)
}
