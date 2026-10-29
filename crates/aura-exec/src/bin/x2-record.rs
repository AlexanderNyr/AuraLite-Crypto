//! `x2-record` — run the X2 fixture through the real executor and pin the
//! measured block.
//!
//! The expectation block of `tests/fixtures/x2_golden.txt` is written by the
//! independent Python assembler; this binary writes the second block: the exact
//! total gas (instructions + host calls), the host part, the status, the write
//! count and the event count for every vector. That is what turns "the executor
//! charges the table" into "the executor charges exactly this much", so a
//! `wasmi` bump or a gas-table edit cannot slip through unnoticed.
//!
//! ```text
//! cargo run -p aura-exec --bin x2-record
//! make gen-x2-vectors
//! ```

use std::path::PathBuf;

use aura_exec::{
    BlockContext, ContractCall, ExecStatus, Executor, GAS_TABLE, GasRow, SandboxError,
};
use aura_state::{Account, MerkleTree, StateKey, apply_diff};
use aura_types::{Address, Amount, Hash, Nonce};

const MARKER: &str = "# --- measured by `cargo run -p aura-exec --bin x2-record` ---";

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/x2_golden.txt")
}

fn decode(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
        .collect()
}

fn slot(key: &str) -> [u8; 32] {
    let bytes = key.as_bytes();
    let mut slot = [0u8; 32];
    slot[..bytes.len()].copy_from_slice(bytes);
    slot
}

/// The parts of a vector this recorder needs.
#[derive(Debug, Default, Clone)]
struct Vector {
    label: String,
    wasm: Vec<u8>,
    gas: u64,
    arg: Option<i32>,
    accounts: Vec<(Address, u128)>,
    slots: Vec<(Address, [u8; 32], Vec<u8>)>,
}

fn parse(text: &str) -> Vec<Vector> {
    let mut out: Vec<Vector> = Vec::new();
    for line in text.lines().take_while(|line| *line != MARKER) {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key == "vector" {
            out.push(Vector {
                label: value.to_string(),
                ..Vector::default()
            });
            continue;
        }
        let Some(vector) = out.last_mut() else {
            continue;
        };
        let parts: Vec<&str> = value.split(':').collect();
        match key {
            "wasm" => vector.wasm = decode(value),
            "gas" => vector.gas = value.parse().expect("gas"),
            "arg" => {
                vector.arg = if value.is_empty() {
                    None
                } else {
                    Some(value.parse().expect("arg"))
                }
            }
            "pre_account" => vector.accounts.push((
                Address::from_slice(&decode(parts[0])).expect("address"),
                parts[1].parse().expect("units"),
            )),
            "pre_slot" => vector.slots.push((
                Address::from_slice(&decode(parts[0])).expect("address"),
                slot(parts[1]),
                decode(parts[2]),
            )),
            _ => {}
        }
    }
    out
}

fn base_tree(vector: &Vector) -> MerkleTree {
    let mut entries: Vec<(Hash, Vec<u8>)> = vec![];
    for (owner, units) in &vector.accounts {
        let account = Account::new(
            Nonce::new(0),
            Amount::from_units(*units),
            Hash::ZERO,
            Hash::ZERO,
        );
        entries.push((StateKey::account(owner).digest(), account.to_bytes()));
    }
    for (owner, key, value) in &vector.slots {
        entries.push((StateKey::storage(owner, key).digest(), value.clone()));
    }
    entries.sort_by_key(|(path, _)| *path);
    MerkleTree::from_entries(1, &entries)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = fixture_path();
    let text = std::fs::read_to_string(&path)?;
    let vectors = parse(&text);
    let executor = Executor::new();
    let contract = Address::from_slice(&[0x11u8; 20])?;

    let mut measured: Vec<String> = Vec::new();
    for vector in &vectors {
        let base = base_tree(vector);
        let call = ContractCall {
            contract,
            caller: Address::from_slice(&[0x22u8; 20])?,
            block: BlockContext {
                height: 7,
                timestamp_ms: 1_700_000_000_000,
            },
            gas_limit: vector.gas,
        };
        let receipt = match executor.call(&vector.wasm, base.clone(), call, vector.arg) {
            Ok(receipt) => receipt,
            Err(SandboxError::ImportsNotAllowed { .. }) => executor.rejected(base.tree_root()),
            Err(error) => return Err(format!("{}: {error:?}", vector.label).into()),
        };
        let status = match receipt.status {
            ExecStatus::Returned(value) => format!("returned:{value}"),
            ExecStatus::OutOfGas => "out_of_gas".to_string(),
            ExecStatus::Trap(kind) => format!("trap:{}", kind.code()),
            ExecStatus::Rejected => "rejected".to_string(),
        };
        if let Some(diff) = &receipt.diff {
            // Round-trip the diff so the recording also proves it applies.
            apply_diff(&base, diff)?;
        }
        measured.push(format!(
            "measured_{}_gas={}",
            vector.label, receipt.gas_used
        ));
        measured.push(format!(
            "measured_{}_host_gas={}",
            vector.label, receipt.host_gas
        ));
        measured.push(format!("measured_{}_status={}", vector.label, status));
        measured.push(format!(
            "measured_{}_writes={}",
            vector.label, receipt.writes
        ));
        measured.push(format!(
            "measured_{}_events={}",
            vector.label,
            receipt.events.len()
        ));
        println!(
            "{:<24} {:<24} gas={:<8} host={:<5} writes={} events={}",
            vector.label,
            status,
            receipt.gas_used,
            receipt.host_gas,
            receipt.writes,
            receipt.events.len()
        );
    }

    let mut out: Vec<String> = text
        .lines()
        .take_while(|line| *line != MARKER)
        .map(str::to_string)
        .collect();
    out.push(MARKER.to_string());
    out.extend(measured);
    std::fs::write(&path, out.join("\n") + "\n")?;

    println!();
    println!("wrote {} ({} vectors)", path.display(), vectors.len());
    println!(
        "gas table v{} (ABI v{}): {}",
        aura_exec::ABI_VERSION,
        aura_exec::ABI_VERSION,
        GAS_TABLE
            .iter()
            .map(|row: &GasRow| format!("{}={}", row.name, row.total(0)))
            .collect::<Vec<_>>()
            .join(" ")
    );
    Ok(())
}
