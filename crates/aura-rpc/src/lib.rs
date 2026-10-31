//! JSON-RPC 2.0 transport and protocol implementation for AuraLite-Crypto.
//!
//! The crate deliberately separates JSON-RPC parsing from node state. A
//! [`RpcBackend`] is called only after a complete bounded request has been
//! parsed; HTTP response writing happens after backend methods return, so a
//! backend lock is never held while bytes are written to a client.
//!
//! HTTP accepts JSON-RPC 2.0 POST requests at `/` and `/rpc`. WebSocket at
//! `/ws` supports the same request methods plus the `subscribe` method, which
//! streams durable commit notifications. Batch requests and notifications
//! follow JSON-RPC 2.0 semantics.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod server;

use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use serde_json::{Map, Number, Value, json};

pub use server::{MAX_HTTP_BODY, MAX_WS_MESSAGE, RpcServerHandle};

/// Optional HTTP route extension used by versioned gateways.
///
/// The D4 JSON-RPC endpoints remain the default. Extensions receive already
/// bounded, parsed HTTP requests and may claim only routes they own.
pub trait HttpExtension: Send + Sync + 'static {
    /// Handle an HTTP request, returning `None` when the route is not owned.
    fn handle(
        &self,
        method: &str,
        path: &str,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Option<HttpExtensionResponse>;
}

/// A bounded response produced by an [`HttpExtension`].
pub struct HttpExtensionResponse {
    /// HTTP status code; unsupported codes are serialized as 500.
    pub status: u16,
    /// MIME type for the response body.
    pub content_type: String,
    /// Response bytes.
    pub body: Vec<u8>,
}

impl HttpExtensionResponse {
    /// Construct an extension response.
    pub fn new(status: u16, content_type: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type: content_type.into(),
            body,
        }
    }
}

/// Error returned by a node-backed RPC operation.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcError {
    /// JSON-RPC error code.
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
    /// Optional structured error data.
    pub data: Option<Value>,
}

impl RpcError {
    /// Construct an RPC error without additional data.
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Attach structured error data.
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// Result of polling a WebSocket commit subscription.
#[derive(Debug)]
pub enum SubscriptionPoll {
    /// One owned commit event is ready.
    Event(Value),
    /// No event arrived before the requested timeout.
    Timeout,
    /// The source has been closed.
    Closed,
}

/// Pull-based subscription adapter; polling is performed outside node-state locks.
pub trait RpcSubscription: Send + 'static {
    /// Poll for a single event.
    fn poll(&mut self, timeout: Duration) -> SubscriptionPoll;
}

impl RpcSubscription for Receiver<Value> {
    fn poll(&mut self, timeout: Duration) -> SubscriptionPoll {
        match self.recv_timeout(timeout) {
            Ok(value) => SubscriptionPoll::Event(value),
            Err(RecvTimeoutError::Timeout) => SubscriptionPoll::Timeout,
            Err(RecvTimeoutError::Disconnected) => SubscriptionPoll::Closed,
        }
    }
}

/// Node operations exposed over JSON-RPC.
///
/// Implementations should hold internal state locks only while accessing or
/// mutating state, then return owned JSON values. The RPC server serializes and
/// writes those values after the method has returned.
pub trait RpcBackend: Send + Sync + 'static {
    /// Submit canonical signed transaction bytes encoded as `0x` hex.
    fn submit_tx(&self, params: &Value) -> Result<Value, RpcError>;
    /// Read a committed account balance and nonce by bech32 address.
    fn get_balance(&self, params: &Value) -> Result<Value, RpcError>;
    /// Read pending or committed transaction status by transaction hash.
    fn get_tx(&self, params: &Value) -> Result<Value, RpcError>;
    /// Read a committed block by height, or the latest committed block when omitted/null.
    fn get_block(&self, params: &Value) -> Result<Value, RpcError>;
    /// Execute a read-only contract call; unsupported execution must fail explicitly.
    fn call(&self, params: &Value) -> Result<Value, RpcError>;
    /// Estimate gas for a supported transaction request.
    fn estimate_gas(&self, params: &Value) -> Result<Value, RpcError>;
    /// Subscribe to durable commit events for one WebSocket client.
    fn subscribe(&self) -> Result<Box<dyn RpcSubscription>, RpcError>;
}

