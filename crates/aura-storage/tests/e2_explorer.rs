//! E2 explorer gate (ECOSYSTEM_PLAN E2): "10 user scenarios green;
//! reproducible contract build = same code_hash" with named
//! negative-control messages. Scenarios are served over real HTTP from
//! the explorer's stdlib web UI.

use aura_storage::explorer::{
    self, ExplorerStore, SearchHit, TxRecord, TxStatus, build_contract, verify_contract,
};
use aura_storage::indexer::{BlockRef, IngestOutcome};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

const BLOCKS: u64 = 40;
const ANCESTOR: u64 = 35;
const FORK_TIP: u64 = 42;

const ADDER_SRC: &str = "push 3\npush 4\nadd\nreturn";
const ALIAS_SRC: &str = "  PUSH   003  \n\npush 4\n# comment\nADD\nReturn";
const COUNTER_SRC: &str = "push 1\npush 2\nmul\npush 5\nadd\ndup\ndrop\nreturn";

// Fixture code hashes (tools/fixtures/e2_contracts.json pins the same
// values; tools/e2_explorer_check.py recomputes them independently).
const ADDER_CODE_HASH: &str = "64e3e32d01fc52d835122ece7b22607e3dbc7dfaa0e30fbe958b8ca5465d0b0d";
const COUNTER_CODE_HASH: &str = "b17231dd16285c6493af18cc5b56c9d1ad51de21138da39e573d533f7d2d412a";

/// Deterministic block on branch `fork` at `height`.
fn blk(fork: u8, height: u64) -> BlockRef {
    let mut hash = [0u8; 32];
    hash[0] = fork;
    hash[1..9].copy_from_slice(&height.to_le_bytes());
    let parent = if height <= 1 {
        [0u8; 32]
    } else if fork == 1 && height == ANCESTOR + 1 {
        blk(0, ANCESTOR).hash
    } else {
        let mut p = [0u8; 32];
        p[0] = fork;
        p[1..9].copy_from_slice(&(height - 1).to_le_bytes());
        p
    };
    BlockRef {
        hash,
        parent,
        height,
    }
}

fn txh(tag: u8) -> [u8; 32] {
    [tag; 32]
}

