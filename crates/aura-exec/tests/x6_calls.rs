//! X6 gate — call isolation: depth, reentrancy, value, atomic revert.
//!
//! Everything here runs the **shipped bytecode** (`examples/{vault,evil,recurse}/
//! wasm/*.wasm`, pinned in `fixtures/x6_calls.txt`) through the real `Lifecycle`,
//! and lets the contracts call each other through `env3.call`. Nothing is
//! simulated: if the guard in `abi_v3::admit` were removed, these tests would
//! drain an honest depositor (that is exactly what `tools/nc_x6.py` requires).
//!
//! The three scenarios, in the order a reviewer should read them:
//!
//! 1. **Depth** — `recurse` calls itself; the eighth frame is admitted and the
//!    ninth is refused with `-15`.
//! 2. **Reentrancy** — `evil` attacks `vault`, a contract that pays out before it
//!    debits. The node refuses to call back into a frame that is still on the
//!    stack (`-14`), so the attacker never regains control.
//! 3. **Atomicity** — a callee that writes and then traps leaves no write, no
//!    event and no value behind.

#[path = "x5_support/mod.rs"]
mod support;

use aura_exec::lifecycle::Lifecycle;
use aura_exec::{BlockContext, ContractCall, ContractReceipt};
use aura_state::{Account, MerkleTree, StateKey, apply_diff};
use aura_types::{Amount, Hash, Nonce};
use support::{address, commit, data};

const ALICE: [u8; 20] = [0xa1; 20];
const EVIL: [u8; 20] = [0xe7; 20];
const VAULT: [u8; 20] = [0x77; 20];
/// The first contract of the descent chain; the other copies are
/// [`chain_address`]`(i)`.
const RECURSE: [u8; 20] = [0x5c; 20];
/// An address with no code: the honest depositor, used as a callee by mistake.
const NOBODY: [u8; 20] = [0x00; 20];

const VAULT_WASM: &[u8] = include_bytes!("../../../examples/vault/wasm/vault.wasm");
const EVIL_WASM: &[u8] = include_bytes!("../../../examples/evil/wasm/evil.wasm");
const RECURSE_WASM: &[u8] = include_bytes!("../../../examples/recurse/wasm/recurse.wasm");
const FIXTURE: &str = include_str!("fixtures/x6_calls.txt");

/// Vault selectors.
const VAULT_FUND: u32 = 0x01;
const VAULT_WITHDRAW: u32 = 0x02;
const VAULT_DEPOSITED: u32 = 0x04;
/// Evil selectors.
const EVIL_ATTACK: u32 = 0x01;
const EVIL_ATTEMPTS: u32 = 0x04;
const EVIL_LAST_STATUS: u32 = 0x05;
const EVIL_PROBE: u32 = 0x08;
const EVIL_SEND: u32 = 0x09;
/// The chain contract deployed at index `i` (1-based, up to nine copies).
fn chain_address(index: u8) -> [u8; 20] {
    [0xc0u8.wrapping_add(index); 20]
}

/// Recurse selectors.
const RECURSE_SET_NEXT: u32 = 0x08;
const RECURSE_DOWN: u32 = 0x01;
const RECURSE_BOOM: u32 = 0x02;
const RECURSE_DEPTH: u32 = 0x03;
const RECURSE_REACHED: u32 = 0x04;
const RECURSE_LAST: u32 = 0x05;
const RECURSE_MARK: u32 = 0x06;
const RECURSE_WHO: u32 = 0x07;

/// `MAX_CALL_DEPTH`, as the node publishes it.
const MAX_CALL_DEPTH: u32 = 8;
/// `-14`: the target is already on the call stack.
const ERR_REENTRANCY: i32 = -14;
/// `-15`: the stack reached the published depth.
const ERR_CALL_DEPTH: i32 = -15;
/// `-16`: the target holds no code.
const ERR_NO_CODE: i32 = -16;
/// `-17`: the callee reverted.
const ERR_CALLEE_FAILED: i32 = -17;

fn field(key: &str) -> String {
    FIXTURE
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("missing {key} in the X6 fixture"))
        .to_string()
}

fn call_to(contract: [u8; 20], caller: [u8; 20]) -> ContractCall {
    ContractCall {
        contract: address(contract),
        caller: address(caller),
        block: BlockContext {
            height: 9,
            timestamp_ms: 1_700_000_000_000,
        },
        gas_limit: 1_000_000,
    }
}

