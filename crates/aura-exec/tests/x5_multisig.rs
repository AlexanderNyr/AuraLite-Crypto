//! X5 gate (ARC-M1): the minimal multisig, including the cases that lose money.
//!
//! A multisig is small enough to review completely, so the tests aim at the three
//! ways it fails in the field: a foreign key voting, the same key voting twice, and
//! a proposal executing twice. The contract holds native ALCRY, so the balances
//! asserted here are real account balances in the state tree, not contract storage.

mod x5_support;

use aura_exec::lifecycle::{Lifecycle, account_of};
use aura_exec::{ContractReceipt, ExecStatus};
use aura_state::{Account, MerkleTree, StateView};
use aura_types::{Amount, Hash, Nonce};
use x5_support::*;

const MULTISIG: &[u8] = include_bytes!("../../../standards/multisig/wasm/multisig.wasm");

const WALLET: [u8; 20] = [0x77; 20];
const CREATOR: [u8; 20] = [0x22; 20];
const S1: [u8; 20] = [0x81; 20];
const S2: [u8; 20] = [0x82; 20];
const S3: [u8; 20] = [0x83; 20];
const PAYEE: [u8; 20] = [0x99; 20];
const STRANGER: [u8; 20] = [0xaa; 20];
const ZERO: [u8; 20] = [0x00; 20];

const SEL_SET_POLICY: u32 = 0x01;
const SEL_PROPOSE: u32 = 0x02;
const SEL_APPROVE: u32 = 0x03;
const SEL_REVOKE: u32 = 0x04;
const SEL_PROPOSAL: u32 = 0x05;
const SEL_SIGNERS: u32 = 0x06;
const SEL_THRESHOLD: u32 = 0x07;
const SEL_CREATOR: u32 = 0x08;

const ERR_INSUFFICIENT_BALANCE: i32 = -6;
const ERR_NOT_CREATOR: i32 = -22;
const ERR_ZERO_ADDRESS: i32 = -23;
const ERR_NOT_SIGNER: i32 = -28;
const ERR_POLICY: i32 = -29;
const ERR_NO_PROPOSAL: i32 = -30;
const ERR_ALREADY_EXECUTED: i32 = -31;

fn invoke(
    lifecycle: &Lifecycle,
    tree: &MerkleTree,
    who: [u8; 20],
    calldata: Vec<u8>,
) -> ContractReceipt {
    x5_support::invoke(lifecycle, tree, WALLET, who, calldata)
}

/// Deploy the wallet, fund it with `funds` base units and install a 2-of-3 policy.
fn deploy_wallet(funds: u128) -> (Lifecycle, MerkleTree) {
    let lifecycle = Lifecycle::new();
    let accounts = [CREATOR, S1, S2, S3, PAYEE, STRANGER];
    let mut entries: Vec<(Hash, Vec<u8>)> = accounts
        .iter()
        .map(|who| {
            let account =
                Account::new(Nonce::new(0), Amount::from_units(0), Hash::ZERO, Hash::ZERO);
            (
                aura_state::StateKey::account(&address(*who)).digest(),
                account.to_bytes(),
            )
        })
        .collect();
    // The wallet starts funded so a proposal has something to send.
    let wallet = Account::new(
        Nonce::new(0),
        Amount::from_units(funds),
        Hash::ZERO,
        Hash::ZERO,
    );
    entries.push((
        aura_state::StateKey::account(&address(WALLET)).digest(),
        wallet.to_bytes(),
    ));
    let start = MerkleTree::from_entries(1, &entries);

    let deployed = lifecycle
        .deploy(
            &start,
            call_from(WALLET, CREATOR, 1_000_000),
            MULTISIG.to_vec(),
            Amount::from_units(0),
            None,
        )
        .expect("deploy runs");
    assert!(deployed.committed(), "{:?}", deployed.status);
    let mut tree = commit(&start, &lifecycle_receipt(&deployed));

    let policy = invoke(
        &lifecycle,
        &tree,
        CREATOR,
        data(
            SEL_SET_POLICY,
            &[u8_arg(2), addr_arg(S1), addr_arg(S2), addr_arg(S3)].concat(),
        ),
    );
    assert_eq!(policy.status, ExecStatus::Returned(0), "policy installs");
    tree = commit(&tree, &policy);
    (lifecycle, tree)
}

