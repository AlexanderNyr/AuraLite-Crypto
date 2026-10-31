use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use aura_grpc_rest::{GrpcServerHandle, RestGateway, proto};
use aura_rpc::{RpcBackend, RpcError, RpcService, RpcSubscription};
use serde_json::{Value, json};
use tonic::Request;

struct TestBackend;

impl RpcBackend for TestBackend {
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

#[tokio::test]
async fn live_grpc_and_rest_share_the_json_rpc_backend() {
    let service = RpcService::new(Arc::new(TestBackend));
    let rest = Arc::new(RestGateway::new(service.clone()));
    let mut http =
        aura_rpc::RpcServerHandle::spawn_with_extension("127.0.0.1:0", service.clone(), Some(rest))
            .unwrap();
    let mut grpc = GrpcServerHandle::spawn("127.0.0.1:0", service).unwrap();

    let mut socket = TcpStream::connect(http.local_addr()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket
        .write_all(
            b"GET /api/v1.1/balances/alice HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.contains("\"apiVersion\":\"1.1\""));
    assert!(response.contains("\"balance\":\"4200\""));

    let endpoint = format!("http://{}", grpc.local_addr());
    let channel = tonic::transport::Endpoint::from_shared(endpoint)
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = proto::aura_node_client::AuraNodeClient::new(channel);
    let balance = client
        .get_balance(Request::new(proto::GetBalanceRequest {
            address: "alice".to_owned(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(balance.balance, "4200");
    assert_eq!(balance.nonce, 9);
    assert_eq!(balance.api_version, "1.1");

    let unsupported = client
        .contract_call(Request::new(proto::ContractCallRequest {
            params_json: "{}".to_owned(),
        }))
        .await
        .unwrap_err();
    assert_eq!(unsupported.code(), tonic::Code::Unimplemented);

    http.shutdown();
    grpc.shutdown();
}
