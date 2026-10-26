//! `Encode`/`Decode` implementations for primitives, collections and C1 types.

use std::collections::BTreeMap;

use aura_types::{
    ADDRESS_LEN, Address, Amount, ChainId, Denom, Domain, Gas, HASH_LEN, Hash, Height, Nonce,
};

use crate::codec::{Decode, Encode, Reader, Writer, read_count, read_length, write_varint};
use crate::error::CodecError;

macro_rules! fixed_int {
    ($ty:ty, $len:expr) => {
        impl Encode for $ty {
            fn encode(&self, writer: &mut Writer) {
                writer.push_bytes(&self.to_le_bytes());
            }
        }

        impl Decode for $ty {
            fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
                let bytes = reader.read_exact($len)?;
                let mut array = [0u8; $len];
                array.copy_from_slice(bytes);
                Ok(<$ty>::from_le_bytes(array))
            }
        }
    };
}

// NOTE: `usize`/`isize` intentionally have NO implementation. Their width
// depends on the target, so encoding one would break byte-identical state.
fixed_int!(u8, 1);
fixed_int!(u16, 2);
fixed_int!(u32, 4);
fixed_int!(u64, 8);
fixed_int!(u128, 16);

impl Encode for bool {
    fn encode(&self, writer: &mut Writer) {
        writer.push_byte(if *self { 1 } else { 0 });
    }
}

impl Decode for bool {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        match reader.read_byte()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(CodecError::InvalidBool(other)),
        }
    }
}

impl<T: Encode> Encode for Option<T> {
    fn encode(&self, writer: &mut Writer) {
        match self {
            None => writer.push_byte(0),
            Some(value) => {
                writer.push_byte(1);
                value.encode(writer);
            }
        }
    }
}

impl<T: Decode> Decode for Option<T> {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        match reader.read_byte()? {
            0 => Ok(None),
            1 => Ok(Some(T::decode(reader)?)),
            other => Err(CodecError::InvalidOptionTag(other)),
        }
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, writer: &mut Writer) {
        write_varint(writer, self.len() as u64);
        for item in self {
            item.encode(writer);
        }
    }
}

impl<T: Decode> Decode for Vec<T> {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let count = read_count(reader)?;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            out.push(T::decode(reader)?);
        }
        Ok(out)
    }
}

impl Encode for String {
    fn encode(&self, writer: &mut Writer) {
        let bytes = self.as_bytes();
        write_varint(writer, bytes.len() as u64);
        writer.push_bytes(bytes);
    }
}

impl Decode for String {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let length = read_length(reader)?;
        let bytes = reader.read_exact(length)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| CodecError::InvalidUtf8)
    }
}

impl<K, V> Encode for BTreeMap<K, V>
where
    K: Encode + Ord,
    V: Encode,
{
    fn encode(&self, writer: &mut Writer) {
        write_varint(writer, self.len() as u64);
        for (key, value) in self {
            key.encode(writer);
            value.encode(writer);
        }
    }
}

impl<K, V> Decode for BTreeMap<K, V>
where
    K: Decode + Ord,
    V: Decode,
{
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let count = read_count(reader)?;
        let mut out = BTreeMap::new();
        for _ in 0..count {
            let key = K::decode(reader)?;
            if let Some((last, _)) = out.last_key_value() {
                if &key == last {
                    return Err(CodecError::DuplicateMapKey);
                }
                if &key < last {
                    return Err(CodecError::NonCanonicalMapOrder);
                }
            }
            let value = V::decode(reader)?;
            out.insert(key, value);
        }
        Ok(out)
    }
}

impl Encode for Hash {
    fn encode(&self, writer: &mut Writer) {
        writer.push_bytes(self.as_bytes());
    }
}

impl Decode for Hash {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let bytes = reader.read_exact(HASH_LEN)?;
        Hash::from_slice(bytes).map_err(|_| CodecError::InvalidHash)
    }
}

impl Encode for Address {
    fn encode(&self, writer: &mut Writer) {
        writer.push_bytes(self.as_bytes());
    }
}

impl Decode for Address {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let bytes = reader.read_exact(ADDRESS_LEN)?;
        Address::from_slice(bytes).map_err(|_| CodecError::InvalidAddress)
    }
}

impl Encode for Amount {
    fn encode(&self, writer: &mut Writer) {
        self.units().encode(writer);
    }
}

impl Decode for Amount {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Amount::from_units(u128::decode(reader)?))
    }
}

impl Encode for Nonce {
    fn encode(&self, writer: &mut Writer) {
        self.value().encode(writer);
    }
}

impl Decode for Nonce {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Nonce::new(u64::decode(reader)?))
    }
}

impl Encode for Height {
    fn encode(&self, writer: &mut Writer) {
        self.value().encode(writer);
    }
}

impl Decode for Height {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Height::new(u64::decode(reader)?))
    }
}

impl Encode for Gas {
    fn encode(&self, writer: &mut Writer) {
        self.value().encode(writer);
    }
}

impl Decode for Gas {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Gas::new(u64::decode(reader)?))
    }
}

impl Encode for ChainId {
    fn encode(&self, writer: &mut Writer) {
        let bytes = self.as_str().as_bytes();
        write_varint(writer, bytes.len() as u64);
        writer.push_bytes(bytes);
    }
}

impl Decode for ChainId {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let text = String::decode(reader)?;
        ChainId::new(&text).map_err(|_| CodecError::InvalidChainId)
    }
}

impl Encode for Domain {
    fn encode(&self, writer: &mut Writer) {
        let tag = match self {
            Domain::Tx => 0,
            Domain::Vote => 1,
            Domain::Block => 2,
            Domain::Contract => 3,
            Domain::Genesis => 4,
        };
        writer.push_byte(tag);
    }
}

impl Decode for Domain {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        match reader.read_byte()? {
            0 => Ok(Domain::Tx),
            1 => Ok(Domain::Vote),
            2 => Ok(Domain::Block),
            3 => Ok(Domain::Contract),
            4 => Ok(Domain::Genesis),
            other => Err(CodecError::InvalidDomainTag(other)),
        }
    }
}

impl Encode for Denom {
    fn encode(&self, writer: &mut Writer) {
        let tag = if *self == Denom::NATIVE {
            0
        } else if *self == Denom::SMALLEST {
            1
        } else {
            2
        };
        writer.push_byte(tag);
    }
}

impl Decode for Denom {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        match reader.read_byte()? {
            0 => Ok(Denom::NATIVE),
            1 => Ok(Denom::SMALLEST),
            other => Err(CodecError::InvalidDenomTag(other)),
        }
    }
}