/// The lifecycle receipt and the contract receipt carry the same diff; this keeps
/// the helper above readable.
fn lifecycle_receipt(receipt: &aura_exec::lifecycle::LifecycleReceipt) -> ContractReceipt {
    ContractReceipt {
        status: receipt.status,
        gas_used: receipt.gas_used,
        host_gas: receipt.host_gas,
        events: receipt.events.clone(),
        writes: receipt.writes,
        diff: receipt.diff.clone(),
        result_root: receipt.result_root,
        return_data: receipt.ret.clone(),
    }
}

fn proposal_id(receipt: &ContractReceipt) -> u128 {
    assert_eq!(
        receipt.status,
        ExecStatus::Returned(0),
        "{:?}",
        receipt.status
    );
    u128::from_le_bytes(receipt.return_data.clone().try_into().expect("16 bytes"))
}

/// Open a proposal and commit it — a proposal that is not committed is invisible,
/// which is exactly the bug this helper exists to prevent in the tests.
fn propose(lifecycle: &Lifecycle, tree: &mut MerkleTree, who: [u8; 20], amount: u128) -> u128 {
    let receipt = invoke(
        lifecycle,
        tree,
        who,
        data(
            SEL_PROPOSE,
            &[u8_arg(1), addr_arg(PAYEE), u128_arg(amount)].concat(),
        ),
    );
    let pid = proposal_id(&receipt);
    *tree = commit(tree, &receipt);
    pid
}

fn wallet_balance(tree: &MerkleTree) -> u128 {
    account_of(&StateView::new(tree.clone(), 0), &address(WALLET))
        .balance
        .units()
}

#[test]
fn the_policy_is_installed_once_by_the_creator() {
    let (lifecycle, tree) = deploy_wallet(0);
    let signers = x5_support::invoke(
        &lifecycle,
        &tree,
        WALLET,
        x5_support::ASKER,
        data(SEL_SIGNERS, &[]),
    );
    assert_eq!(signers.return_data.len(), 60);
    assert_eq!(&signers.return_data[..20], &S1);
    assert_eq!(&signers.return_data[20..40], &S2);
    assert_eq!(&signers.return_data[40..], &S3);

    let threshold = x5_support::invoke(
        &lifecycle,
        &tree,
        WALLET,
        x5_support::ASKER,
        data(SEL_THRESHOLD, &[]),
    );
    assert_eq!(threshold.return_data, vec![2]);

    let creator = x5_support::invoke(
        &lifecycle,
        &tree,
        WALLET,
        x5_support::ASKER,
        data(SEL_CREATOR, &[]),
    );
    assert_eq!(creator.return_data, CREATOR.to_vec());

    // A second install — even by the creator — is refused: a reconfigurable
    // multisig is a single key with extra steps.
    let again = invoke(
        &lifecycle,
        &tree,
        CREATOR,
        data(
            SEL_SET_POLICY,
            &[u8_arg(1), addr_arg(PAYEE), addr_arg(PAYEE), addr_arg(PAYEE)].concat(),
        ),
    );
    assert_eq!(again.status, returned(ERR_POLICY));
    assert_no_effect(&again, &tree);
}

