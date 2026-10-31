//! D4 JSON-RPC adapter over the D2/D3 node pipeline.

use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use aura_rpc::{RpcBackend, RpcError, RpcSubscription, SubscriptionPoll};
use aura_state::Account;
use aura_tx::{Tx, TxKind};
use aura_types::{Address, Hash};
use serde_json::{Value, json};

use crate::pipeline::{Pipeline, TransactionReceipt};

/// Backend that exposes the native-transfer pipeline through JSON-RPC.
#[derive(Clone)]
pub struct PipelineRpcBackend {
    pipeline: Arc<Mutex<Pipeline>>,
}

impl PipelineRpcBackend {
    /// Construct from the shared live pipeline.
    pub fn new(pipeline: Arc<Mutex<Pipeline>>) -> Self {
        Self { pipeline }
    }

    /// Shared pipeline handle, for the local block-production task.
    pub fn pipeline(&self) -> Arc<Mutex<Pipeline>> {
        Arc::clone(&self.pipeline)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Pipeline>, RpcError> {
        self.pipeline
            .lock()
            .map_err(|_| RpcError::new(-32603, "node state lock poisoned"))
    }
}

impl RpcBackend for PipelineRpcBackend {
    fn submit_tx(&self, params: &Value) -> Result<Value, RpcError> {
        let hex = required_string(params, "tx")?;
        let bytes = decode_hex(hex, 4 * 1024 * 1024)?;
        let tx = Tx::from_canonical_bytes(&bytes).map_err(|error| {
            RpcError::new(-32602, format!("invalid canonical transaction: {error}"))
        })?;
        let txid = self
            .lock()?
            .submit(tx)
            .map_err(|error| RpcError::new(-32010, format!("transaction rejected: {error}")))?;
        Ok(Value::String(format!("0x{}", txid.to_hex())))
    }

    fn get_balance(&self, params: &Value) -> Result<Value, RpcError> {
        let address = Address::from_bech32(required_string(params, "address")?)
            .map_err(|error| RpcError::new(-32602, format!("invalid address: {error}")))?;
        let account = self
            .lock()?
            .account(&address)
            .unwrap_or_else(Account::default);
        Ok(json!({
            "address": address.to_string(),
            "balance": account.balance.units().to_string(),
            "nonce": account.nonce.value(),
        }))
    }

    fn get_tx(&self, params: &Value) -> Result<Value, RpcError> {
        let txid = parse_hash(required_string(params, "txid")?)?;
        let pipeline = self.lock()?;
        if pipeline.contains_pending_txid(&txid) {
            return Ok(json!({"txid":format!("0x{}", txid.to_hex()), "status":"pending"}));
        }
        match pipeline.stored_receipt(&txid).map_err(internal)? {
            Some(bytes) => {
                let receipt = TransactionReceipt::from_bytes(&bytes).map_err(internal)?;
                Ok(
                    json!({"txid":format!("0x{}", txid.to_hex()), "status":"committed", "receipt":receipt_json(&receipt)}),
                )
            }
            None => Ok(Value::Null),
        }
    }

    fn get_block(&self, params: &Value) -> Result<Value, RpcError> {
        let height = match params {
            Value::Null => None,
            Value::Object(object) => match object.get("height") {
                None | Some(Value::Null) => None,
                Some(value) => Some(value.as_u64().ok_or_else(|| {
                    RpcError::new(-32602, "height must be an unsigned integer or null")
                })?),
            },
            _ => return Err(RpcError::new(-32602, "params must be an object or null")),
        };
        let block = self.lock()?.committed_block(height);
        Ok(block
            .map(|block| {
                json!({
                    "height": block.header.height.value(),
                    "hash": format!("0x{}", block.block_hash().to_hex()),
                    "parentHash": format!("0x{}", block.header.parent_hash.to_hex()),
                    "stateRoot": format!("0x{}", block.header.state_root.to_hex()),
                    "txRoot": format!("0x{}", block.header.tx_root.to_hex()),
                    "chainId": block.header.chain_id.as_str(),
                    "timestampMs": block.header.timestamp_ms,
                    "gasUsed": block.header.gas_used.value().to_string(),
                    "baseFee": block.header.base_fee.units().to_string(),
                    "transactionCount": block.body.txs.len(),
                })
            })
            .unwrap_or(Value::Null))
    }

