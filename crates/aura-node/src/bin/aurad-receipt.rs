//! `aurad-receipt` — O2 milestone measurements (D-O2-2, D-O2-3).
//!
//! Measures the `ROADMAP.md` §5 criteria on the shipped pipeline and
//! cluster and writes the `milestone.txt` receipt that
//! `aura-milestone check` validates:
//!
//! - M0: genesis -> 1000 signed faucet transfers -> committed state root,
//!   replayed twice in fresh stores; both roots recorded (byte-identical
//!   is the M0 criterion).
//! - M1: the in-process 4-validator `DevnetCluster` steps 100 committed
//!   blocks (block interval measured), all four heads compared for desync,
//!   then a faucet transfer and a `get_balance` call through the shipped
//!   JSON-RPC dispatch (`aura_rpc::RpcService` — the same handler the HTTP
//!   listener calls).
//!
//! The tool never fabricates values: every receipt line is a measurement.
//! Usage: `aurad-receipt [--out release/milestone.txt]`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use aura_node::devnet::{
    DEVNET_CHAIN_ID, DEVNET_VALIDATOR_COUNT, DevnetCluster, build_faucet_transfer,
};
use aura_node::rpc::PipelineRpcBackend;
use aura_rpc::RpcService;
use aura_types::{Address, Amount};
use serde_json::Value;

/// M0 replay size (ROADMAP §5: 1000 tx).
const M0_TXS: u64 = 1000;

/// M1 committed blocks (ROADMAP §5: 100 blocks).
const M1_BLOCKS: u64 = 100;

/// Deterministic per-index recipient (M0 replay).
fn recipient(index: u64) -> Address {
    let mut raw = [0xA0u8; 20];
    raw[..8].copy_from_slice(&index.to_le_bytes());
    Address::from_bytes(raw)
}

/// One M0 replay: genesis -> `M0_TXS` transfers -> final state root hex.
fn replay_once(base: &Path, run: u64) -> String {
    let dir = base.join(format!("m0-run-{run}"));
    let mut cluster = DevnetCluster::open(&dir).expect("open M0 cluster");
    for index in 0..M0_TXS {
        let tx = build_faucet_transfer(
            DEVNET_CHAIN_ID,
            recipient(index),
            Amount::from_units(1),
            index,
        )
        .expect("build M0 transfer");
        cluster.submit_and_gossip_tx(0, tx).expect("submit M0 tx");
    }
    // Step until every pipeline has drained its mempool.
    for _ in 0..10_000 {
        let pending: usize = cluster
            .pipelines
            .iter()
            .map(|pipeline| pipeline.mempool_len())
            .sum();
        if pending == 0 {
            break;
        }
        cluster.step_block().expect("step M0 block");
    }
    let pending: usize = cluster
        .pipelines
        .iter()
        .map(|pipeline| pipeline.mempool_len())
        .sum();
    assert_eq!(pending, 0, "M0 mempool must drain");
    cluster.pipelines[0].storage_head().root.to_hex()
}

/// The two M0 roots.
fn measure_m0(base: &Path) -> (String, String) {
    let first = replay_once(base, 0);
    let second = replay_once(base, 1);
    (first, second)
}

/// The M1 measurements: block interval, head agreement (desync) and an RPC
/// balance read (D-O2-3).
fn measure_m1(base: &Path) -> (i64, i64, i64, i64) {
    let dir = base.join("m1-cluster");
    let mut cluster = DevnetCluster::open(&dir).expect("open M1 cluster");

    let start = Instant::now();
    for _ in 0..M1_BLOCKS {
        cluster.step_block().expect("step M1 block");
    }
    let elapsed_ms = start.elapsed().as_millis() as i64;
    let block_ms = elapsed_ms / M1_BLOCKS as i64;
    // Commitment lags proposal by the finality depth; the criterion counts
    // committed blocks.
    for _ in 0..16 {
        if cluster.pipelines[0].committed_height() >= M1_BLOCKS {
            break;
        }
        cluster.step_block().expect("step M1 drain block");
    }

    let reference = (
        cluster.pipelines[0].committed_height(),
        cluster.pipelines[0].storage_head().root,
    );
    let mut desync = 0i64;
    for pipeline in &cluster.pipelines {
        if (pipeline.committed_height(), pipeline.storage_head().root) != reference {
            desync += 1;
        }
    }
    let height = cluster.pipelines[0].committed_height() as i64;

    // A faucet transfer, one block, then the recipient balance over the
    // shipped JSON-RPC dispatch.
    let recipient = Address::from_bytes([0xAB; 20]);
    let tx = build_faucet_transfer(DEVNET_CHAIN_ID, recipient, Amount::from_units(250_000), 0)
        .expect("build M1 transfer");
    cluster.submit_and_gossip_tx(0, tx).expect("submit M1 tx");
    cluster.step_block().expect("step M1 transfer block");
    // Wait until the transfer is committed on every pipeline.
    for _ in 0..16 {
        if cluster.pipelines[0]
            .account(&recipient)
            .is_some_and(|a| a.balance.units() > 0)
        {
            break;
        }
        cluster.step_block().expect("step M1 transfer drain");
    }

    let pipeline = cluster.pipelines.remove(0);
    let backend: Arc<dyn aura_rpc::RpcBackend> =
        Arc::new(PipelineRpcBackend::new(Arc::new(Mutex::new(pipeline))));
    let service = RpcService::new(backend);
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "get_balance",
        "params": { "address": recipient.to_bech32() },
    });
    let request_bytes = serde_json::to_vec(&request).expect("encode request");
    let response = service
        .process_http_message(&request_bytes)
        .expect("rpc response");
    let value: Value = serde_json::from_slice(&response).expect("decode response");
    let balance: i64 = value["result"]["balance"]
        .as_str()
        .expect("balance string")
        .parse()
        .expect("balance integer");

    (block_ms, height, desync, balance)
}

fn main() {
    let mut out_path = PathBuf::from("release/milestone.txt");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                index += 1;
                out_path = PathBuf::from(args.get(index).expect("--out value"));
            }
            other => {
                eprintln!("error: usage: unknown argument {other}");
                std::process::exit(64);
            }
        }
        index += 1;
    }

    let base = std::env::temp_dir().join(format!("aura-o2-receipt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create measurement dir");

    let (root_a, root_b) = measure_m0(&base);
    let (block_ms, height, desync, balance) = measure_m1(&base);
    let _ = std::fs::remove_dir_all(&base);

    let receipt = format!(
        "# M0/M1 milestone receipts (O2) — measured by `aurad-receipt`, checked by `aura-milestone check`.\n\
         rule milestones-v1\n\
         m0 txs {M0_TXS}\n\
         m0 state_root {root_a}\n\
         m0 state_root {root_b}\n\
         m1 validators {DEVNET_VALIDATOR_COUNT}\n\
         m1 block_ms {block_ms}\n\
         m1 height {height}\n\
         m1 desync {desync}\n\
         m1 rpc_balance {balance}\n"
    );
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).expect("create out dir");
    }
    std::fs::write(&out_path, &receipt).expect("write receipt");
    print!("{receipt}");
}
