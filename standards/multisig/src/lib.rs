//! ARC-M1 — the AuraLite minimal multisig (X5).
//!
//! Three signers, a threshold, and one action: send native ALCRY out of the
//! contract's own balance. Small on purpose — a multisig is the one contract where
//! "we could not fit it in a review" is a reason not to ship it.
//!
//! # Method table
//!
//! | selector | method | arguments | answer |
//! |---|---|---|---|
//! | `0x01` | `set_policy` | `threshold: u8`, `s1`, `s2`, `s3: [u8;20]` | — |
//! | `0x02` | `propose` | `kind: u8`, `to: [u8;20]`, `amount: u128` | `u128` (proposal id) |
//! | `0x03` | `approve` | `pid: u128` | — |
//! | `0x04` | `revoke` | `pid: u128` | — |
//! | `0x05` | `proposal` | `pid: u128` | 33 bytes: `executed u8`, `approvals u128`, `amount u128` |
//! | `0x06` | `signers` | — | 60 bytes: three addresses |
//! | `0x07` | `threshold` | — | 1 byte |
//! | `0x08` | `creator` | — | `[u8;20]` |
//!
//! # Rules that are not negotiable
//!
//! * the policy is set **once**, by the deployer, and can never be changed — a
//!   multisig that can be reconfigured by its own proposal is a single key with
//!   extra steps;
//! * only a signer may propose, and only a signer may vote, and never twice: the
//!   vote is a state flag per `(proposal, signer)`;
//! * a proposal executes **exactly once**: the `executed` flag is written before
//!   the transfer and rolled back if the transfer fails, so a failed execution is
//!   retryable and a successful one cannot repeat;
//! * execution requires the contract's own balance to cover the amount — checked
//!   *before* the flag, so a proposal cannot be marked executed by a transaction
//!   that had nothing to send.
//!
//! # What is deliberately missing
//!
//! No arbitrary call payload (that is a cross-contract call, X6), no signer
//! replacement, no timelock, no per-proposal expiry. Each of those is a design
//! decision with its own failure mode; none belongs in a "minimal" contract.

#![no_std]

use aura_sdk::prelude::*;

aura_sdk::panic_handler!();

/// Selector: install the policy (deployer only, once).
pub const SEL_SET_POLICY: u32 = 0x01;
/// Selector: open a proposal (signer only).
pub const SEL_PROPOSE: u32 = 0x02;
/// Selector: vote for a proposal (signer only, once).
pub const SEL_APPROVE: u32 = 0x03;
/// Selector: take a vote back while the proposal is unexecuted.
pub const SEL_REVOKE: u32 = 0x04;
/// Selector: read a proposal.
pub const SEL_PROPOSAL: u32 = 0x05;
/// Selector: read the signer set.
pub const SEL_SIGNERS: u32 = 0x06;
/// Selector: read the threshold.
pub const SEL_THRESHOLD: u32 = 0x07;
/// Selector: read the creator.
pub const SEL_CREATOR: u32 = 0x08;

/// Only the deployer may install the policy.
pub const ERR_NOT_CREATOR: i32 = -22;
/// A zero address is not a signer.
pub const ERR_ZERO_ADDRESS: i32 = -23;
/// The caller is not a signer.
pub const ERR_NOT_SIGNER: i32 = -28;
/// The policy is already installed, or the arguments are unusable.
pub const ERR_POLICY: i32 = -29;
/// The proposal does not exist.
pub const ERR_NO_PROPOSAL: i32 = -30;
/// The proposal already ran.
pub const ERR_ALREADY_EXECUTED: i32 = -31;
/// This signer already voted.
pub const ERR_ALREADY_VOTED: i32 = -32;
/// The contract cannot cover the amount.
pub const ERR_INSUFFICIENT_BALANCE: i32 = -6;
/// Checked arithmetic failed.
pub const ERR_OVERFLOW: i32 = -7;

/// How many signers the policy has. Fixed: a variable list needs calldata on
/// deploy, and `deploy` deliberately has none.
pub const SIGNER_COUNT: usize = 3;

/// Action kind: send native ALCRY to an address.
pub const KIND_TRANSFER: u8 = 1;

const CREATOR: Slot<[u8; 20]> = Slot::at(b"c");
const THRESHOLD: Slot<u8> = Slot::at(b"n");
const PROPOSALS: Slot<u128> = Slot::at(b"p");

fn signer_slot(index: usize) -> Slot<[u8; 20]> {
    match index {
        0 => Slot::at(b"s0"),
        1 => Slot::at(b"s1"),
        _ => Slot::at(b"s2"),
    }
}

