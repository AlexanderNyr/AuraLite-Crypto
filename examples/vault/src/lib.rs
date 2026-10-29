//! X6 reference contract: a vault that pays out **before** it debits.
//!
//! This contract is deliberately written the way a reentrancy bug is written. It
//! sends the money first and updates its ledger afterwards, which is exactly the
//! ordering that lets a receiver call back in and withdraw again. It is shipped
//! this way on purpose: the gate that matters is not "does a nice contract stay
//! safe", it is "does the *machine* stop a contract that is written badly from
//! draining its neighbours".
//!
//! The shape of a withdrawal:
//!
//! ```text
//! 1. check    ledger[caller] >= amount
//! 2. call     caller <- amount, calldata = `on_receive()`      <-- re-entry point
//! 3. debit    ledger[caller] -= amount                          <-- too late by design
//! ```
//!
//! Step 2 hands control to code the vault does not own. X6's rule is that a
//! frame may not re-enter an address already on the call stack (`-14`), so the
//! naive ordering above is safe on this chain for a reason that belongs to the
//! node, not to the contract. `tools/nc_x6.py` disables that rule and requires
//! the gate to go red — with it disabled, the attacker in `examples/evil`
//! withdraws twice and the honest depositor cannot be paid out.
//!
//! A note on the failure of step 2: a receiver that reverts does **not** block
//! the withdrawal. The transfer is undone by the node (the callee's frame owns
//! its own value), the ledger is still debited, and the vault keeps the money —
//! a documented Stage 1 limitation, measured in `docs/evidence/x6/report.md`.

#![no_std]

use aura_sdk::prelude::*;

aura_sdk::panic_handler!();

/// Selector: credit an account's ledger (owner only).
pub const SEL_FUND: u32 = 0x01;
/// Selector: withdraw everything credited to the caller.
pub const SEL_WITHDRAW: u32 = 0x02;
/// Selector: the callback a receiver sees.
pub const SEL_ON_RECEIVE: u32 = 0x03;
/// Selector: read an account's ledger entry.
pub const SEL_DEPOSITED: u32 = 0x04;
/// Selector: read the owner.
pub const SEL_OWNER: u32 = 0x05;

/// `-30`: the caller is not the owner.
pub const ERR_NOT_OWNER: i32 = -30;
/// `-6`: the ledger does not cover the withdrawal.
pub const ERR_INSUFFICIENT: i32 = -6;
/// `-23`: a zero address is never a valid account.
pub const ERR_ZERO_ADDRESS: i32 = -23;

fn ledger_key(who: &[u8; 20]) -> Key {
    unwrap(Key::new(&[b"d", who]))
}

fn ledger_of(who: &[u8; 20]) -> u128 {
    Slot::<u128>::with_key(ledger_key(who)).get_or(0)
}

fn set_ledger(who: &[u8; 20], value: u128) {
    Slot::<u128>::with_key(ledger_key(who)).write(value);
}

fn owner() -> [u8; 20] {
    Slot::<[u8; 20]>::at(b"o").get_or([0u8; 20])
}

fn is_zero(address: &[u8; 20]) -> bool {
    address.iter().all(|byte| *byte == 0)
}

fn reply_u128(value: u128) -> u32 {
    unwrap(reply(&value.to_le_bytes()));
    0
}

/// The deployer owns the ledger. Runs inside `Lifecycle::deploy`.
#[constructor]
pub fn init() -> u32 {
    let deployer = unwrap(caller());
    Slot::<[u8; 20]>::at(b"o").write(deployer);
    0
}

#[entrypoint]
pub fn run() -> u32 {
    dispatch! {
        SEL_FUND => fund([u8; 20], u128),
        SEL_WITHDRAW => withdraw(u128),
        SEL_ON_RECEIVE => on_receive(),
        SEL_DEPOSITED => deposited([u8; 20]),
        SEL_OWNER => owner_method(),
    }
}

/// Credit `who` by `amount`. The native balance itself arrived at deploy.
fn fund(who: [u8; 20], amount: u128) -> u32 {
    if unwrap(caller()) != owner() {
        return ERR_NOT_OWNER as u32;
    }
    if is_zero(&who) {
        return ERR_ZERO_ADDRESS as u32;
    }
    let current = ledger_of(&who);
    let Some(next) = current.checked_add(amount) else {
        return ERR_INSUFFICIENT as u32;
    };
    set_ledger(&who, next);
    unwrap(emit(b"fund", &who));
    0
}

/// Pay `amount` to the caller, then debit the ledger — the unsafe order.
fn withdraw(amount: u128) -> u32 {
    let who = unwrap(caller());
    let credited = ledger_of(&who);
    if credited < amount {
        return ERR_INSUFFICIENT as u32;
    }
    // Step 2a: the money leaves. `env.transfer` is the native-coin path and it
    // does not call back, so it works for an account and for a contract alike.
    if let Err(error) = transfer(&who, amount) {
        return error.code() as u32;
    }
    // Step 2b: the re-entry point. The receiver is *told*, and its answer is
    // deliberately ignored: a receiver that reverts must not be able to block a
    // withdrawal, and on this chain it cannot re-enter anyway (`-14`). This is
    // the line that a reentrancy bug lives on — the ledger below is still not
    // debited.
    let calldata = SEL_ON_RECEIVE.to_le_bytes();
    let status = match call_contract(&who, &calldata, 0) {
        Ok(()) => 0i32,
        Err(error) => error.code(),
    };
    // Published, not swallowed silently: `-14` here is the node refusing to call
    // back into a frame that is still on the stack, and it is the single fact the
    // X6 gate reads to prove the reentrancy guard is on.
    unwrap(emit(b"notify", &status.to_le_bytes()));
    set_ledger(&who, credited - amount);
    unwrap(emit(b"withdraw", &amount.to_le_bytes()));
    0
}

/// The callback. A vault only acknowledges; a receiver that wants to attack
/// exports this selector and does its work there.
fn on_receive() -> u32 {
    0
}

fn deposited(who: [u8; 20]) -> u32 {
    reply_u128(ledger_of(&who))
}

fn owner_method() -> u32 {
    unwrap(reply(&owner()));
    0
}
