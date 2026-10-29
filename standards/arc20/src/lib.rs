//! ARC-20 — the AuraLite fungible token standard (X5).
//!
//! # What makes this a standard and not an example
//!
//! * the **method table is fixed and numbered**: a wallet calls selector `0x03`
//!   for `transfer_from` on every ARC-20 token and gets the same arguments;
//! * `from` is never an argument — the address a transfer spends from is
//!   [`aura_sdk::abi2::caller`], which comes from the chain;
//! * the arithmetic is checked and the failure codes are published, so a caller
//!   can distinguish "not enough balance" from "this would overflow";
//! * a successful method answers with `0`; `balance_of` and friends answer with
//!   exactly 16 little-endian bytes through `reply`.
//!
//! # Method table
//!
//! | selector | method | arguments | answer |
//! |---|---|---|---|
//! | `0x01` | `transfer` | `to: [u8;20]`, `amount: u128` | — |
//! | `0x02` | `approve` | `spender: [u8;20]`, `amount: u128` | — |
//! | `0x03` | `transfer_from` | `from: [u8;20]`, `to: [u8;20]`, `amount: u128` | — |
//! | `0x04` | `balance_of` | `who: [u8;20]` | `u128` |
//! | `0x05` | `allowance` | `owner: [u8;20]`, `spender: [u8;20]` | `u128` |
//! | `0x06` | `total_supply` | — | `u128` |
//! | `0x07` | `mint` | `to: [u8;20]`, `amount: u128` | — |
//! | `0x08` | `burn` | `amount: u128` | — |
//! | `0x09` | `owner` | — | `[u8;20]` |
//! | `0x0a` | `transfer_ownership` | `new_owner: [u8;20]` | — |
//!
//! # Storage layout
//!
//! | key | value | note |
//! |---|---|---|
//! | `t` | `u128` | total supply |
//! | `o` | `[u8;20]` | owner (may mint) |
//! | `b` + address | `u128` | balance, 21-byte key |
//! | `a` + address + 10 bytes of `BLAKE3(owner‖spender)` | `u128` | allowance |
//!
//! The allowance key is **hashed** because `owner‖spender` is 40 bytes and a slot
//! is 32: the alternative is a second indirection the standard does not need.
//! The digest is computed by the node (`env.hash`), so it is the same on every
//! node; a collision would need two pairs with the same first 80 bits of
//! BLAKE3.
//!
//! # What this contract deliberately does not do
//!
//! * no `transfer_from` reentrancy guard — there is nothing to re-enter: X5 has
//!   no cross-contract calls, and the guard belongs with them in X6;
//! * no token metadata (name/symbol/decimals) — it is not letter-addressable
//!   state, and a standard that reads it must agree on its encoding first;
//! * no `approve` race advice in code, only in the event stream: a spender that
//!   needs certainty reads `allowance` after the transaction commits.

#![no_std]

use aura_sdk::prelude::*;

aura_sdk::panic_handler!();

/// Selector: transfer from the caller.
pub const SEL_TRANSFER: u32 = 0x01;
/// Selector: approve a spender.
pub const SEL_APPROVE: u32 = 0x02;
/// Selector: transfer from an owner who approved the caller.
pub const SEL_TRANSFER_FROM: u32 = 0x03;
/// Selector: read a balance.
pub const SEL_BALANCE_OF: u32 = 0x04;
/// Selector: read an allowance.
pub const SEL_ALLOWANCE: u32 = 0x05;
/// Selector: read the total supply.
pub const SEL_TOTAL_SUPPLY: u32 = 0x06;
/// Selector: mint (owner only).
pub const SEL_MINT: u32 = 0x07;
/// Selector: burn from the caller.
pub const SEL_BURN: u32 = 0x08;
/// Selector: read the owner.
pub const SEL_OWNER: u32 = 0x09;
/// Selector: hand ownership over.
pub const SEL_TRANSFER_OWNERSHIP: u32 = 0x0a;

