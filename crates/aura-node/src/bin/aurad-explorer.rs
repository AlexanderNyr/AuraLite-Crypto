//! `aurad-explorer` — the O4 public block explorer over the node
//! `RpcBackend` (D-O4-5).
//!
//! Reads query lines (`head`, `block <height>`, `tx <txid>`,
//! `balance <bech32>`) and prints a deterministic one-line answer per
//! query through `PipelineRpcBackend`:
//!
//! ```text
//! head height=12 hash=0x... txs=1680
//! block 3 hash=0x... txs=140
//! tx 0x... status=committed
//! balance auralite1... 9998320
//! ```
//!
//! Design: `docs/o4-testnet-public-plan.md`, guide:
//! `docs/testnet-public.md`.
//! Usage: `aurad-explorer [--queries <file>]`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use aura_node::devnet::{
    DEVNET_CHAIN_ID, DEVNET_FAUCET_INITIAL_BALANCE, build_faucet_transfer, faucet_address,
};
use aura_node::pipeline::{Pipeline, PipelineConfig};
use aura_node::rpc::PipelineRpcBackend;
use aura_rpc::RpcBackend;
use aura_types::{Address, Amount};
use serde_json::{Value, json};

fn main() {
    let mut queries_path = PathBuf::from("release/explorer-queries.txt");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--queries" => {
                index += 1;
                queries_path = PathBuf::from(args.get(index).expect("--queries value"));
            }
            other => {
                eprintln!("error: usage: unknown argument {other}");
                std::process::exit(64);
            }
        }
        index += 1;
    }

    // The service runs over the same RpcBackend the node RPC exposes.
    let dir = std::env::temp_dir().join(format!("aura-o4-explorer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut pipeline = Pipeline::open(PipelineConfig {
        chain_id: DEVNET_CHAIN_ID.to_owned(),
        data_dir: dir.clone(),
        validator_seed: [42u8; 32],
        allocations: vec![(
            faucet_address(),
            Amount::from_units(DEVNET_FAUCET_INITIAL_BALANCE),
        )],
        genesis_time_ms: 0,
    })
    .expect("open explorer pipeline");
    // A short committed history to explore: one faucet transfer per block.
    for nonce in 0..8u64 {
        let tx = build_faucet_transfer(
            DEVNET_CHAIN_ID,
            {
                let mut raw = [0xA2u8; 20];
                raw[..8].copy_from_slice(&nonce.to_le_bytes());
                Address::from_bytes(raw)
            },
            Amount::from_units(1),
            nonce,
        )
        .expect("build explorer transfer");
        pipeline.submit(tx).expect("submit explorer transfer");
        pipeline.produce_block().expect("produce explorer block");
    }
    let backend = PipelineRpcBackend::new(Arc::new(Mutex::new(pipeline)));

    let queries = std::fs::read_to_string(&queries_path).unwrap_or_else(|_| {
        eprintln!(
            "error: usage: cannot read queries {}",
            queries_path.display()
        );
        std::process::exit(64);
    });

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for raw in queries.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split(' ');
        let verb = fields.next().unwrap_or_default();
        let answer = match verb {
            "head" if fields.next().is_none() => {
                let block = backend.get_block(&Value::Null).expect("head block");
                format!(
                    "head height={} hash={} txs={}",
                    block["height"],
                    block["hash"].as_str().unwrap_or("-"),
                    block["transactionCount"]
                )
            }
            "block" => {
                let height: u64 = fields
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| {
                        eprintln!("error: usage: bad query line: {line}");
                        std::process::exit(64);
                    });
                if fields.next().is_some() {
                    eprintln!("error: usage: bad query line: {line}");
                    std::process::exit(64);
                }
                match backend
                    .get_block(&json!({ "height": height }))
                    .expect("block query")
                {
                    Value::Null => format!("block {height} missing"),
                    block => format!(
                        "block {height} hash={} txs={}",
                        block["hash"].as_str().unwrap_or("-"),
                        block["transactionCount"]
                    ),
                }
            }
            "tx" => {
                let txid = fields.next().unwrap_or_default();
                if txid.is_empty() || fields.next().is_some() {
                    eprintln!("error: usage: bad query line: {line}");
                    std::process::exit(64);
                }
                match backend.get_tx(&json!({ "txid": txid })).expect("tx query") {
                    Value::Null => format!("tx {txid} missing"),
                    receipt => format!(
                        "tx {txid} status={}",
                        receipt["status"].as_str().unwrap_or("-")
                    ),
                }
            }
            "balance" => {
                let address = fields.next().unwrap_or_default();
                if address.is_empty() || fields.next().is_some() {
                    eprintln!("error: usage: bad query line: {line}");
                    std::process::exit(64);
                }
                let account = backend
                    .get_balance(&json!({ "address": address }))
                    .expect("balance query");
                format!(
                    "balance {address} {}",
                    account["balance"].as_str().unwrap_or("-")
                )
            }
            _ => {
                eprintln!("error: usage: bad query line: {line}");
                std::process::exit(64);
            }
        };
        let _ = writeln!(out, "{answer}");
    }
    let _ = out.flush();
    let _ = std::fs::remove_dir_all(&dir);
}
