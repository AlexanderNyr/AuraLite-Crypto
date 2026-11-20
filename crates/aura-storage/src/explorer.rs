//! E2 chain explorer over the E1 indexer (ECOSYSTEM_PLAN E2).
//!
//! Read-only views (blocks, transactions, accounts, validators,
//! contracts with verified source) served as JSON or as a single-page
//! web UI over plain HTTP. Every block fact is answered by the
//! [`ChainIndexer`] alone (decision D-E1: the explorer gets block data
//! only from the indexer). Contract builds are deterministic: the same
//! source always produces the same code bytes and the same `code_hash`.

use crate::indexer::ChainIndexer;
use aura_state::sha256_bytes;
use aura_types::Hash;
use std::collections::{BTreeMap, HashMap};

/// Depth in canonical blocks after which a transaction is final.
pub const FINALITY_DEPTH: u64 = 8;

/// Transaction as indexed by the explorer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TxRecord {
    /// Transaction hash.
    pub hash: [u8; 32],
    /// Hash of the block that included it.
    pub block_hash: [u8; 32],
    /// Height of the including block.
    pub block_height: u64,
    /// Sender address.
    pub from: String,
    /// Recipient address.
    pub to: String,
    /// Transferred amount.
    pub amount: u64,
    /// Fee paid by the sender.
    pub fee: u64,
}

/// Transaction status derived from the canonical chain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TxStatus {
    /// The including block is unknown to the indexer.
    Unknown,
    /// The including block is known but no longer canonical.
    Orphaned,
    /// The including block is canonical, fewer than [`FINALITY_DEPTH`]
    /// confirmations deep.
    Confirmed,
    /// At least [`FINALITY_DEPTH`] confirmations deep.
    Finalized,
}

/// Block summary shown by the explorer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BlockView {
    /// Block height.
    pub height: u64,
    /// Block hash.
    pub hash: [u8; 32],
    /// Parent block hash.
    pub parent: [u8; 32],
    /// Whether the block is on the canonical chain.
    pub canonical: bool,
    /// Number of indexed transactions in this block.
    pub tx_count: usize,
}

/// Account view: canonical balance and history.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AccountView {
    /// Address.
    pub address: String,
    /// Canonical balance (genesis plus canonical transfers).
    pub balance: u64,
    /// Canonical transaction hashes in chain order.
    pub history: Vec<[u8; 32]>,
}

/// Validator entry.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ValidatorView {
    /// Validator address.
    pub address: String,
    /// Staked amount.
    pub stake: u64,
    /// Blocks proposed (as tallied by the explorer feed).
    pub proposed: u64,
}

/// Deployed contract with verified source.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ContractRecord {
    /// Contract address.
    pub address: String,
    /// Contract source code.
    pub source: String,
    /// Built code bytes (ALVM-1).
    pub code: Vec<u8>,
    /// Code hash of the built code.
    pub code_hash: Hash,
}

/// Search result: one of the four object kinds.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SearchHit {
    /// A block.
    Block(BlockView),
    /// A transaction hash.
    Tx([u8; 32]),
    /// An account address.
    Account(String),
    /// A contract address.
    Contract(String),
}

fn canonicalize_line(raw: &str) -> String {
    let joined = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let lowered = joined.to_lowercase();
    if lowered.is_empty() || lowered.starts_with('#') {
        String::new()
    } else {
        lowered
    }
}

fn emit_op(line: &str, out: &mut Vec<u8>) -> Result<(), String> {
    let parts: Vec<&str> = line.split(' ').collect();
    match parts[0] {
        "push" => {
            let value: u64 = parts
                .get(1)
                .ok_or_else(|| "push needs an argument".to_string())?
                .parse()
                .map_err(|_| "push argument must be a number".to_string())?;
            out.push(0x01);
            out.extend_from_slice(&value.to_le_bytes());
        }
        "add" => out.push(0x02),
        "sub" => out.push(0x03),
        "mul" => out.push(0x04),
        "dup" => out.push(0x05),
        "drop" => out.push(0x06),
        "return" => out.push(0x07),
        other => return Err(format!("unknown opcode: {other}")),
    }
    Ok(())
}

