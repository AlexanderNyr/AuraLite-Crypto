//! X5 gate (ARC-721): the non-fungible token standard, method by method.
//!
//! Same harness as the ARC-20 gate: the shipped artifact, the real `Lifecycle`,
//! observable behaviour only. What is specific to a non-fungible standard is that
//! ownership is per token id, so the tests concentrate on the three ways a
//! per-token contract goes wrong: a token existing twice, a transfer that does not
//! clear the approval, and a counter that disagrees with the ownership map.

mod x5_support;

use aura_exec::lifecycle::Lifecycle;
use aura_exec::{ContractReceipt, ExecStatus};
use aura_state::MerkleTree;
use x5_support::*;

const ARC721: &[u8] = include_bytes!("../../../standards/arc721/wasm/arc721.wasm");

const NFT: [u8; 20] = [0x55; 20];
const OWNER: [u8; 20] = [0x22; 20];
const ALICE: [u8; 20] = [0x33; 20];
const BOB: [u8; 20] = [0x44; 20];
const CAROL: [u8; 20] = [0x66; 20];
const ZERO: [u8; 20] = [0x00; 20];

const SEL_MINT: u32 = 0x01;
const SEL_TRANSFER: u32 = 0x02;
const SEL_APPROVE: u32 = 0x03;
const SEL_TRANSFER_FROM: u32 = 0x04;
const SEL_OWNER_OF: u32 = 0x05;
const SEL_GET_APPROVED: u32 = 0x06;
const SEL_BALANCE_OF: u32 = 0x07;
const SEL_TOTAL_SUPPLY: u32 = 0x08;
const SEL_BURN: u32 = 0x09;
const SEL_OWNER: u32 = 0x0a;

const ERR_NOT_TOKEN_OWNER: i32 = -6;
const ERR_NOT_OWNER: i32 = -22;
const ERR_ZERO_ADDRESS: i32 = -23;
const ERR_NO_TOKEN: i32 = -25;
const ERR_TOKEN_EXISTS: i32 = -26;
const ERR_NOT_APPROVED: i32 = -27;

fn deploy_nft() -> (Lifecycle, MerkleTree) {
    deploy(ARC721, NFT, OWNER, &[OWNER, ALICE, BOB, CAROL])
}

fn invoke(
    lifecycle: &Lifecycle,
    tree: &MerkleTree,
    who: [u8; 20],
    calldata: Vec<u8>,
) -> ContractReceipt {
    x5_support::invoke(lifecycle, tree, NFT, who, calldata)
}

fn read_u128(lifecycle: &Lifecycle, tree: &MerkleTree, calldata: Vec<u8>) -> u128 {
    x5_support::read_u128(lifecycle, tree, NFT, calldata)
}

fn read_addr(lifecycle: &Lifecycle, tree: &MerkleTree, calldata: Vec<u8>) -> [u8; 20] {
    x5_support::read_addr(lifecycle, tree, NFT, calldata)
}

fn mint_token(lifecycle: &Lifecycle, tree: &MerkleTree, to: [u8; 20], id: u128) -> MerkleTree {
    let receipt = invoke(
        lifecycle,
        tree,
        OWNER,
        data(SEL_MINT, &[addr_arg(to), u128_arg(id)].concat()),
    );
    assert_eq!(receipt.status, ExecStatus::Returned(0), "mint must succeed");
    commit(tree, &receipt)
}

fn holder_of(lifecycle: &Lifecycle, tree: &MerkleTree, id: u128) -> [u8; 20] {
    read_addr(lifecycle, tree, data(SEL_OWNER_OF, &u128_arg(id)))
}

fn balance_of(lifecycle: &Lifecycle, tree: &MerkleTree, who: [u8; 20]) -> u128 {
    read_u128(lifecycle, tree, data(SEL_BALANCE_OF, &addr_arg(who)))
}

#[test]
fn arc721_deploys_empty_and_records_its_owner() {
    let (lifecycle, tree) = deploy_nft();
    assert_eq!(read_u128(&lifecycle, &tree, data(SEL_TOTAL_SUPPLY, &[])), 0);
    assert_eq!(
        read_addr(&lifecycle, &tree, data(SEL_OWNER, &[])),
        OWNER,
        "the deployer owns the collection"
    );
}