fn signers() -> [[u8; 20]; SIGNER_COUNT] {
    [
        signer_slot(0).get().unwrap_or(None).unwrap_or([0u8; 20]),
        signer_slot(1).get().unwrap_or(None).unwrap_or([0u8; 20]),
        signer_slot(2).get().unwrap_or(None).unwrap_or([0u8; 20]),
    ]
}

/// Key of a per-proposal field: `prefix + pid`.
fn pid_key(prefix: &[u8], pid: u128) -> Key {
    unwrap(Key::new(&[prefix, &pid.to_le_bytes()]))
}

/// Key of a per-(proposal, signer) field.
///
/// `1 + 16 + 20 = 37` bytes does not fit a 32-byte slot, so the pair is hashed by
/// the node and the first 10 digest bytes are used — the same rule ARC-20 uses for
/// its allowances.
fn vote_key(pid: u128, signer: &[u8; 20]) -> Key {
    let mut buf = [0u8; 36];
    buf[..16].copy_from_slice(&pid.to_le_bytes());
    buf[16..].copy_from_slice(signer);
    let digest = unwrap(hash(&buf));
    unwrap(Key::new(&[b"v", &digest[..10]]))
}

fn is_zero(address: &[u8; 20]) -> bool {
    address.iter().all(|byte| *byte == 0)
}

fn is_signer(who: &[u8; 20]) -> bool {
    signers().iter().any(|signer| signer == who)
}

fn threshold() -> u8 {
    THRESHOLD.get().unwrap_or(None).unwrap_or(0)
}

fn executed(pid: u128) -> bool {
    unwrap(Slot::<u8>::with_key(pid_key(b"e", pid)).get())
        .map(|flag| flag == 1)
        .unwrap_or(false)
}

fn approvals(pid: u128) -> u128 {
    unwrap(Slot::<u128>::with_key(pid_key(b"y", pid)).get()).unwrap_or(0)
}

fn set_approvals(pid: u128, value: u128) {
    Slot::<u128>::with_key(pid_key(b"y", pid)).write(value);
}

fn voted(pid: u128, signer: &[u8; 20]) -> bool {
    unwrap(Slot::<u8>::with_key(vote_key(pid, signer)).get())
        .map(|flag| flag == 1)
        .unwrap_or(false)
}

fn proposal_exists(pid: u128) -> bool {
    unwrap(Slot::<u8>::with_key(pid_key(b"k", pid)).get()).is_some()
}

#[constructor]
pub fn init() -> u32 {
    CREATOR.write(unwrap(caller()));
    0
}

#[entrypoint]
pub fn run() -> u32 {
    dispatch! {
        SEL_SET_POLICY => set_policy(u8, [u8; 20], [u8; 20], [u8; 20]),
        SEL_PROPOSE => propose(u8, [u8; 20], u128),
        SEL_APPROVE => approve(u128),
        SEL_REVOKE => revoke(u128),
        SEL_PROPOSAL => proposal(u128),
        SEL_SIGNERS => signers_method(),
        SEL_THRESHOLD => threshold_method(),
        SEL_CREATOR => creator_method(),
    }
}

fn set_policy(threshold: u8, s1: [u8; 20], s2: [u8; 20], s3: [u8; 20]) -> u32 {
    let caller = unwrap(caller());
    if caller != CREATOR.get_or([0u8; 20]) {
        return ERR_NOT_CREATOR as u32;
    }
    if THRESHOLD.get().unwrap_or(None).is_some() {
        return ERR_POLICY as u32;
    }
    let set = [s1, s2, s3];
    if threshold == 0 || usize::from(threshold) > SIGNER_COUNT {
        return ERR_POLICY as u32;
    }
    if set.iter().any(is_zero) {
        return ERR_ZERO_ADDRESS as u32;
    }
    // Distinct signers: a duplicated address would silently lower the threshold.
    if set[0] == set[1] || set[1] == set[2] || set[0] == set[2] {
        return ERR_POLICY as u32;
    }
    for (index, signer) in set.iter().enumerate() {
        signer_slot(index).write(*signer);
    }
    THRESHOLD.write(threshold);
    0
}

