//! D2 end-to-end transaction pipeline gate.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use aura_crypto::ed25519_public_key;
use aura_node::pipeline::{
    D2_BASE_FEE, Pipeline, PipelineConfig, PipelineError, TRANSFER_GAS, encode_transfer_payload,
};
use aura_tx::{Tx, TxKind, TxParams, UnsignedTx, signer_address};
use aura_types::{Address, Amount, ChainId, Gas, Height, Nonce};

const CHAIN: &str = "auralite-test-1";
const SENDER_SEED: [u8; 32] = [0x31; 32];
const RECIPIENT: Address = Address::from_bytes([0x52; 20]);
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("aura-d2-{tag}-{}-{serial}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sender() -> Address {
    signer_address(&ed25519_public_key(&SENDER_SEED))
}

fn pipeline(tag: &str, balance: u128) -> (Pipeline, PathBuf) {
    let dir = temp_dir(tag);
    let node = Pipeline::open(PipelineConfig {
        chain_id: CHAIN.to_string(),
        data_dir: dir.clone(),
        validator_seed: [0xA7; 32],
        allocations: vec![(sender(), Amount::from_units(balance))],
        genesis_time_ms: 1_700_000_000_000,
    })
    .expect("pipeline should initialise");
    (node, dir)
}

fn transfer(
    seed: [u8; 32],
    chain: &str,
    nonce: u64,
    amount: u128,
    gas_limit: u64,
    max_fee: u128,
    tip: u128,
) -> Tx {
    let signer = signer_address(&ed25519_public_key(&seed));
    UnsignedTx::new(TxParams {
        chain_id: ChainId::new(chain).unwrap(),
        signer,
        nonce: Nonce::new(nonce),
        kind: TxKind::Transfer,
        payload: encode_transfer_payload(RECIPIENT, Amount::from_units(amount)),
        gas_limit: Gas::new(gas_limit),
        max_fee: Amount::from_units(max_fee),
        tip: Amount::from_units(tip),
        expiry: Height::new(100),
    })
    .unwrap()
    .sign(&seed)
    .unwrap()
}

