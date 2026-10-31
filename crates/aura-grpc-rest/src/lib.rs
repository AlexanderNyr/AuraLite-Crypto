//! D5 versioned REST gateway and protobuf gRPC service over the D4 API.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::io;
use std::net::{SocketAddr, TcpListener, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use aura_rpc::{HttpExtension, HttpExtensionResponse, RpcService};
use serde_json::{Map, Value, json};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

/// Generated protobuf types and gRPC client/server stubs for stable package `aura.api.v1`.
#[allow(missing_docs)]
pub mod proto {
    tonic::include_proto!("aura.api.v1");
}

const OPENAPI_YAML: &str = include_str!("../../../docs/api/openapi-d5.yaml");
const OPENAPI_JSON: &str = include_str!("../../../docs/api/openapi-d5.json");
const API_VERSION: &str = "1.1";

/// REST route extension attached to the bounded D4 HTTP listener.
#[derive(Clone)]
pub struct RestGateway {
    service: RpcService,
}

impl RestGateway {
    /// Create a REST gateway backed by the existing D4 service.
    pub fn new(service: RpcService) -> Self {
        Self { service }
    }
}

impl HttpExtension for RestGateway {
    fn handle(
        &self,
        method: &str,
        path: &str,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Option<HttpExtensionResponse> {
        let path = path.split('?').next().unwrap_or(path);
        if method == "GET" && path == "/openapi.yaml" {
            return Some(HttpExtensionResponse::new(
                200,
                "application/yaml; charset=utf-8",
                OPENAPI_YAML.as_bytes().to_vec(),
            ));
        }
        if method == "GET" && path == "/openapi.json" {
            return Some(HttpExtensionResponse::new(
                200,
                "application/json",
                OPENAPI_JSON.as_bytes().to_vec(),
            ));
        }
        if !path.starts_with("/api/") {
            return None;
        }
        let Some((version, resource)) = path
            .strip_prefix("/api/")
            .and_then(|tail| tail.split_once('/'))
        else {
            return Some(error_response(404, -32040, "unknown API route"));
        };
        let version = match version {
            "v1" => "1.0",
            "v1.1" => API_VERSION,
            _ => return Some(error_response(404, -32041, "unsupported API version")),
        };
        let segments = resource.split('/').collect::<Vec<_>>();
        let route = match (method, segments.as_slice()) {
            ("GET", ["balances", address]) if !address.is_empty() => {
                ("get_balance", json!({"address": address}))
            }
            ("GET", ["transactions", transaction_hash]) if !transaction_hash.is_empty() => {
                ("get_tx", json!({"txid": transaction_hash}))
            }
            ("GET", ["blocks", height]) if !height.is_empty() => {
                let height = if *height == "latest" {
                    Value::Null
                } else if let Ok(value) = height.parse::<u64>() {
                    json!(value)
                } else {
                    return Some(error_response(
                        400,
                        -32602,
                        "height must be `latest` or an unsigned integer",
                    ));
                };
                ("get_block", json!({"height": height}))
            }
            ("POST", ["transactions"]) => {
                if !is_json_content_type(content_type) {
                    return Some(error_response(
                        415,
                        -32600,
                        "Content-Type must be application/json",
                    ));
                }
                let object = match strict_json_object(body, &["tx"]) {
                    Ok(object) => object,
                    Err(message) => return Some(error_response(400, -32602, &message)),
                };
                let Some(tx) = object.get("tx").and_then(Value::as_str) else {
                    return Some(error_response(400, -32602, "`tx` must be a string"));
                };
                ("submit_tx", json!({"tx": tx}))
            }
            ("POST", ["gas-estimate"]) => {
                if !is_json_content_type(content_type) {
                    return Some(error_response(
                        415,
                        -32600,
                        "Content-Type must be application/json",
                    ));
                }
                let object = match strict_json_object(body, &["kind", "tx"]) {
                    Ok(object) => object,
                    Err(message) => return Some(error_response(400, -32602, &message)),
                };
                if object.is_empty() {
                    return Some(error_response(400, -32602, "provide `kind` or `tx`"));
                }
                if object.values().any(|value| !value.is_string()) {
                    return Some(error_response(
                        400,
                        -32602,
                        "`kind` and `tx` must be strings",
                    ));
                }
                ("estimate_gas", Value::Object(object))
            }
            ("GET", ["balances"]) | ("GET", ["transactions"]) | ("GET", ["blocks"]) => {
                return Some(error_response(
                    404,
                    -32040,
                    "resource identifier is required",
                ));
            }
            ("POST", ["transactions", _]) | ("POST", ["gas-estimate", _]) => {
                return Some(error_response(
                    405,
                    -32042,
                    "method not allowed for this route",
                ));
            }
            _ => return Some(error_response(404, -32040, "unknown API route")),
        };
        let (rpc_method, params) = route;
        match invoke_rpc(&self.service, rpc_method, params) {
            Ok(result) => {
                let mut envelope = Map::new();
                envelope.insert("result".to_owned(), result);
                if version == API_VERSION {
                    envelope.insert(
                        "apiVersion".to_owned(),
                        Value::String(API_VERSION.to_owned()),
                    );
                }
                Some(HttpExtensionResponse::new(
                    200,
                    "application/json",
                    serde_json::to_vec(&Value::Object(envelope)).unwrap_or_else(|_| b"{}".to_vec()),
                ))
            }
            Err((status, code, message)) => Some(error_response(status, code, &message)),
        }
    }
}

fn is_json_content_type(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
    })
}