/// Not enough balance (the ABI's own code, reused so a caller needs one table).
pub const ERR_INSUFFICIENT_BALANCE: i32 = -6;
/// Checked arithmetic failed.
pub const ERR_OVERFLOW: i32 = -7;
/// The caller is not the owner.
pub const ERR_NOT_OWNER: i32 = -22;
/// A transfer to the zero address is a burn in disguise; refuse it.
pub const ERR_ZERO_ADDRESS: i32 = -23;
/// A `transfer_from` above the allowance.
pub const ERR_ALLOWANCE: i32 = -24;

const TOTAL: Slot<u128> = Slot::at(b"t");
const OWNER: Slot<[u8; 20]> = Slot::at(b"o");

/// Balance slot of `who`.
fn balance_key(who: &[u8; 20]) -> Key {
    unwrap(Key::new(&[b"b", who]))
}

/// Allowance slot of `(owner, spender)`, keyed by a truncated digest.
fn allowance_key(owner: &[u8; 20], spender: &[u8; 20]) -> Key {
    let mut buf = [0u8; 40];
    buf[..20].copy_from_slice(owner);
    buf[20..].copy_from_slice(spender);
    let digest = unwrap(hash(&buf));
    unwrap(Key::new(&[b"a", &digest[..10]]))
}

fn balance_of(who: &[u8; 20]) -> u128 {
    Slot::<u128>::with_key(balance_key(who)).get_or(0)
}

fn set_balance(who: &[u8; 20], value: u128) {
    Slot::<u128>::with_key(balance_key(who)).write(value);
}

fn allowance_of(owner: &[u8; 20], spender: &[u8; 20]) -> u128 {
    Slot::<u128>::with_key(allowance_key(owner, spender)).get_or(0)
}

fn set_allowance(owner: &[u8; 20], spender: &[u8; 20], value: u128) {
    Slot::<u128>::with_key(allowance_key(owner, spender)).write(value);
}

fn is_zero(address: &[u8; 20]) -> bool {
    address.iter().all(|byte| *byte == 0)
}

/// Answer with 16 little-endian bytes.
fn reply_u128(value: u128) -> u32 {
    unwrap(reply(&value.to_le_bytes()));
    0
}

/// Move `amount` from `from` to `to`, or return a published failure code.
///
/// The order is deliberate: check, then write. A partially applied transfer is
/// impossible because a failure returns before the first write — and even if a
/// later write failed, the call's trap/refusal would revert the whole overlay.
fn move_tokens(from: &[u8; 20], to: &[u8; 20], amount: u128) -> i32 {
    if is_zero(to) {
        return ERR_ZERO_ADDRESS;
    }
    let from_balance = balance_of(from);
    let Some(next_from) = from_balance.checked_sub(amount) else {
        return ERR_INSUFFICIENT_BALANCE;
    };
    let Some(next_to) = balance_of(to).checked_add(amount) else {
        return ERR_OVERFLOW;
    };
    if amount == 0 {
        // A zero transfer is a no-op, not an error: wallets send them by mistake
        // and refusing would make `transfer(x, 0)` a different kind of call.
        return 0;
    }
    set_balance(from, next_from);
    set_balance(to, next_to);
    let mut event = [0u8; 56];
    event[..20].copy_from_slice(from);
    event[20..40].copy_from_slice(to);
    event[40..].copy_from_slice(&amount.to_le_bytes());
    unwrap(emit(b"transfer", &event));
    0
}

/// Set the supply and the owner. Runs inside `Lifecycle::deploy`.
///
/// The deployer becomes the owner, so the initial supply can only be minted by a
/// signed transaction afterwards. A constructor that minted to an argument would
/// need calldata, and `deploy` has none — that is a deliberate X3 property: an
/// address's first state is a function of its code, not of a payload.
#[constructor]
pub fn init() -> u32 {
    let deployer = unwrap(caller());
    OWNER.write(deployer);
    TOTAL.write(0);
    0
}

/// Dispatch one ARC-20 call.
#[entrypoint]
pub fn run() -> u32 {
    dispatch! {
        SEL_TRANSFER => transfer([u8; 20], u128),
        SEL_APPROVE => approve([u8; 20], u128),
        SEL_TRANSFER_FROM => transfer_from([u8; 20], [u8; 20], u128),
        SEL_BALANCE_OF => balance_of_method([u8; 20]),
        SEL_ALLOWANCE => allowance_method([u8; 20], [u8; 20]),
        SEL_TOTAL_SUPPLY => total_supply(),
        SEL_MINT => mint([u8; 20], u128),
        SEL_BURN => burn(u128),
        SEL_OWNER => owner(),
        SEL_TRANSFER_OWNERSHIP => transfer_ownership([u8; 20]),
    }
}

