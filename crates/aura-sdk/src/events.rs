//! Events (X4).
//!
//! An event is a topic and a payload. The SDK checks the published limits
//! *before* the call ([`MAX_TOPIC_LEN`], [`MAX_VALUE_LEN`]) so a contract that
//! overruns gets the error at the call site, and a contract that wants a
//! structured payload writes it into a buffer it owns — there is no allocator in
//! a contract, by design.
//!
//! ```ignore
//! let mut data = [0u8; 24];
//! data[..8].copy_from_slice(&amount.to_le_bytes());
//! emit(b"transfer", &data[..8]);
//! ```

use crate::abi::{Error, MAX_TOPIC_LEN, MAX_VALUE_LEN};

/// Append an event to the receipt.
///
/// On a successful call the events land in the receipt and in the state diff; on
/// a trap or an out-of-gas they are discarded with the rest of the call (X2/X3
/// commit rule).
pub fn emit(topic: &[u8], data: &[u8]) -> Result<(), Error> {
    if topic.is_empty() {
        return Err(Error::TopicTooLong);
    }
    if topic.len() > MAX_TOPIC_LEN {
        return Err(Error::TopicTooLong);
    }
    if data.len() > MAX_VALUE_LEN {
        return Err(Error::ValueTooLong);
    }
    crate::abi::emit(topic, data)
}

/// Emit an event and trap if it fails — the common case in a contract, where a
/// lost event means the call was wrong.
pub fn emit_or_abort(topic: &[u8], data: &[u8]) {
    if emit(topic, data).is_err() {
        crate::abort();
    }
}

/// Emit an event whose payload is a single `u64`.
pub fn emit_u64(topic: &[u8], value: u64) -> Result<(), Error> {
    emit(topic, &value.to_le_bytes())
}

/// Emit an event whose payload is a single `u128`.
pub fn emit_u128(topic: &[u8], value: u128) -> Result<(), Error> {
    emit(topic, &value.to_le_bytes())
}