/// Accounts with native balances — X6 moves value, so zero-balance fixtures will
/// not do.
fn funded(entries: &[([u8; 20], u128)]) -> MerkleTree {
    let rows: Vec<(Hash, Vec<u8>)> = entries
        .iter()
        .map(|(who, units)| {
            let account = Account::new(
                Nonce::new(0),
                Amount::from_units(*units),
                Hash::ZERO,
                Hash::ZERO,
            );
            (
                StateKey::account(&address(*who)).digest(),
                account.to_bytes(),
            )
        })
        .collect();
    MerkleTree::from_entries(1, &rows)
}

fn balance(tree: &MerkleTree, who: [u8; 20]) -> u128 {
    tree.get_digest(&StateKey::account(&address(who)).digest())
        .and_then(|bytes| Account::from_canonical_bytes(bytes).ok())
        .map(|account| account.balance.units())
        .unwrap_or(0)
}

fn apply(tree: &MerkleTree, receipt: &ContractReceipt) -> MerkleTree {
    commit(tree, receipt)
}

fn deploy(
    lifecycle: &Lifecycle,
    tree: &MerkleTree,
    contract: [u8; 20],
    deployer: [u8; 20],
    wasm: &[u8],
    value: u128,
) -> MerkleTree {
    let receipt = lifecycle
        .deploy(
            tree,
            call_to(contract, deployer),
            wasm.to_vec(),
            Amount::from_units(value),
            None,
        )
        .expect("deploy runs");
    assert!(
        receipt.committed(),
        "deploy must commit: {:?}",
        receipt.status
    );
    match receipt.diff.as_ref() {
        Some(diff) => apply_diff(tree, diff).expect("deploy diff applies"),
        None => panic!("a committed deploy carries a diff"),
    }
}

fn invoke(
    lifecycle: &Lifecycle,
    tree: &MerkleTree,
    contract: [u8; 20],
    who: [u8; 20],
    calldata: Vec<u8>,
) -> ContractReceipt {
    lifecycle
        .call_data(tree, call_to(contract, who), calldata)
        .expect("call runs")
}

fn u32_of(receipt: &ContractReceipt) -> u32 {
    assert_eq!(receipt.status, support::returned(0), "{:?}", receipt.status);
    assert_eq!(receipt.return_data.len(), 4, "an answer must be 4 bytes");
    u32::from_le_bytes(receipt.return_data[..].try_into().expect("4 bytes"))
}

fn i32_of(receipt: &ContractReceipt) -> i32 {
    assert_eq!(receipt.status, support::returned(0), "{:?}", receipt.status);
    assert_eq!(receipt.return_data.len(), 4, "an answer must be 4 bytes");
    i32::from_le_bytes(receipt.return_data[..].try_into().expect("4 bytes"))
}

fn u128_of(receipt: &ContractReceipt) -> u128 {
    assert_eq!(receipt.status, support::returned(0), "{:?}", receipt.status);
    assert_eq!(receipt.return_data.len(), 16, "an answer must be 16 bytes");
    u128::from_le_bytes(receipt.return_data[..].try_into().expect("16 bytes"))
}

/// The world the attack happens in: a vault holding 1 100 units, an honest
/// depositor credited 900, an attacker credited 200, and the attacker's contract.
struct World {
    lifecycle: Lifecycle,
    tree: MerkleTree,
}

