//! X6 reference contract: an attacker that re-enters a vault.
//!
//! It does the simplest thing a hostile contract can do to a vault that pays out
//! before it debits: withdraw, and from inside the payout call back in and
//! withdraw again.
//!
//! ```text
//! attack(vault, amount)
//!   └─ vault.withdraw(amount)
//!        └─ evil.on_receive()            <- the vault called us back
//!             └─ vault.withdraw(amount)  <- the second entry, refused with -14
//! ```
//!
//! The attacker records what happened instead of trapping: an attack that
//! reverts is indistinguishable from one that never ran, and the gate needs the
//! difference. `attempts` and `last_status` are part of the test surface.

#![no_std]

use aura_sdk::prelude::*;

aura_sdk::panic_handler!();

/// Selector: attack a vault.
pub const SEL_ATTACK: u32 = 0x01;
/// Selector: the callback a vault sends value with.
pub const SEL_ON_RECEIVE: u32 = 0x03;
/// Selector: how many times we tried to withdraw.
pub const SEL_ATTEMPTS: u32 = 0x04;
/// Selector: the status of the last attempt.
pub const SEL_LAST_STATUS: u32 = 0x05;
/// Selector: who called us last (the vault, if the payout arrived).
pub const SEL_LAST_CALLER: u32 = 0x06;
/// Selector: write a counter and then trap.
pub const SEL_BOOM: u32 = 0x07;
/// Selector: call `target` with `selector` and report what came back.
pub const SEL_PROBE: u32 = 0x08;
/// Selector: call `target` with `selector`, sending `amount` of native value.
pub const SEL_SEND: u32 = 0x09;

const TARGET: Slot<[u8; 20]> = Slot::at(b"t");
const AMOUNT: Slot<u128> = Slot::at(b"a");
const ATTEMPTS: Slot<u32> = Slot::at(b"n");
const LAST: Slot<i32> = Slot::at(b"s");
const CALLER: Slot<[u8; 20]> = Slot::at(b"c");
const MARK: Slot<u32> = Slot::at(b"m");

#[constructor]
pub fn init() -> u32 {
    0
}

#[entrypoint]
pub fn run() -> u32 {
    dispatch! {
        SEL_ATTACK => attack([u8; 20], u128),
        SEL_ON_RECEIVE => on_receive(),
        SEL_ATTEMPTS => attempts(),
        SEL_LAST_STATUS => last_status(),
        SEL_LAST_CALLER => last_caller(),
        SEL_BOOM => boom(),
        SEL_PROBE => probe([u8; 20], u32),
        SEL_SEND => send([u8; 20], u32, u128),
    }
}

/// Remember the target and start the attack.
fn attack(vault: [u8; 20], amount: u128) -> u32 {
    TARGET.write(vault);
    AMOUNT.write(amount);
    ATTEMPTS.write(0);
    LAST.write(0);
    withdraw_once()
}

/// The callback: try the withdrawal a second time.
///
/// On a correct chain the node refuses this with `-14` before the vault runs at
/// all, so the vault's ledger cannot be read twice. The status is stored, not
/// returned as a failure: the attacker wants the first withdrawal to succeed.
fn on_receive() -> u32 {
    // The address that paid us, as the chain reports it. A contract that took the
    // caller from calldata could be told anything; this one records what the node
    // says, which is how the gate proves the caller of a nested frame is the
    // *contract*, not the transaction's signer.
    CALLER.write(unwrap(caller()));
    withdraw_once()
}

/// Write a counter, then trap — the callee a caller cannot rely on.
fn boom() -> u32 {
    MARK.write(MARK.get_or(0).saturating_add(1));
    abort()
}

/// Call any address with a bare selector and answer with
/// `status i32 LE || returndata`.
///
/// A failed call is reported, not trapped: the gate has to see the *code* the
/// node refused with, and a trap would throw it away.
fn probe(target: [u8; 20], selector: u32) -> u32 {
    let calldata = selector.to_le_bytes();
    let status = match call_contract(&target, &calldata, 0) {
        Ok(()) => 0i32,
        Err(error) => error.code(),
    };
    // The caller's own write, on purpose: it must survive the callee's revert.
    LAST.write(status);
    let mut answer = [0u8; 4];
    answer.copy_from_slice(&status.to_le_bytes());
    let mut out = [0u8; 64];
    let n = returndata(&mut out).unwrap_or(0);
    let mut reply_bytes = [0u8; 4 + 64];
    reply_bytes[..4].copy_from_slice(&answer);
    reply_bytes[4..4 + n].copy_from_slice(&out[..n]);
    unwrap(reply(&reply_bytes[..4 + n]));
    0
}

/// Call with value attached. If the callee reverts, the value must come back —
/// the attacker keeps its money, which is the assertion the gate makes.
fn send(target: [u8; 20], selector: u32, amount: u128) -> u32 {
    let calldata = selector.to_le_bytes();
    let status = match call_contract(&target, &calldata, amount) {
        Ok(()) => 0i32,
        Err(error) => error.code(),
    };
    LAST.write(status);
    unwrap(reply(&status.to_le_bytes()));
    0
}

fn last_caller() -> u32 {
    unwrap(reply(&CALLER.get_or([0u8; 20])));
    0
}

fn withdraw_once() -> u32 {
    let target = TARGET.get_or([0u8; 20]);
    let amount = AMOUNT.get_or(0);
    let attempts = ATTEMPTS.get_or(0).saturating_add(1);
    ATTEMPTS.write(attempts);
    let mut calldata = [0u8; 4 + 16];
    calldata[..4].copy_from_slice(&0x02u32.to_le_bytes()); // vault.withdraw
    calldata[4..].copy_from_slice(&amount.to_le_bytes());
    let status = match call_contract(&target, &calldata, 0) {
        Ok(()) => 0,
        Err(error) => error.code(),
    };
    LAST.write(status);
    unwrap(emit(b"attempt", &status.to_le_bytes()));
    // A failed attempt is the expected outcome of an attack; the call itself
    // still succeeds, which is what makes the receipt readable.
    0
}

fn attempts() -> u32 {
    let value = ATTEMPTS.get_or(0);
    unwrap(reply(&value.to_le_bytes()));
    0
}

fn last_status() -> u32 {
    let value = LAST.get_or(0);
    unwrap(reply(&value.to_le_bytes()));
    0
}