/// Parsed JSON-RPC request processor.
#[derive(Clone)]
pub struct RpcService {
    backend: Arc<dyn RpcBackend>,
}

impl RpcService {
    /// Create a service around a node RPC backend.
    pub fn new(backend: Arc<dyn RpcBackend>) -> Self {
        Self { backend }
    }

    /// Process one UTF-8 JSON-RPC message for HTTP.
    ///
    /// Returns `None` when a valid message contains only notifications, as
    /// required by JSON-RPC 2.0. Malformed JSON produces `-32700` with `id:null`.
    pub fn process_http_message(&self, message: &[u8]) -> Option<Vec<u8>> {
        let response = match serde_json::from_slice::<Value>(message) {
            Ok(value) => self.process_value(value, false),
            Err(_) => Some(error_response(
                Value::Null,
                RpcError::new(-32700, "Parse error"),
            )),
        };
        response.map(|value| serde_json::to_vec(&value).unwrap_or_else(|_| b"null".to_vec()))
    }

    /// Process a WebSocket JSON-RPC message. `subscribe` is intentionally
    /// reserved for the WebSocket transport and is handled by the server.
    pub fn process_websocket_message(&self, message: &[u8]) -> Option<Vec<u8>> {
        let parsed = match serde_json::from_slice::<Value>(message) {
            Ok(value) => Some(value),
            Err(_) => {
                return Some(
                    serde_json::to_vec(&error_response(
                        Value::Null,
                        RpcError::new(-32700, "Parse error"),
                    ))
                    .unwrap_or_default(),
                );
            }
        }?;
        self.process_value(parsed, true)
            .map(|value| serde_json::to_vec(&value).unwrap_or_else(|_| b"null".to_vec()))
    }

    pub(crate) fn backend(&self) -> &Arc<dyn RpcBackend> {
        &self.backend
    }

    fn process_value(&self, value: Value, websocket: bool) -> Option<Value> {
        if let Value::Array(requests) = value {
            if requests.is_empty() || requests.len() > 32 {
                return Some(error_response(
                    Value::Null,
                    RpcError::new(-32600, "Invalid Request"),
                ));
            }
            let responses = requests
                .into_iter()
                .filter_map(|request| self.process_single(request, websocket))
                .collect::<Vec<_>>();
            return (!responses.is_empty()).then_some(Value::Array(responses));
        }
        self.process_single(value, websocket)
    }

    fn process_single(&self, request: Value, websocket: bool) -> Option<Value> {
        let Value::Object(mut object) = request else {
            return Some(error_response(
                Value::Null,
                RpcError::new(-32600, "Invalid Request"),
            ));
        };
        let id_present = object.contains_key("id");
        let id = object.remove("id").unwrap_or(Value::Null);
        let valid_id = matches!(id, Value::Null | Value::String(_))
            || id.as_i64().is_some()
            || id.as_u64().is_some();
        let method = object.remove("method");
        let version = object.remove("jsonrpc");
        let params = object.remove("params").unwrap_or(Value::Null);
        let invalid = !object.is_empty()
            || version != Some(Value::String("2.0".to_owned()))
            || !valid_id
            || !matches!(method, Some(Value::String(_)))
            || !matches!(params, Value::Null | Value::Array(_) | Value::Object(_));
        if invalid {
            return Some(error_response(
                Value::Null,
                RpcError::new(-32600, "Invalid Request"),
            ));
        }
        let Value::String(method) = method.expect("validated method") else {
            unreachable!()
        };
        if !id_present {
            // A valid notification is executed but never answered, including
            // when the method returns an application-level error.
            let _ = self.dispatch(&method, &params, websocket);
            return None;
        }
        let result = self.dispatch(&method, &params, websocket);
        Some(match result {
            Ok(value) => success_response(id, value),
            Err(error) => error_response(id, error),
        })
    }

    fn dispatch(&self, method: &str, params: &Value, websocket: bool) -> Result<Value, RpcError> {
        match method {
            "submit_tx" => self.backend.submit_tx(params),
            "get_balance" => self.backend.get_balance(params),
            "get_tx" => self.backend.get_tx(params),
            "get_block" => self.backend.get_block(params),
            "call" => self.backend.call(params),
            "estimate_gas" => self.backend.estimate_gas(params),
            "subscribe" if websocket => Err(RpcError::new(
                -32005,
                "subscribe is handled by the WebSocket transport",
            )),
            "subscribe" => Err(RpcError::new(
                -32005,
                "subscribe requires WebSocket transport",
            )),
            _ => Err(RpcError::new(-32601, "Method not found")),
        }
    }
}