#[test]
fn thousand_transfers_finalize_to_durable_receipts_and_events() {
    let (mut node, dir) = pipeline("thousand", 30_000_000);
    let events = node.subscribe();
    let mut txids = Vec::with_capacity(1_000);
    for nonce in 0..1_000 {
        let tx = transfer(
            SENDER_SEED,
            CHAIN,
            nonce,
            1,
            TRANSFER_GAS,
            D2_BASE_FEE.units(),
            0,
        );
        txids.push(node.submit(tx).unwrap());
    }

    let first = node.produce_block().unwrap();
    assert_eq!(first.transaction_count, 1_000);
    assert_eq!(first.block.body.txs.len(), 1_000);
    assert_eq!(first.block.header.gas_used.value(), TRANSFER_GAS * 1_000);
    assert_eq!(first.block.header.height.value(), 1);
    assert_eq!(first.block.header.timestamp_ms, 1_700_000_001_000);
    assert!(first.committed.is_empty());
    assert_eq!(
        node.storage_head().version,
        1,
        "no state before 3-chain finality"
    );
    assert!(events.try_recv().is_err(), "no event before durable commit");

    let second = node.produce_block().unwrap();
    assert!(
        second.block.header.qc.is_some(),
        "child carries its parent QC"
    );
    let finalized = node.produce_block().unwrap();
    assert_eq!(finalized.committed.len(), 1);
    let commit = &finalized.committed[0];
    assert_eq!(commit.block.header.height.value(), 1);
    assert_eq!(commit.receipts.len(), 1_000);
    assert_eq!(commit.storage.version, 2);
    assert_eq!(commit.storage.root, commit.block.header.state_root);
    assert_eq!(node.storage_head().root, commit.block.header.state_root);

    let sender_account = node.account(&sender()).unwrap();
    assert_eq!(sender_account.nonce.value(), 1_000);
    assert_eq!(sender_account.balance.units(), 8_999_000);
    let recipient_account = node.account(&RECIPIENT).unwrap();
    assert_eq!(recipient_account.balance.units(), 1_000);

    for (nonce, receipt) in commit.receipts.iter().enumerate() {
        assert_eq!(receipt.nonce.value(), nonce as u64);
        assert_eq!(receipt.height.value(), 1);
        assert_eq!(receipt.amount.units(), 1);
        assert_eq!(receipt.gas_used.value(), TRANSFER_GAS);
        assert_eq!(receipt.base_fee_burned.units(), TRANSFER_GAS as u128);
    }
    assert_eq!(
        node.stored_receipt(&txids[0]).unwrap(),
        Some(commit.receipts[0].to_bytes())
    );
    let event = events.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(
        event.committed.block.block_hash(),
        commit.block.block_hash()
    );
    assert_eq!(event.committed.receipts.len(), 1_000);
    assert_eq!(node.storage_head().version, 2);
    drop(node);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn receipt_is_durable_before_commit_event() {
    let (mut node, dir) = pipeline("receipt-order", 1_000_000);
    let events = node.subscribe();
    let txid = node
        .submit(transfer(SENDER_SEED, CHAIN, 0, 1, TRANSFER_GAS, 1, 0))
        .unwrap();
    let _ = node.produce_block().unwrap();
    let _ = node.produce_block().unwrap();
    assert_eq!(node.storage_head().version, 1);
    assert!(events.try_recv().is_err());
    let committed = node.produce_block().unwrap();
    assert_eq!(committed.committed.len(), 1);
    assert!(node.stored_receipt(&txid).unwrap().is_some());
    let event = events.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(event.committed.receipts[0].txid, txid);
    drop(node);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn refuses_to_restart_committed_state_without_a_finality_checkpoint() {
    let (mut node, dir) = pipeline("reopen", 1_000_000);
    node.submit(transfer(SENDER_SEED, CHAIN, 0, 1, TRANSFER_GAS, 1, 0))
        .unwrap();
    let _ = node.produce_block().unwrap();
    let _ = node.produce_block().unwrap();
    let _ = node.produce_block().unwrap();
    drop(node);

    let reopened = Pipeline::open(PipelineConfig {
        chain_id: CHAIN.to_string(),
        data_dir: dir.clone(),
        validator_seed: [0xA7; 32],
        allocations: vec![(sender(), Amount::from_units(1_000_000))],
        genesis_time_ms: 1_700_000_000_000,
    });
    assert!(matches!(
        reopened,
        Err(PipelineError::RecoveryCheckpointRequired)
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn rejects_replay_from_another_chain() {
    let (mut node, dir) = pipeline("wrong-chain", 1_000_000);
    let tx = transfer(SENDER_SEED, "other-chain", 0, 1, TRANSFER_GAS, 1, 0);
    assert!(matches!(
        node.submit(tx),
        Err(PipelineError::InvalidTransaction(
            aura_tx::TxError::WrongChain(_, _)
        ))
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn rejects_nonce_gap() {
    let (mut node, dir) = pipeline("nonce-gap", 1_000_000);
    let tx = transfer(SENDER_SEED, CHAIN, 1, 1, TRANSFER_GAS, 1, 0);
    assert!(matches!(
        node.submit(tx),
        Err(PipelineError::NonceMismatch {
            expected: 0,
            got: 1
        })
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn rejects_insufficient_balance_for_value_and_fee() {
    let (mut node, dir) = pipeline("low-balance", 1_000);
    let tx = transfer(SENDER_SEED, CHAIN, 0, 1, TRANSFER_GAS, 1, 0);
    assert!(matches!(
        node.submit(tx),
        Err(PipelineError::InsufficientBalance)
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn rejects_transfer_below_gas_floor() {
    let (mut node, dir) = pipeline("low-gas", 1_000_000);
    let tx = transfer(SENDER_SEED, CHAIN, 0, 1, 1, 1, 0);
    assert!(matches!(
        node.submit(tx),
        Err(PipelineError::GasLimitTooLow {
            required: TRANSFER_GAS,
            got: 1
        })
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn rejects_fee_cap_below_base_fee_plus_tip() {
    let (mut node, dir) = pipeline("fee-cap", 1_000_000);
    let tx = transfer(SENDER_SEED, CHAIN, 0, 1, TRANSFER_GAS, 1, 1);
    assert!(matches!(node.submit(tx), Err(PipelineError::FeeCapTooLow)));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn rejects_duplicate_transaction_ids() {
    let (mut node, dir) = pipeline("duplicate", 1_000_000);
    let tx = transfer(SENDER_SEED, CHAIN, 0, 1, TRANSFER_GAS, 1, 0);
    node.submit(tx.clone()).unwrap();
    assert!(matches!(
        node.submit(tx),
        Err(PipelineError::DuplicateTransaction)
    ));
    let _ = fs::remove_dir_all(dir);
}