#[test]
fn a_token_can_only_be_minted_once() {
    let (lifecycle, tree) = deploy_nft();
    let tree = mint_token(&lifecycle, &tree, ALICE, 7);
    assert_eq!(holder_of(&lifecycle, &tree, 7), ALICE);

    let again = invoke(
        &lifecycle,
        &tree,
        OWNER,
        data(SEL_MINT, &[addr_arg(BOB), u128_arg(7)].concat()),
    );
    assert_eq!(again.status, returned(ERR_TOKEN_EXISTS));
    assert_no_effect(&again, &tree);
    assert_eq!(
        holder_of(&lifecycle, &tree, 7),
        ALICE,
        "a refused mint must not move the token"
    );
}

#[test]
fn only_the_contract_owner_mints() {
    let (lifecycle, tree) = deploy_nft();
    let denied = invoke(
        &lifecycle,
        &tree,
        ALICE,
        data(SEL_MINT, &[addr_arg(ALICE), u128_arg(1)].concat()),
    );
    assert_eq!(denied.status, returned(ERR_NOT_OWNER));
    assert_no_effect(&denied, &tree);
}

#[test]
fn transferring_moves_ownership_and_the_counter() {
    let (lifecycle, tree) = deploy_nft();
    let tree = mint_token(&lifecycle, &tree, ALICE, 1);
    assert_eq!(balance_of(&lifecycle, &tree, ALICE), 1);

    let moved = invoke(
        &lifecycle,
        &tree,
        ALICE,
        data(SEL_TRANSFER, &[addr_arg(BOB), u128_arg(1)].concat()),
    );
    assert_eq!(moved.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &moved);
    assert_eq!(holder_of(&lifecycle, &tree, 1), BOB);
    assert_eq!(balance_of(&lifecycle, &tree, ALICE), 0);
    assert_eq!(balance_of(&lifecycle, &tree, BOB), 1);

    // Only the holder may transfer, and only to a non-zero address.
    let not_holder = invoke(
        &lifecycle,
        &tree,
        ALICE,
        data(SEL_TRANSFER, &[addr_arg(ALICE), u128_arg(1)].concat()),
    );
    assert_eq!(not_holder.status, returned(ERR_NOT_TOKEN_OWNER));
    let to_zero = invoke(
        &lifecycle,
        &tree,
        BOB,
        data(SEL_TRANSFER, &[addr_arg(ZERO), u128_arg(1)].concat()),
    );
    assert_eq!(to_zero.status, returned(ERR_ZERO_ADDRESS));
}

#[test]
fn a_transfer_clears_the_approval() {
    // The classic NFT bug: a stale approval lets the old spender move the token
    // again after it changed hands.
    let (lifecycle, tree) = deploy_nft();
    let tree = mint_token(&lifecycle, &tree, ALICE, 3);

    let approved = invoke(
        &lifecycle,
        &tree,
        ALICE,
        data(SEL_APPROVE, &[addr_arg(CAROL), u128_arg(3)].concat()),
    );
    assert_eq!(approved.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &approved);
    assert_eq!(
        read_addr(&lifecycle, &tree, data(SEL_GET_APPROVED, &u128_arg(3))),
        CAROL
    );

    // Carol moves it to herself; the approval must be gone afterwards.
    let moved = invoke(
        &lifecycle,
        &tree,
        CAROL,
        data(
            SEL_TRANSFER_FROM,
            &[addr_arg(ALICE), addr_arg(CAROL), u128_arg(3)].concat(),
        ),
    );
    assert_eq!(moved.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &moved);
    assert_eq!(
        read_addr(&lifecycle, &tree, data(SEL_GET_APPROVED, &u128_arg(3))),
        ZERO,
        "a transfer must clear the approval"
    );
}

#[test]
fn transfer_from_needs_the_approval() {
    let (lifecycle, tree) = deploy_nft();
    let tree = mint_token(&lifecycle, &tree, ALICE, 4);

    let unapproved = invoke(
        &lifecycle,
        &tree,
        BOB,
        data(
            SEL_TRANSFER_FROM,
            &[addr_arg(ALICE), addr_arg(BOB), u128_arg(4)].concat(),
        ),
    );
    assert_eq!(unapproved.status, returned(ERR_NOT_APPROVED));
    assert_no_effect(&unapproved, &tree);

    // The owner does not need one.
    let by_owner = invoke(
        &lifecycle,
        &tree,
        ALICE,
        data(
            SEL_TRANSFER_FROM,
            &[addr_arg(ALICE), addr_arg(BOB), u128_arg(4)].concat(),
        ),
    );
    assert_eq!(by_owner.status, ExecStatus::Returned(0));
}