/// Build contract `source` into deterministic ALVM-1 bytecode.
///
/// Source lines are canonicalized first (whitespace collapsed, case
/// folded, comments dropped), so formatting-equivalent sources build to
/// identical code bytes and the same code hash.
pub fn build_contract(source: &str) -> Result<(Vec<u8>, Hash), String> {
    let mut code = Vec::new();
    for raw in source.split('\n') {
        let line = canonicalize_line(raw);
        if line.is_empty() {
            continue;
        }
        emit_op(&line, &mut code)?;
    }
    let hash = sha256_bytes(&code);
    Ok((code, hash))
}

/// Verify that `source` builds to the deployed `expected` code hash.
pub fn verify_contract(source: &str, expected: &Hash) -> bool {
    match build_contract(source) {
        Ok((_, hash)) => hash == *expected,
        Err(_) => false,
    }
}

/// Read-only explorer state. Block facts come only from the indexer.
#[derive(Debug, Default)]
pub struct ExplorerStore {
    index: ChainIndexer,
    txs: HashMap<[u8; 32], TxRecord>,
    genesis: BTreeMap<String, u64>,
    validators: Vec<ValidatorView>,
    contracts: BTreeMap<String, ContractRecord>,
}

impl ExplorerStore {
    /// Fresh explorer with an empty indexer.
    pub fn new() -> Self {
        Self::default()
    }

    /// The indexer this explorer reads block facts from.
    pub fn index(&self) -> &ChainIndexer {
        &self.index
    }

    /// Mutable indexer access (feeding blocks).
    pub fn index_mut(&mut self) -> &mut ChainIndexer {
        &mut self.index
    }

    /// Index one transaction.
    pub fn add_tx(&mut self, tx: TxRecord) {
        self.txs.insert(tx.hash, tx);
    }

    /// Set a genesis balance.
    pub fn set_genesis(&mut self, address: &str, balance: u64) {
        self.genesis.insert(address.to_string(), balance);
    }

    /// Register a validator.
    pub fn add_validator(&mut self, address: &str, stake: u64) {
        self.validators.push(ValidatorView {
            address: address.to_string(),
            stake,
            proposed: 0,
        });
    }

    /// Tally one proposed block for a validator.
    pub fn note_proposal(&mut self, address: &str) {
        if let Some(v) = self.validators.iter_mut().find(|v| v.address == address) {
            v.proposed += 1;
        }
    }

    /// Deploy a contract from `source`; the explorer stores the built
    /// code and its hash as the deployed code hash.
    pub fn deploy_contract(&mut self, address: &str, source: &str) -> Result<(), String> {
        let (code, code_hash) = build_contract(source)?;
        self.contracts.insert(
            address.to_string(),
            ContractRecord {
                address: address.to_string(),
                source: source.to_string(),
                code,
                code_hash,
            },
        );
        Ok(())
    }

    /// Latest canonical blocks, newest first.
    pub fn latest_blocks(&self, n: usize) -> Vec<BlockView> {
        let tip = self.index.tip_height();
        let start = tip.saturating_sub(n as u64 - 1);
        (start..=tip)
            .rev()
            .filter_map(|height| self.block_by_height(height))
            .collect()
    }

    /// Canonical block view by height (None when not canonical).
    pub fn block_by_height(&self, height: u64) -> Option<BlockView> {
        let hash = self.index.canonical(height)?;
        self.block_by_hash(&hash)
    }

    /// Block view by hash (canonical flag preserved).
    pub fn block_by_hash(&self, hash: &[u8; 32]) -> Option<BlockView> {
        let block = self.index.block(hash)?;
        Some(BlockView {
            height: block.height,
            hash: block.hash,
            parent: block.parent,
            canonical: self.index.is_canonical(hash),
            tx_count: self
                .txs
                .values()
                .filter(|tx| tx.block_hash == *hash)
                .count(),
        })
    }