fn success_response(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0", "result":result, "id":id})
}

fn error_response(id: Value, error: RpcError) -> Value {
    let mut body = Map::new();
    body.insert("code".to_owned(), Value::Number(Number::from(error.code)));
    body.insert("message".to_owned(), Value::String(error.message));
    if let Some(data) = error.data {
        body.insert("data".to_owned(), data);
    }
    json!({"jsonrpc":"2.0", "error":Value::Object(body), "id":id})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    struct Mock;

    impl RpcBackend for Mock {
        fn submit_tx(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(json!("0xabc"))
        }
        fn get_balance(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(json!({"balance":"10","nonce":1}))
        }
        fn get_tx(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(json!({"status":"pending"}))
        }
        fn get_block(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(json!({"height":0}))
        }
        fn call(&self, _: &Value) -> Result<Value, RpcError> {
            Err(RpcError::new(-32004, "unsupported"))
        }
        fn estimate_gas(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(json!(21000))
        }
        fn subscribe(&self) -> Result<Box<dyn RpcSubscription>, RpcError> {
            let (_tx, rx) = mpsc::channel();
            Ok(Box::new(rx))
        }
    }

    fn service() -> RpcService {
        RpcService::new(Arc::new(Mock))
    }

    #[test]
    fn request_success_and_error_shapes_are_json_rpc_2() {
        let ok = service()
            .process_http_message(br#"{"jsonrpc":"2.0","id":7,"method":"get_block","params":{}}"#)
            .unwrap();
        let value: Value = serde_json::from_slice(&ok).unwrap();
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["result"]["height"], 0);
        assert_eq!(value["id"], 7);
        let err = service()
            .process_http_message(br#"{"jsonrpc":"2.0","id":"x","method":"call","params":{}}"#)
            .unwrap();
        let value: Value = serde_json::from_slice(&err).unwrap();
        assert_eq!(value["error"]["code"], -32004);
        assert_eq!(value["id"], "x");
    }

    #[test]
    fn notifications_and_batch_follow_protocol_semantics() {
        assert!(
            service()
                .process_http_message(br#"{"jsonrpc":"2.0","method":"get_block"}"#)
                .is_none()
        );
        let batch = service().process_http_message(br#"[{"jsonrpc":"2.0","id":1,"method":"get_block"},{"jsonrpc":"2.0","method":"get_balance","params":{}}]"#).unwrap();
        let value: Value = serde_json::from_slice(&batch).unwrap();
        assert_eq!(value.as_array().unwrap().len(), 1);
        assert_eq!(value[0]["id"], 1);
    }

    #[test]
    fn batch_limit_is_enforced() {
        let requests = (0..33)
            .map(|id| json!({"jsonrpc":"2.0","id":id,"method":"get_block"}))
            .collect::<Vec<_>>();
        let encoded = serde_json::to_vec(&Value::Array(requests)).unwrap();
        let response: Value =
            serde_json::from_slice(&service().process_http_message(&encoded).unwrap()).unwrap();
        assert_eq!(response["error"]["code"], -32600);
    }

    #[test]
    fn malformed_and_invalid_requests_are_distinct() {
        let parse = service().process_http_message(b"{").unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&parse).unwrap()["error"]["code"],
            -32700
        );
        let invalid = service()
            .process_http_message(br#"{"jsonrpc":"1.0","id":1,"method":"get_block"}"#)
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&invalid).unwrap()["error"]["code"],
            -32600
        );
    }

    #[test]
    fn invalid_ids_and_unknown_methods_fail_closed() {
        let invalid = service()
            .process_http_message(br#"{"jsonrpc":"2.0","id":{},"method":"get_block"}"#)
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&invalid).unwrap()["error"]["code"],
            -32600
        );
        let unknown = service()
            .process_http_message(br#"{"jsonrpc":"2.0","id":2,"method":"missing"}"#)
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&unknown).unwrap()["error"]["code"],
            -32601
        );
    }
}
