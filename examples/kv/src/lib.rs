//! Reference contract: a one-slot key/value store with a migration (X4).
//!
//! Shows the argument convention (`#[entrypoint(arg)]`, `#[constructor(arg)]`)
//! and a `migrate` that runs during an upgrade.

#![no_std]

use aura_sdk::prelude::*;

aura_sdk::panic_handler!();

/// The stored value.
const VALUE: Slot<u64> = Slot::at(b"value");
/// Written by `migrate` only: proves the migration ran.
const MIGRATED: Slot<u8> = Slot::at(b"migrated");
/// The schema version.
const VERSION: Slot<u32> = Slot::at(b"version");

/// Store the initial value (the deploy argument, or 0).
#[constructor(arg)]
pub fn init(initial: i32) -> u32 {
    VALUE.write(initial as u64);
    VERSION.write(1);
    0
}

/// Store the argument and return it.
#[entrypoint(arg)]
pub fn run(value: i32) -> u32 {
    VALUE.write(value as u64);
    emit_or_abort(b"value", &(value as u64).to_le_bytes());
    value as u32
}

/// Schema 1 -> 2: keep the value, mark the slot, bump the version.
#[migrate]
pub fn migrate(from: i32, to: i32) -> u32 {
    MIGRATED.write(1);
    VERSION.write(to as u32);
    from as u32 + to as u32
}
