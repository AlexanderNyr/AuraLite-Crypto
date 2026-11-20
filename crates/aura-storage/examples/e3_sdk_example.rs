//! E3 SDK example (Rust): the same demo as `sdk/python/example.py`
//! and `sdk/ts/example.ts`, run in CI by tools/e3_examples_check.py.
//! Spawns a live faucet + explorer, drives them through
//! `aura_storage::sdk::AuraLiteClient`, asserts the outcomes and
//! prints a one-line `E3-EXAMPLE-OK` summary.

use aura_storage::explorer::ExplorerStore;
use aura_storage::faucet::{Faucet, FaucetConfig, Network};
use aura_storage::indexer::{BlockRef, IngestOutcome};
use aura_storage::sdk::AuraLiteClient;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

const DAY: u64 = 86_400;

fn main() {
    // Live faucet.
    let faucet = Arc::new(Mutex::new(Faucet::new(
        Network::Testnet,
        FaucetConfig {
            payout: 10,
            funding: 1000,
        },
    )));
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let serve_faucet = Arc::clone(&faucet);
    std::thread::spawn(move || aura_storage::faucet::serve(serve_faucet, listener));

    // Live explorer with a three-block chain.
    let mut store = ExplorerStore::new();
    for height in 1..=3u64 {
        let mut hash = [0u8; 32];
        hash[1..9].copy_from_slice(&height.to_le_bytes());
        let mut parent = [0u8; 32];
        if height > 1 {
            parent[1..9].copy_from_slice(&(height - 1).to_le_bytes());
        }
        let block = BlockRef {
            hash,
            parent,
            height,
        };
        assert_eq!(store.index_mut().ingest(block), IngestOutcome::Indexed);
    }
    let store = Arc::new(store);
    let explorer_listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let explorer_port = explorer_listener.local_addr().expect("addr").port();
    let serve_store = Arc::clone(&store);
    std::thread::spawn(move || aura_storage::explorer::serve(serve_store, explorer_listener));

    // The demo: same steps as the TS/Python examples.
    let client = AuraLiteClient::new(&format!("http://127.0.0.1:{port}"));
    let explorer = AuraLiteClient::new(&format!("http://127.0.0.1:{explorer_port}"));

    let (_, first) = client
        .faucet_request(3600, "testnet", "alice")
        .expect("rpc");
    assert!(
        first.contains("\"outcome\":\"paid\""),
        "first request must pay"
    );
    let (_, repeat) = client
        .faucet_request(4000, "testnet", "alice")
        .expect("rpc");
    assert!(
        repeat.contains("\"outcome\":\"denied_rate_limit\""),
        "same-day repeat must be denied"
    );
    let (_, next_day) = client
        .faucet_request(DAY + 3600, "testnet", "alice")
        .expect("rpc");
    assert!(
        next_day.contains("\"outcome\":\"paid\""),
        "next day must pay"
    );
    let (_, wrong_net) = client
        .faucet_request(DAY + 4000, "mainnet", "bob")
        .expect("rpc");
    assert!(
        wrong_net.contains("\"outcome\":\"denied_wrong_network\""),
        "mainnet must be refused"
    );
    let (_, status) = client.faucet_status().expect("rpc");
    assert!(
        status.contains("\"payouts\":2"),
        "two payouts in the ledger"
    );
    let (health_code, health) = explorer.health().expect("rpc");
    assert_eq!(health_code, 200, "explorer health");
    assert!(health.contains("\"tip_height\":3"), "explorer tip is 3");
    let (block_code, block) = explorer.block(2).expect("rpc");
    assert_eq!(block_code, 200, "explorer block");
    assert!(block.contains("\"height\":2"), "block 2 served");

    println!(
        "E3-EXAMPLE-OK {{\"lang\":\"rust\",\"payouts\":2,\"denied_rate_limit\":1,\"denied_wrong_network\":1,\"tip_height\":3}}"
    );
}
