//! Reference contract: a counter (X4).
//!
//! Shows the four things every contract does: declare a typed slot, initialise
//! it in `init`, read-modify-write it in `run`, and emit an event.

#![no_std]

use aura_sdk::prelude::*;

aura_sdk::panic_handler!();

/// The counter lives in its own slot; the key is a compile-time constant.
const COUNT: Slot<u64> = Slot::at(b"count");

/// Set the counter to zero. Runs inside `Lifecycle::deploy`.
#[constructor]
pub fn init() -> u32 {
    COUNT.write(0);
    0
}

/// Increment and report the new value — which is also the call's return value,
/// so a caller sees the number without reading state.
#[entrypoint]
pub fn run() -> u32 {
    let next = COUNT.get_or(0) + 1;
    COUNT.write(next);
    emit_or_abort(b"count", &next.to_le_bytes());
    next as u32
}
