//! ARC-721 — the AuraLite non-fungible token standard (X5).
//!
//! One contract, many tokens, each with exactly one owner at a time. The
//! difference from ARC-20 is not cosmetic: ownership is per **token id**, so every
//! mutating method has to answer three questions in a fixed order — does the token
//! exist, does the caller have the right, and will the write keep the per-account
//! counter consistent.
//!
//! # Method table
//!
//! | selector | method | arguments | answer |
//! |---|---|---|---|
//! | `0x01` | `mint` | `to: [u8;20]`, `token_id: u128` | — |
//! | `0x02` | `transfer` | `to: [u8;20]`, `token_id: u128` | — |
//! | `0x03` | `approve` | `spender: [u8;20]`, `token_id: u128` | — |
//! | `0x04` | `transfer_from` | `from: [u8;20]`, `to: [u8;20]`, `token_id: u128` | — |
//! | `0x05` | `owner_of` | `token_id: u128` | `[u8;20]` |
//! | `0x06` | `get_approved` | `token_id: u128` | `[u8;20]` |
//! | `0x07` | `balance_of` | `who: [u8;20]` | `u128` |
//! | `0x08` | `total_supply` | — | `u128` |
//! | `0x09` | `burn` | `token_id: u128` | — |
//! | `0x0a` | `owner` | — | `[u8;20]` |
//!
//! # Storage layout
//!
//! | key | value | note |
//! |---|---|---|
//! | `t` | `u128` | number of tokens that exist |
//! | `o` | `[u8;20]` | contract owner (may mint) |
//! | `w` + token id | `[u8;20]` | owner of a token; **absent = does not exist** |
//! | `a` + token id | `[u8;20]` | approved spender, or absent |
//! | `b` + address | `u128` | how many tokens an address holds |
//!
//! "Exists" is not a flag: a token exists exactly when its owner slot is present.
//! A separate flag would be a second source of truth, and the first bug in any
//! such contract is the two of them disagreeing.
//!
//! # Transfers clear the approval
//!
//! Every transfer — by the owner or by the approved spender — deletes the
//! approval slot. Keeping it would let a spender move the token again after the
//! owner had already handed it to someone else, which is the classic NFT
//! stale-approval bug.

#![no_std]

use aura_sdk::prelude::*;

aura_sdk::panic_handler!();

/// Selector: mint a token (contract owner only).
pub const SEL_MINT: u32 = 0x01;
/// Selector: transfer a token the caller owns.
pub const SEL_TRANSFER: u32 = 0x02;
/// Selector: approve a spender for one token.
pub const SEL_APPROVE: u32 = 0x03;
/// Selector: transfer a token on behalf of its owner.
pub const SEL_TRANSFER_FROM: u32 = 0x04;
/// Selector: read a token's owner.
pub const SEL_OWNER_OF: u32 = 0x05;
/// Selector: read a token's approved spender.
pub const SEL_GET_APPROVED: u32 = 0x06;
/// Selector: how many tokens an address holds.
pub const SEL_BALANCE_OF: u32 = 0x07;
/// Selector: how many tokens exist.
pub const SEL_TOTAL_SUPPLY: u32 = 0x08;
/// Selector: destroy a token the caller owns.
pub const SEL_BURN: u32 = 0x09;
/// Selector: read the contract owner.
pub const SEL_OWNER: u32 = 0x0a;

/// The caller is not the owner of the token.
pub const ERR_NOT_TOKEN_OWNER: i32 = -6;
/// Checked arithmetic failed.
pub const ERR_OVERFLOW: i32 = -7;
/// The caller is not the contract owner.
pub const ERR_NOT_OWNER: i32 = -22;
/// A zero address is not a valid owner.
pub const ERR_ZERO_ADDRESS: i32 = -23;
/// The token id does not exist.
pub const ERR_NO_TOKEN: i32 = -25;
/// The token id already exists.
pub const ERR_TOKEN_EXISTS: i32 = -26;
/// The caller is neither the owner nor the approved spender.
pub const ERR_NOT_APPROVED: i32 = -27;