fn strict_json_object(body: &[u8], allowed: &[&str]) -> Result<Map<String, Value>, String> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| "malformed JSON body".to_owned())?;
    let object = value
        .as_object()
        .ok_or_else(|| "request body must be a JSON object".to_owned())?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unknown field `{key}`"));
        }
    }
    Ok(object.clone())
}

fn invoke_rpc(
    service: &RpcService,
    method: &str,
    params: Value,
) -> Result<Value, (u16, i64, String)> {
    let message = json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params});
    let request = serde_json::to_vec(&message)
        .map_err(|_| (500, -32603, "request encoding failed".to_owned()))?;
    let response = service
        .process_http_message(&request)
        .ok_or_else(|| (500, -32603, "RPC service returned no response".to_owned()))?;
    let response: Value = serde_json::from_slice(&response)
        .map_err(|_| (500, -32603, "RPC service returned invalid JSON".to_owned()))?;
    if let Some(error) = response.get("error") {
        let code = error.get("code").and_then(Value::as_i64).unwrap_or(-32603);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("RPC request failed")
            .to_owned();
        let status = match code {
            -32602..=-32600 => 400,
            -32004 => 501,
            -32010 => 400,
            -32005 => 405,
            _ => 500,
        };
        return Err((status, code, message));
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| (500, -32603, "RPC response omitted result".to_owned()))
}

fn error_response(status: u16, code: i64, message: &str) -> HttpExtensionResponse {
    HttpExtensionResponse::new(
        status,
        "application/json",
        serde_json::to_vec(&json!({"error":{"code":code,"message":message}}))
            .unwrap_or_else(|_| b"{}".to_vec()),
    )
}

/// Running gRPC listener using the stable `aura.api.v1` protobuf package.
pub struct GrpcServerHandle {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl GrpcServerHandle {
    /// Bind and start the gRPC listener on the requested address.
    pub fn spawn<A: ToSocketAddrs>(addr: A, service: RpcService) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("aura-grpc-listener".to_owned())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(io::Error::other(error)));
                        return;
                    }
                };
                let incoming = {
                    let _guard = runtime.enter();
                    let listener = match tokio::net::TcpListener::from_std(listener) {
                        Ok(listener) => listener,
                        Err(error) => {
                            let _ = ready_tx.send(Err(error));
                            return;
                        }
                    };
                    TcpListenerStream::new(listener)
                };
                let grpc_service =
                    proto::aura_node_server::AuraNodeServer::new(GrpcApi { service });
                let _ = ready_tx.send(Ok(()));
                let shutdown = async move {
                    while !thread_stop.load(Ordering::Acquire) {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                };
                let server = tonic::transport::Server::builder()
                    .add_service(grpc_service)
                    .serve_with_incoming(incoming);
                runtime.block_on(async move {
                    tokio::select! {
                        _ = server => {},
                        _ = shutdown => {},
                    }
                });
            })?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                addr,
                stop,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                let _ = join.join();
                Err(io::Error::other("gRPC listener startup thread exited"))
            }
        }
    }

    /// Return the socket address selected by the listener.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stop the gRPC listener and wait for the server task to exit.
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for GrpcServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Clone)]
struct GrpcApi {
    service: RpcService,
}