    /// Transaction record with chain-derived status.
    pub fn tx(&self, hash: &[u8; 32]) -> Option<(TxRecord, TxStatus)> {
        let record = self.txs.get(hash)?.clone();
        let status = self.tx_status(&record);
        Some((record, status))
    }

    /// Status of one transaction, derived from the canonical chain.
    pub fn tx_status(&self, tx: &TxRecord) -> TxStatus {
        if self.index.is_canonical(&tx.block_hash) {
            let confirmations = self.index.tip_height().saturating_sub(tx.block_height) + 1;
            if confirmations >= FINALITY_DEPTH {
                TxStatus::Finalized
            } else {
                TxStatus::Confirmed
            }
        } else if self.index.knows(&tx.block_hash) {
            TxStatus::Orphaned
        } else {
            TxStatus::Unknown
        }
    }

    fn tx_is_canonical(&self, tx: &TxRecord) -> bool {
        self.index.is_canonical(&tx.block_hash)
    }

    /// Account balance and canonical history.
    pub fn account(&self, address: &str) -> AccountView {
        let mut balance = self.genesis.get(address).copied().unwrap_or(0);
        let mut history = Vec::new();
        let mut touching: Vec<&TxRecord> = self
            .txs
            .values()
            .filter(|tx| tx.from == address || tx.to == address)
            .collect();
        touching.sort_by_key(|tx| (tx.block_height, tx.hash));
        for tx in touching {
            if !self.tx_is_canonical(tx) {
                continue;
            }
            if tx.from == address {
                balance = balance.saturating_sub(tx.amount + tx.fee);
            }
            if tx.to == address {
                balance += tx.amount;
            }
            history.push(tx.hash);
        }
        AccountView {
            address: address.to_string(),
            balance,
            history,
        }
    }

    /// Registered validators.
    pub fn validators(&self) -> &[ValidatorView] {
        &self.validators
    }

    /// Consensus health summary (height and canonical length both come
    /// from the indexer).
    pub fn consensus_health(&self) -> (u64, usize, usize, u64) {
        let stake: u64 = self.validators.iter().map(|v| v.stake).sum();
        (
            self.index.tip_height(),
            self.index.canonical_count(),
            self.validators.len(),
            stake,
        )
    }

    /// Contract record with verified source.
    pub fn contract(&self, address: &str) -> Option<&ContractRecord> {
        self.contracts.get(address)
    }

    /// Whether the stored source verifies against the deployed code hash.
    pub fn contract_verified(&self, address: &str) -> bool {
        self.contracts
            .get(address)
            .map(|c| verify_contract(&c.source, &c.code_hash))
            .unwrap_or(false)
    }

    /// Search blocks, transactions, accounts and contracts.
    pub fn search(&self, query: &str) -> Option<SearchHit> {
        if let Ok(height) = query.parse::<u64>()
            && let Some(block) = self.block_by_height(height)
        {
            return Some(SearchHit::Block(block));
        }
        if let Some(hash) = parse_hex32(query) {
            if self.txs.contains_key(&hash) {
                return Some(SearchHit::Tx(hash));
            }
            if self.index.knows(&hash)
                && let Some(block) = self.block_by_hash(&hash)
            {
                return Some(SearchHit::Block(block));
            }
        }
        if self.genesis.contains_key(query)
            || self
                .txs
                .values()
                .any(|tx| tx.from == query || tx.to == query)
        {
            return Some(SearchHit::Account(query.to_string()));
        }
        if self.contracts.contains_key(query) {
            return Some(SearchHit::Contract(query.to_string()));
        }
        None
    }

