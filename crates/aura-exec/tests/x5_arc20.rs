//! X5 gate (ARC-20): the fungible token standard, method by method.
//!
//! The bytecode under test is the shipped artifact
//! (`standards/arc20/wasm/arc20.wasm`), deployed through the real
//! `Lifecycle::deploy` and called through `Lifecycle::call_data` — no mocks, no
//! direct host calls. Every assertion is about observable behaviour: a status
//! code, a committed diff, a balance, an event, or 16 bytes of return data.

mod x5_support;

use aura_exec::lifecycle::Lifecycle;
use aura_exec::{ContractReceipt, ExecStatus};
use aura_state::{MerkleTree, StateView};
use x5_support::*;

const ARC20: &[u8] = include_bytes!("../../../standards/arc20/wasm/arc20.wasm");

const TOKEN: [u8; 20] = [0x11; 20];
const OWNER: [u8; 20] = [0x22; 20];
const ALICE: [u8; 20] = [0x33; 20];
const BOB: [u8; 20] = [0x44; 20];
const ZERO: [u8; 20] = [0x00; 20];

// The published selector table. A change here is a different standard.
const SEL_TRANSFER: u32 = 0x01;
const SEL_APPROVE: u32 = 0x02;
const SEL_TRANSFER_FROM: u32 = 0x03;
const SEL_BALANCE_OF: u32 = 0x04;
const SEL_ALLOWANCE: u32 = 0x05;
const SEL_TOTAL_SUPPLY: u32 = 0x06;
const SEL_MINT: u32 = 0x07;
const SEL_BURN: u32 = 0x08;
const SEL_OWNER: u32 = 0x09;
const SEL_TRANSFER_OWNERSHIP: u32 = 0x0a;

// The published failure codes.
const ERR_INSUFFICIENT_BALANCE: i32 = -6;
const ERR_OVERFLOW: i32 = -7;
const ERR_BAD_SELECTOR: i32 = -20;
const ERR_BAD_ARGS: i32 = -21;
const ERR_NOT_OWNER: i32 = -22;
const ERR_ZERO_ADDRESS: i32 = -23;
const ERR_ALLOWANCE: i32 = -24;

fn deploy_token() -> (Lifecycle, MerkleTree) {
    deploy(ARC20, TOKEN, OWNER, &[OWNER, ALICE, BOB])
}

fn invoke(
    lifecycle: &Lifecycle,
    tree: &MerkleTree,
    who: [u8; 20],
    calldata: Vec<u8>,
) -> ContractReceipt {
    x5_support::invoke(lifecycle, tree, TOKEN, who, calldata)
}

fn read_u128(lifecycle: &Lifecycle, tree: &MerkleTree, calldata: Vec<u8>) -> u128 {
    x5_support::read_u128(lifecycle, tree, TOKEN, calldata)
}

fn read_storage(tree: &MerkleTree, key: &[u8]) -> Option<Vec<u8>> {
    storage(tree, TOKEN, key)
}

fn transfer(to: [u8; 20], amount: u128) -> Vec<u8> {
    data(SEL_TRANSFER, &[addr_arg(to), u128_arg(amount)].concat())
}

fn balance_method(who: [u8; 20]) -> Vec<u8> {
    data(SEL_BALANCE_OF, &addr_arg(who))
}

fn allowance_method(owner: [u8; 20], spender: [u8; 20]) -> Vec<u8> {
    data(
        SEL_ALLOWANCE,
        &[addr_arg(owner), addr_arg(spender)].concat(),
    )
}

fn balance_of(lifecycle: &Lifecycle, tree: &MerkleTree, who: [u8; 20]) -> u128 {
    read_u128(lifecycle, tree, balance_method(who))
}

fn mint_to(lifecycle: &Lifecycle, tree: &MerkleTree, who: [u8; 20], amount: u128) -> MerkleTree {
    let receipt = invoke(
        lifecycle,
        tree,
        OWNER,
        data(SEL_MINT, &[addr_arg(who), u128_arg(amount)].concat()),
    );
    assert_eq!(receipt.status, ExecStatus::Returned(0), "mint must succeed");
    commit(tree, &receipt)
}

#[test]
fn arc20_deploys_and_records_its_owner() {
    let (lifecycle, tree) = deploy_token();
    assert_eq!(
        read_storage(&tree, b"o").expect("owner key"),
        OWNER.to_vec()
    );
    assert_eq!(
        read_storage(&tree, b"t").expect("supply key"),
        0u128.to_le_bytes()
    );
    let receipt = invoke(&lifecycle, &tree, ALICE, data(SEL_OWNER, &[]));
    assert_eq!(receipt.status, ExecStatus::Returned(0));
    assert_eq!(receipt.return_data, OWNER.to_vec());
}

