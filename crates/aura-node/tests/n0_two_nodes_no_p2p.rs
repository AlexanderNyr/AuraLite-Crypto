//! N0 live two-node RPC isolation test: measures "no P2P" across two independent
//! node HTTP/JSON-RPC servers and proves Node B cannot observe Node A's tx.

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aura_crypto::ed25519_public_key;
use aura_grpc_rest::RestGateway;
use aura_node::observability::{NodeHttpExtension, NodeMetrics};
use aura_node::pipeline::{
    D2_BASE_FEE, Pipeline, PipelineConfig, TRANSFER_GAS, encode_transfer_payload,
};
use aura_node::rpc::PipelineRpcBackend;
use aura_rpc::{RpcServerHandle, RpcService};
use aura_tx::{Tx, TxKind, TxParams, UnsignedTx, signer_address};
use aura_types::{Address, Amount, ChainId, Gas, Height, Nonce};
use serde_json::{Value, json};

const CHAIN: &str = "auralite-n0-baseline-1";
const SENDER_SEED: [u8; 32] = [0x50; 32];
const RECIPIENT: Address = Address::from_bytes([0xAB; 20]);
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn sender() -> Address {
    signer_address(&ed25519_public_key(&SENDER_SEED))
}

fn temporary_dir(label: &str) -> PathBuf {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("aura-n0-{label}-{}-{id}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn signed_transfer(nonce: u64, amount: u128) -> Tx {
    UnsignedTx::new(TxParams {
        chain_id: ChainId::new(CHAIN).unwrap(),
        signer: sender(),
        nonce: Nonce::new(nonce),
        kind: TxKind::Transfer,
        payload: encode_transfer_payload(RECIPIENT, Amount::from_units(amount)),
        gas_limit: Gas::new(TRANSFER_GAS),
        max_fee: D2_BASE_FEE,
        tip: Amount::ZERO,
        expiry: Height::new(100),
    })
    .unwrap()
    .sign(&SENDER_SEED)
    .unwrap()
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

struct LiveNode {
    server: RpcServerHandle,
    pipeline: Arc<Mutex<Pipeline>>,
    root: PathBuf,
}

impl LiveNode {
    fn spawn(label: &str, validator_seed: [u8; 32]) -> Self {
        let root = temporary_dir(label);
        let pid_file = root.join("aurad.pid");
        fs::write(&pid_file, format!("{}\n", std::process::id())).unwrap();
        let pipeline = Pipeline::open(PipelineConfig {
            chain_id: CHAIN.to_owned(),
            data_dir: root.join("state"),
            validator_seed,
            allocations: vec![(sender(), Amount::from_units(2_000_000))],
            genesis_time_ms: 1_700_000_000_000,
        })
        .unwrap();
        let pipeline = Arc::new(Mutex::new(pipeline));
        let backend = Arc::new(PipelineRpcBackend::new(Arc::clone(&pipeline)));
        let service = RpcService::new(backend);
        let rest = Arc::new(RestGateway::new(service.clone()));
        let metrics = Arc::new(NodeMetrics::new(false));
        let extension = Arc::new(NodeHttpExtension::new(
            rest,
            Arc::clone(&pipeline),
            metrics,
            pid_file,
            CHAIN.to_owned(),
        ));
        let server =
            RpcServerHandle::spawn_with_extension("127.0.0.1:0", service, Some(extension)).unwrap();
        Self {
            server,
            pipeline,
            root,
        }
    }

    fn addr(&self) -> SocketAddr {
        self.server.local_addr()
    }

    fn shutdown(mut self) {
        self.server.shutdown();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn http_get_json(address: SocketAddr, path: &str) -> Value {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 200 OK\r\n"),
        "unexpected HTTP response: {response}"
    );
    let body = response.split_once("\r\n\r\n").unwrap().1;
    serde_json::from_str(body).unwrap()
}

fn rpc_post(address: SocketAddr, id: u64, method: &str, params: Value) -> Value {
    let payload = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    }))
    .unwrap();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    )
    .unwrap();
    stream.write_all(&payload).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 200 OK\r\n"),
        "unexpected RPC HTTP response: {response}"
    );
    let body = response.split_once("\r\n\r\n").unwrap().1;
    serde_json::from_str(body).unwrap()
}

