//! H3 fuzz oracles: deterministic totality and determinism checks for every
//! decoder area the campaign fuzzes (docs/h3-fuzz-plan.md §1, D-H3-2).
//!
//! A decoder panic fails the oracle with
//! `H3: fuzz oracle caught a decoder panic in … (injected bug found)` —
//! this is exactly what `tools/nc_h3.py` requires when it injects bugs.
//! Witnesses are ordinary boundary inputs (empty, truncated, length-prefix
//! bombs, wasm magic, JSON braces, `0xff` runs), not trigger patterns.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::Duration;

use aura_rpc::{RpcBackend, RpcError, RpcService, RpcSubscription, SubscriptionPoll};
use aura_testkit::{FuzzOutcome, fuzz_one};
use serde_json::Value;

const WITNESSES: &[&[u8]] = &[
    &[],
    &[0x00],
    &[0xff],
    &[0xff, 0xff, 0xff, 0xff],
    b"\x00asm\x01\x00\x00\x00",
    b"{",
    b"{}",
    &[0x00, 0x00, 0x00, 0x00],
    &[0x7f, 0x45, 0x4c, 0x46],
];

/// Feed every witness through `f`; a decoder panic means the fuzz oracle
/// caught an injected (or real) bug. Also checks determinism: the accept /
/// reject verdict for the same input must not change between calls.
fn total_and_deterministic(label: &str, inputs: &[&[u8]], f: impl Fn(&[u8]) -> bool) {
    for (index, input) in inputs.iter().enumerate() {
        let first = catch_unwind(AssertUnwindSafe(|| f(*input)));
        let verdict = match first {
            Ok(verdict) => verdict,
            Err(_) => panic!(
                "H3: fuzz oracle caught a decoder panic in {label} input {index} (injected bug found)"
            ),
        };
        let second = catch_unwind(AssertUnwindSafe(|| f(*input))).unwrap_or_else(|_| {
            panic!(
                "H3: fuzz oracle caught a decoder panic in {label} input {index} (injected bug found)"
            )
        });
        assert_eq!(
            verdict, second,
            "H3: fuzz oracle found non-deterministic verdict in {label} input {index}"
        );
    }
}

/// Verdict over the portable harness: `Completed` is success, a caught
/// decoder `Panic` is the injected-bug signal, `UnknownTarget` is a
/// misconfigured oracle and must never pass.
fn portable(target: &'static str) -> impl Fn(&[u8]) -> bool {
    move |data| match fuzz_one(target, data) {
        FuzzOutcome::Completed => true,
        FuzzOutcome::Panic => {
            panic!("H3: fuzz oracle caught a decoder panic in {target} (injected bug found)")
        }
        FuzzOutcome::UnknownTarget => panic!("H3: unknown portable fuzz target {target}"),
    }
}

#[test]
fn p2p_frame_decoders_are_total() {
    total_and_deterministic("p2p-handshake", WITNESSES, |data| {
        aura_p2p::transport::HandshakeHello::decode(data).is_ok()
    });
    total_and_deterministic("p2p-gossip", WITNESSES, |data| {
        aura_p2p::gossipsub::GossipMessage::decode(data).is_ok()
    });
}

#[test]
fn codec_decoder_is_total() {
    total_and_deterministic("codec", WITNESSES, portable("codec"));
}

#[test]
fn tx_decoder_is_total() {
    total_and_deterministic("tx", WITNESSES, portable("tx"));
}

#[test]
fn wasm_verifier_is_total() {
    total_and_deterministic("wasm-gate", WITNESSES, |data| {
        let sandbox = aura_exec::sandbox::Sandbox::new();
        sandbox.gate(data).is_ok()
    });
}

struct RejectingBackend;

impl RpcBackend for RejectingBackend {
    fn submit_tx(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32000, "unsupported in h3 oracle"))
    }
    fn get_balance(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32000, "unsupported in h3 oracle"))
    }
    fn get_tx(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32000, "unsupported in h3 oracle"))
    }
    fn get_block(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32000, "unsupported in h3 oracle"))
    }
    fn call(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32000, "unsupported in h3 oracle"))
    }
    fn estimate_gas(&self, _: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(-32000, "unsupported in h3 oracle"))
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

#[test]
fn rpc_parsers_are_total() {
    total_and_deterministic("rpc-http", WITNESSES, |data| {
        let service = RpcService::new(Arc::new(RejectingBackend));
        service.process_http_message(data).is_some()
    });
    total_and_deterministic("rpc-websocket", WITNESSES, |data| {
        let service = RpcService::new(Arc::new(RejectingBackend));
        service.process_websocket_message(data).is_some()
    });
}

#[test]
fn portable_targets_are_total() {
    for target in ["state-key", "genesis-json", "block", "merkle-proof"] {
        total_and_deterministic(target, WITNESSES, portable(target));
    }
}
