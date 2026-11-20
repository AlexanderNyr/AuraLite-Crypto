//! E3 rate-limited faucet (ECOSYSTEM_PLAN E3).
//!
//! Testnet-only faucet with the phase gate policy: **1 payout per
//! address per day**. The service is a deterministic state machine
//! (`request` takes explicit timestamps), served over plain HTTP for
//! the SDK examples. Payouts are ledger entries with funds deducted
//! (deviation recorded in the E3 receipt: no on-chain transfer while
//! W1-FORMAT-v1 is frozen).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// Network the faucet is willing to serve.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Network {
    /// The only network the faucet serves.
    Testnet,
    /// Anything else: refused.
    Mainnet,
}

/// Result of one faucet request.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FaucetOutcome {
    /// One payout issued for (address, day).
    Paid {
        /// Day index (unix day of the request time).
        day: u64,
        /// Amount paid out.
        payout: u64,
    },
    /// This address was already paid on this day.
    DeniedRateLimit,
    /// The request named a network other than testnet.
    DeniedWrongNetwork,
    /// The faucet ran out of funds.
    DeniedOutOfFunds,
    /// The address is empty or oversized.
    DeniedBadAddress,
}

/// Faucet parameters.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FaucetConfig {
    /// Amount paid per successful request.
    pub payout: u64,
    /// Initial funding in the ledger.
    pub funding: u64,
}

/// Deterministic rate-limited faucet: 1 payout per address per day.
#[derive(Debug)]
pub struct Faucet {
    network: Network,
    config: FaucetConfig,
    funds: u64,
    paid: HashSet<(String, u64)>,
    payouts: u64,
}

/// Longest accepted address.
pub const MAX_ADDRESS_LEN: usize = 64;

fn day_of(now: u64) -> u64 {
    now / 86_400
}

impl Faucet {
    /// Faucet serving `network` with `config`.
    pub fn new(network: Network, config: FaucetConfig) -> Self {
        Self {
            network,
            config,
            funds: config.funding,
            paid: HashSet::new(),
            payouts: 0,
        }
    }

    /// Unix day index of `now`.
    pub fn day(now: u64) -> u64 {
        day_of(now)
    }

    /// Issue one request at time `now`. At most one payout per
    /// address per day; testnet only; stops at zero funds.
    pub fn request(&mut self, now: u64, network: Network, address: &str) -> FaucetOutcome {
        if address.is_empty() || address.len() > MAX_ADDRESS_LEN {
            return FaucetOutcome::DeniedBadAddress;
        }
        if network != self.network {
            return FaucetOutcome::DeniedWrongNetwork;
        }
        let day = now / 86_400;
        let key = (address.to_string(), day);
        if self.paid.contains(&key) {
            return FaucetOutcome::DeniedRateLimit;
        }
        if self.funds < self.config.payout {
            return FaucetOutcome::DeniedOutOfFunds;
        }
        self.funds = self.funds.saturating_sub(self.config.payout);
        self.paid.insert(key);
        self.payouts += 1;
        FaucetOutcome::Paid {
            day,
            payout: self.config.payout,
        }
    }

    /// Remaining funds.
    pub fn funds(&self) -> u64 {
        self.funds
    }

    /// Total payouts issued.
    pub fn payouts(&self) -> u64 {
        self.payouts
    }

    /// Handle one HTTP request; returns (status, content type, body).
    ///
    /// Routes: `POST /faucet` with JSON
    /// `{"now":<u64>,"network":"testnet"|"mainnet","address":"..."}`,
    /// `GET /faucet/status`.
    pub fn handle(&mut self, request: &str) -> (u16, &'static str, String) {
        let line = request.lines().next().unwrap_or("");
        let method = line.split_whitespace().next().unwrap_or("");
        let path = line.split_whitespace().nth(1).unwrap_or("/");
        let path = path.split('?').next().unwrap_or("/");
        match (method, path) {
            ("GET", "/faucet/status") => (
                200,
                "application/json",
                format!(
                    "{{\"funds\":{},\"payouts\":{},\"network\":\"{}\"}}",
                    self.funds,
                    self.payouts,
                    if self.network == Network::Testnet {
                        "testnet"
                    } else {
                        "mainnet"
                    }
                ),
            ),
            ("POST", "/faucet") => {
                let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
                let now: u64 = json_field(body, "now")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let network = match json_field(body, "network").as_deref() {
                    Some("mainnet") => Network::Mainnet,
                    _ => Network::Testnet,
                };
                let address = json_field(body, "address").unwrap_or_default();
                let outcome = self.request(now, network, &address);
                (200, "application/json", json_outcome(&outcome))
            }
            _ => (404, "text/plain", "not found".to_string()),
        }
    }
}

/// Parse `"name":value` or `"name":"value"` from a flat JSON body.
fn json_field(body: &str, name: &str) -> Option<String> {
    let pattern = format!("\"{name}\":");
    let start = body.find(&pattern)? + pattern.len();
    let rest = &body[start..];
    let rest = rest.trim_start();
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        Some(stripped[..end].to_string())
    } else {
        let end = rest
            .find(|c: char| c == ',' || c == '}' || c.is_whitespace())
            .unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

fn json_outcome(outcome: &FaucetOutcome) -> String {
    match outcome {
        FaucetOutcome::Paid { day, payout } => {
            format!("{{\"outcome\":\"paid\",\"day\":{day},\"payout\":{payout}}}")
        }
        FaucetOutcome::DeniedRateLimit => "{\"outcome\":\"denied_rate_limit\"}".to_string(),
        FaucetOutcome::DeniedWrongNetwork => "{\"outcome\":\"denied_wrong_network\"}".to_string(),
        FaucetOutcome::DeniedOutOfFunds => "{\"outcome\":\"denied_out_of_funds\"}".to_string(),
        FaucetOutcome::DeniedBadAddress => "{\"outcome\":\"denied_bad_address\"}".to_string(),
    }
}

/// Serve the faucet over HTTP (one thread per request; shared state).
pub fn serve(faucet: Arc<Mutex<Faucet>>, listener: std::net::TcpListener) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let faucet = Arc::clone(&faucet);
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let mut req = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        req.extend_from_slice(&chunk[..n]);
                        if request_complete(&req) {
                            break;
                        }
                    }
                    Err(_) => return,
                }
            }
            let request = String::from_utf8_lossy(&req).into_owned();
            let (status, ctype, body) = match faucet.lock() {
                Ok(mut guard) => guard.handle(&request),
                Err(_) => (500, "text/plain", "lock poisoned".to_string()),
            };
            let reason = if status == 200 { "OK" } else { "ERR" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });
    }
}

fn request_complete(req: &[u8]) -> bool {
    let Some(header_end) = req.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let head = String::from_utf8_lossy(&req[..header_end]);
    let content_length = head
        .lines()
        .find_map(|line| {
            let lower = line.to_ascii_lowercase();
            lower
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
        })
        .unwrap_or(0);
    req.len() >= header_end + 4 + content_length
}