const TOTAL: Slot<u128> = Slot::at(b"t");
const OWNER: Slot<[u8; 20]> = Slot::at(b"o");

fn token_owner_key(token_id: u128) -> Key {
    unwrap(Key::new(&[b"w", &token_id.to_le_bytes()]))
}

fn approval_key(token_id: u128) -> Key {
    unwrap(Key::new(&[b"a", &token_id.to_le_bytes()]))
}

fn count_key(who: &[u8; 20]) -> Key {
    unwrap(Key::new(&[b"b", who]))
}

fn token_owner(token_id: u128) -> Option<[u8; 20]> {
    unwrap(Slot::<[u8; 20]>::with_key(token_owner_key(token_id)).get())
}

fn set_token_owner(token_id: u128, who: [u8; 20]) {
    Slot::<[u8; 20]>::with_key(token_owner_key(token_id)).write(who);
}

fn clear_token(token_id: u128) {
    unwrap(Slot::<[u8; 20]>::with_key(token_owner_key(token_id)).clear());
    unwrap(Slot::<[u8; 20]>::with_key(approval_key(token_id)).clear());
}

fn approved(token_id: u128) -> [u8; 20] {
    unwrap(Slot::<[u8; 20]>::with_key(approval_key(token_id)).get()).unwrap_or([0u8; 20])
}

fn set_approved(token_id: u128, who: [u8; 20]) {
    if who == [0u8; 20] {
        unwrap(Slot::<[u8; 20]>::with_key(approval_key(token_id)).clear());
    } else {
        Slot::<[u8; 20]>::with_key(approval_key(token_id)).write(who);
    }
}

fn holdings(who: &[u8; 20]) -> u128 {
    unwrap(Slot::<u128>::with_key(count_key(who)).get()).unwrap_or(0)
}

fn set_holdings(who: &[u8; 20], value: u128) {
    Slot::<u128>::with_key(count_key(who)).write(value);
}

fn is_zero(address: &[u8; 20]) -> bool {
    address.iter().all(|byte| *byte == 0)
}

fn reply_u128(value: u128) -> u32 {
    unwrap(reply(&value.to_le_bytes()));
    0
}

fn reply_addr(value: [u8; 20]) -> u32 {
    unwrap(reply(&value));
    0
}

/// Emit `transfer(from, to, token_id)`.
fn emit_transfer(from: [u8; 20], to: [u8; 20], token_id: u128) {
    let mut event = [0u8; 56];
    event[..20].copy_from_slice(&from);
    event[20..40].copy_from_slice(&to);
    event[40..].copy_from_slice(&token_id.to_le_bytes());
    unwrap(emit(b"transfer", &event));
}

/// Move `token_id` from `from` to `to`, keeping counts and approvals honest.
fn move_token(from: [u8; 20], to: [u8; 20], token_id: u128) -> i32 {
    if is_zero(&to) {
        return ERR_ZERO_ADDRESS;
    }
    let Some(from_count) = holdings(&from).checked_sub(1) else {
        return ERR_NOT_TOKEN_OWNER;
    };
    let Some(to_count) = holdings(&to).checked_add(1) else {
        return ERR_OVERFLOW;
    };
    set_token_owner(token_id, to);
    set_holdings(&from, from_count);
    set_holdings(&to, to_count);
    set_approved(token_id, [0u8; 20]);
    emit_transfer(from, to, token_id);
    0
}

#[constructor]
pub fn init() -> u32 {
    OWNER.write(unwrap(caller()));
    TOTAL.write(0);
    0
}

#[entrypoint]
pub fn run() -> u32 {
    dispatch! {
        SEL_MINT => mint([u8; 20], u128),
        SEL_TRANSFER => transfer([u8; 20], u128),
        SEL_APPROVE => approve([u8; 20], u128),
        SEL_TRANSFER_FROM => transfer_from([u8; 20], [u8; 20], u128),
        SEL_OWNER_OF => owner_of(u128),
        SEL_GET_APPROVED => get_approved(u128),
        SEL_BALANCE_OF => balance_of([u8; 20]),
        SEL_TOTAL_SUPPLY => total_supply(),
        SEL_BURN => burn(u128),
        SEL_OWNER => owner(),
    }
}