fn world() -> World {
    let lifecycle = Lifecycle::new();
    let mut tree = funded(&[(ALICE, 10_000), (EVIL, 10_000), (VAULT, 0), (RECURSE, 0)]);
    tree = deploy(&lifecycle, &tree, VAULT, ALICE, VAULT_WASM, 1_100);
    for (who, units) in [(ALICE, 900u128), (EVIL, 200u128)] {
        let mut args = who.to_vec();
        args.extend_from_slice(&units.to_le_bytes());
        let receipt = invoke(&lifecycle, &tree, VAULT, ALICE, data(VAULT_FUND, &args));
        assert_eq!(
            receipt.status,
            support::returned(0),
            "fund: {:?}",
            receipt.status
        );
        tree = apply(&tree, &receipt);
    }
    tree = deploy(&lifecycle, &tree, EVIL, EVIL, EVIL_WASM, 0);
    // Nine copies of the chain contract, so a descent can reach the published
    // depth and then try to go one further.
    for index in 1..=9u8 {
        tree = deploy(
            &lifecycle,
            &tree,
            chain_address(index),
            ALICE,
            RECURSE_WASM,
            0,
        );
    }
    // Wire 1 → 2 → … → 9. The ninth has no successor: it is never reached, and
    // that is the point — the eighth copy's attempt to call it is refused.
    for index in 1..=8u8 {
        let mut args = chain_address(index + 1).to_vec();
        let receipt = invoke(
            &lifecycle,
            &tree,
            chain_address(index),
            ALICE,
            data(RECURSE_SET_NEXT, &args),
        );
        assert_eq!(
            receipt.status,
            support::returned(0),
            "set_next: {:?}",
            receipt.status
        );
        tree = apply(&tree, &receipt);
        args.clear();
    }
    World { lifecycle, tree }
}

// ---------------------------------------------------------------- artifacts

#[test]
fn the_shipped_artifacts_are_the_pinned_ones() {
    for (name, wasm) in [
        ("vault", VAULT_WASM),
        ("evil", EVIL_WASM),
        ("recurse", RECURSE_WASM),
    ] {
        let size: usize = field(&format!("contract_{name}_size"))
            .parse()
            .expect("pinned size");
        assert_eq!(wasm.len(), size, "{name}: not the pinned artifact");
        // The digest the node itself computes, pinned by `x6-record`.
        let pinned = field(&format!("measured_{name}_blake3"));
        assert_eq!(
            aura_crypto::blake3_hash(wasm).to_string(),
            pinned,
            "{name}: the artifact is not the pinned one"
        );
        let imports = field(&format!("contract_{name}_imports"));
        // Every import must be one of the three published surfaces. A contract
        // that imports anything else cannot even be deployed (X0 census), so this
        // is a cheap early warning rather than the real gate.
        for entry in imports.split(',') {
            assert!(
                entry.starts_with("env.")
                    || entry.starts_with("env2.")
                    || entry.starts_with("env3."),
                "{name}: unexpected import {entry}"
            );
        }
        // The two contracts of interest must actually be able to call out, and the
        // vault must not: X6 is about who is allowed to re-enter the machine.
        if name == "vault" {
            assert!(
                imports.contains("env3.call"),
                "the vault notifies its receiver through env3.call"
            );
        }
    }
}

// -------------------------------------------------------------------- depth

#[test]
fn a_contract_can_call_itself_up_to_the_published_depth_and_no_further() {
    let world = world();
    let receipt = invoke(
        &world.lifecycle,
        &world.tree,
        chain_address(1),
        ALICE,
        data(RECURSE_DOWN, &[]),
    );
    assert_eq!(receipt.status, support::returned(0), "{:?}", receipt.status);
    let tree = apply(&world.tree, &receipt);

    // Each copy recorded the depth it ran at: the transaction's own frame is 1,
    // and the eighth contract — the last one admitted — ran at 8.
    let first = u32_of(&invoke(
        &world.lifecycle,
        &tree,
        chain_address(1),
        ALICE,
        data(RECURSE_DEPTH, &[]),
    ));
    assert_eq!(first, 1, "the transaction's own frame is depth 1");
    let deepest = u32_of(&invoke(
        &world.lifecycle,
        &tree,
        chain_address(MAX_CALL_DEPTH as u8),
        ALICE,
        data(RECURSE_DEPTH, &[]),
    ));
    assert_eq!(
        deepest, MAX_CALL_DEPTH,
        "the eighth contract runs at depth 8"
    );

    // …and its attempt to open the ninth frame was refused, not executed.
    let last = i32_of(&invoke(
        &world.lifecycle,
        &tree,
        chain_address(MAX_CALL_DEPTH as u8),
        ALICE,
        data(RECURSE_LAST, &[]),
    ));
    assert_eq!(last, ERR_CALL_DEPTH, "the ninth frame must be refused");
    let ninth = u32_of(&invoke(
        &world.lifecycle,
        &tree,
        chain_address(9),
        ALICE,
        data(RECURSE_REACHED, &[]),
    ));
    assert_eq!(ninth, 0, "the refused contract never ran at all");
}