#[tonic::async_trait]
impl proto::aura_node_server::AuraNode for GrpcApi {
    async fn submit_transaction(
        &self,
        request: Request<proto::SubmitTransactionRequest>,
    ) -> Result<Response<proto::SubmitTransactionResponse>, Status> {
        let result = invoke_rpc(
            &self.service,
            "submit_tx",
            json!({"tx":request.into_inner().signed_transaction_hex}),
        )
        .map_err(status_from_error)?;
        let transaction_hash = result
            .as_str()
            .ok_or_else(|| Status::internal("transaction hash is not a string"))?;
        Ok(Response::new(proto::SubmitTransactionResponse {
            transaction_hash: transaction_hash.to_owned(),
            api_version: API_VERSION.to_owned(),
        }))
    }

    async fn get_balance(
        &self,
        request: Request<proto::GetBalanceRequest>,
    ) -> Result<Response<proto::GetBalanceResponse>, Status> {
        let result = invoke_rpc(
            &self.service,
            "get_balance",
            json!({"address":request.into_inner().address}),
        )
        .map_err(status_from_error)?;
        let response = Response::new(proto::GetBalanceResponse {
            address: result
                .get("address")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            balance: result
                .get("balance")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            nonce: result
                .get("nonce")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            api_version: API_VERSION.to_owned(),
        });
        Ok(response)
    }

    async fn get_transaction(
        &self,
        request: Request<proto::GetTransactionRequest>,
    ) -> Result<Response<proto::GetTransactionResponse>, Status> {
        let result = invoke_rpc(
            &self.service,
            "get_tx",
            json!({"txid":request.into_inner().transaction_hash}),
        )
        .map_err(status_from_error)?;
        Ok(Response::new(proto::GetTransactionResponse {
            result_json: serde_json::to_string(&result)
                .map_err(|_| Status::internal("result encoding failed"))?,
            api_version: API_VERSION.to_owned(),
        }))
    }

    async fn get_block(
        &self,
        request: Request<proto::GetBlockRequest>,
    ) -> Result<Response<proto::GetBlockResponse>, Status> {
        let height = request.into_inner().height.filter(|height| *height != 0);
        let result = invoke_rpc(&self.service, "get_block", json!({"height":height}))
            .map_err(status_from_error)?;
        Ok(Response::new(proto::GetBlockResponse {
            result_json: serde_json::to_string(&result)
                .map_err(|_| Status::internal("result encoding failed"))?,
            api_version: API_VERSION.to_owned(),
        }))
    }

    async fn estimate_gas(
        &self,
        request: Request<proto::EstimateGasRequest>,
    ) -> Result<Response<proto::EstimateGasResponse>, Status> {
        let request = request.into_inner();
        let params = if !request.signed_transaction_hex.is_empty() {
            json!({"tx":request.signed_transaction_hex})
        } else {
            json!({"kind":request.kind})
        };
        let result =
            invoke_rpc(&self.service, "estimate_gas", params).map_err(status_from_error)?;
        Ok(Response::new(proto::EstimateGasResponse {
            gas_units: result
                .get("gasUnits")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            gas: result
                .get("gas")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            api_version: API_VERSION.to_owned(),
        }))
    }

    async fn contract_call(
        &self,
        request: Request<proto::ContractCallRequest>,
    ) -> Result<Response<proto::ContractCallResponse>, Status> {
        let params: Value = serde_json::from_str(&request.into_inner().params_json)
            .map_err(|_| Status::invalid_argument("params_json must contain a JSON value"))?;
        let result = invoke_rpc(&self.service, "call", params).map_err(status_from_error)?;
        Ok(Response::new(proto::ContractCallResponse {
            result_json: serde_json::to_string(&result)
                .map_err(|_| Status::internal("result encoding failed"))?,
            api_version: API_VERSION.to_owned(),
        }))
    }
}

