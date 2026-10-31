//! D4 node-backed JSON-RPC contract tests over the D2/D3 pipeline.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aura_crypto::ed25519_public_key;
use aura_node::pipeline::{
    D2_BASE_FEE, Pipeline, PipelineConfig, TRANSFER_GAS, encode_transfer_payload,
};
use aura_node::rpc::PipelineRpcBackend;
use aura_rpc::{RpcBackend, RpcService, SubscriptionPoll};
use aura_tx::{Tx, TxKind, TxParams, UnsignedTx, signer_address};
use aura_types::{Address, Amount, ChainId, Gas, Height, Nonce};
use serde_json::{Value, json};

const CHAIN: &str = "auralite-d4-rpc-1";
const SEED: [u8; 32] = [0x44; 32];
const VALIDATOR: [u8; 32] = [0x45; 32];
const RECIPIENT: Address = Address::from_bytes([0x99; 20]);
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn sender() -> Address {
    signer_address(&ed25519_public_key(&SEED))
}

fn temp_dir() -> PathBuf {
    let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("aura-d4-rpc-{}-{id}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn transfer(nonce: u64) -> Tx {
    UnsignedTx::new(TxParams {
        chain_id: ChainId::new(CHAIN).unwrap(),
        signer: sender(),
        nonce: Nonce::new(nonce),
        kind: TxKind::Transfer,
        payload: encode_transfer_payload(RECIPIENT, Amount::from_units(100)),
        gas_limit: Gas::new(TRANSFER_GAS),
        max_fee: D2_BASE_FEE,
        tip: Amount::ZERO,
        expiry: Height::new(100),
    })
    .unwrap()
    .sign(&SEED)
    .unwrap()
}

fn service() -> (RpcService, Arc<PipelineRpcBackend>, PathBuf) {
    let path = temp_dir();
    let pipeline = Pipeline::open(PipelineConfig {
        chain_id: CHAIN.to_owned(),
        data_dir: path.clone(),
        validator_seed: VALIDATOR,
        allocations: vec![(sender(), Amount::from_units(2_000_000))],
        genesis_time_ms: 1_700_000_000_000,
    })
    .unwrap();
    let shared = Arc::new(Mutex::new(pipeline));
    let backend = Arc::new(PipelineRpcBackend::new(shared));
    (RpcService::new(backend.clone()), backend, path)
}

fn call(service: &RpcService, id: u64, method: &str, params: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
    let bytes = service
        .process_http_message(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn d4_rpc_methods_cover_admission_queries_errors_and_durable_subscriptions() {
    let (service, backend, path) = service();
    assert!(!backend.pipeline().lock().unwrap().needs_block_production());
    let initial = call(
        &service,
        1,
        "get_balance",
        json!({"address":sender().to_string()}),
    );
    assert_eq!(initial["result"]["balance"], "2000000");
    assert_eq!(initial["result"]["nonce"], 0);

    let tx = transfer(0);
    let txid = tx.txid();
    let submit = call(
        &service,
        2,
        "submit_tx",
        json!({"tx":format!("0x{}", encode_hex(&tx.to_bytes()))}),
    );
    assert_eq!(submit["result"], format!("0x{}", txid.to_hex()));
    assert!(backend.pipeline().lock().unwrap().needs_block_production());
    assert_eq!(
        call(
            &service,
            3,
            "estimate_gas",
            json!({"tx":format!("0x{}", encode_hex(&tx.to_bytes()))})
        )["result"]["gasUnits"],
        TRANSFER_GAS
    );
    assert_eq!(
        call(
            &service,
            4,
            "get_tx",
            json!({"txid":format!("0x{}", txid.to_hex())})
        )["result"]["status"],
        "pending"
    );

    let unsupported = call(
        &service,
        5,
        "call",
        json!({"to":RECIPIENT.to_string(),"data":"0x"}),
    );
    assert_eq!(unsupported["error"]["code"], -32004);
    let bad_address = call(
        &service,
        6,
        "get_balance",
        json!({"address":"not-an-address"}),
    );
    assert_eq!(bad_address["error"]["code"], -32602);

    let events = backend.subscribe().unwrap();
    {
        let pipeline = backend.pipeline();
        let mut pipeline = pipeline.lock().unwrap();
        assert_eq!(pipeline.produce_block().unwrap().transaction_count, 1);
        assert!(pipeline.produce_block().unwrap().committed.is_empty());
        assert_eq!(pipeline.produce_block().unwrap().committed.len(), 1);
        assert!(!pipeline.needs_block_production());
    }
    let mut events = events;
    let event = match events.poll(Duration::from_secs(2)) {
        SubscriptionPoll::Event(value) => value,
        other => panic!("expected a committed event, got {other:?}"),
    };
    assert_eq!(event["height"], 1);
    let committed = call(
        &service,
        7,
        "get_tx",
        json!({"txid":format!("0x{}", txid.to_hex())}),
    );
    assert_eq!(committed["result"]["status"], "committed");
    assert_eq!(committed["result"]["receipt"]["nonce"], 0);
    assert_eq!(committed["result"]["receipt"]["amount"], "100");

    let block = call(&service, 8, "get_block", json!({}));
    assert_eq!(block["result"]["height"], 1);
    assert_eq!(block["result"]["transactionCount"], 1);
    let genesis = call(&service, 9, "get_block", json!({"height":0}));
    assert_eq!(genesis["result"]["height"], 0);
    assert!(call(&service, 10, "get_block", json!({"height":99}))["result"].is_null());
    assert!(
        call(
            &service,
            11,
            "get_tx",
            json!({"txid":format!("0x{}", "11".repeat(32))})
        )["result"]
            .is_null()
    );
    drop(service);
    drop(backend);
    let _ = fs::remove_dir_all(path);
}

#[test]
fn rpc_error_codes_are_stable_for_application_failures() {
    let (service, _backend, path) = service();
    let unknown = call(&service, 1, "no_such_method", json!({}));
    assert_eq!(unknown["error"]["code"], -32601);
    let malformed = call(&service, 2, "submit_tx", json!({"tx":"0x123"}));
    assert_eq!(malformed["error"]["code"], -32602);
    let subscribe_over_http = call(&service, 3, "subscribe", json!({}));
    assert_eq!(subscribe_over_http["error"]["code"], -32005);
    let _ = fs::remove_dir_all(path);
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(HEX[(byte >> 4) as usize] as char);
        text.push(HEX[(byte & 0xf) as usize] as char);
    }
    text
}
