#![no_main]

use std::sync::Arc;
use std::time::Duration;

use aura_rpc::{RpcBackend, RpcError, RpcService, RpcSubscription, SubscriptionPoll};
use libfuzzer_sys::fuzz_target;
use serde_json::Value;

struct RejectingBackend;

impl RpcBackend for RejectingBackend {
    fn submit_tx(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32000, "unsupported in parser fuzz target"))
    }

    fn get_balance(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32000, "unsupported in parser fuzz target"))
    }

    fn get_tx(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32000, "unsupported in parser fuzz target"))
    }

    fn get_block(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32000, "unsupported in parser fuzz target"))
    }

    fn call(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32000, "unsupported in parser fuzz target"))
    }

    fn estimate_gas(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32000, "unsupported in parser fuzz target"))
    }

    fn subscribe(&self) -> Result<Box<dyn RpcSubscription>, RpcError> {
        Ok(Box::new(ClosedSubscription))
    }
}

struct ClosedSubscription;

impl RpcSubscription for ClosedSubscription {
    fn poll(&mut self, _: Duration) -> SubscriptionPoll {
        SubscriptionPoll::Closed
    }
}

// Exercise JSON parsing, envelope validation, batch/notification handling, and
// dispatch without binding a socket or relying on mutable node state.
fuzz_target!(|data: &[u8]| {
    let service = RpcService::new(Arc::new(RejectingBackend));
    let _ = service.process_http_message(data);
    let _ = service.process_websocket_message(data);
});