#[test]
fn two_nodes_over_rpc_measure_no_p2p_and_node_b_cannot_see_node_a_tx() {
    let node_a = LiveNode::spawn("node-a", [0xA1; 32]);
    let node_b = LiveNode::spawn("node-b", [0xB2; 32]);

    // Both nodes report healthy local processes with zero P2P peers and P2P unimplemented.
    let health_a = http_get_json(node_a.addr(), "/health");
    let health_b = http_get_json(node_b.addr(), "/health");
    assert_eq!(health_a["status"], "healthy");
    assert_eq!(health_b["status"], "healthy");
    assert_eq!(health_a["peerCount"], 0);
    assert_eq!(health_b["peerCount"], 0);
    assert_eq!(health_a["p2p"], "not_implemented");
    assert_eq!(health_b["p2p"], "not_implemented");

    // Submit a signed transfer to Node A over HTTP JSON-RPC.
    let tx = signed_transfer(0, 250);
    let txid_hex = format!("0x{}", tx.txid().to_hex());
    let raw_hex = format!("0x{}", encode_hex(&tx.to_bytes()));
    let submitted = rpc_post(node_a.addr(), 1, "submit_tx", json!({ "tx": raw_hex }));
    assert_eq!(submitted["result"], txid_hex);

    // Node A observes the transaction as pending; Node B has no P2P link and returns null.
    let tx_on_a = rpc_post(node_a.addr(), 2, "get_tx", json!({ "txid": txid_hex }));
    assert_eq!(tx_on_a["result"]["status"], "pending");
    let tx_on_b = rpc_post(node_b.addr(), 3, "get_tx", json!({ "txid": txid_hex }));
    assert!(
        tx_on_b["result"].is_null(),
        "without P2P, Node B must not see Node A's pending tx: {tx_on_b}"
    );

    // Drive Node A through its local 3-chain finality so the transfer is committed on Node A.
    {
        let mut pipe_a = node_a.pipeline.lock().unwrap();
        assert_eq!(pipe_a.produce_block().unwrap().transaction_count, 1);
        assert!(pipe_a.produce_block().unwrap().committed.is_empty());
        assert_eq!(pipe_a.produce_block().unwrap().committed.len(), 1);
    }

    // Node A now serves the committed tx and updated recipient balance over RPC.
    let committed_on_a = rpc_post(node_a.addr(), 4, "get_tx", json!({ "txid": txid_hex }));
    assert_eq!(committed_on_a["result"]["status"], "committed");
    let balance_on_a = rpc_post(
        node_a.addr(),
        5,
        "get_balance",
        json!({ "address": RECIPIENT.to_string() }),
    );
    assert_eq!(balance_on_a["result"]["balance"], "250");

    // Node B still cannot observe Node A's committed tx or state transition.
    let committed_on_b = rpc_post(node_b.addr(), 6, "get_tx", json!({ "txid": txid_hex }));
    assert!(
        committed_on_b["result"].is_null(),
        "without P2P, Node B must not see Node A's committed tx: {committed_on_b}"
    );
    let balance_on_b = rpc_post(
        node_b.addr(),
        7,
        "get_balance",
        json!({ "address": RECIPIENT.to_string() }),
    );
    assert_eq!(balance_on_b["result"]["balance"], "0");
    let head_on_b = rpc_post(node_b.addr(), 8, "get_block", json!({}));
    assert_eq!(head_on_b["result"]["height"], 0);

    node_a.shutdown();
    node_b.shutdown();
}