fn propose(kind: u8, to: [u8; 20], amount: u128) -> u32 {
    let caller = unwrap(caller());
    if !is_signer(&caller) {
        return ERR_NOT_SIGNER as u32;
    }
    if kind != KIND_TRANSFER {
        return ERR_POLICY as u32;
    }
    if is_zero(&to) {
        return ERR_ZERO_ADDRESS as u32;
    }
    let Some(next) = PROPOSALS.get_or(0).checked_add(1) else {
        return ERR_OVERFLOW as u32;
    };
    // The id is the new counter value, so two proposals can never share one.
    let pid = next;
    Slot::<u8>::with_key(pid_key(b"k", pid)).write(kind);
    Slot::<[u8; 20]>::with_key(pid_key(b"d", pid)).write(to);
    Slot::<u128>::with_key(pid_key(b"m", pid)).write(amount);
    Slot::<u128>::with_key(pid_key(b"y", pid)).write(0);
    PROPOSALS.write(next);
    unwrap(reply(&pid.to_le_bytes()));
    0
}

fn approve(pid: u128) -> u32 {
    let caller = unwrap(caller());
    if !is_signer(&caller) {
        return ERR_NOT_SIGNER as u32;
    }
    if !proposal_exists(pid) {
        return ERR_NO_PROPOSAL as u32;
    }
    if executed(pid) {
        return ERR_ALREADY_EXECUTED as u32;
    }
    if voted(pid, &caller) {
        return ERR_ALREADY_VOTED as u32;
    }
    Slot::<u8>::with_key(vote_key(pid, &caller)).write(1);
    let Some(count) = approvals(pid).checked_add(1) else {
        return ERR_OVERFLOW as u32;
    };
    set_approvals(pid, count);
    if count < u128::from(threshold()) {
        return 0;
    }
    execute(pid)
}

fn revoke(pid: u128) -> u32 {
    let caller = unwrap(caller());
    if !is_signer(&caller) {
        return ERR_NOT_SIGNER as u32;
    }
    if !proposal_exists(pid) {
        return ERR_NO_PROPOSAL as u32;
    }
    if executed(pid) {
        return ERR_ALREADY_EXECUTED as u32;
    }
    if !voted(pid, &caller) {
        return ERR_ALREADY_VOTED as u32;
    }
    unwrap(Slot::<u8>::with_key(vote_key(pid, &caller)).clear());
    let Some(count) = approvals(pid).checked_sub(1) else {
        return ERR_OVERFLOW as u32;
    };
    set_approvals(pid, count);
    0
}

/// Send the proposal's amount. Called only from `approve`, only at the threshold.
fn execute(pid: u128) -> u32 {
    let to = unwrap(Slot::<[u8; 20]>::with_key(pid_key(b"d", pid)).get()).unwrap_or([0u8; 20]);
    let amount = unwrap(Slot::<u128>::with_key(pid_key(b"m", pid)).get()).unwrap_or(0);
    // Check the balance first: marking a proposal executed and then failing to pay
    // would leave a proposal that can never run and never be retried.
    let own = unwrap(self_address());
    let funds = match balance(&own) {
        Ok(funds) => funds,
        Err(error) => return error.code() as u32,
    };
    if funds < amount {
        return ERR_INSUFFICIENT_BALANCE as u32;
    }
    Slot::<u8>::with_key(pid_key(b"e", pid)).write(1);
    if let Err(error) = transfer(&to, amount) {
        // The money did not move, so the flag must not stay set: a proposal that
        // failed is retryable.
        unwrap(Slot::<u8>::with_key(pid_key(b"e", pid)).clear());
        return error.code() as u32;
    }
    let mut event = [0u8; 36];
    event[..20].copy_from_slice(&to);
    event[20..].copy_from_slice(&amount.to_le_bytes());
    unwrap(emit(b"executed", &event));
    0
}

fn proposal(pid: u128) -> u32 {
    if !proposal_exists(pid) {
        return ERR_NO_PROPOSAL as u32;
    }
    let amount = unwrap(Slot::<u128>::with_key(pid_key(b"m", pid)).get()).unwrap_or(0);
    let mut out = [0u8; 33];
    out[0] = u8::from(executed(pid));
    out[1..17].copy_from_slice(&approvals(pid).to_le_bytes());
    out[17..].copy_from_slice(&amount.to_le_bytes());
    unwrap(reply(&out));
    0
}

fn signers_method() -> u32 {
    let mut out = [0u8; 60];
    for (index, signer) in signers().iter().enumerate() {
        out[index * 20..(index + 1) * 20].copy_from_slice(signer);
    }
    unwrap(reply(&out));
    0
}

fn threshold_method() -> u32 {
    unwrap(reply(&[threshold()]));
    0
}

fn creator_method() -> u32 {
    unwrap(reply(&CREATOR.get_or([0u8; 20])));
    0
}
