//! D3 mempool acceptance: 10,000 contiguous transfers with no lost nonce or spend.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use aura_crypto::ed25519_public_key;
use aura_node::pipeline::{
    D2_BASE_FEE, Pipeline, PipelineConfig, TRANSFER_GAS, encode_transfer_payload,
};
use aura_tx::{Tx, TxKind, TxParams, UnsignedTx, signer_address};
use aura_types::{Address, Amount, ChainId, Gas, Height, Nonce};

const CHAIN: &str = "auralite-d3-mempool-1";
const SENDER_SEED: [u8; 32] = [0x61; 32];
const RECIPIENT: Address = Address::from_bytes([0x72; 20]);
const TRANSFERS: u64 = 10_000;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("aura-d3-mempool-{}-{serial}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sender() -> Address {
    signer_address(&ed25519_public_key(&SENDER_SEED))
}

fn transfer(nonce: u64) -> Tx {
    UnsignedTx::new(TxParams {
        chain_id: ChainId::new(CHAIN).unwrap(),
        signer: sender(),
        nonce: Nonce::new(nonce),
        kind: TxKind::Transfer,
        payload: encode_transfer_payload(RECIPIENT, Amount::ONE),
        gas_limit: Gas::new(TRANSFER_GAS),
        max_fee: D2_BASE_FEE,
        tip: Amount::ZERO,
        expiry: Height::new(100),
    })
    .unwrap()
    .sign(&SENDER_SEED)
    .unwrap()
}

#[test]
fn ten_thousand_transactions_keep_nonce_balance_and_receipts_consistent() {
    let dir = temp_dir();
    let mut node = Pipeline::open(PipelineConfig {
        chain_id: CHAIN.to_owned(),
        data_dir: dir.clone(),
        validator_seed: [0xA7; 32],
        allocations: vec![(sender(), Amount::from_units(500_000_000))],
        genesis_time_ms: 1_700_100_000_000,
    })
    .unwrap();
    let events = node.subscribe();
    let mut txids = Vec::with_capacity(TRANSFERS as usize);
    for nonce in 0..TRANSFERS {
        txids.push(node.submit(transfer(nonce)).unwrap());
    }
    assert_eq!(node.mempool_len(), TRANSFERS as usize);
    assert_eq!(node.mempool_capacity(), 16_384);

    let proposed = node.produce_block().unwrap();
    assert_eq!(proposed.transaction_count, TRANSFERS as usize);
    assert_eq!(proposed.block.body.txs.len(), TRANSFERS as usize);
    assert_eq!(
        proposed.block.header.gas_used.value(),
        TRANSFER_GAS * TRANSFERS
    );
    assert!(proposed.committed.is_empty());
    assert_eq!(node.mempool_len(), 0);
    assert!(events.try_recv().is_err(), "no event before 3-chain commit");

    let _ = node.produce_block().unwrap();
    let finalized = node.produce_block().unwrap();
    assert_eq!(finalized.committed.len(), 1);
    let committed = &finalized.committed[0];
    assert_eq!(committed.receipts.len(), TRANSFERS as usize);
    assert_eq!(committed.storage.root, committed.block.header.state_root);
    assert_eq!(node.storage_head().root, committed.block.header.state_root);

    let sender_account = node.account(&sender()).unwrap();
    assert_eq!(sender_account.nonce.value(), TRANSFERS);
    assert_eq!(sender_account.balance.units(), 289_990_000);
    assert_eq!(
        node.account(&RECIPIENT).unwrap().balance.units(),
        u128::from(TRANSFERS)
    );
    let unique_receipts = committed
        .receipts
        .iter()
        .map(|receipt| receipt.txid)
        .collect::<BTreeSet<_>>();
    assert_eq!(unique_receipts.len(), TRANSFERS as usize);
    for (nonce, receipt) in committed.receipts.iter().enumerate() {
        assert_eq!(receipt.nonce.value(), nonce as u64);
        assert_eq!(receipt.height.value(), 1);
        assert_eq!(receipt.amount, Amount::ONE);
        assert_eq!(receipt.gas_used.value(), TRANSFER_GAS);
        assert_eq!(receipt.base_fee_burned.units(), TRANSFER_GAS as u128);
    }
    assert_eq!(
        node.stored_receipt(&txids[TRANSFERS as usize - 1]).unwrap(),
        Some(committed.receipts[TRANSFERS as usize - 1].to_bytes())
    );
    let event = events.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(event.committed.receipts.len(), TRANSFERS as usize);
    assert_eq!(node.storage_head().version, 2);
    drop(node);
    let _ = fs::remove_dir_all(dir);
}