    /// Handle one HTTP request; returns (status, content type, body).
    pub fn handle(&self, request: &str) -> (u16, &'static str, String) {
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let path = path.split('?').next().unwrap_or("/");
        let query = request
            .lines()
            .next()
            .and_then(|line| line.split_once('?').map(|(_, q)| q))
            .and_then(|q| q.split_whitespace().next())
            .unwrap_or("");
        match path {
            "/" => (200, "text/html", self.render_page()),
            "/health" => (200, "application/json", self.json_health()),
            "/blocks" => (200, "application/json", self.json_blocks()),
            p if p.starts_with("/block/") => match p["/block/".len()..].parse::<u64>() {
                Ok(height) => match self.block_by_height(height) {
                    Some(block) => (200, "application/json", json_block(&block)),
                    None => (404, "application/json", "{}".to_string()),
                },
                Err(_) => (400, "application/json", "{}".to_string()),
            },
            p if p.starts_with("/tx/") => match parse_hex32(&p["/tx/".len()..]) {
                Some(hash) => match self.tx(&hash) {
                    Some((tx, status)) => (200, "application/json", json_tx(&tx, status)),
                    None => (404, "application/json", "{}".to_string()),
                },
                None => (400, "application/json", "{}".to_string()),
            },
            p if p.starts_with("/account/") => {
                let address = &p["/account/".len()..];
                (
                    200,
                    "application/json",
                    json_account(&self.account(address)),
                )
            }
            "/validators" => (200, "application/json", self.json_validators()),
            "/contracts" => (200, "application/json", self.json_contracts()),
            p if p.starts_with("/contract/") => {
                let address = &p["/contract/".len()..];
                match self.contract(address) {
                    Some(record) => (
                        200,
                        "application/json",
                        json_contract(record, self.contract_verified(address)),
                    ),
                    None => (404, "application/json", "{}".to_string()),
                }
            }
            "/search" => {
                let q = query.strip_prefix("q=").unwrap_or(query);
                match self.search(q) {
                    Some(hit) => (200, "application/json", json_search(&hit)),
                    None => (404, "application/json", "{}".to_string()),
                }
            }
            _ => (404, "text/plain", "not found".to_string()),
        }
    }

    fn json_health(&self) -> String {
        let (tip, canonical, validators, stake) = self.consensus_health();
        format!(
            "{{\"tip_height\":{tip},\"canonical_count\":{canonical},\"validators\":{validators},\"total_stake\":{stake}}}"
        )
    }

    fn json_blocks(&self) -> String {
        let items: Vec<String> = self.latest_blocks(10).iter().map(json_block).collect();
        format!("[{}]", items.join(","))
    }

    fn json_validators(&self) -> String {
        let items: Vec<String> = self
            .validators
            .iter()
            .map(|v| {
                format!(
                    "{{\"address\":\"{}\",\"stake\":{},\"proposed\":{}}}",
                    v.address, v.stake, v.proposed
                )
            })
            .collect();
        format!("[{}]", items.join(","))
    }

    fn json_contracts(&self) -> String {
        let items: Vec<String> = self
            .contracts
            .values()
            .map(|c| json_contract(c, self.contract_verified(&c.address)))
            .collect();
        format!("[{}]", items.join(","))
    }