fn hex32(hash: &[u8; 32]) -> String {
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

/// The E2 fixture world: 40 blocks, a 7-block reorg at height 35,
/// three transactions (one ends up orphaned), two validators and two
/// contracts.
fn build_store() -> ExplorerStore {
    let mut store = ExplorerStore::new();
    for height in 1..=BLOCKS {
        let outcome = store.index_mut().ingest(blk(0, height));
        assert_eq!(outcome, IngestOutcome::Indexed);
    }
    let fork: Vec<BlockRef> = ((ANCESTOR + 1)..=FORK_TIP).map(|h| blk(1, h)).collect();
    assert!(store.index_mut().reorg_to(ANCESTOR, &fork));

    store.set_genesis("alice", 1000);
    store.set_genesis("charlie", 500);
    store.set_genesis("eve", 700);
    store.add_tx(TxRecord {
        hash: txh(1),
        block_hash: blk(0, 5).hash,
        block_height: 5,
        from: "alice".to_string(),
        to: "bob".to_string(),
        amount: 100,
        fee: 1,
    });
    store.add_tx(TxRecord {
        hash: txh(2),
        block_hash: blk(0, 38).hash,
        block_height: 38,
        from: "charlie".to_string(),
        to: "dave".to_string(),
        amount: 50,
        fee: 2,
    });
    store.add_tx(TxRecord {
        hash: txh(3),
        block_hash: blk(1, 40).hash,
        block_height: 40,
        from: "eve".to_string(),
        to: "frank".to_string(),
        amount: 70,
        fee: 3,
    });

    store.add_validator("val-a", 100);
    store.add_validator("val-b", 150);
    for _ in 0..20 {
        store.note_proposal("val-a");
    }
    for _ in 0..22 {
        store.note_proposal("val-b");
    }

    store
        .deploy_contract("ct-adder", ADDER_SRC)
        .expect("adder builds");
    store
        .deploy_contract("ct-counter", COUNTER_SRC)
        .expect("counter builds");
    store
}

/// Fixture world served over real HTTP.
fn world() -> (Arc<ExplorerStore>, u16) {
    let store = Arc::new(build_store());
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let serve_store = Arc::clone(&store);
    std::thread::spawn(move || explorer::serve(serve_store, listener));
    (store, port)
}

fn http_get(port: u16, path: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let request = format!("GET {path} HTTP/1.1\r\nHost: explorer\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("send");
    let mut buf = String::new();
    stream.read_to_string(&mut buf).expect("read");
    let status = buf
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = buf.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body)
}

#[test]
fn e2s_01_home_page_shows_all_sections() {
    let (_store, port) = world();
    let (status, body) = http_get(port, "/");
    assert_eq!(
        status, 200,
        "E2: home page must show blocks, tx, accounts, validators, contracts and search"
    );
    for section in [
        "blocks",
        "tx",
        "accounts",
        "validators",
        "contracts",
        "search",
    ] {
        assert!(
            body.contains(&format!("id=\"{section}\"")),
            "E2: home page must show blocks, tx, accounts, validators, contracts and search"
        );
    }
}

#[test]
fn e2s_02_latest_blocks_come_from_the_indexer() {
    let (store, port) = world();
    let (status, body) = http_get(port, "/blocks");
    assert_eq!(
        status, 200,
        "E2: explorer must read blocks from the indexer"
    );
    let expected = store.latest_blocks(10);
    assert_eq!(
        expected.len(),
        10,
        "E2: explorer must read blocks from the indexer"
    );
    for view in &expected {
        assert!(
            body.contains(&hex32(&view.hash)),
            "E2: explorer must read blocks from the indexer"
        );
    }
    for height in 1..=FORK_TIP {
        let from_index = store.index().canonical(height);
        let from_view = store.block_by_height(height).map(|b| b.hash);
        assert_eq!(
            from_index, from_view,
            "E2: explorer must read blocks from the indexer"
        );
    }
}

#[test]
fn e2s_03_block_lookup_by_height_and_hash_agree() {
    let (store, port) = world();
    let hash = blk(1, 40).hash;
    let (by_height_status, by_height) = http_get(port, "/block/40");
    let (by_hash_status, by_hash) = http_get(port, &format!("/search?q={}", hex32(&hash)));
    assert_eq!(
        by_height_status, 200,
        "E2: block lookup by height and hash must agree"
    );
    assert_eq!(
        by_hash_status, 200,
        "E2: block lookup by height and hash must agree"
    );
    assert!(
        by_height.contains(&hex32(&hash)),
        "E2: block lookup by height and hash must agree"
    );
    assert!(
        by_hash.contains("\"kind\":\"block\""),
        "E2: block lookup by height and hash must agree"
    );
    assert!(
        by_hash.contains("\"height\":40"),
        "E2: block lookup by height and hash must agree"
    );
    assert_eq!(
        store.block_by_height(40),
        store.block_by_hash(&hash),
        "E2: block lookup by height and hash must agree"
    );
}

#[test]
fn e2s_04_tx_status_follows_the_canonical_chain() {
    let (store, port) = world();
    let (status_a, body_a) = http_get(port, &format!("/tx/{}", hex32(&txh(1))));
    let (status_b, body_b) = http_get(port, &format!("/tx/{}", hex32(&txh(2))));
    let (status_c, body_c) = http_get(port, &format!("/tx/{}", hex32(&txh(3))));
    assert_eq!(
        status_a, 200,
        "E2: tx status must follow the canonical chain"
    );
    assert_eq!(
        status_b, 200,
        "E2: tx status must follow the canonical chain"
    );
    assert_eq!(
        status_c, 200,
        "E2: tx status must follow the canonical chain"
    );
    assert!(
        body_a.contains("\"status\":\"finalized\""),
        "E2: tx status must follow the canonical chain"
    );
    assert!(
        body_b.contains("\"status\":\"orphaned\""),
        "E2: tx status must follow the canonical chain"
    );
    assert!(
        body_c.contains("\"status\":\"confirmed\""),
        "E2: tx status must follow the canonical chain"
    );
    let unknown = TxRecord {
        hash: txh(9),
        block_hash: txh(8),
        block_height: 1,
        from: "x".to_string(),
        to: "y".to_string(),
        amount: 1,
        fee: 0,
    };
    assert_eq!(
        store.tx_status(&unknown),
        TxStatus::Unknown,
        "E2: tx status must follow the canonical chain"
    );
}

#[test]
fn e2s_05_account_history_matches_the_chain() {
    let (store, port) = world();
    let cases: [(&str, u64, usize); 6] = [
        ("alice", 899, 1),
        ("bob", 100, 1),
        ("charlie", 500, 0),
        ("dave", 0, 0),
        ("eve", 627, 1),
        ("frank", 70, 1),
    ];
    for (address, balance, history_len) in cases {
        let (status, body) = http_get(port, &format!("/account/{address}"));
        assert_eq!(status, 200, "E2: account history must match the chain");
        assert!(
            body.contains(&format!("\"balance\":{balance}")),
            "E2: account history must match the chain"
        );
        assert_eq!(
            store.account(address).history.len(),
            history_len,
            "E2: account history must match the chain"
        );
        assert!(
            body.contains(&format!("\"address\":\"{address}\"")),
            "E2: account history must match the chain"
        );
    }
}

#[test]
fn e2s_06_transaction_finality_is_unambiguous() {
    let (store, port) = world();
    let a = store.tx(&txh(1)).expect("txA indexed");
    let b = store.tx(&txh(2)).expect("txB indexed");
    let c = store.tx(&txh(3)).expect("txC indexed");
    assert_eq!(
        a.1,
        TxStatus::Finalized,
        "E2: transaction finality must be unambiguous"
    );
    assert_eq!(
        b.1,
        TxStatus::Orphaned,
        "E2: transaction finality must be unambiguous"
    );
    assert_eq!(
        c.1,
        TxStatus::Confirmed,
        "E2: transaction finality must be unambiguous"
    );
    for (tag, word) in [(1u8, "finalized"), (2, "orphaned"), (3, "confirmed")] {
        let (status, body) = http_get(port, &format!("/tx/{}", hex32(&txh(tag))));
        assert_eq!(status, 200, "E2: transaction finality must be unambiguous");
        assert_eq!(
            body.matches("\"status\":\"").count(),
            1,
            "E2: transaction finality must be unambiguous"
        );
        assert!(
            body.contains(&format!("\"status\":\"{word}\"")),
            "E2: transaction finality must be unambiguous"
        );
    }
}

#[test]
fn e2s_07_validator_view_shows_consensus_health() {
    let (store, port) = world();
    let (status_v, body_v) = http_get(port, "/validators");
    let (status_h, body_h) = http_get(port, "/health");
    assert_eq!(
        status_v, 200,
        "E2: validator view must show consensus health"
    );
    assert_eq!(
        status_h, 200,
        "E2: validator view must show consensus health"
    );
    assert!(
        body_v.contains("val-a"),
        "E2: validator view must show consensus health"
    );
    assert!(
        body_v.contains("val-b"),
        "E2: validator view must show consensus health"
    );
    assert!(
        body_v.contains("\"proposed\":20"),
        "E2: validator view must show consensus health"
    );
    assert!(
        body_v.contains("\"proposed\":22"),
        "E2: validator view must show consensus health"
    );
    let (tip, canonical, count, stake) = store.consensus_health();
    assert_eq!(
        tip, FORK_TIP,
        "E2: validator view must show consensus health"
    );
    assert_eq!(
        canonical, FORK_TIP as usize,
        "E2: validator view must show consensus health"
    );
    assert_eq!(count, 2, "E2: validator view must show consensus health");
    assert_eq!(stake, 250, "E2: validator view must show consensus health");
    assert!(
        body_h.contains("\"tip_height\":42"),
        "E2: validator view must show consensus health"
    );
    assert!(
        body_h.contains("\"canonical_count\":42"),
        "E2: validator view must show consensus health"
    );
}

#[test]
fn e2s_08_search_finds_blocks_txs_accounts_and_contracts() {
    let (store, port) = world();
    let (status_block, body_block) = http_get(port, "/search?q=40");
    let (status_tx, body_tx) = http_get(port, &format!("/search?q={}", hex32(&txh(1))));
    let (status_acc, body_acc) = http_get(port, "/search?q=alice");
    let (status_ct, body_ct) = http_get(port, "/search?q=ct-adder");
    for status in [status_block, status_tx, status_acc, status_ct] {
        assert_eq!(
            status, 200,
            "E2: search must find blocks, txs, accounts and contracts"
        );
    }
    assert!(
        body_block.contains("\"kind\":\"block\""),
        "E2: search must find blocks, txs, accounts and contracts"
    );
    assert!(
        body_tx.contains("\"kind\":\"tx\""),
        "E2: search must find blocks, txs, accounts and contracts"
    );
    assert!(
        body_acc.contains("\"kind\":\"account\""),
        "E2: search must find blocks, txs, accounts and contracts"
    );
    assert!(
        body_ct.contains("\"kind\":\"contract\""),
        "E2: search must find blocks, txs, accounts and contracts"
    );
    assert_eq!(
        store.search("ct-adder"),
        Some(SearchHit::Contract("ct-adder".to_string())),
        "E2: search must find blocks, txs, accounts and contracts"
    );
}

#[test]
fn e2s_09_contract_page_shows_the_verified_source() {
    let (store, port) = world();
    let (status, body) = http_get(port, "/contract/ct-adder");
    assert_eq!(
        status, 200,
        "E2: contract page must show the verified source"
    );
    assert!(
        body.contains("push 3\\npush 4\\nadd\\nreturn"),
        "E2: contract page must show the verified source"
    );
    assert!(
        body.contains(ADDER_CODE_HASH),
        "E2: contract page must show the verified source"
    );
    assert!(
        body.contains("\"verified\":true"),
        "E2: contract page must show the verified source"
    );
    assert!(
        store.contract_verified("ct-adder"),
        "E2: contract page must show the verified source"
    );
    assert!(
        store.contract_verified("ct-counter"),
        "E2: contract page must show the verified source"
    );
}

#[test]
fn e2s_10_web_ui_renders_the_same_data_as_the_api() {
    let (_store, port) = world();
    let (status_page, page) = http_get(port, "/");
    let (status_blocks, blocks) = http_get(port, "/blocks");
    let (status_contracts, contracts) = http_get(port, "/contracts");
    assert_eq!(
        status_page, 200,
        "E2: the web UI must render the same data as the API"
    );
    assert_eq!(
        status_blocks, 200,
        "E2: the web UI must render the same data as the API"
    );
    assert_eq!(
        status_contracts, 200,
        "E2: the web UI must render the same data as the API"
    );
    let latest_hash = hex32(&blk(1, FORK_TIP).hash);
    assert!(
        page.contains(&latest_hash),
        "E2: the web UI must render the same data as the API"
    );
    assert!(
        blocks.contains(&latest_hash),
        "E2: the web UI must render the same data as the API"
    );
    assert!(
        page.contains(ADDER_CODE_HASH),
        "E2: the web UI must render the same data as the API"
    );
    assert!(
        contracts.contains(ADDER_CODE_HASH),
        "E2: the web UI must render the same data as the API"
    );
}

#[test]
fn e2_property_contract_build_is_reproducible() {
    let (code_a, hash_a) = build_contract(ADDER_SRC).expect("adder builds");
    let (code_b, hash_b) = build_contract(ADDER_SRC).expect("adder rebuilds");
    assert_eq!(
        code_a, code_b,
        "E2: contract build must be reproducible (same code_hash)"
    );
    assert_eq!(
        hash_a, hash_b,
        "E2: contract build must be reproducible (same code_hash)"
    );
    let (_alias_code, alias_hash) = match build_contract(ALIAS_SRC) {
        Ok(built) => built,
        Err(_) => panic!("E2: contract build must be reproducible (same code_hash)"),
    };
    assert_eq!(
        alias_hash.to_hex(),
        ADDER_CODE_HASH,
        "E2: contract build must be reproducible (same code_hash)"
    );
    let (_counter_code, counter_hash) = build_contract(COUNTER_SRC).expect("counter builds");
    assert_eq!(
        counter_hash.to_hex(),
        COUNTER_CODE_HASH,
        "E2: contract build must be reproducible (same code_hash)"
    );
    assert_ne!(
        hash_a, counter_hash,
        "E2: contract build must be reproducible (same code_hash)"
    );
}

#[test]
fn e2_property_verified_source_matches_code_hash() {
    let (_, adder_hash) = build_contract(ADDER_SRC).expect("adder builds");
    assert!(
        verify_contract(ADDER_SRC, &adder_hash),
        "E2: verified source must match the deployed code_hash"
    );
    assert!(
        !verify_contract(COUNTER_SRC, &adder_hash),
        "E2: verified source must match the deployed code_hash"
    );
    assert!(
        !verify_contract("push 1\nthisisnotanop", &adder_hash),
        "E2: verified source must match the deployed code_hash"
    );
}