fn mint(to: [u8; 20], token_id: u128) -> u32 {
    let caller = unwrap(caller());
    if caller != OWNER.get_or([0u8; 20]) {
        return ERR_NOT_OWNER as u32;
    }
    if is_zero(&to) {
        return ERR_ZERO_ADDRESS as u32;
    }
    if token_owner(token_id).is_some() {
        return ERR_TOKEN_EXISTS as u32;
    }
    let Some(next_total) = TOTAL.get_or(0).checked_add(1) else {
        return ERR_OVERFLOW as u32;
    };
    let Some(next_count) = holdings(&to).checked_add(1) else {
        return ERR_OVERFLOW as u32;
    };
    set_token_owner(token_id, to);
    set_holdings(&to, next_count);
    TOTAL.write(next_total);
    let mut event = [0u8; 36];
    event[..20].copy_from_slice(&to);
    event[20..].copy_from_slice(&token_id.to_le_bytes());
    unwrap(emit(b"mint", &event));
    0
}

fn transfer(to: [u8; 20], token_id: u128) -> u32 {
    let caller = unwrap(caller());
    match token_owner(token_id) {
        None => ERR_NO_TOKEN as u32,
        Some(owner) if owner != caller => ERR_NOT_TOKEN_OWNER as u32,
        Some(owner) => move_token(owner, to, token_id) as u32,
    }
}

fn approve(spender: [u8; 20], token_id: u128) -> u32 {
    let caller = unwrap(caller());
    match token_owner(token_id) {
        None => ERR_NO_TOKEN as u32,
        Some(owner) if owner != caller => ERR_NOT_TOKEN_OWNER as u32,
        Some(_) => {
            set_approved(token_id, spender);
            // 20 bytes of spender + 16 of token id. The width matters: an
            // over-long buffer here is a panic in the guest, which the host sees
            // as a trap — a bug this gate caught once already.
            let mut event = [0u8; 36];
            event[..20].copy_from_slice(&spender);
            event[20..].copy_from_slice(&token_id.to_le_bytes());
            unwrap(emit(b"approve", &event));
            0
        }
    }
}

fn transfer_from(from: [u8; 20], to: [u8; 20], token_id: u128) -> u32 {
    let caller = unwrap(caller());
    let owner = match token_owner(token_id) {
        None => return ERR_NO_TOKEN as u32,
        Some(owner) => owner,
    };
    if owner != from {
        return ERR_NOT_TOKEN_OWNER as u32;
    }
    if caller != owner && approved(token_id) != caller {
        return ERR_NOT_APPROVED as u32;
    }
    move_token(from, to, token_id) as u32
}

fn owner_of(token_id: u128) -> u32 {
    match token_owner(token_id) {
        Some(owner) => reply_addr(owner),
        None => ERR_NO_TOKEN as u32,
    }
}

fn get_approved(token_id: u128) -> u32 {
    if token_owner(token_id).is_none() {
        return ERR_NO_TOKEN as u32;
    }
    reply_addr(approved(token_id))
}

fn balance_of(who: [u8; 20]) -> u32 {
    reply_u128(holdings(&who))
}

fn total_supply() -> u32 {
    reply_u128(TOTAL.get_or(0))
}

fn burn(token_id: u128) -> u32 {
    let caller = unwrap(caller());
    let owner = match token_owner(token_id) {
        None => return ERR_NO_TOKEN as u32,
        Some(owner) => owner,
    };
    if owner != caller {
        return ERR_NOT_TOKEN_OWNER as u32;
    }
    let Some(next_count) = holdings(&owner).checked_sub(1) else {
        return ERR_NOT_TOKEN_OWNER as u32;
    };
    let Some(next_total) = TOTAL.get_or(0).checked_sub(1) else {
        return ERR_OVERFLOW as u32;
    };
    clear_token(token_id);
    set_holdings(&owner, next_count);
    TOTAL.write(next_total);
    emit_transfer(owner, [0u8; 20], token_id);
    0
}

fn owner() -> u32 {
    reply_addr(OWNER.get_or([0u8; 20]))
}
