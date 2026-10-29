//! Record the measured numbers of the X5 standards.
//!
//! Written by hand, read by `tools/build_standards.py`, which appends this output
//! to `crates/aura-exec/tests/fixtures/x5_standards.txt` verbatim. The point is the
//! same as X4's `x4-record`: a number in the report must be **measured**, and the
//! tool that measures it must be the one that ships.
//!
//! What is measured:
//!
//! * the BLAKE3 digest of each shipped artifact, computed by the node's own
//!   `aura-crypto`, so the Rust gate can verify a digest it can actually compute;
//! * the gas of deploying each standard and of the methods the gate exercises.
//!
//! Output format is `key=value`, one per line, no timestamps: a receipt that
//! changes when nothing changed is not a receipt.

use aura_crypto::blake3_hash;
use aura_exec::lifecycle::Lifecycle;
use aura_exec::{BlockContext, ContractCall};
use aura_state::{Account, MerkleTree, StateKey};
use aura_types::{Address, Amount, Hash, Nonce};

const ARC20: &[u8] = include_bytes!("../../../../standards/arc20/wasm/arc20.wasm");
const ARC721: &[u8] = include_bytes!("../../../../standards/arc721/wasm/arc721.wasm");
const MULTISIG: &[u8] = include_bytes!("../../../../standards/multisig/wasm/multisig.wasm");

const TOKEN: [u8; 20] = [0x11; 20];
const NFT: [u8; 20] = [0x55; 20];
const WALLET: [u8; 20] = [0x77; 20];
const OWNER: [u8; 20] = [0x22; 20];
const ALICE: [u8; 20] = [0x33; 20];
const BOB: [u8; 20] = [0x44; 20];
const S1: [u8; 20] = [0x81; 20];
const S2: [u8; 20] = [0x82; 20];
const S3: [u8; 20] = [0x83; 20];

fn address(bytes: [u8; 20]) -> Address {
    Address::from_slice(&bytes).expect("20-byte address")
}

fn call(contract: [u8; 20], caller: [u8; 20]) -> ContractCall {
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

fn base(accounts: &[[u8; 20]]) -> MerkleTree {
    let entries: Vec<(Hash, Vec<u8>)> = accounts
        .iter()
        .map(|who| {
            let account =
                Account::new(Nonce::new(0), Amount::from_units(0), Hash::ZERO, Hash::ZERO);
            (
                StateKey::account(&address(*who)).digest(),
                account.to_bytes(),
            )
        })
        .collect();
    MerkleTree::from_entries(1, &entries)
}

fn commit(tree: &MerkleTree, diff: Option<&aura_state::StateDiff>) -> MerkleTree {
    match diff {
        Some(diff) => aura_state::apply_diff(tree, diff).expect("diff applies"),
        None => tree.clone(),
    }
}

fn calldata(selector: u32, args: &[u8]) -> Vec<u8> {
    let mut out = selector.to_le_bytes().to_vec();
    out.extend_from_slice(args);
    out
}

fn main() {
    let lifecycle = Lifecycle::new();
    let accounts = [OWNER, ALICE, BOB, S1, S2, S3];

    for (name, code, contract) in [
        ("arc20", ARC20, TOKEN),
        ("arc721", ARC721, NFT),
        ("multisig", MULTISIG, WALLET),
    ] {
        println!("standard_{name}_blake3={}", blake3_hash(code));
        println!("standard_{name}_bytes={}", code.len());

        let start = base(&accounts);
        let deployed = lifecycle
            .deploy(
                &start,
                call(contract, OWNER),
                code.to_vec(),
                Amount::from_units(0),
                None,
            )
            .expect("deploy runs");
        println!("measured_{name}_deploy_gas={}", deployed.gas_used);
        let tree = commit(&start, deployed.diff.as_ref());

        match name {
            "arc20" => {
                let minted = lifecycle
                    .call_data(
                        &tree,
                        call(contract, OWNER),
                        calldata(
                            0x07,
                            &[ALICE.to_vec(), 1_000u128.to_le_bytes().to_vec()].concat(),
                        ),
                    )
                    .expect("call runs");
                println!("measured_arc20_mint_gas={}", minted.gas_used);
                let tree = commit(&tree, minted.diff.as_ref());
                let moved = lifecycle
                    .call_data(
                        &tree,
                        call(contract, ALICE),
                        calldata(
                            0x01,
                            &[BOB.to_vec(), 400u128.to_le_bytes().to_vec()].concat(),
                        ),
                    )
                    .expect("call runs");
                println!("measured_arc20_transfer_gas={}", moved.gas_used);
                let tree = commit(&tree, moved.diff.as_ref());
                let read = lifecycle
                    .call_data(&tree, call(contract, ALICE), calldata(0x04, &ALICE))
                    .expect("call runs");
                println!("measured_arc20_balance_of_gas={}", read.gas_used);
            }
            "arc721" => {
                let minted = lifecycle
                    .call_data(
                        &tree,
                        call(contract, OWNER),
                        calldata(
                            0x01,
                            &[ALICE.to_vec(), 1u128.to_le_bytes().to_vec()].concat(),
                        ),
                    )
                    .expect("call runs");
                println!("measured_arc721_mint_gas={}", minted.gas_used);
                let tree = commit(&tree, minted.diff.as_ref());
                let moved = lifecycle
                    .call_data(
                        &tree,
                        call(contract, ALICE),
                        calldata(0x02, &[BOB.to_vec(), 1u128.to_le_bytes().to_vec()].concat()),
                    )
                    .expect("call runs");
                println!("measured_arc721_transfer_gas={}", moved.gas_used);
            }
            _ => {
                let policy = lifecycle
                    .call_data(
                        &tree,
                        call(contract, OWNER),
                        calldata(
                            0x01,
                            &[vec![2u8], S1.to_vec(), S2.to_vec(), S3.to_vec()].concat(),
                        ),
                    )
                    .expect("call runs");
                println!("measured_multisig_policy_gas={}", policy.gas_used);
                let tree = commit(&tree, policy.diff.as_ref());
                let proposed = lifecycle
                    .call_data(
                        &tree,
                        call(contract, S1),
                        calldata(
                            0x02,
                            &[vec![1u8], BOB.to_vec(), 100u128.to_le_bytes().to_vec()].concat(),
                        ),
                    )
                    .expect("call runs");
                println!("measured_multisig_propose_gas={}", proposed.gas_used);
                let tree = commit(&tree, proposed.diff.as_ref());
                let vote = lifecycle
                    .call_data(
                        &tree,
                        call(contract, S1),
                        calldata(0x03, &1u128.to_le_bytes()),
                    )
                    .expect("call runs");
                println!("measured_multisig_approve_gas={}", vote.gas_used);
            }
        }
    }
}