    fn call(&self, _params: &Value) -> Result<Value, RpcError> {
        Err(RpcError::new(
            -32004,
            "contract execution is not available; D4 exposes the method but this node executes native transfers only",
        ))
    }

    fn estimate_gas(&self, params: &Value) -> Result<Value, RpcError> {
        let object = params
            .as_object()
            .ok_or_else(|| RpcError::new(-32602, "params must be an object"))?;
        let kind = if let Some(encoded) = object.get("tx").and_then(Value::as_str) {
            let tx = Tx::from_canonical_bytes(&decode_hex(encoded, 4 * 1024 * 1024)?).map_err(
                |error| RpcError::new(-32602, format!("invalid canonical transaction: {error}")),
            )?;
            tx.unsigned.kind
        } else if object.get("kind").and_then(Value::as_str) == Some("transfer") {
            TxKind::Transfer
        } else {
            return Err(RpcError::new(
                -32602,
                "estimate_gas requires tx or kind=transfer",
            ));
        };
        if kind != TxKind::Transfer {
            return Err(RpcError::new(
                -32004,
                "gas estimation is only implemented for native transfers",
            ));
        }
        Ok(
            json!({"gas":crate::pipeline::TRANSFER_GAS.to_string(), "gasUnits":crate::pipeline::TRANSFER_GAS}),
        )
    }

    fn subscribe(&self) -> Result<Box<dyn RpcSubscription>, RpcError> {
        let events = self.lock()?.subscribe_managed();
        Ok(Box::new(PipelineSubscription { events }))
    }
}

struct PipelineSubscription {
    events: crate::pipeline::ManagedSubscription,
}

impl RpcSubscription for PipelineSubscription {
    fn poll(&mut self, timeout: Duration) -> SubscriptionPoll {
        match self.events.recv_timeout(timeout) {
            Ok(event) => SubscriptionPoll::Event(block_event_json(event.committed)),
            Err(mpsc::RecvTimeoutError::Timeout) => SubscriptionPoll::Timeout,
            Err(mpsc::RecvTimeoutError::Disconnected) => SubscriptionPoll::Closed,
        }
    }
}

fn required_string<'a>(params: &'a Value, name: &str) -> Result<&'a str, RpcError> {
    params
        .as_object()
        .and_then(|object| object.get(name))
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::new(-32602, format!("missing string parameter `{name}`")))
}

fn parse_hash(value: &str) -> Result<Hash, RpcError> {
    Hash::from_hex(value.strip_prefix("0x").unwrap_or(value))
        .map_err(|error| RpcError::new(-32602, format!("invalid transaction hash: {error}")))
}

fn decode_hex(value: &str, maximum: usize) -> Result<Vec<u8>, RpcError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if !value.len().is_multiple_of(2) || value.len() / 2 > maximum {
        return Err(RpcError::new(
            -32602,
            "hex byte string has odd length or exceeds the size limit",
        ));
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let (pairs, remainder) = value.as_bytes().as_chunks::<2>();
    if !remainder.is_empty() {
        return Err(RpcError::new(-32602, "invalid hex byte string"));
    }
    for pair in pairs {
        let high =
            hex_digit(pair[0]).ok_or_else(|| RpcError::new(-32602, "invalid hex byte string"))?;
        let low =
            hex_digit(pair[1]).ok_or_else(|| RpcError::new(-32602, "invalid hex byte string"))?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn receipt_json(receipt: &TransactionReceipt) -> Value {
    json!({
        "txid": format!("0x{}", receipt.txid.to_hex()),
        "from": receipt.signer.to_string(),
        "nonce": receipt.nonce.value(),
        "to": receipt.recipient.to_string(),
        "amount": receipt.amount.units().to_string(),
        "gasUsed": receipt.gas_used.value().to_string(),
        "baseFeeBurned": receipt.base_fee_burned.units().to_string(),
        "tipPaid": receipt.tip_paid.units().to_string(),
        "height": receipt.height.value(),
    })
}

fn block_event_json(block: crate::pipeline::CommittedBlock) -> Value {
    json!({
        "height": block.block.header.height.value(),
        "hash": format!("0x{}", block.block.block_hash().to_hex()),
        "stateRoot": format!("0x{}", block.block.header.state_root.to_hex()),
        "transactionCount": block.receipts.len(),
        "storageVersion": block.storage.version,
    })
}

fn internal(error: crate::pipeline::PipelineError) -> RpcError {
    RpcError::new(-32603, format!("node state read failed: {error}"))
}