fn status_from_error((_, code, message): (u16, i64, String)) -> Status {
    match code {
        -32602..=-32600 => Status::invalid_argument(message),
        -32004 | -32005 => Status::unimplemented(message),
        -32010 => Status::failed_precondition(message),
        _ => Status::internal(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_rpc::{RpcBackend, RpcError, RpcSubscription, SubscriptionPoll};

    struct FakeBackend;

    impl RpcBackend for FakeBackend {
        fn submit_tx(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(Value::String("0xabc".to_owned()))
        }
        fn get_balance(&self, params: &Value) -> Result<Value, RpcError> {
            Ok(json!({"address":params["address"],"balance":"4200","nonce":9}))
        }
        fn get_tx(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(Value::Null)
        }
        fn get_block(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(Value::Null)
        }
        fn call(&self, _: &Value) -> Result<Value, RpcError> {
            Err(RpcError::new(-32004, "contract execution unsupported"))
        }
        fn estimate_gas(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(json!({"gas":"21000","gasUnits":21000}))
        }
        fn subscribe(&self) -> Result<Box<dyn RpcSubscription>, RpcError> {
            let (_sender, receiver) = mpsc::channel();
            Ok(Box::new(receiver))
        }
    }

    fn gateway() -> RestGateway {
        RestGateway::new(RpcService::new(Arc::new(FakeBackend)))
    }

    fn call(
        method: &str,
        path: &str,
        content_type: Option<&str>,
        body: &[u8],
    ) -> HttpExtensionResponse {
        gateway()
            .handle(method, path, content_type, body)
            .expect("gateway route")
    }

    fn json_body(response: &HttpExtensionResponse) -> Value {
        serde_json::from_slice(&response.body).unwrap()
    }

    #[test]
    fn rest_rejects_unknown_json_fields() {
        let response = call(
            "POST",
            "/api/v1/transactions",
            Some("application/json"),
            br#"{"tx":"0x01","admin":true}"#,
        );
        assert_eq!(response.status, 400);
        assert!(
            json_body(&response)["error"]["message"]
                .as_str()
                .unwrap()
                .contains("unknown field")
        );
    }

    #[test]
    fn rest_v11_is_additive_and_accepts_v1_requests() {
        let v1 = call("GET", "/api/v1/balances/alice", None, b"");
        let v11 = call("GET", "/api/v1.1/balances/alice", None, b"");
        let v1_json = json_body(&v1);
        let v11_json = json_body(&v11);
        assert_eq!(v1.status, 200);
        assert_eq!(v11.status, 200);
        assert_eq!(v1_json["result"], v11_json["result"]);
        assert_eq!(v11_json["apiVersion"], "1.1");
        assert_eq!(v1_json["result"]["balance"], "4200");
    }

    #[test]
    fn rest_rejects_unsupported_version() {
        let response = call("GET", "/api/v2/balances/alice", None, b"");
        assert_eq!(response.status, 404);
        assert_eq!(json_body(&response)["error"]["code"], -32041);
    }

    #[test]
    fn openapi_documents_both_versions_and_closed_request_schemas() {
        let yaml = std::str::from_utf8(&call("GET", "/openapi.yaml", None, b"").body)
            .unwrap()
            .to_owned();
        let json_doc: Value =
            serde_json::from_slice(&call("GET", "/openapi.json", None, b"").body).unwrap();
        assert!(yaml.contains("/api/v1.1/balances/{address}"));
        assert_eq!(yaml.matches("additionalProperties: false").count(), 5);
        assert_eq!(json_doc["info"]["version"], "1.1.0");
        assert!(
            json_doc["paths"]
                .get("/api/v1/balances/{address}")
                .is_some()
        );
    }

    #[test]
    fn protobuf_v11_response_decodes_with_the_v1_field_set() {
        use prost::Message;
        #[derive(Message)]
        struct V1BalanceResponse {
            #[prost(string, tag = "1")]
            address: String,
            #[prost(string, tag = "2")]
            balance: String,
            #[prost(uint64, tag = "3")]
            nonce: u64,
        }
        let v11 = proto::GetBalanceResponse {
            address: "alice".to_owned(),
            balance: "4200".to_owned(),
            nonce: 9,
            api_version: "1.1".to_owned(),
        };
        let old_client = V1BalanceResponse::decode(v11.encode_to_vec().as_slice()).unwrap();
        assert_eq!(old_client.address, "alice");
        assert_eq!(old_client.balance, "4200");
        assert_eq!(old_client.nonce, 9);
    }

    #[tokio::test]
    async fn unsupported_contract_calls_remain_explicit_grpc_errors() {
        let grpc = GrpcApi {
            service: RpcService::new(Arc::new(FakeBackend)),
        };
        let result = <GrpcApi as proto::aura_node_server::AuraNode>::contract_call(
            &grpc,
            Request::new(proto::ContractCallRequest {
                params_json: "{}".to_owned(),
            }),
        )
        .await;
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unimplemented);
    }

    #[test]
    fn closed_subscription_is_available_for_test_backend() {
        let mut subscription = FakeBackend.subscribe().unwrap();
        assert!(matches!(
            subscription.poll(Duration::ZERO),
            SubscriptionPoll::Closed
        ));
    }
}