#[test]
fn reading_a_missing_token_is_an_error_not_a_zero_address() {
    let (lifecycle, tree) = deploy_nft();
    for selector in [SEL_OWNER_OF, SEL_GET_APPROVED] {
        let receipt = invoke(&lifecycle, &tree, ALICE, data(selector, &u128_arg(99)));
        assert_eq!(
            receipt.status,
            returned(ERR_NO_TOKEN),
            "selector {selector}"
        );
        assert!(
            receipt.return_data.is_empty(),
            "no answer for a missing token"
        );
    }
    // And a transfer of a missing token is refused the same way.
    let receipt = invoke(
        &lifecycle,
        &tree,
        ALICE,
        data(SEL_TRANSFER, &[addr_arg(BOB), u128_arg(99)].concat()),
    );
    assert_eq!(receipt.status, returned(ERR_NO_TOKEN));
}

#[test]
fn burn_destroys_the_token_and_decrements_both_counters() {
    let (lifecycle, tree) = deploy_nft();
    let tree = mint_token(&lifecycle, &tree, ALICE, 5);
    let burned = invoke(&lifecycle, &tree, ALICE, data(SEL_BURN, &u128_arg(5)));
    assert_eq!(burned.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &burned);

    assert_eq!(read_u128(&lifecycle, &tree, data(SEL_TOTAL_SUPPLY, &[])), 0);
    assert_eq!(balance_of(&lifecycle, &tree, ALICE), 0);
    let gone = invoke(&lifecycle, &tree, ALICE, data(SEL_OWNER_OF, &u128_arg(5)));
    assert_eq!(gone.status, returned(ERR_NO_TOKEN));

    // A burned id can be minted again — the id is an identifier, not a counter.
    let tree = mint_token(&lifecycle, &tree, BOB, 5);
    assert_eq!(holder_of(&lifecycle, &tree, 5), BOB);
}

#[test]
fn the_supply_and_the_ownership_map_never_disagree() {
    // The invariant a per-token contract must keep: total supply equals the sum of
    // the holdings, after every kind of operation.
    let (lifecycle, tree) = deploy_nft();
    let mut tree = tree;
    for (id, to) in [(1u128, ALICE), (2, ALICE), (3, BOB)] {
        tree = mint_token(&lifecycle, &tree, to, id);
    }
    let moved = invoke(
        &lifecycle,
        &tree,
        ALICE,
        data(SEL_TRANSFER, &[addr_arg(CAROL), u128_arg(1)].concat()),
    );
    tree = commit(&tree, &moved);
    let burned = invoke(&lifecycle, &tree, BOB, data(SEL_BURN, &u128_arg(3)));
    tree = commit(&tree, &burned);

    let supply = read_u128(&lifecycle, &tree, data(SEL_TOTAL_SUPPLY, &[]));
    let sum = balance_of(&lifecycle, &tree, ALICE)
        + balance_of(&lifecycle, &tree, BOB)
        + balance_of(&lifecycle, &tree, CAROL);
    assert_eq!(supply, sum, "supply must equal the sum of holdings");
    assert_eq!(supply, 2);
}

#[test]
fn calldata_that_does_not_decode_is_refused() {
    let (lifecycle, tree) = deploy_nft();
    let cases: Vec<(Vec<u8>, i32, &str)> = vec![
        (vec![], -20, "no calldata"),
        (data(0xffff, &[]), -20, "unknown selector"),
        (data(SEL_MINT, &addr_arg(ALICE)), -21, "missing token id"),
        (
            data(SEL_OWNER_OF, &[u128_arg(1), vec![0]].concat()),
            -21,
            "trailing byte",
        ),
    ];
    for (calldata, expected, what) in cases {
        let receipt = invoke(&lifecycle, &tree, OWNER, calldata);
        assert_eq!(receipt.status, returned(expected), "{what}");
        assert_no_effect(&receipt, &tree);
    }
}
