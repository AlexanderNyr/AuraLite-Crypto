//! `aurad-faucet` — the O4 public faucet service over the node
//! `RpcBackend` (D-O4-5).
//!
//! Reads `payout <bech32-address> <units> <nonce>` request lines and
//! pays each one through `PipelineRpcBackend::submit_tx` (an ordinary
//! `build_faucet_transfer` transaction), subject to a deterministic
//! per-run payout budget (`--limit`). One line out per line in:
//! `payout <address> <units> ok` or
//! `payout <address> <units> refused: faucet limit exceeded`.
//!
//! Design: `docs/o4-testnet-public-plan.md`, guide:
//! `docs/testnet-public.md`.
//! Usage: `aurad-faucet [--limit <units>] [--requests <file>]`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use aura_node::devnet::{
    DEVNET_CHAIN_ID, DEVNET_FAUCET_INITIAL_BALANCE, build_faucet_transfer, faucet_address,
};
use aura_node::pipeline::{Pipeline, PipelineConfig};
use aura_node::rpc::PipelineRpcBackend;
use aura_rpc::RpcBackend;
use aura_types::{Address, Amount};
use serde_json::json;

/// Default per-run payout budget (units).
const DEFAULT_LIMIT: u64 = 1_000;

fn main() {
    let mut limit: u64 = DEFAULT_LIMIT;
    let mut requests_path = PathBuf::from("release/faucet-requests.txt");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--address" => {
                println!("faucet {}", faucet_address().to_bech32());
                std::process::exit(0);
            }
            "--limit" => {
                index += 1;
                limit = args
                    .get(index)
                    .expect("--limit value")
                    .parse()
                    .expect("--limit unsigned");
            }
            "--requests" => {
                index += 1;
                requests_path = PathBuf::from(args.get(index).expect("--requests value"));
            }
            other => {
                eprintln!("error: usage: unknown argument {other}");
                std::process::exit(64);
            }
        }
        index += 1;
    }

    // The service runs over the same RpcBackend the node RPC exposes.
    let dir = std::env::temp_dir().join(format!("aura-o4-faucet-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let pipeline = Pipeline::open(PipelineConfig {
        chain_id: DEVNET_CHAIN_ID.to_owned(),
        data_dir: dir.clone(),
        validator_seed: [41u8; 32],
        allocations: vec![(
            faucet_address(),
            Amount::from_units(DEVNET_FAUCET_INITIAL_BALANCE),
        )],
        genesis_time_ms: 0,
    })
    .expect("open faucet pipeline");
    let backend = PipelineRpcBackend::new(Arc::new(Mutex::new(pipeline)));

    let requests = std::fs::read_to_string(&requests_path).unwrap_or_else(|_| {
        eprintln!(
            "error: usage: cannot read requests {}",
            requests_path.display()
        );
        std::process::exit(64);
    });

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut paid: u64 = 0;
    for raw in requests.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split(' ');
        let (Some(verb), Some(address), Some(units), Some(request_nonce)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            eprintln!("error: usage: bad request line: {line}");
            std::process::exit(64);
        };
        if verb != "payout" || fields.next().is_some() {
            eprintln!("error: usage: bad request line: {line}");
            std::process::exit(64);
        }
        let amount: u64 = units.parse().expect("units unsigned");
        let request_nonce: u64 = request_nonce.parse().expect("nonce unsigned");
        let recipient = match Address::from_bech32(address) {
            Ok(value) => value,
            Err(_) => {
                eprintln!("error: usage: bad request line: {line}");
                std::process::exit(64);
            }
        };
        if paid + amount > limit {
            let _ = writeln!(
                out,
                "payout {address} {units} refused: faucet limit exceeded"
            );
            continue;
        }
        let tx = build_faucet_transfer(
            DEVNET_CHAIN_ID,
            recipient,
            Amount::from_units(u128::from(amount)),
            request_nonce,
        )
        .expect("build faucet transfer");
        let bytes = tx.to_bytes();
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let params = json!({ "tx": format!("0x{hex}") });
        match backend.submit_tx(&params) {
            Ok(_) => {
                paid += amount;
                let _ = writeln!(out, "payout {address} {units} ok");
            }
            Err(error) => {
                let _ = writeln!(out, "payout {address} {units} refused: {}", error.message);
            }
        }
    }
    let _ = out.flush();
    let _ = std::fs::remove_dir_all(&dir);
}