#[test]
fn mint_is_owner_only_and_moves_the_total() {
    let (lifecycle, tree) = deploy_token();
    let denied = invoke(
        &lifecycle,
        &tree,
        ALICE,
        data(SEL_MINT, &[addr_arg(ALICE), u128_arg(100)].concat()),
    );
    assert_eq!(denied.status, returned(ERR_NOT_OWNER));
    assert_no_effect(&denied, &tree);

    let minted = invoke(
        &lifecycle,
        &tree,
        OWNER,
        data(SEL_MINT, &[addr_arg(ALICE), u128_arg(1_000)].concat()),
    );
    assert_eq!(minted.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &minted);
    assert_eq!(
        read_u128(&lifecycle, &tree, data(SEL_TOTAL_SUPPLY, &[])),
        1_000
    );
    assert_eq!(balance_of(&lifecycle, &tree, ALICE), 1_000);

    let (topic, payload) = split_event(&minted.events[0]);
    assert_eq!(topic, b"mint");
    assert_eq!(&payload[20..], &1_000u128.to_le_bytes());
}

#[test]
fn transfer_moves_balance_and_refuses_what_it_must() {
    let (lifecycle, tree) = deploy_token();
    let tree = mint_to(&lifecycle, &tree, ALICE, 1_000);

    let moved = invoke(&lifecycle, &tree, ALICE, transfer(BOB, 400));
    assert_eq!(moved.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &moved);
    assert_eq!(balance_of(&lifecycle, &tree, ALICE), 600);
    assert_eq!(balance_of(&lifecycle, &tree, BOB), 400);

    let too_much = invoke(&lifecycle, &tree, ALICE, transfer(BOB, 601));
    assert_eq!(too_much.status, returned(ERR_INSUFFICIENT_BALANCE));
    assert_no_effect(&too_much, &tree);

    let to_zero = invoke(&lifecycle, &tree, ALICE, transfer(ZERO, 1));
    assert_eq!(to_zero.status, returned(ERR_ZERO_ADDRESS));

    let zero = invoke(&lifecycle, &tree, ALICE, transfer(BOB, 0));
    assert_eq!(zero.status, ExecStatus::Returned(0));
    assert_eq!(
        zero.diff
            .expect("a no-op yields an empty diff")
            .changes
            .len(),
        0,
        "a zero transfer must not touch storage"
    );
}

#[test]
fn approve_and_transfer_from_spend_exactly_the_allowance() {
    let (lifecycle, tree) = deploy_token();
    let tree = mint_to(&lifecycle, &tree, ALICE, 1_000);

    let approved = invoke(
        &lifecycle,
        &tree,
        ALICE,
        data(SEL_APPROVE, &[addr_arg(BOB), u128_arg(250)].concat()),
    );
    assert_eq!(approved.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &approved);
    assert_eq!(
        read_u128(&lifecycle, &tree, allowance_method(ALICE, BOB)),
        250
    );

    let spent = invoke(
        &lifecycle,
        &tree,
        BOB,
        data(
            SEL_TRANSFER_FROM,
            &[addr_arg(ALICE), addr_arg(BOB), u128_arg(100)].concat(),
        ),
    );
    assert_eq!(spent.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &spent);
    assert_eq!(
        read_u128(&lifecycle, &tree, allowance_method(ALICE, BOB)),
        150,
        "the allowance decreases by exactly the amount"
    );
    assert_eq!(balance_of(&lifecycle, &tree, BOB), 100);

    let over = invoke(
        &lifecycle,
        &tree,
        BOB,
        data(
            SEL_TRANSFER_FROM,
            &[addr_arg(ALICE), addr_arg(BOB), u128_arg(151)].concat(),
        ),
    );
    assert_eq!(over.status, returned(ERR_ALLOWANCE));
    assert_no_effect(&over, &tree);
    assert_eq!(
        read_u128(&lifecycle, &tree, allowance_method(ALICE, BOB)),
        150
    );
}

#[test]
fn burn_reduces_balance_and_supply() {
    let (lifecycle, tree) = deploy_token();
    let tree = mint_to(&lifecycle, &tree, ALICE, 1_000);
    let burned = invoke(&lifecycle, &tree, ALICE, data(SEL_BURN, &u128_arg(400)));
    assert_eq!(burned.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &burned);
    assert_eq!(balance_of(&lifecycle, &tree, ALICE), 600);
    assert_eq!(
        read_u128(&lifecycle, &tree, data(SEL_TOTAL_SUPPLY, &[])),
        600
    );

    let too_much = invoke(&lifecycle, &tree, ALICE, data(SEL_BURN, &u128_arg(601)));
    assert_eq!(too_much.status, returned(ERR_INSUFFICIENT_BALANCE));
}

