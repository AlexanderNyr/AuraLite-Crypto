//! Record the measured gas of the X6 call-isolation scenarios.
//!
//! Same rule as every other recorder in this repository: the numbers the report
//! quotes come from the node actually executing the shipped bytecode, not from a
//! table somebody typed. Run it and paste nothing — `tools/build_x6_contracts.py`
//! appends this output to the fixture verbatim.

use aura_exec::lifecycle::Lifecycle;
use aura_exec::lifecycle::LifecycleReceipt;
use aura_exec::{BlockContext, ContractCall, ContractReceipt};
use aura_state::{Account, MerkleTree, StateKey, apply_diff};
use aura_types::{Address, Amount, Hash, Nonce};

const VAULT: [u8; 20] = [0x77; 20];
const EVIL: [u8; 20] = [0xe7; 20];
const RECURSE: [u8; 20] = [0x5c; 20];
const ALICE: [u8; 20] = [0xa1; 20];

fn address(bytes: [u8; 20]) -> Address {
    Address::from_slice(&bytes).expect("20-byte address")
}

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

fn apply(tree: &MerkleTree, diff: &Option<aura_state::StateDiff>) -> MerkleTree {
    match diff {
        Some(diff) => apply_diff(tree, diff).expect("diff applies"),
        None => tree.clone(),
    }
}

/// Both receipt types carry the diff that commits; only the name differs.
trait DiffOf {
    fn diff_of(&self) -> &Option<aura_state::StateDiff>;
}

impl DiffOf for ContractReceipt {
    fn diff_of(&self) -> &Option<aura_state::StateDiff> {
        &self.diff
    }
}

impl DiffOf for LifecycleReceipt {
    fn diff_of(&self) -> &Option<aura_state::StateDiff> {
        &self.diff
    }
}

fn data(selector: u32, args: &[u8]) -> Vec<u8> {
    let mut out = selector.to_le_bytes().to_vec();
    out.extend_from_slice(args);
    out
}

fn main() {
    let lifecycle = Lifecycle::new();
    let vault_wasm = include_bytes!("../../../../examples/vault/wasm/vault.wasm");
    let evil_wasm = include_bytes!("../../../../examples/evil/wasm/evil.wasm");
    let recurse_wasm = include_bytes!("../../../../examples/recurse/wasm/recurse.wasm");
    let accounts = [
        (ALICE, 10_000u128),
        (EVIL, 10_000),
        (VAULT, 0),
        (RECURSE, 0),
    ];

    // Deploy the vault with the honest depositor's money and credit the attacker.
    let mut tree = funded(&accounts);
    let deploy_vault = lifecycle
        .deploy(
            &tree,
            call_to(VAULT, ALICE),
            vault_wasm.to_vec(),
            // The vault's native balance arrives at deploy: `fund` only keeps the
            // ledger, it does not move money, and a vault with an empty balance
            // would fail every payout for a boring reason.
            Amount::from_units(1_100),
            None,
        )
        .expect("vault deploys");
    tree = apply(&tree, deploy_vault.diff_of());
    let fund = lifecycle
        .call_data(
            &tree,
            call_to(VAULT, ALICE),
            data(0x01, &[&ALICE[..], &900u128.to_le_bytes()].concat()),
        )
        .expect("fund runs");
    tree = apply(&tree, fund.diff_of());
    let credit = lifecycle
        .call_data(
            &tree,
            call_to(VAULT, ALICE),
            data(0x01, &[&EVIL[..], &200u128.to_le_bytes()].concat()),
        )
        .expect("credit runs");
    tree = apply(&tree, credit.diff_of());

    let deploy_evil = lifecycle
        .deploy(
            &tree,
            call_to(EVIL, EVIL),
            evil_wasm.to_vec(),
            Amount::from_units(0),
            None,
        )
        .expect("evil deploys");
    tree = apply(&tree, deploy_evil.diff_of());
    let deploy_recurse = lifecycle
        .deploy(
            &tree,
            call_to(RECURSE, ALICE),
            recurse_wasm.to_vec(),
            Amount::from_units(0),
            None,
        )
        .expect("recurse deploys");
    tree = apply(&tree, deploy_recurse.diff_of());

    // The attack: withdraw, and re-enter from the payout callback.
    let attack = lifecycle
        .call_data(
            &tree,
            call_to(EVIL, EVIL),
            data(0x01, &[&VAULT[..], &200u128.to_le_bytes()].concat()),
        )
        .expect("attack runs");
    let after_attack = apply(&tree, attack.diff_of());
    let attempts = lifecycle
        .call_data(&after_attack, call_to(EVIL, ALICE), data(0x04, &[]))
        .expect("attempts run");
    let last = lifecycle
        .call_data(&after_attack, call_to(EVIL, ALICE), data(0x05, &[]))
        .expect("status runs");

    // The honest depositor, after the attack.
    let withdraw = lifecycle
        .call_data(
            &after_attack,
            call_to(VAULT, ALICE),
            data(0x02, &900u128.to_le_bytes()),
        )
        .expect("withdraw runs");

    // Descent: how far a contract can call itself.
    let down = lifecycle
        .call_data(&tree, call_to(RECURSE, ALICE), data(0x01, &[]))
        .expect("descent runs");

    // The digests a node computes over the artifacts, so the gate can verify the
    // shipped bytes instead of trusting the size.
    for (name, wasm) in [
        ("vault", vault_wasm.as_slice()),
        ("evil", evil_wasm.as_slice()),
        ("recurse", recurse_wasm.as_slice()),
    ] {
        println!("measured_{name}_blake3={}", aura_crypto::blake3_hash(wasm));
    }
    println!("measured_vault_deploy_gas={}", deploy_vault.gas_used);
    println!("measured_evil_deploy_gas={}", deploy_evil.gas_used);
    println!("measured_recurse_deploy_gas={}", deploy_recurse.gas_used);
    println!("measured_fund_gas={}", fund.gas_used);
    println!("measured_attack_gas={}", attack.gas_used);
    println!("measured_withdraw_after_attack_gas={}", withdraw.gas_used);
    println!("measured_descent_gas={}", down.gas_used);
    println!(
        "measured_attack_attempts={}",
        u32::from_le_bytes(attempts.return_data[..4].try_into().expect("4 bytes"))
    );
    println!(
        "measured_attack_last_status={}",
        i32::from_le_bytes(last.return_data[..4].try_into().expect("4 bytes"))
    );
}
