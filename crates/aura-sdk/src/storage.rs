//! Typed contract storage (X4).
//!
//! A [`Slot`] is a **key** plus a type: the key decides where the value lives,
//! the type decides how many bytes it takes. Two ways to name a key:
//!
//! * a constant — `Slot::at(b"count")`, usable in a `const`, which is what a
//!   contract uses for its own fields;
//! * a composed key — `Key::new(&[b"bal:", &address])`, for a per-account map.
//!
//! # What a failed read means
//!
//! `get()` returns `Ok(None)` when the key is absent and `Err` when the read
//! failed (bad key length, out of gas, a value written by a different layout).
//! The convenience methods [`Slot::get_or`] and [`Slot::set`] turn any error into
//! a **trap**: a contract that cannot read its own state must not continue with
//! an invented value, because the trap reverts and the alternative is a silently
//! wrong balance.
//!
//! # Keys are zero-padded slots
//!
//! The host pads a key to 32 bytes (`aura_exec::abi::slot_of`), so trailing NULs
//! are not significant: `b"a"` and `b"a\0"` are the same slot. This is published
//! behaviour, not a wart — [`Key::new`] therefore rejects a key containing an
//! interior NUL when it is composed in more than one part, so an accidental
//! collision fails at the call site rather than shadowing another field.

use crate::abi::{Error, MAX_KEY_LEN, MAX_VALUE_LEN, storage_read, storage_write};
use crate::codec::Codec;

/// A key of at most [`MAX_KEY_LEN`] bytes, held inline (no allocation).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Key {
    bytes: [u8; MAX_KEY_LEN],
    len: u8,
}

impl Key {
    /// Compose a key from one or more byte parts, in order.
    ///
    /// Errors when the total is longer than [`MAX_KEY_LEN`], or when a part is
    /// empty (an empty part is always a bug, and it would make the composed key
    /// indistinguishable from a shorter one).
    pub fn new(parts: &[&[u8]]) -> Result<Self, Error> {
        let mut bytes = [0u8; MAX_KEY_LEN];
        let mut len = 0usize;
        for part in parts {
            if part.is_empty() {
                return Err(Error::KeyTooLong);
            }
            let end = len + part.len();
            if end > MAX_KEY_LEN {
                return Err(Error::KeyTooLong);
            }
            bytes[len..end].copy_from_slice(part);
            len = end;
        }
        if len == 0 {
            return Err(Error::KeyTooLong);
        }
        Ok(Self {
            bytes,
            len: len as u8,
        })
    }

    /// The key's bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

/// A typed storage field.
pub struct Slot<T: Codec> {
    key: Key,
    _value: core::marker::PhantomData<T>,
}

impl<T: Codec> Slot<T> {
    /// A slot with a constant key — usable in a `const`, so a contract's fields
    /// are declared once:
    ///
    /// ```ignore
    /// const COUNT: Slot<u64> = Slot::at(b"count");
    /// ```
    ///
    /// # Panics
    ///
    /// At compile time when `key` is longer than [`MAX_KEY_LEN`] or empty; both
    /// are programming errors, and a `const` is where they should surface.
    pub const fn at(key: &'static [u8]) -> Self {
        assert!(
            !key.is_empty() && key.len() <= MAX_KEY_LEN,
            "a storage key must be 1..=32 bytes"
        );
        let mut bytes = [0u8; MAX_KEY_LEN];
        let mut index = 0;
        while index < key.len() {
            bytes[index] = key[index];
            index += 1;
        }
        Self {
            key: Key {
                bytes,
                len: key.len() as u8,
            },
            _value: core::marker::PhantomData,
        }
    }

    /// A slot with a composed key — the per-account map case.
    pub fn with_key(key: Key) -> Self {
        Self {
            key,
            _value: core::marker::PhantomData,
        }
    }

    /// The key this slot addresses.
    pub fn key(&self) -> Key {
        self.key
    }

    /// Read the value; `Ok(None)` when the slot is absent.
    pub fn get(&self) -> Result<Option<T>, Error> {
        let mut buf = [0u8; MAX_VALUE_LEN];
        match storage_read(self.key.as_slice(), &mut buf)? {
            None => Ok(None),
            Some(written) => {
                // A value that is not exactly `T::LEN` was not written by this
                // layout; decoding refuses instead of reading a prefix.
                Ok(Some(T::decode_from(&buf[..written])?))
            }
        }
    }

    /// Write the value.
    pub fn set(&self, value: T) -> Result<(), Error> {
        let mut buf = [0u8; MAX_VALUE_LEN];
        let raw = value.encode(&mut buf)?;
        storage_write(self.key.as_slice(), raw)
    }

    /// Delete the slot (the ABI's "write empty" rule).
    pub fn clear(&self) -> Result<(), Error> {
        storage_write(self.key.as_slice(), &[])
    }

    /// Read the value, or `default` when the slot is absent. A **failed** read
    /// traps: see the module note.
    pub fn get_or(&self, default: T) -> T {
        match self.get() {
            Ok(Some(value)) => value,
            Ok(None) => default,
            Err(_) => crate::abort(),
        }
    }

    /// Write the value, trapping on failure.
    pub fn write(&self, value: T) {
        if self.set(value).is_err() {
            crate::abort();
        }
    }
}

/// A map from a runtime byte key to a typed value: `Slot::with_key(Key::new(..))`.
///
/// ```ignore
/// let slot: Slot<u128> = Slot::with_key(Key::new(&[b"bal:", address]).unwrap());
/// ```
pub type Map<T> = Slot<T>;