fn transfer(to: [u8; 20], amount: u128) -> u32 {
    let from = unwrap(caller());
    move_tokens(&from, &to, amount) as u32
}

fn approve(spender: [u8; 20], amount: u128) -> u32 {
    if is_zero(&spender) {
        return ERR_ZERO_ADDRESS as u32;
    }
    let owner = unwrap(caller());
    set_allowance(&owner, &spender, amount);
    let mut event = [0u8; 56];
    event[..20].copy_from_slice(&owner);
    event[20..40].copy_from_slice(&spender);
    event[40..].copy_from_slice(&amount.to_le_bytes());
    unwrap(emit(b"approval", &event));
    0
}

fn transfer_from(from: [u8; 20], to: [u8; 20], amount: u128) -> u32 {
    let spender = unwrap(caller());
    let allowed = allowance_of(&from, &spender);
    if allowed < amount {
        return ERR_ALLOWANCE as u32;
    }
    match move_tokens(&from, &to, amount) {
        0 => {
            // Decrementing after a successful move keeps the failure path free of
            // writes; an infinite approval (`u128::MAX`) is not special-cased, so
            // the published rule is "the allowance always decreases by exactly
            // the amount".
            set_allowance(&from, &spender, allowed - amount);
            0
        }
        code => code as u32,
    }
}

fn balance_of_method(who: [u8; 20]) -> u32 {
    reply_u128(balance_of(&who))
}

fn allowance_method(owner: [u8; 20], spender: [u8; 20]) -> u32 {
    reply_u128(allowance_of(&owner, &spender))
}

fn total_supply() -> u32 {
    reply_u128(TOTAL.get_or(0))
}

fn mint(to: [u8; 20], amount: u128) -> u32 {
    let caller = unwrap(caller());
    if caller != OWNER.get_or([0u8; 20]) {
        return ERR_NOT_OWNER as u32;
    }
    if is_zero(&to) {
        return ERR_ZERO_ADDRESS as u32;
    }
    let Some(next_total) = TOTAL.get_or(0).checked_add(amount) else {
        return ERR_OVERFLOW as u32;
    };
    let Some(next_balance) = balance_of(&to).checked_add(amount) else {
        return ERR_OVERFLOW as u32;
    };
    TOTAL.write(next_total);
    set_balance(&to, next_balance);
    let mut event = [0u8; 36];
    event[..20].copy_from_slice(&to);
    event[20..].copy_from_slice(&amount.to_le_bytes());
    unwrap(emit(b"mint", &event));
    0
}

fn burn(amount: u128) -> u32 {
    let owner = unwrap(caller());
    let Some(next_balance) = balance_of(&owner).checked_sub(amount) else {
        return ERR_INSUFFICIENT_BALANCE as u32;
    };
    let Some(next_total) = TOTAL.get_or(0).checked_sub(amount) else {
        return ERR_OVERFLOW as u32;
    };
    if amount == 0 {
        return 0;
    }
    set_balance(&owner, next_balance);
    TOTAL.write(next_total);
    let mut event = [0u8; 36];
    event[..20].copy_from_slice(&owner);
    event[20..].copy_from_slice(&amount.to_le_bytes());
    unwrap(emit(b"burn", &event));
    0
}

fn owner() -> u32 {
    let value = OWNER.get_or([0u8; 20]);
    unwrap(reply(&value));
    0
}

fn transfer_ownership(new_owner: [u8; 20]) -> u32 {
    let caller = unwrap(caller());
    if caller != OWNER.get_or([0u8; 20]) {
        return ERR_NOT_OWNER as u32;
    }
    if is_zero(&new_owner) {
        return ERR_ZERO_ADDRESS as u32;
    }
    OWNER.write(new_owner);
    unwrap(emit(b"ownership", &new_owner));
    0
}
