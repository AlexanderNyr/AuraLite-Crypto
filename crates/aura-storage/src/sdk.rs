//! E3 client SDK (Rust) for the AuraLite ecosystem HTTP APIs
//! (ECOSYSTEM_PLAN E3: Rust/TS/Python SDK).
//!
//! Zero-dependency HTTP client for the explorer (E2) and the faucet
//! (E3): [`AuraLiteClient`] talks JSON over plain HTTP/1.1, the same
//! protocol as `sdk/ts/` and `sdk/python/`. Examples live in
//! `crates/aura-storage/examples/e3_sdk_example.rs`.

use std::io::{Read, Write};
use std::net::TcpStream;

/// Client for one explorer+faucet endpoint pair.
#[derive(Debug, Clone)]
pub struct AuraLiteClient {
    base: String,
}

impl AuraLiteClient {
    /// Client for `base` like `http://127.0.0.1:8080`.
    pub fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
        }
    }

    fn host_port(&self) -> Result<(String, u16), String> {
        let rest = self
            .base
            .strip_prefix("http://")
            .ok_or_else(|| "only http:// bases are supported".to_string())?;
        let (host, port) = rest
            .split_once(':')
            .ok_or_else(|| "base must include a port".to_string())?;
        let port: u16 = port.parse().map_err(|_| "bad port".to_string())?;
        Ok((host.to_string(), port))
    }

    fn exchange(&self, request: &str) -> Result<(u16, String), String> {
        let (host, port) = self.host_port()?;
        let mut stream = TcpStream::connect((host.as_str(), port)).map_err(|e| e.to_string())?;
        stream
            .write_all(request.as_bytes())
            .map_err(|e| e.to_string())?;
        let mut buf = String::new();
        stream.read_to_string(&mut buf).map_err(|e| e.to_string())?;
        let status = buf
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let body = buf.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        Ok((status, body))
    }

    /// GET a path; returns (status, body).
    pub fn get(&self, path: &str) -> Result<(u16, String), String> {
        let request = format!("GET {path} HTTP/1.1\r\nHost: auralite\r\nConnection: close\r\n\r\n");
        self.exchange(&request)
    }

    /// POST JSON to a path; returns (status, body).
    pub fn post(&self, path: &str, body: &str) -> Result<(u16, String), String> {
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: auralite\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        self.exchange(&request)
    }

    /// Request faucet funds at time `now` (explicit time keeps CI
    /// deterministic; the demo protocol carries it in the body).
    pub fn faucet_request(
        &self,
        now: u64,
        network: &str,
        address: &str,
    ) -> Result<(u16, String), String> {
        let body = format!("{{\"now\":{now},\"network\":\"{network}\",\"address\":\"{address}\"}}");
        self.post("/faucet", &body)
    }

    /// Faucet status JSON.
    pub fn faucet_status(&self) -> Result<(u16, String), String> {
        self.get("/faucet/status")
    }

    /// Explorer block-by-height JSON.
    pub fn block(&self, height: u64) -> Result<(u16, String), String> {
        self.get(&format!("/block/{height}"))
    }

    /// Explorer account JSON.
    pub fn account(&self, address: &str) -> Result<(u16, String), String> {
        self.get(&format!("/account/{address}"))
    }

    /// Explorer health JSON.
    pub fn health(&self) -> Result<(u16, String), String> {
        self.get("/health")
    }
}