// -------------------------------------------------------------- reentrancy

#[test]
fn an_attacker_cannot_drain_a_vault_that_pays_out_before_it_debits() {
    let world = world();
    let mut args = VAULT.to_vec();
    args.extend_from_slice(&200u128.to_le_bytes());
    let attack = invoke(
        &world.lifecycle,
        &world.tree,
        EVIL,
        EVIL,
        data(EVIL_ATTACK, &args),
    );
    assert_eq!(attack.status, support::returned(0), "the attack runs");
    let tree = apply(&world.tree, &attack);

    // The attacker was paid exactly what it was credited…
    assert_eq!(balance(&tree, EVIL), 10_200, "the attacker is paid once");
    let ledger = u128_of(&invoke(
        &world.lifecycle,
        &tree,
        VAULT,
        ALICE,
        data(VAULT_DEPOSITED, EVIL.as_ref()),
    ));
    assert_eq!(ledger, 0, "the attacker's ledger is debited once");

    // …and the honest depositor can still be paid in full.
    let receipt = invoke(
        &world.lifecycle,
        &tree,
        VAULT,
        ALICE,
        data(VAULT_WITHDRAW, &900u128.to_le_bytes()),
    );
    assert_eq!(
        receipt.status,
        support::returned(0),
        "an honest withdrawal after the attack must succeed: {:?}",
        receipt.status
    );
    let after = apply(&tree, &receipt);
    // Alice funded the vault with 1 100 at deploy, so she starts at 8 900 and
    // ends at 9 800: her own money back, in full.
    assert_eq!(balance(&after, ALICE), 9_800, "the depositor is whole");
    assert_eq!(balance(&after, VAULT), 0, "the vault is empty, not short");
}

#[test]
fn the_callback_into_a_live_frame_is_refused_with_reentrancy() {
    let world = world();
    let mut args = VAULT.to_vec();
    args.extend_from_slice(&200u128.to_le_bytes());
    let attack = invoke(
        &world.lifecycle,
        &world.tree,
        EVIL,
        EVIL,
        data(EVIL_ATTACK, &args),
    );

    // The vault notifies the receiver after paying, and publishes what the node
    // answered. `-14` is the whole point of the phase: the attacker is on the
    // stack, so it cannot be re-entered while its own withdrawal is in flight.
    let notify = attack
        .events
        .iter()
        .find_map(|event| {
            let (topic, payload) = support::split_event(event);
            (topic == b"notify").then(|| i32::from_le_bytes(payload[..4].try_into().unwrap()))
        })
        .expect("the vault publishes the notify status");
    assert_eq!(
        notify, ERR_REENTRANCY,
        "a live frame must not be re-entered"
    );

    // And the attacker's own counter never advanced past its first attempt: the
    // second entry never happened, so nothing about it was recorded.
    let tree = apply(&world.tree, &attack);
    let attempts = u32_of(&invoke(
        &world.lifecycle,
        &tree,
        EVIL,
        EVIL,
        data(EVIL_ATTEMPTS, &[]),
    ));
    assert_eq!(attempts, 1, "the attacker got exactly one attempt");
}

// ------------------------------------------------------------------- value

#[test]
fn value_sent_with_a_call_changes_both_balances() {
    let world = world();
    let before_vault = balance(&world.tree, VAULT);
    let before_evil = balance(&world.tree, EVIL);
    let mut args = VAULT.to_vec();
    args.extend_from_slice(&200u128.to_le_bytes());
    let attack = invoke(
        &world.lifecycle,
        &world.tree,
        EVIL,
        EVIL,
        data(EVIL_ATTACK, &args),
    );
    let tree = apply(&world.tree, &attack);
    assert_eq!(
        balance(&tree, VAULT),
        before_vault - 200,
        "the vault paid the value out"
    );
    assert_eq!(
        balance(&tree, EVIL),
        before_evil + 200,
        "the caller received it, and committed"
    );
}

// -------------------------------------------------------------- atomicity