#[test]
fn arithmetic_overflow_is_refused_not_wrapped() {
    let (lifecycle, tree) = deploy_token();
    let tree = mint_to(&lifecycle, &tree, ALICE, u128::MAX - 10);
    let receipt = invoke(
        &lifecycle,
        &tree,
        OWNER,
        data(SEL_MINT, &[addr_arg(ALICE), u128_arg(11)].concat()),
    );
    assert_eq!(receipt.status, returned(ERR_OVERFLOW));
    assert_no_effect(&receipt, &tree);
}

#[test]
fn ownership_hands_over_and_the_old_owner_loses_the_right() {
    let (lifecycle, tree) = deploy_token();
    let handover = invoke(
        &lifecycle,
        &tree,
        OWNER,
        data(SEL_TRANSFER_OWNERSHIP, &addr_arg(ALICE)),
    );
    assert_eq!(handover.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &handover);

    let denied = invoke(
        &lifecycle,
        &tree,
        OWNER,
        data(SEL_MINT, &[addr_arg(OWNER), u128_arg(1)].concat()),
    );
    assert_eq!(denied.status, returned(ERR_NOT_OWNER));

    let allowed = invoke(
        &lifecycle,
        &tree,
        ALICE,
        data(SEL_MINT, &[addr_arg(ALICE), u128_arg(1)].concat()),
    );
    assert_eq!(allowed.status, ExecStatus::Returned(0));
}

#[test]
fn calldata_that_does_not_decode_is_refused() {
    let (lifecycle, tree) = deploy_token();
    let cases: Vec<(Vec<u8>, i32, &str)> = vec![
        (vec![], ERR_BAD_SELECTOR, "no calldata at all"),
        (vec![0x01, 0x00], ERR_BAD_SELECTOR, "half a selector"),
        (data(0xffff, &[]), ERR_BAD_SELECTOR, "unknown selector"),
        (
            data(SEL_TRANSFER, &addr_arg(BOB)),
            ERR_BAD_ARGS,
            "missing amount",
        ),
        (
            data(
                SEL_TRANSFER,
                &[addr_arg(BOB), u128_arg(1), vec![0]].concat(),
            ),
            ERR_BAD_ARGS,
            "one trailing byte",
        ),
        (
            data(SEL_TOTAL_SUPPLY, &[0]),
            ERR_BAD_ARGS,
            "an argument for a nullary method",
        ),
    ];
    for (calldata, expected, what) in cases {
        let receipt = invoke(&lifecycle, &tree, ALICE, calldata);
        assert_eq!(
            receipt.status,
            returned(expected),
            "{what} must be refused with {expected}"
        );
        assert_no_effect(&receipt, &tree);
    }
}

#[test]
fn a_refused_call_commits_nothing() {
    let (lifecycle, tree) = deploy_token();
    let tree = mint_to(&lifecycle, &tree, ALICE, 1_000);
    let before = tree.entries();
    let receipt = invoke(&lifecycle, &tree, ALICE, transfer(BOB, 5_000));
    assert_eq!(receipt.status, returned(ERR_INSUFFICIENT_BALANCE));
    assert_no_effect(&receipt, &tree);
    assert_eq!(before, tree.entries());
}

#[test]
fn the_same_calldata_twice_produces_the_same_root() {
    let run = || {
        let (lifecycle, tree) = deploy_token();
        let tree = mint_to(&lifecycle, &tree, ALICE, 1_000);
        let receipt = invoke(&lifecycle, &tree, ALICE, transfer(BOB, 123));
        let committed = commit(&tree, &receipt);
        format!(
            "{:?}{:?}",
            committed.root_at(committed.version()).expect("root"),
            committed.entries()
        )
    };
    assert_eq!(run(), run());
}

#[test]
fn a_token_holds_no_native_balance() {
    // The token is not ALCRY: minting 1 000 tokens must not create 1 000 base
    // units of native currency.
    let (lifecycle, tree) = deploy_token();
    let tree = mint_to(&lifecycle, &tree, ALICE, 1_000);
    let view = StateView::new(tree, 0);
    assert_eq!(
        aura_exec::lifecycle::account_of(&view, &address(TOKEN))
            .balance
            .units(),
        0
    );
}
