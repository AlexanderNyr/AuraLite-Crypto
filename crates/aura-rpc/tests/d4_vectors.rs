//! 50 independently specified JSON-RPC 2.0 contract vectors.

use std::sync::{Arc, mpsc};

use aura_rpc::{RpcBackend, RpcError, RpcService, RpcSubscription};
use serde_json::{Value, json};

struct VectorBackend;

impl RpcBackend for VectorBackend {
    fn submit_tx(&self, _: &Value) -> Result<Value, RpcError> {
        Ok(json!("0x01"))
    }
    fn get_balance(&self, _: &Value) -> Result<Value, RpcError> {
        Ok(json!({"balance":"1","nonce":0}))
    }
    fn get_tx(&self, _: &Value) -> Result<Value, RpcError> {
        Ok(Value::Null)
    }
    fn get_block(&self, _: &Value) -> Result<Value, RpcError> {
        Ok(json!({"height":0}))
    }
    fn call(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32004, "unsupported"))
    }
    fn estimate_gas(&self, _: &Value) -> Result<Value, RpcError> {
        Ok(json!({"gasUnits":21000}))
    }
    fn subscribe(&self) -> Result<Box<dyn RpcSubscription>, RpcError> {
        let (_sender, receiver) = mpsc::channel();
        Ok(Box::new(receiver))
    }
}

#[test]
fn all_fifty_json_rpc_golden_vectors_match_the_protocol_contract() {
    let service = RpcService::new(Arc::new(VectorBackend));
    let mut name = String::new();
    let mut request = None;
    let mut expected = None;
    let mut count = 0;
    for line in include_str!("fixtures/d4_json_rpc.txt").lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("vector=") {
            if !name.is_empty() {
                run_vector(
                    &service,
                    &name,
                    request.take().unwrap(),
                    expected.take().unwrap(),
                );
                count += 1;
            }
            name = value.to_owned();
        } else if let Some(value) = line.strip_prefix("request_hex=") {
            request = Some(decode_hex(value));
        } else if let Some(value) = line.strip_prefix("expected=") {
            expected = Some(value.to_owned());
        } else {
            panic!("unexpected fixture line: {line}");
        }
    }
    if !name.is_empty() {
        run_vector(
            &service,
            &name,
            request.take().unwrap(),
            expected.take().unwrap(),
        );
        count += 1;
    }
    assert_eq!(count, 50, "D4 requires exactly 50 vectors");
}

fn run_vector(service: &RpcService, name: &str, request: Vec<u8>, expected: String) {
    let response = service.process_http_message(&request);
    match expected.as_str() {
        "none" => assert!(
            response.is_none(),
            "{name}: notification must not produce a body"
        ),
        "ok" => {
            let value: Value = serde_json::from_slice(
                &response.unwrap_or_else(|| panic!("{name}: response missing")),
            )
            .unwrap();
            assert!(value.get("error").is_none(), "{name}: {value}");
            assert!(value.get("result").is_some(), "{name}: {value}");
            assert_eq!(value["jsonrpc"], "2.0", "{name}");
        }
        "batch:1" | "batch:2" => {
            let expected_len: usize = expected.trim_start_matches("batch:").parse().unwrap();
            let value: Value = serde_json::from_slice(&response.unwrap()).unwrap();
            assert_eq!(
                value.as_array().unwrap().len(),
                expected_len,
                "{name}: {value}"
            );
        }
        error if error.starts_with("error:") => {
            let code: i64 = error.trim_start_matches("error:").parse().unwrap();
            let value: Value = serde_json::from_slice(
                &response.unwrap_or_else(|| panic!("{name}: error response missing")),
            )
            .unwrap();
            assert_eq!(value["error"]["code"], code, "{name}: {value}");
            assert_eq!(value["jsonrpc"], "2.0", "{name}");
        }
        other => panic!("unknown expected vector outcome {other}"),
    }
}

fn decode_hex(text: &str) -> Vec<u8> {
    let (pairs, remainder) = text.as_bytes().as_chunks::<2>();
    assert!(remainder.is_empty());
    pairs
        .iter()
        .map(|pair| {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => panic!("invalid fixture hex"),
            };
            (digit(pair[0]) << 4) | digit(pair[1])
        })
        .collect()
}