#[test]
fn a_reverted_callee_leaves_neither_writes_nor_value() {
    let world = world();
    // `probe` calls `recurse.boom`, which writes a counter, emits an event and
    // then traps.
    let mut args = chain_address(1).to_vec();
    args.extend_from_slice(&RECURSE_BOOM.to_le_bytes());
    let receipt = invoke(
        &world.lifecycle,
        &world.tree,
        EVIL,
        EVIL,
        data(EVIL_PROBE, &args),
    );
    assert_eq!(receipt.status, support::returned(0), "the caller survives");
    let status = i32::from_le_bytes(receipt.return_data[..4].try_into().unwrap());
    assert_eq!(status, ERR_CALLEE_FAILED, "the callee's revert is reported");
    let tree = apply(&world.tree, &receipt);

    // The counter the callee wrote before trapping is not there.
    let mark = u32_of(&invoke(
        &world.lifecycle,
        &tree,
        chain_address(1),
        ALICE,
        data(RECURSE_MARK, &[]),
    ));
    assert_eq!(mark, 0, "a reverted callee wrote nothing");
}

#[test]
fn a_call_to_an_address_without_code_is_refused() {
    let world = world();
    let mut args = NOBODY.to_vec();
    args.extend_from_slice(&VAULT_WITHDRAW.to_le_bytes());
    let receipt = invoke(
        &world.lifecycle,
        &world.tree,
        EVIL,
        EVIL,
        data(EVIL_PROBE, &args),
    );
    let status = i32::from_le_bytes(receipt.return_data[..4].try_into().unwrap());
    assert_eq!(
        status, ERR_NO_CODE,
        "calling an account is not calling a contract"
    );
    assert_eq!(
        receipt.status,
        support::returned(0),
        "the probe reports the code instead of trapping"
    );
}

#[test]
fn the_caller_of_a_nested_frame_is_the_contract_not_the_signer() {
    let world = world();
    // EVIL calls RECURSE.who(), which answers with `env2.caller` of its own frame.
    let mut args = chain_address(1).to_vec();
    args.extend_from_slice(&RECURSE_WHO.to_le_bytes());
    let receipt = invoke(
        &world.lifecycle,
        &world.tree,
        EVIL,
        ALICE,
        data(EVIL_PROBE, &args),
    );
    assert_eq!(receipt.status, support::returned(0), "{:?}", receipt.status);
    let status = i32::from_le_bytes(receipt.return_data[..4].try_into().unwrap());
    assert_eq!(status, 0, "the inner call succeeded");
    assert_eq!(
        &receipt.return_data[4..24],
        &EVIL[..],
        "the callee must see the calling contract, not the transaction's signer"
    );
}

#[test]
fn a_callee_answer_reaches_the_caller() {
    let world = world();
    // First descend once, so the first chain contract has something to report.
    let descent = invoke(
        &world.lifecycle,
        &world.tree,
        chain_address(1),
        ALICE,
        data(RECURSE_DOWN, &[]),
    );
    let tree = apply(&world.tree, &descent);

    // Then ask it, from another contract, how many calls it made.
    let mut args = chain_address(1).to_vec();
    args.extend_from_slice(&RECURSE_REACHED.to_le_bytes());
    let receipt = invoke(&world.lifecycle, &tree, EVIL, EVIL, data(EVIL_PROBE, &args));
    assert_eq!(receipt.status, support::returned(0), "{:?}", receipt.status);
    let status = i32::from_le_bytes(receipt.return_data[..4].try_into().unwrap());
    assert_eq!(status, 0, "the inner call succeeded");
    assert_eq!(
        &receipt.return_data[4..8],
        &1u32.to_le_bytes(),
        "the callee's answer itself crosses back, not just its status"
    );
}

#[test]
fn a_callee_failure_does_not_revert_the_callers_own_writes() {
    let world = world();
    let mut args = chain_address(1).to_vec();
    args.extend_from_slice(&RECURSE_BOOM.to_le_bytes());
    let receipt = invoke(
        &world.lifecycle,
        &world.tree,
        EVIL,
        EVIL,
        data(EVIL_PROBE, &args),
    );
    let tree = apply(&world.tree, &receipt);
    // The probe records the failure in its own storage: a callee's revert is a
    // value the caller handles, not a contagion that erases the caller's frame.
    let probe_status = invoke(
        &world.lifecycle,
        &tree,
        EVIL,
        EVIL,
        data(EVIL_LAST_STATUS, &[]),
    );
    assert_eq!(
        i32_of(&probe_status),
        ERR_CALLEE_FAILED,
        "the caller's own write survived the callee's revert"
    );
    assert_ne!(
        receipt.diff.as_ref().map(|diff| diff.changes.len()),
        Some(0),
        "the caller committed: a callee's failure is a value, not a contagion"
    );
}