#[test]
fn a_bad_policy_is_refused() {
    let lifecycle = Lifecycle::new();
    let start = base(&[CREATOR]);
    let deployed = lifecycle
        .deploy(
            &start,
            call_from(WALLET, CREATOR, 1_000_000),
            MULTISIG.to_vec(),
            Amount::from_units(0),
            None,
        )
        .expect("deploy runs");
    let tree = commit(&start, &lifecycle_receipt(&deployed));

    // threshold, three signers, expected status, what the case is
    type PolicyCase = (u8, [u8; 20], [u8; 20], [u8; 20], i32, &'static str);
    let cases: Vec<PolicyCase> = vec![
        (0, S1, S2, S3, ERR_POLICY, "a zero threshold"),
        (
            4,
            S1,
            S2,
            S3,
            ERR_POLICY,
            "a threshold above the signer count",
        ),
        (2, S1, S1, S3, ERR_POLICY, "a duplicated signer"),
        (2, S1, ZERO, S3, ERR_ZERO_ADDRESS, "a zero address"),
    ];
    for (threshold, a, b, c, expected, what) in cases {
        let receipt = invoke(
            &lifecycle,
            &tree,
            CREATOR,
            data(
                SEL_SET_POLICY,
                &[u8_arg(threshold), addr_arg(a), addr_arg(b), addr_arg(c)].concat(),
            ),
        );
        assert_eq!(receipt.status, returned(expected), "{what}");
        assert_no_effect(&receipt, &tree);
    }

    // A stranger cannot install a policy at all.
    let stranger = invoke(
        &lifecycle,
        &tree,
        STRANGER,
        data(
            SEL_SET_POLICY,
            &[
                u8_arg(1),
                addr_arg(STRANGER),
                addr_arg(STRANGER),
                addr_arg(STRANGER),
            ]
            .concat(),
        ),
    );
    assert_eq!(stranger.status, returned(ERR_NOT_CREATOR));
}

#[test]
fn two_of_three_moves_the_money_and_only_once() {
    let (lifecycle, mut tree) = deploy_wallet(1_000);
    let pid = propose(&lifecycle, &mut tree, S1, 400);

    // One vote is not enough.
    let first = invoke(&lifecycle, &tree, S1, data(SEL_APPROVE, &u128_arg(pid)));
    assert_eq!(first.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &first);
    assert_eq!(
        wallet_balance(&tree),
        1_000,
        "one of two votes moves nothing"
    );

    // The second reaches the threshold and pays.
    let second = invoke(&lifecycle, &tree, S2, data(SEL_APPROVE, &u128_arg(pid)));
    assert_eq!(second.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &second);
    assert_eq!(wallet_balance(&tree), 600);
    assert_eq!(
        account_of(&StateView::new(tree.clone(), 0), &address(PAYEE))
            .balance
            .units(),
        400
    );

    // A third vote cannot re-execute it.
    let third = invoke(&lifecycle, &tree, S3, data(SEL_APPROVE, &u128_arg(pid)));
    assert_eq!(third.status, returned(ERR_ALREADY_EXECUTED));
    assert_no_effect(&third, &tree);
    assert_eq!(wallet_balance(&tree), 600, "the wallet pays exactly once");

    let state = x5_support::invoke(
        &lifecycle,
        &tree,
        WALLET,
        x5_support::ASKER,
        data(SEL_PROPOSAL, &u128_arg(pid)),
    );
    assert_eq!(state.return_data.len(), 33);
    assert_eq!(state.return_data[0], 1, "executed");
    assert_eq!(
        u128::from_le_bytes(state.return_data[1..17].try_into().expect("16")),
        2
    );
    assert_eq!(
        u128::from_le_bytes(state.return_data[17..].try_into().expect("16")),
        400
    );
}

#[test]
fn a_signer_cannot_vote_twice() {
    let (lifecycle, mut tree) = deploy_wallet(1_000);
    let pid = propose(&lifecycle, &mut tree, S1, 100);
    let first = invoke(&lifecycle, &tree, S1, data(SEL_APPROVE, &u128_arg(pid)));
    assert_eq!(first.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &first);

    let again = invoke(&lifecycle, &tree, S1, data(SEL_APPROVE, &u128_arg(pid)));
    assert_eq!(again.status, returned(-32), "already voted");
    assert_no_effect(&again, &tree);
    assert_eq!(
        wallet_balance(&tree),
        1_000,
        "one key counted twice is impossible"
    );
}

#[test]
fn a_stranger_can_neither_propose_nor_vote() {
    let (lifecycle, mut tree) = deploy_wallet(1_000);
    let proposal = invoke(
        &lifecycle,
        &tree,
        STRANGER,
        data(
            SEL_PROPOSE,
            &[u8_arg(1), addr_arg(STRANGER), u128_arg(1)].concat(),
        ),
    );
    assert_eq!(proposal.status, returned(ERR_NOT_SIGNER));
    assert_no_effect(&proposal, &tree);

    let pid = propose(&lifecycle, &mut tree, S1, 1);
    let vote = invoke(
        &lifecycle,
        &tree,
        STRANGER,
        data(SEL_APPROVE, &u128_arg(pid)),
    );
    assert_eq!(vote.status, returned(ERR_NOT_SIGNER));
    assert_no_effect(&vote, &tree);
}

#[test]
fn a_vote_can_be_taken_back() {
    let (lifecycle, mut tree) = deploy_wallet(1_000);
    let pid = propose(&lifecycle, &mut tree, S1, 300);
    let vote = invoke(&lifecycle, &tree, S1, data(SEL_APPROVE, &u128_arg(pid)));
    let tree = commit(&tree, &vote);

    let revoked = invoke(&lifecycle, &tree, S1, data(SEL_REVOKE, &u128_arg(pid)));
    assert_eq!(revoked.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &revoked);
    assert_eq!(wallet_balance(&tree), 1_000);

    // With the vote withdrawn, one remaining vote is not the threshold.
    let second = invoke(&lifecycle, &tree, S2, data(SEL_APPROVE, &u128_arg(pid)));
    assert_eq!(second.status, ExecStatus::Returned(0));
    let tree = commit(&tree, &second);
    assert_eq!(
        wallet_balance(&tree),
        1_000,
        "a revoked vote does not count"
    );
}

#[test]
fn a_proposal_the_wallet_cannot_fund_stays_retryable() {
    let (lifecycle, mut tree) = deploy_wallet(100);
    let pid = propose(&lifecycle, &mut tree, S1, 1_000);

    let first = invoke(&lifecycle, &tree, S1, data(SEL_APPROVE, &u128_arg(pid)));
    let tree = commit(&tree, &first);
    let second = invoke(&lifecycle, &tree, S2, data(SEL_APPROVE, &u128_arg(pid)));
    assert_eq!(second.status, returned(ERR_INSUFFICIENT_BALANCE));
    let tree = commit(&tree, &second);
    assert_eq!(wallet_balance(&tree), 100, "nothing moved");

    // The approval that tipped it over still stands, the proposal is *not* marked
    // executed, and the vote can be tried again once the wallet is funded.
    let state = x5_support::invoke(
        &lifecycle,
        &tree,
        WALLET,
        x5_support::ASKER,
        data(SEL_PROPOSAL, &u128_arg(pid)),
    );
    assert_eq!(state.return_data[0], 0, "not executed");
}

#[test]
fn unknown_proposals_and_kinds_are_refused() {
    let (lifecycle, tree) = deploy_wallet(1_000);
    let missing = invoke(&lifecycle, &tree, S1, data(SEL_APPROVE, &u128_arg(7)));
    assert_eq!(missing.status, returned(ERR_NO_PROPOSAL));

    let bad_kind = invoke(
        &lifecycle,
        &tree,
        S1,
        data(
            SEL_PROPOSE,
            &[u8_arg(9), addr_arg(PAYEE), u128_arg(1)].concat(),
        ),
    );
    assert_eq!(bad_kind.status, returned(ERR_POLICY));

    let zero_payee = invoke(
        &lifecycle,
        &tree,
        S1,
        data(
            SEL_PROPOSE,
            &[u8_arg(1), addr_arg(ZERO), u128_arg(1)].concat(),
        ),
    );
    assert_eq!(zero_payee.status, returned(ERR_ZERO_ADDRESS));
}

#[test]
fn proposal_ids_are_unique_and_monotonic() {
    let (lifecycle, mut tree) = deploy_wallet(1_000);
    let first = propose(&lifecycle, &mut tree, S1, 1);
    let second = propose(&lifecycle, &mut tree, S2, 2);
    assert_eq!(first, 1);
    assert_eq!(second, 2, "two proposals never share an id");
}
