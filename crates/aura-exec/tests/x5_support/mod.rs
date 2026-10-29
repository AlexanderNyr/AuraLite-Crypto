//! Shared harness for the X5 standard gates.
//!
//! Three contracts, one way of driving them: deploy through the real
//! `Lifecycle`, call through `Lifecycle::call_data`, commit the diff into a real
//! `MerkleTree`. Only the selector tables differ, and those live in each test
//! file because they are part of the standard being asserted.
//!
//! The support module is `mod`-included, not a test target, so Cargo does not try
//! to run it.

#![allow(dead_code)]

use aura_exec::lifecycle::Lifecycle;
use aura_exec::{BlockContext, ContractCall, ContractReceipt, ExecStatus};
use aura_state::{Account, MerkleTree, StateKey, apply_diff};
use aura_types::{Address, Amount, Hash, Nonce};

/// A call's status as a contract reports it: a returned `i32`, sign-extended.
pub fn returned(code: i32) -> ExecStatus {
    ExecStatus::Returned(i64::from(code))
}

pub fn address(bytes: [u8; 20]) -> Address {
    Address::from_slice(&bytes).expect("20-byte address")
}

pub fn call_from(contract: [u8; 20], who: [u8; 20], gas: u64) -> ContractCall {
    ContractCall {
        contract: address(contract),
        caller: address(who),
        block: BlockContext {
            height: 9,
            timestamp_ms: 1_700_000_000_000,
        },
        gas_limit: gas,
    }
}

/// A tree with empty accounts for every address that will transact.
pub fn base(who: &[[u8; 20]]) -> MerkleTree {
    let entries: Vec<(Hash, Vec<u8>)> = who
        .iter()
        .map(|address| {
            let account =
                Account::new(Nonce::new(0), Amount::from_units(0), Hash::ZERO, Hash::ZERO);
            (
                StateKey::account(&self::address(*address)).digest(),
                account.to_bytes(),
            )
        })
        .collect();
    MerkleTree::from_entries(1, &entries)
}

/// Deploy `code` at `contract` from `deployer` and return the committed tree.
pub fn deploy(
    code: &[u8],
    contract: [u8; 20],
    deployer: [u8; 20],
    accounts: &[[u8; 20]],
) -> (Lifecycle, MerkleTree) {
    let lifecycle = Lifecycle::new();
    let start = base(accounts);
    let receipt = lifecycle
        .deploy(
            &start,
            call_from(contract, deployer, 1_000_000),
            code.to_vec(),
            Amount::from_units(0),
            None,
        )
        .expect("deploy runs");
    assert!(
        receipt.committed(),
        "init must commit: {:?}",
        receipt.status
    );
    let tree = commit_diff(&start, receipt.diff.as_ref().expect("deploy diff"));
    (lifecycle, tree)
}

/// Apply a receipt's diff to `tree`, or return it unchanged when nothing committed.
pub fn commit(tree: &MerkleTree, receipt: &ContractReceipt) -> MerkleTree {
    match &receipt.diff {
        Some(diff) => commit_diff(tree, diff),
        None => tree.clone(),
    }
}

fn commit_diff(tree: &MerkleTree, diff: &aura_state::StateDiff) -> MerkleTree {
    apply_diff(tree, diff).expect("diff applies")
}

/// Calldata: 4-byte little-endian selector, then the arguments.
pub fn data(selector: u32, args: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + args.len());
    out.extend_from_slice(&selector.to_le_bytes());
    out.extend_from_slice(args);
    out
}

pub fn addr_arg(who: [u8; 20]) -> Vec<u8> {
    who.to_vec()
}

pub fn u128_arg(value: u128) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

pub fn u8_arg(value: u8) -> Vec<u8> {
    vec![value]
}

/// One call, returning the receipt.
pub fn invoke(
    lifecycle: &Lifecycle,
    tree: &MerkleTree,
    contract: [u8; 20],
    who: [u8; 20],
    calldata: Vec<u8>,
) -> ContractReceipt {
    lifecycle
        .call_data(tree, call_from(contract, who, 1_000_000), calldata)
        .expect("call runs")
}

/// One call whose answer is a `u128`.
pub fn read_u128(
    lifecycle: &Lifecycle,
    tree: &MerkleTree,
    contract: [u8; 20],
    calldata: Vec<u8>,
) -> u128 {
    let receipt = invoke(lifecycle, tree, contract, ASKER, calldata);
    assert_eq!(
        receipt.status,
        ExecStatus::Returned(0),
        "{:?}",
        receipt.status
    );
    assert_eq!(receipt.return_data.len(), 16, "an answer must be 16 bytes");
    u128::from_le_bytes(receipt.return_data.clone().try_into().expect("16 bytes"))
}

/// One call whose answer is an address (20 bytes).
pub fn read_addr(
    lifecycle: &Lifecycle,
    tree: &MerkleTree,
    contract: [u8; 20],
    calldata: Vec<u8>,
) -> [u8; 20] {
    let receipt = invoke(lifecycle, tree, contract, ASKER, calldata);
    assert_eq!(
        receipt.status,
        ExecStatus::Returned(0),
        "{:?}",
        receipt.status
    );
    assert_eq!(receipt.return_data.len(), 20, "an answer must be 20 bytes");
    receipt.return_data.clone().try_into().expect("20 bytes")
}

/// The account a read-only helper call comes from. It never matters: a read
/// cannot write, and the standards require no ownership to read.
pub const ASKER: [u8; 20] = [0xee; 20];

/// A raw storage value of `contract`, keyed exactly (zero-padded to 32 bytes).
pub fn storage(tree: &MerkleTree, contract: [u8; 20], key: &[u8]) -> Option<Vec<u8>> {
    let mut slot = [0u8; 32];
    assert!(key.len() <= 32, "a slot key is at most 32 bytes");
    slot[..key.len()].copy_from_slice(key);
    tree.get_digest(&StateKey::storage(&address(contract), &slot).digest())
        .map(<[u8]>::to_vec)
}

/// Assert that a call changed nothing in the tree's content.
///
/// X2's rule is that `Returned(_)` — any value, negative included — is a
/// *successful* call and gets a diff. A standard that answers "not enough
/// balance" with `-6` is therefore still a committed call; what makes that honest
/// is that it writes nothing, so the change set is empty and the entries of the
/// tree are unchanged. A trap would give `diff: None` and throw the reason away,
/// which is why the standards do not use one for a business-level refusal.
pub fn assert_no_effect(receipt: &ContractReceipt, before: &MerkleTree) {
    let diff = receipt
        .diff
        .as_ref()
        .expect("a returned status carries a diff");
    assert!(
        diff.changes.is_empty(),
        "a refusal must not write: {} entries",
        diff.changes.len()
    );
    let after = commit(before, receipt);
    assert_eq!(
        after.entries(),
        before.entries(),
        "the content of the tree must be unchanged"
    );
}

/// Events are encoded by the host as `len(topic) u32 LE || topic || data`.
pub fn split_event(event: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let len = u32::from_le_bytes(event[..4].try_into().expect("4-byte length")) as usize;
    (event[4..4 + len].to_vec(), event[4 + len..].to_vec())
}