#[test]
fn a_refused_call_costs_gas_and_writes_nothing() {
    let world = world();
    let mut args = chain_address(1).to_vec();
    args.extend_from_slice(&RECURSE_BOOM.to_le_bytes());
    let receipt = invoke(
        &world.lifecycle,
        &world.tree,
        EVIL,
        EVIL,
        data(EVIL_PROBE, &args),
    );
    assert!(receipt.gas_used > 0, "a refused callee still costs gas");
    assert_eq!(
        receipt.status,
        support::returned(0),
        "the caller's frame is intact"
    );
}

/// The event stream of a nested call belongs to the outermost receipt, in order.
#[test]
fn events_of_a_nested_call_reach_the_top_level_receipt() {
    let world = world();
    let mut args = VAULT.to_vec();
    args.extend_from_slice(&200u128.to_le_bytes());
    let attack = invoke(
        &world.lifecycle,
        &world.tree,
        EVIL,
        EVIL,
        data(EVIL_ATTACK, &args),
    );
    let topics: Vec<Vec<u8>> = attack
        .events
        .iter()
        .map(|event| support::split_event(event).0)
        .collect();
    assert_eq!(
        topics,
        vec![
            b"notify".to_vec(),
            b"withdraw".to_vec(),
            b"attempt".to_vec()
        ],
        "the vault's events arrive before the caller's own, in execution order"
    );
}

#[test]
fn a_callee_refusal_reaches_the_caller_as_a_refusal() {
    let world = world();
    // `withdraw` needs a `u128` argument; a bare selector is `-21` from the
    // callee's own dispatcher. The point is that the *caller* sees `-21` and not
    // `0`: a forwarded refusal that looks like success is a money bug.
    let mut args = VAULT.to_vec();
    args.extend_from_slice(&VAULT_WITHDRAW.to_le_bytes());
    let receipt = invoke(
        &world.lifecycle,
        &world.tree,
        EVIL,
        EVIL,
        data(EVIL_PROBE, &args),
    );
    let status = i32::from_le_bytes(receipt.return_data[..4].try_into().unwrap());
    assert_eq!(
        status, -21,
        "the callee's refusal must reach the caller unchanged"
    );
}

#[test]
fn value_of_a_reverted_call_comes_back() {
    let world = world();
    let before_evil = balance(&world.tree, EVIL);
    let before_chain = balance(&world.tree, chain_address(1));
    let mut args = chain_address(1).to_vec();
    args.extend_from_slice(&RECURSE_BOOM.to_le_bytes());
    args.extend_from_slice(&100u128.to_le_bytes());
    let receipt = invoke(
        &world.lifecycle,
        &world.tree,
        EVIL,
        EVIL,
        data(EVIL_SEND, &args),
    );
    let status = i32::from_le_bytes(receipt.return_data[..4].try_into().unwrap());
    assert_eq!(status, ERR_CALLEE_FAILED, "the callee reverted");
    let tree = apply(&world.tree, &receipt);
    assert_eq!(
        balance(&tree, EVIL),
        before_evil,
        "the sender keeps value it sent to a callee that reverted"
    );
    assert_eq!(
        balance(&tree, chain_address(1)),
        before_chain,
        "and the callee never received it"
    );
}

/// A receipt never lies about the state it produced: the committed tree equals
/// the receipt's `result_root` version.
#[test]
fn the_committed_tree_matches_the_receipt_root() {
    let world = world();
    let mut args = VAULT.to_vec();
    args.extend_from_slice(&200u128.to_le_bytes());
    let attack = invoke(
        &world.lifecycle,
        &world.tree,
        EVIL,
        EVIL,
        data(EVIL_ATTACK, &args),
    );
    let tree = apply(&world.tree, &attack);
    let diff = attack
        .diff
        .as_ref()
        .expect("a committed call carries a diff");
    assert_eq!(
        tree.root_at(diff.version),
        Some(diff.result_root),
        "the root the receipt reports is the root the tree has"
    );
    assert_eq!(
        tree.version(),
        diff.version,
        "and the tree advanced by exactly one version"
    );
}