    /// Single-page web UI: all explorer sections with live data.
    pub fn render_page(&self) -> String {
        let blocks: Vec<String> = self
            .latest_blocks(5)
            .iter()
            .map(|b| format!("<tr><td>{}</td><td>{}</td></tr>", b.height, hex32(&b.hash)))
            .collect();
        let validators: Vec<String> = self
            .validators
            .iter()
            .map(|v| format!("<li>{} stake={}</li>", v.address, v.stake))
            .collect();
        let contracts: Vec<String> = self
            .contracts
            .values()
            .map(|c| {
                format!(
                    "<article><h3>{}</h3><pre>{}</pre><p>code_hash={} verified={}</p></article>",
                    c.address,
                    html_escape(&c.source),
                    c.code_hash.to_hex(),
                    self.contract_verified(&c.address)
                )
            })
            .collect();
        let accounts: Vec<String> = ["alice", "bob", "charlie", "eve"]
            .iter()
            .map(|a| {
                let view = self.account(a);
                format!(
                    "<li>{} balance={} txs={}</li>",
                    a,
                    view.balance,
                    view.history.len()
                )
            })
            .collect();
        format!(
            "<!doctype html><html><head><title>AuraLite Explorer</title></head><body>\
<h1>AuraLite Explorer</h1>\
<section id=\"blocks\"><h2>Blocks</h2><table>{}</table></section>\
<section id=\"tx\"><h2>Transactions</h2><p>Use /tx/&lt;hash&gt; or the search box.</p></section>\
<section id=\"accounts\"><h2>Accounts</h2><ul>{}</ul></section>\
<section id=\"validators\"><h2>Validators</h2><ul>{}</ul></section>\
<section id=\"contracts\"><h2>Contracts</h2>{}</section>\
<section id=\"search\"><h2>Search</h2><form action=\"/search\"><input name=\"q\"/></form></section>\
</body></html>",
            blocks.join(""),
            accounts.join(""),
            validators.join(""),
            contracts.join("")
        )
    }
}

/// Serve the explorer over HTTP on `listener` (one thread per request).
pub fn serve(store: std::sync::Arc<ExplorerStore>, listener: std::net::TcpListener) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let store = std::sync::Arc::clone(&store);
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let mut req = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        req.extend_from_slice(&chunk[..n]);
                        if req.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => return,
                }
            }
            let request = String::from_utf8_lossy(&req).into_owned();
            let (status, ctype, body) = store.handle(&request);
            let reason = if status == 200 { "OK" } else { "ERR" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });
    }
}

fn hex32(hash: &[u8; 32]) -> String {
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

fn parse_hex32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn json_hash(hash: &[u8; 32]) -> String {
    format!("\"{}\"", hex32(hash))
}

fn json_block(block: &BlockView) -> String {
    format!(
        "{{\"height\":{},\"hash\":{},\"parent\":{},\"canonical\":{},\"tx_count\":{}}}",
        block.height,
        json_hash(&block.hash),
        json_hash(&block.parent),
        block.canonical,
        block.tx_count
    )
}

fn json_tx(tx: &TxRecord, status: TxStatus) -> String {
    let status = match status {
        TxStatus::Unknown => "unknown",
        TxStatus::Orphaned => "orphaned",
        TxStatus::Confirmed => "confirmed",
        TxStatus::Finalized => "finalized",
    };
    format!(
        "{{\"hash\":{},\"block_height\":{},\"from\":\"{}\",\"to\":\"{}\",\"amount\":{},\"fee\":{},\"status\":\"{}\"}}",
        json_hash(&tx.hash),
        tx.block_height,
        tx.from,
        tx.to,
        tx.amount,
        tx.fee,
        status
    )
}

fn json_account(view: &AccountView) -> String {
    let history: Vec<String> = view.history.iter().map(json_hash).collect();
    format!(
        "{{\"address\":\"{}\",\"balance\":{},\"history\":[{}]}}",
        view.address,
        view.balance,
        history.join(",")
    )
}

fn json_contract(record: &ContractRecord, verified: bool) -> String {
    format!(
        "{{\"address\":\"{}\",\"code_hash\":\"{}\",\"verified\":{},\"source\":\"{}\"}}",
        record.address,
        record.code_hash.to_hex(),
        verified,
        record.source.replace('\n', "\\n")
    )
}

fn json_search(hit: &SearchHit) -> String {
    match hit {
        SearchHit::Block(block) => {
            format!("{{\"kind\":\"block\",\"data\":{}}}", json_block(block))
        }
        SearchHit::Tx(hash) => format!("{{\"kind\":\"tx\",\"hash\":{}}}", json_hash(hash)),
        SearchHit::Account(address) => {
            format!("{{\"kind\":\"account\",\"address\":\"{address}\"}}")
        }
        SearchHit::Contract(address) => {
            format!("{{\"kind\":\"contract\",\"address\":\"{address}\"}}")
        }
    }
}
