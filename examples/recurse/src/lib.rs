//! X6 reference contract: it calls the next contract in a chain until the node
//! says stop.
//!
//! The depth limit is only real if something measures it. This contract is
//! deployed many times at different addresses; each copy is told who comes next
//! and then calls that address's [`SEL_DOWN`]. Each copy records the depth it ran
//! at and what the call below it answered, so a test can read the whole descent
//! from state instead of trusting a log.
//!
//! Why a chain and not `self.down()`: X6 refuses any call to an address that is
//! **already on the stack** (`-14`), so a contract cannot even call itself. The
//! depth limit therefore governs chains of *distinct* contracts, which is the
//! shape a real router → pool → token path has.
//!
//! It is also the atomicity fixture: [`SEL_BOOM`] writes a counter and then traps,
//! so a caller can check that a callee which reverts leaves **nothing** behind.

#![no_std]

use aura_sdk::prelude::*;

aura_sdk::panic_handler!();

/// Selector: descend into the configured next contract.
pub const SEL_DOWN: u32 = 0x01;
/// Selector: write a counter and then trap.
pub const SEL_BOOM: u32 = 0x02;
/// Selector: the depth this frame ran at, recorded by the last `down`.
pub const SEL_DEPTH: u32 = 0x03;
/// Selector: how many `down` calls this contract made.
pub const SEL_REACHED: u32 = 0x04;
/// Selector: the status of the last nested call.
pub const SEL_LAST: u32 = 0x05;
/// Selector: the value of the counter `boom` writes before it traps.
pub const SEL_MARK: u32 = 0x06;
/// Selector: who is calling this frame.
pub const SEL_WHO: u32 = 0x07;
/// Selector: set the next contract in the chain.
pub const SEL_SET_NEXT: u32 = 0x08;

const NEXT: Slot<[u8; 20]> = Slot::at(b"nx");
const DEPTH: Slot<u32> = Slot::at(b"d");
const REACHED: Slot<u32> = Slot::at(b"r");
const LAST: Slot<i32> = Slot::at(b"l");
const MARK: Slot<u32> = Slot::at(b"m");

#[constructor]
pub fn init() -> u32 {
    REACHED.write(0);
    LAST.write(0);
    0
}

#[entrypoint]
pub fn run() -> u32 {
    dispatch! {
        SEL_DOWN => down(),
        SEL_BOOM => boom(),
        SEL_DEPTH => depth_method(),
        SEL_REACHED => reached(),
        SEL_LAST => last(),
        SEL_MARK => mark(),
        SEL_WHO => who(),
        SEL_SET_NEXT => set_next([u8; 20]),
    }
}

fn set_next(next: [u8; 20]) -> u32 {
    NEXT.write(next);
    0
}

/// Record the depth, then descend one frame.
fn down() -> u32 {
    DEPTH.write(unwrap(depth()));
    REACHED.write(REACHED.get_or(0).saturating_add(1));
    let status = match call_contract(&NEXT.get_or([0u8; 20]), &SEL_DOWN.to_le_bytes(), 0) {
        Ok(()) => 0,
        Err(error) => error.code(),
    };
    LAST.write(status);
    0
}

/// Write a counter, then trap. A callee that reverts must leave no trace.
fn boom() -> u32 {
    MARK.write(MARK.get_or(0).saturating_add(1));
    unwrap(emit(b"boom", &[]));
    abort()
}

fn depth_method() -> u32 {
    reply_u32(DEPTH.get_or(0))
}

fn reached() -> u32 {
    reply_u32(REACHED.get_or(0))
}

fn last() -> u32 {
    let value = LAST.get_or(0);
    unwrap(reply(&value.to_le_bytes()));
    0
}

fn mark() -> u32 {
    reply_u32(MARK.get_or(0))
}

fn who() -> u32 {
    unwrap(reply(&unwrap(caller())));
    0
}

fn reply_u32(value: u32) -> u32 {
    unwrap(reply(&value.to_le_bytes()));
    0
}
