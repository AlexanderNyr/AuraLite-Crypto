//! D8 `aura` CLI — operator and client command-line interface covering:
//! - `genesis` (`hash`, `verify`, `show`)
//! - `keys` (`derive`, `inspect`, `sign`, `verify`)
//! - `tx` (`transfer`, `decode`, `hash`)
//! - `query` (`balance`, `block`, `tx`, `health`)
//! - `node` (`config-check`, `status`, `health`)
//! - `snapshot` (`create`, `verify`, `info`)
//! - `upgrade` (`plan`, `verify`)

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use aura_crypto::{blake3_hash, constant_time_eq, ed25519_public_key, sign, verify};
use aura_genesis::from_json_file;
use aura_tx::{Tx, TxKind, TxParams, UnsignedTx, signer_address};
use aura_types::{Address, Amount, ChainId, Domain, Gas, Hash, Height, Nonce, PROTOCOL_VERSION};
use serde_json::{Value, json};

/// Exit code on successful command completion.
pub const EXIT_OK: i32 = 0;
/// Exit code on verification/execution/RPC error.
pub const EXIT_ERROR: i32 = 1;
/// Exit code on invalid CLI usage or missing flags.
pub const EXIT_USAGE: i32 = 2;

/// Native transfer gas floor enforced by `aura tx transfer`.
pub const CLI_TRANSFER_GAS: u64 = 21_000;
/// Native transfer base fee units enforced by `aura tx transfer`.
pub const CLI_BASE_FEE_UNITS: u128 = 1;

/// Domain tag for deterministic D8 CLI snapshot manifests.
pub const SNAPSHOT_CLI_DOMAIN: &[u8] = b"AuraLite-Crypto/cli-snapshot-v1\0";
/// Domain tag for deterministic D8 CLI upgrade plans.
pub const UPGRADE_CLI_DOMAIN: &[u8] = b"AuraLite-Crypto/cli-upgrade-v1\0";

/// Known TOML keys permitted in `aurad` configuration files.
pub const ALLOWED_NODE_TOML_KEYS: &[&str] = &[
    "chain_id",
    "data_dir",
    "rpc_addr",
    "grpc_addr",
    "validator_key",
    "log_filter",
];

/// Full `aura --help` documentation covering all 7 subcommand families and flags.
pub const HELP_TEXT: &str = "\
aura — AuraLite-Crypto operator and client CLI (D8)

USAGE:
  aura --help | -h | help [subcommand]
  aura --version | -V
  aura <subcommand> <action> [flags]

SUBCOMMANDS:
  genesis   Deterministic genesis inspection and verification (C8)
    aura genesis hash   --file <genesis.json>
    aura genesis verify --file <genesis.json> --expect <64-hex>
    aura genesis show   --file <genesis.json>

  keys      Ed25519 key derivation, inspection, domain signing and verification
    aura keys derive  --seed <64-hex>
    aura keys inspect --pubkey <64-hex>
    aura keys sign    --seed <64-hex> --chain-id <id> --message <hex>
    aura keys verify  --pubkey <64-hex> --chain-id <id> --message <hex> --signature <128-hex>

  tx        Canonical transaction construction, signing, decoding and hashing
    aura tx transfer  --seed <64-hex> --chain-id <id> --to <aura1...> --amount <u128> --nonce <u64>
                      [--gas-limit <u64>] [--max-fee <u128>] [--tip <u128>] [--expiry <u64>] [--rpc <host:port>]
    aura tx decode    --tx <0x-hex> [--chain-id <id>] [--height <u64>]
    aura tx hash      --tx <0x-hex>

  query     JSON-RPC 2.0 and HTTP queries against a live aurad node
    aura query balance --rpc <host:port> --address <aura1...>
    aura query block   --rpc <host:port> [--height <u64>]
    aura query tx      --rpc <host:port> --txid <0x-hex>
    aura query health  --rpc <host:port>

  node      Node configuration validation and live daemon operations
    aura node config-check --file <config.toml> [--chain-id <id>]
    aura node status       --rpc <host:port>
    aura node health       --rpc <host:port>

  snapshot  Deterministic state snapshot manifest creation and verification
    aura snapshot create --chain-id <id> --height <u64> --state-root <64-hex> [--output <path>]
    aura snapshot verify --chain-id <id> --height <u64> --state-root <64-hex> --expect <64-hex>
    aura snapshot info   --chain-id <id> --height <u64> --state-root <64-hex>

  upgrade   Deterministic protocol upgrade plan generation and verification
    aura upgrade plan   --chain-id <id> --from-version <u32> --to-version <u32> --activation-height <u64> --binary-sha256 <64-hex>
    aura upgrade verify --chain-id <id> --from-version <u32> --to-version <u32> --activation-height <u64> --binary-sha256 <64-hex> --expect <64-hex>

EXIT CODES:
  0  Command succeeded
  1  Verification, execution, or RPC error
  2  Invalid command-line usage or missing required flag
";

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

/// Encode bytes as lowercase hex.
pub fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Decode a hex string (with optional `0x`/`0X` prefix) into bytes.
pub fn decode_hex(raw: &str) -> Result<Vec<u8>, String> {
    let trimmed = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw);
    if !trimmed.len().is_multiple_of(2) {
        return Err("hex string must have an even number of nibbles".to_owned());
    }
    let mut out = Vec::with_capacity(trimmed.len() / 2);
    for pair in trimmed.as_bytes().as_chunks::<2>().0 {
        let hi = nibble(pair[0])?;
        let lo = nibble(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn nibble(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(format!("invalid hex character '{}'", b as char)),
    }
}

fn decode_hex_32(raw: &str, label: &str) -> Result<[u8; 32], String> {
    let bytes = decode_hex(raw)?;
    bytes
        .try_into()
        .map_err(|_| format!("{label} must be 32 bytes (64 hex chars)"))
}

fn decode_hex_64(raw: &str, label: &str) -> Result<[u8; 64], String> {
    let bytes = decode_hex(raw)?;
    bytes
        .try_into()
        .map_err(|_| format!("{label} must be 64 bytes (128 hex chars)"))
}

/// Compute the deterministic D8 snapshot manifest hash over `(chain_id, height, state_root)`.
pub fn compute_snapshot_hash(chain_id: &str, height: u64, state_root: &[u8; 32]) -> Hash {
    let mut preimage = Vec::with_capacity(SNAPSHOT_CLI_DOMAIN.len() + chain_id.len() + 8 + 32);
    preimage.extend_from_slice(SNAPSHOT_CLI_DOMAIN);
    preimage.extend_from_slice(&(chain_id.len() as u32).to_le_bytes());
    preimage.extend_from_slice(chain_id.as_bytes());
    preimage.extend_from_slice(&height.to_le_bytes());
    preimage.extend_from_slice(state_root);
    blake3_hash(&preimage)
}

/// Compute the deterministic D8 protocol upgrade plan hash.
pub fn compute_upgrade_plan_hash(
    chain_id: &str,
    from_version: u32,
    to_version: u32,
    activation_height: u64,
    binary_sha256: &[u8; 32],
) -> Result<Hash, String> {
    if to_version <= from_version {
        return Err(format!(
            "to_version ({to_version}) must be strictly greater than from_version ({from_version})"
        ));
    }
    if activation_height == 0 {
        return Err("activation_height must be > 0".to_owned());
    }
    let mut preimage =
        Vec::with_capacity(UPGRADE_CLI_DOMAIN.len() + chain_id.len() + 4 + 4 + 8 + 32);
    preimage.extend_from_slice(UPGRADE_CLI_DOMAIN);
    preimage.extend_from_slice(&(chain_id.len() as u32).to_le_bytes());
    preimage.extend_from_slice(chain_id.as_bytes());
    preimage.extend_from_slice(&from_version.to_le_bytes());
    preimage.extend_from_slice(&to_version.to_le_bytes());
    preimage.extend_from_slice(&activation_height.to_le_bytes());
    preimage.extend_from_slice(binary_sha256);
    Ok(blake3_hash(&preimage))
}

/// Execute the `aura` CLI with `args` (excluding `argv[0]`), writing stdout to
/// `out` and stderr to `err`, and returning the process exit code.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let _ = write!(err, "{HELP_TEXT}");
        return EXIT_USAGE;
    }
    match args[0].as_str() {
        "--help" | "-h" | "help" => {
            let _ = write!(out, "{HELP_TEXT}");
            EXIT_OK
        }
        "--version" | "-V" | "version" => {
            let _ = writeln!(
                out,
                "aura {} (D8 CLI; protocol v{PROTOCOL_VERSION})",
                env!("CARGO_PKG_VERSION")
            );
            EXIT_OK
        }
        "genesis" => run_genesis(&args[1..], out, err),
        "keys" => run_keys(&args[1..], out, err),
        "tx" => run_tx(&args[1..], out, err),
        "query" => run_query(&args[1..], out, err),
        "node" => run_node(&args[1..], out, err),
        "snapshot" => run_snapshot(&args[1..], out, err),
        "upgrade" => run_upgrade(&args[1..], out, err),
        other => {
            let _ = writeln!(err, "error: unknown subcommand `{other}`\n{HELP_TEXT}");
            EXIT_USAGE
        }
    }
}

fn run_genesis(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let _ = write!(err, "{HELP_TEXT}");
        return EXIT_USAGE;
    }
    if args[0] == "--help" || args[0] == "-h" {
        let _ = write!(out, "{HELP_TEXT}");
        return EXIT_OK;
    }
    let Some(path) = option(args, "--file") else {
        let _ = write!(err, "{HELP_TEXT}");
        return EXIT_USAGE;
    };
    let genesis = match from_json_file(Path::new(&path)) {
        Ok(g) => g,
        Err(e) => {
            let _ = writeln!(err, "genesis: {e}");
            return EXIT_ERROR;
        }
    };
    match args[0].as_str() {
        "hash" => {
            let _ = writeln!(out, "{}", genesis.hash().to_hex());
            EXIT_OK
        }
        "show" => {
            for (k, v) in genesis.summary() {
                let _ = writeln!(out, "{k}={v}");
            }
            EXIT_OK
        }
        "verify" => {
            let Some(expected) = option(args, "--expect") else {
                let _ = write!(err, "{HELP_TEXT}");
                return EXIT_USAGE;
            };
            let actual = genesis.hash().to_hex();
            if actual == expected.to_ascii_lowercase() {
                let _ = writeln!(out, "OK {actual}");
                EXIT_OK
            } else {
                let _ = writeln!(err, "MISMATCH expected {expected} got {actual}");
                EXIT_ERROR
            }
        }
        _ => {
            let _ = write!(err, "{HELP_TEXT}");
            EXIT_USAGE
        }
    }
}

fn run_keys(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let _ = write!(err, "{HELP_TEXT}");
        return EXIT_USAGE;
    }
    if args[0] == "--help" || args[0] == "-h" {
        let _ = write!(out, "{HELP_TEXT}");
        return EXIT_OK;
    }
    match args[0].as_str() {
        "derive" => {
            let Some(seed_hex) = option(args, "--seed") else {
                let _ = writeln!(err, "keys derive: missing --seed <64-hex>");
                return EXIT_USAGE;
            };
            let seed = match decode_hex_32(&seed_hex, "seed") {
                Ok(s) => s,
                Err(e) => {
                    let _ = writeln!(err, "keys derive: {e}");
                    return EXIT_ERROR;
                }
            };
            let pubkey = ed25519_public_key(&seed);
            let address = signer_address(&pubkey);
            let _ = writeln!(out, "public_key={}", encode_hex(&pubkey));
            let _ = writeln!(out, "address={address}");
            EXIT_OK
        }
        "inspect" => {
            let Some(pk_hex) = option(args, "--pubkey") else {
                let _ = writeln!(err, "keys inspect: missing --pubkey <64-hex>");
                return EXIT_USAGE;
            };
            let pubkey = match decode_hex_32(&pk_hex, "pubkey") {
                Ok(p) => p,
                Err(e) => {
                    let _ = writeln!(err, "keys inspect: {e}");
                    return EXIT_ERROR;
                }
            };
            let address = signer_address(&pubkey);
            let _ = writeln!(out, "public_key={}", encode_hex(&pubkey));
            let _ = writeln!(out, "address={address}");
            EXIT_OK
        }
        "sign" => {
            let (Some(seed_hex), Some(chain_id), Some(msg_hex)) = (
                option(args, "--seed"),
                option(args, "--chain-id"),
                option(args, "--message"),
            ) else {
                let _ = writeln!(
                    err,
                    "keys sign: required --seed <64-hex> --chain-id <id> --message <hex>"
                );
                return EXIT_USAGE;
            };
            let seed = match decode_hex_32(&seed_hex, "seed") {
                Ok(s) => s,
                Err(e) => {
                    let _ = writeln!(err, "keys sign: {e}");
                    return EXIT_ERROR;
                }
            };
            let msg = match decode_hex(&msg_hex) {
                Ok(m) => m,
                Err(e) => {
                    let _ = writeln!(err, "keys sign: {e}");
                    return EXIT_ERROR;
                }
            };
            let pubkey = ed25519_public_key(&seed);
            let address = signer_address(&pubkey);
            let sig = sign(&seed, &chain_id, Domain::Tx, &msg);
            let _ = writeln!(out, "public_key={}", encode_hex(&pubkey));
            let _ = writeln!(out, "address={address}");
            let _ = writeln!(out, "signature={}", encode_hex(&sig));
            EXIT_OK
        }
        "verify" => {
            let (Some(pk_hex), Some(chain_id), Some(msg_hex), Some(sig_hex)) = (
                option(args, "--pubkey"),
                option(args, "--chain-id"),
                option(args, "--message"),
                option(args, "--signature"),
            ) else {
                let _ = writeln!(
                    err,
                    "keys verify: required --pubkey <64-hex> --chain-id <id> --message <hex> --signature <128-hex>"
                );
                return EXIT_USAGE;
            };
            let pubkey = match decode_hex_32(&pk_hex, "pubkey") {
                Ok(p) => p,
                Err(e) => {
                    let _ = writeln!(err, "keys verify: {e}");
                    return EXIT_ERROR;
                }
            };
            let msg = match decode_hex(&msg_hex) {
                Ok(m) => m,
                Err(e) => {
                    let _ = writeln!(err, "keys verify: {e}");
                    return EXIT_ERROR;
                }
            };
            let sig_bytes = match decode_hex_64(&sig_hex, "signature") {
                Ok(s) => s,
                Err(e) => {
                    let _ = writeln!(err, "keys verify: {e}");
                    return EXIT_ERROR;
                }
            };
            if verify(&pubkey, &chain_id, Domain::Tx, &msg, &sig_bytes).is_ok() {
                let _ = writeln!(out, "OK");
                EXIT_OK
            } else {
                let _ = writeln!(err, "INVALID SIGNATURE");
                EXIT_ERROR
            }
        }
        _ => {
            let _ = write!(err, "{HELP_TEXT}");
            EXIT_USAGE
        }
    }
}

fn run_tx(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let _ = write!(err, "{HELP_TEXT}");
        return EXIT_USAGE;
    }
    if args[0] == "--help" || args[0] == "-h" {
        let _ = write!(out, "{HELP_TEXT}");
        return EXIT_OK;
    }
    match args[0].as_str() {
        "transfer" => {
            let (Some(seed_hex), Some(chain_id), Some(to_str), Some(amt_str), Some(nonce_str)) = (
                option(args, "--seed"),
                option(args, "--chain-id"),
                option(args, "--to"),
                option(args, "--amount"),
                option(args, "--nonce"),
            ) else {
                let _ = writeln!(
                    err,
                    "tx transfer: required --seed --chain-id --to --amount --nonce"
                );
                return EXIT_USAGE;
            };
            let seed = match decode_hex_32(&seed_hex, "seed") {
                Ok(s) => s,
                Err(e) => {
                    let _ = writeln!(err, "tx transfer: {e}");
                    return EXIT_ERROR;
                }
            };
            let cid = match ChainId::new(&chain_id) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(err, "tx transfer: invalid chain_id: {e}");
                    return EXIT_ERROR;
                }
            };
            let recipient = match Address::from_bech32(&to_str) {
                Ok(a) => a,
                Err(e) => {
                    let _ = writeln!(err, "tx transfer: invalid recipient address: {e}");
                    return EXIT_ERROR;
                }
            };
            let amount: u128 = match amt_str.parse() {
                Ok(v) => v,
                Err(e) => {
                    let _ = writeln!(err, "tx transfer: invalid amount: {e}");
                    return EXIT_ERROR;
                }
            };
            let nonce: u64 = match nonce_str.parse() {
                Ok(v) => v,
                Err(e) => {
                    let _ = writeln!(err, "tx transfer: invalid nonce: {e}");
                    return EXIT_ERROR;
                }
            };
            let gas_limit: u64 = match option(args, "--gas-limit")
                .unwrap_or_else(|| CLI_TRANSFER_GAS.to_string())
                .parse()
            {
                Ok(v) => v,
                Err(e) => {
                    let _ = writeln!(err, "tx transfer: invalid gas-limit: {e}");
                    return EXIT_ERROR;
                }
            };
            if gas_limit < CLI_TRANSFER_GAS {
                let _ = writeln!(
                    err,
                    "tx transfer: gas_limit ({gas_limit}) is below transfer floor ({CLI_TRANSFER_GAS})"
                );
                return EXIT_ERROR;
            }
            let max_fee: u128 = match option(args, "--max-fee")
                .unwrap_or_else(|| "2".to_owned())
                .parse()
            {
                Ok(v) => v,
                Err(e) => {
                    let _ = writeln!(err, "tx transfer: invalid max-fee: {e}");
                    return EXIT_ERROR;
                }
            };
            let tip: u128 = match option(args, "--tip")
                .unwrap_or_else(|| "1".to_owned())
                .parse()
            {
                Ok(v) => v,
                Err(e) => {
                    let _ = writeln!(err, "tx transfer: invalid tip: {e}");
                    return EXIT_ERROR;
                }
            };
            if max_fee < CLI_BASE_FEE_UNITS.saturating_add(tip) {
                let _ = writeln!(
                    err,
                    "tx transfer: max_fee ({max_fee}) must cover base_fee + tip ({})",
                    CLI_BASE_FEE_UNITS.saturating_add(tip)
                );
                return EXIT_ERROR;
            }
            let expiry: u64 = match option(args, "--expiry")
                .unwrap_or_else(|| "1000000".to_owned())
                .parse()
            {
                Ok(v) => v,
                Err(e) => {
                    let _ = writeln!(err, "tx transfer: invalid expiry: {e}");
                    return EXIT_ERROR;
                }
            };

            let pubkey = ed25519_public_key(&seed);
            let signer = signer_address(&pubkey);
            let mut payload = Vec::with_capacity(36);
            payload.extend_from_slice(recipient.as_bytes());
            payload.extend_from_slice(&amount.to_be_bytes());

            let unsigned = match UnsignedTx::new(TxParams {
                chain_id: cid,
                signer,
                nonce: Nonce::new(nonce),
                kind: TxKind::Transfer,
                payload,
                gas_limit: Gas::new(gas_limit),
                max_fee: Amount::from_units(max_fee),
                tip: Amount::from_units(tip),
                expiry: Height::new(expiry),
            }) {
                Ok(u) => u,
                Err(e) => {
                    let _ = writeln!(err, "tx transfer: {e}");
                    return EXIT_ERROR;
                }
            };
            let tx = match unsigned.sign(&seed) {
                Ok(t) => t,
                Err(e) => {
                    let _ = writeln!(err, "tx transfer: {e}");
                    return EXIT_ERROR;
                }
            };
            let raw_hex = format!("0x{}", encode_hex(&tx.to_bytes()));
            let _ = writeln!(out, "txid={}", tx.txid().to_hex());
            let _ = writeln!(out, "signer={signer}");
            let _ = writeln!(out, "recipient={recipient}");
            let _ = writeln!(out, "amount={amount}");
            let _ = writeln!(out, "nonce={nonce}");
            let _ = writeln!(out, "gas_limit={gas_limit}");
            let _ = writeln!(out, "max_fee={max_fee}");
            let _ = writeln!(out, "tip={tip}");
            let _ = writeln!(out, "expiry={expiry}");
            let _ = writeln!(out, "raw_tx={raw_hex}");

            if let Some(rpc_addr) = option(args, "--rpc") {
                match json_rpc_call(&rpc_addr, "submit_tx", json!({ "tx": raw_hex })) {
                    Ok(res) => {
                        let submitted = res.as_str().unwrap_or_default();
                        let _ = writeln!(out, "submitted_txid={submitted}");
                    }
                    Err(e) => {
                        let _ = writeln!(err, "tx transfer rpc submit failed: {e}");
                        return EXIT_ERROR;
                    }
                }
            }
            EXIT_OK
        }
        "decode" => {
            let Some(tx_hex) = option(args, "--tx") else {
                let _ = writeln!(err, "tx decode: missing --tx <0x-hex>");
                return EXIT_USAGE;
            };
            let bytes = match decode_hex(&tx_hex) {
                Ok(b) => b,
                Err(e) => {
                    let _ = writeln!(err, "tx decode: {e}");
                    return EXIT_ERROR;
                }
            };
            let tx = match Tx::from_canonical_bytes(&bytes) {
                Ok(t) => t,
                Err(e) => {
                    let _ = writeln!(err, "tx decode: {e}");
                    return EXIT_ERROR;
                }
            };
            if let Some(chain_str) = option(args, "--chain-id") {
                let cid = match ChainId::new(&chain_str) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = writeln!(err, "tx decode: {e}");
                        return EXIT_ERROR;
                    }
                };
                let height = option(args, "--height")
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                if let Err(e) = tx.validate(cid.as_str(), Height::new(height)) {
                    let _ = writeln!(err, "tx decode validation failed: {e}");
                    return EXIT_ERROR;
                }
            } else if let Err(e) = tx.verify_signature() {
                let _ = writeln!(err, "tx decode signature invalid: {e}");
                return EXIT_ERROR;
            }
            let _ = writeln!(out, "txid={}", tx.txid().to_hex());
            let _ = writeln!(out, "chain_id={}", tx.unsigned.chain_id.as_str());
            let _ = writeln!(out, "signer={}", tx.unsigned.signer);
            let _ = writeln!(out, "nonce={}", tx.unsigned.nonce.value());
            let _ = writeln!(out, "gas_limit={}", tx.unsigned.gas_limit.value());
            let _ = writeln!(out, "max_fee={}", tx.unsigned.max_fee.units());
            let _ = writeln!(out, "tip={}", tx.unsigned.tip.units());
            let _ = writeln!(out, "expiry={}", tx.unsigned.expiry.value());
            if tx.unsigned.kind == TxKind::Transfer && tx.unsigned.payload.len() == 36 {
                let mut addr_bytes = [0u8; 20];
                addr_bytes.copy_from_slice(&tx.unsigned.payload[..20]);
                let mut amt_bytes = [0u8; 16];
                amt_bytes.copy_from_slice(&tx.unsigned.payload[20..]);
                let recipient = Address::from_bytes(addr_bytes);
                let amount = u128::from_be_bytes(amt_bytes);
                let _ = writeln!(out, "recipient={recipient}");
                let _ = writeln!(out, "amount={amount}");
            }
            EXIT_OK
        }
        "hash" => {
            let Some(tx_hex) = option(args, "--tx") else {
                let _ = writeln!(err, "tx hash: missing --tx <0x-hex>");
                return EXIT_USAGE;
            };
            let bytes = match decode_hex(&tx_hex) {
                Ok(b) => b,
                Err(e) => {
                    let _ = writeln!(err, "tx hash: {e}");
                    return EXIT_ERROR;
                }
            };
            let tx = match Tx::from_canonical_bytes(&bytes) {
                Ok(t) => t,
                Err(e) => {
                    let _ = writeln!(err, "tx hash: {e}");
                    return EXIT_ERROR;
                }
            };
            if let Err(e) = tx.verify_signature() {
                let _ = writeln!(err, "tx hash: {e}");
                return EXIT_ERROR;
            }
            let _ = writeln!(out, "{}", tx.txid().to_hex());
            EXIT_OK
        }
        _ => {
            let _ = write!(err, "{HELP_TEXT}");
            EXIT_USAGE
        }
    }
}

fn run_query(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let _ = write!(err, "{HELP_TEXT}");
        return EXIT_USAGE;
    }
    if args[0] == "--help" || args[0] == "-h" {
        let _ = write!(out, "{HELP_TEXT}");
        return EXIT_OK;
    }
    let Some(rpc_addr) = option(args, "--rpc") else {
        let _ = writeln!(err, "query: missing --rpc <host:port>");
        return EXIT_USAGE;
    };
    match args[0].as_str() {
        "balance" => {
            let Some(addr_str) = option(args, "--address") else {
                let _ = writeln!(err, "query balance: missing --address <aura1...>");
                return EXIT_USAGE;
            };
            if let Err(e) = Address::from_bech32(&addr_str) {
                let _ = writeln!(err, "query balance: invalid address: {e}");
                return EXIT_ERROR;
            }
            match json_rpc_call(&rpc_addr, "get_balance", json!({ "address": addr_str })) {
                Ok(val) => {
                    let _ = writeln!(
                        out,
                        "address={}",
                        val["address"].as_str().unwrap_or(&addr_str)
                    );
                    let _ = writeln!(out, "balance={}", val["balance"].as_str().unwrap_or("0"));
                    let _ = writeln!(out, "nonce={}", val["nonce"].as_u64().unwrap_or(0));
                    EXIT_OK
                }
                Err(e) => {
                    let _ = writeln!(err, "query balance: {e}");
                    EXIT_ERROR
                }
            }
        }
        "block" => {
            let params = match option(args, "--height") {
                Some(h_str) => match h_str.parse::<u64>() {
                    Ok(h) => json!({ "height": h }),
                    Err(e) => {
                        let _ = writeln!(err, "query block: invalid height: {e}");
                        return EXIT_ERROR;
                    }
                },
                None => Value::Null,
            };
            match json_rpc_call(&rpc_addr, "get_block", params) {
                Ok(val) if !val.is_null() => {
                    let _ = writeln!(out, "height={}", val["height"].as_u64().unwrap_or(0));
                    let _ = writeln!(out, "hash={}", val["hash"].as_str().unwrap_or(""));
                    let _ = writeln!(
                        out,
                        "state_root={}",
                        val["stateRoot"].as_str().unwrap_or("")
                    );
                    let _ = writeln!(out, "tx_root={}", val["txRoot"].as_str().unwrap_or(""));
                    let _ = writeln!(
                        out,
                        "tx_count={}",
                        val["transactionCount"].as_u64().unwrap_or(0)
                    );
                    EXIT_OK
                }
                Ok(_) => {
                    let _ = writeln!(err, "query block: block not found");
                    EXIT_ERROR
                }
                Err(e) => {
                    let _ = writeln!(err, "query block: {e}");
                    EXIT_ERROR
                }
            }
        }
        "tx" => {
            let Some(txid) = option(args, "--txid") else {
                let _ = writeln!(err, "query tx: missing --txid <0x-hex>");
                return EXIT_USAGE;
            };
            match json_rpc_call(&rpc_addr, "get_tx", json!({ "txid": txid })) {
                Ok(val) if !val.is_null() => {
                    let _ = writeln!(out, "txid={}", val["txid"].as_str().unwrap_or(&txid));
                    let _ = writeln!(
                        out,
                        "status={}",
                        val["status"].as_str().unwrap_or("unknown")
                    );
                    EXIT_OK
                }
                Ok(_) => {
                    let _ = writeln!(err, "query tx: transaction not found");
                    EXIT_ERROR
                }
                Err(e) => {
                    let _ = writeln!(err, "query tx: {e}");
                    EXIT_ERROR
                }
            }
        }
        "health" => match http_get_json(&rpc_addr, "/health") {
            Ok((status_code, val)) => {
                let status = val["status"].as_str().unwrap_or("unknown");
                let chain_id = val["chainId"].as_str().unwrap_or("");
                let height = val["committedHeight"].as_u64().unwrap_or(0);
                let _ = writeln!(out, "status={status}");
                let _ = writeln!(out, "chain_id={chain_id}");
                let _ = writeln!(out, "committed_height={height}");
                if status_code == 200 && status == "healthy" {
                    EXIT_OK
                } else {
                    EXIT_ERROR
                }
            }
            Err(e) => {
                let _ = writeln!(err, "query health: {e}");
                EXIT_ERROR
            }
        },
        _ => {
            let _ = write!(err, "{HELP_TEXT}");
            EXIT_USAGE
        }
    }
}

fn run_node(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let _ = write!(err, "{HELP_TEXT}");
        return EXIT_USAGE;
    }
    if args[0] == "--help" || args[0] == "-h" {
        let _ = write!(out, "{HELP_TEXT}");
        return EXIT_OK;
    }
    match args[0].as_str() {
        "config-check" => {
            let Some(file_path) = option(args, "--file") else {
                let _ = writeln!(err, "node config-check: missing --file <config.toml>");
                return EXIT_USAGE;
            };
            let raw = match std::fs::read_to_string(&file_path) {
                Ok(t) => t,
                Err(e) => {
                    let _ = writeln!(err, "node config-check: {e}");
                    return EXIT_ERROR;
                }
            };
            let table: toml::Table = match toml::from_str(&raw) {
                Ok(t) => t,
                Err(e) => {
                    let _ = writeln!(err, "node config-check: invalid TOML: {e}");
                    return EXIT_ERROR;
                }
            };
            for key in table.keys() {
                if !ALLOWED_NODE_TOML_KEYS.contains(&key.as_str()) {
                    let _ = writeln!(
                        err,
                        "node config-check: unknown configuration field `{key}`"
                    );
                    return EXIT_ERROR;
                }
            }
            let chain_id = option(args, "--chain-id")
                .or_else(|| {
                    table
                        .get("chain_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                })
                .unwrap_or_default();
            if chain_id.trim().is_empty() {
                let _ = writeln!(err, "node config-check: mandatory `chain_id` is missing");
                return EXIT_ERROR;
            }
            let data_dir = table
                .get("data_dir")
                .and_then(|v| v.as_str())
                .unwrap_or(".aura");
            let rpc_addr = table
                .get("rpc_addr")
                .and_then(|v| v.as_str())
                .unwrap_or("127.0.0.1:8545");
            let grpc_addr = table
                .get("grpc_addr")
                .and_then(|v| v.as_str())
                .unwrap_or("127.0.0.1:9090");
            let _ = writeln!(out, "status=valid");
            let _ = writeln!(out, "chain_id={chain_id}");
            let _ = writeln!(out, "data_dir={data_dir}");
            let _ = writeln!(out, "rpc_addr={rpc_addr}");
            let _ = writeln!(out, "grpc_addr={grpc_addr}");
            EXIT_OK
        }
        "status" => {
            let Some(rpc_addr) = option(args, "--rpc") else {
                let _ = writeln!(err, "node status: missing --rpc <host:port>");
                return EXIT_USAGE;
            };
            let health = match http_get_json(&rpc_addr, "/health") {
                Ok((_, v)) => v,
                Err(e) => {
                    let _ = writeln!(err, "node status: {e}");
                    return EXIT_ERROR;
                }
            };
            let block = match json_rpc_call(&rpc_addr, "get_block", Value::Null) {
                Ok(v) => v,
                Err(e) => {
                    let _ = writeln!(err, "node status: {e}");
                    return EXIT_ERROR;
                }
            };
            let _ = writeln!(
                out,
                "status={}",
                health["status"].as_str().unwrap_or("unknown")
            );
            let _ = writeln!(out, "chain_id={}", health["chainId"].as_str().unwrap_or(""));
            let _ = writeln!(
                out,
                "committed_height={}",
                health["committedHeight"].as_u64().unwrap_or(0)
            );
            let _ = writeln!(out, "head_hash={}", block["hash"].as_str().unwrap_or(""));
            let _ = writeln!(
                out,
                "state_root={}",
                block["stateRoot"].as_str().unwrap_or("")
            );
            EXIT_OK
        }
        "health" => run_query(
            &[
                "health".to_owned(),
                "--rpc".to_owned(),
                option(args, "--rpc").unwrap_or_default(),
            ],
            out,
            err,
        ),
        _ => {
            let _ = write!(err, "{HELP_TEXT}");
            EXIT_USAGE
        }
    }
}

fn run_snapshot(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let _ = write!(err, "{HELP_TEXT}");
        return EXIT_USAGE;
    }
    if args[0] == "--help" || args[0] == "-h" {
        let _ = write!(out, "{HELP_TEXT}");
        return EXIT_OK;
    }
    let (Some(chain_id), Some(height_str), Some(root_hex)) = (
        option(args, "--chain-id"),
        option(args, "--height"),
        option(args, "--state-root"),
    ) else {
        let _ = writeln!(
            err,
            "snapshot: required --chain-id <id> --height <u64> --state-root <64-hex>"
        );
        return EXIT_USAGE;
    };
    let height: u64 = match height_str.parse() {
        Ok(h) => h,
        Err(e) => {
            let _ = writeln!(err, "snapshot: invalid height: {e}");
            return EXIT_ERROR;
        }
    };
    let state_root = match decode_hex_32(&root_hex, "state-root") {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(err, "snapshot: {e}");
            return EXIT_ERROR;
        }
    };
    let snap_hash = compute_snapshot_hash(&chain_id, height, &state_root);

    match args[0].as_str() {
        "create" | "info" => {
            if let Some(out_path) = option(args, "--output") {
                let doc = json!({
                    "chain_id": chain_id,
                    "height": height,
                    "state_root": encode_hex(&state_root),
                    "snapshot_hash": snap_hash.to_hex(),
                });
                if let Err(e) = std::fs::write(&out_path, doc.to_string()) {
                    let _ = writeln!(err, "snapshot create: {e}");
                    return EXIT_ERROR;
                }
            }
            let _ = writeln!(out, "chain_id={chain_id}");
            let _ = writeln!(out, "height={height}");
            let _ = writeln!(out, "state_root={}", encode_hex(&state_root));
            let _ = writeln!(out, "snapshot_hash={}", snap_hash.to_hex());
            EXIT_OK
        }
        "verify" => {
            let Some(expect_hex) = option(args, "--expect") else {
                let _ = writeln!(err, "snapshot verify: missing --expect <64-hex>");
                return EXIT_USAGE;
            };
            let expect_bytes = match decode_hex_32(&expect_hex, "expect") {
                Ok(b) => b,
                Err(e) => {
                    let _ = writeln!(err, "snapshot verify: {e}");
                    return EXIT_ERROR;
                }
            };
            if constant_time_eq(snap_hash.as_bytes(), &expect_bytes) {
                let _ = writeln!(out, "OK {}", snap_hash.to_hex());
                EXIT_OK
            } else {
                let _ = writeln!(
                    err,
                    "MISMATCH expected {} got {}",
                    encode_hex(&expect_bytes),
                    snap_hash.to_hex()
                );
                EXIT_ERROR
            }
        }
        _ => {
            let _ = write!(err, "{HELP_TEXT}");
            EXIT_USAGE
        }
    }
}

fn run_upgrade(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let _ = write!(err, "{HELP_TEXT}");
        return EXIT_USAGE;
    }
    if args[0] == "--help" || args[0] == "-h" {
        let _ = write!(out, "{HELP_TEXT}");
        return EXIT_OK;
    }
    let (Some(chain_id), Some(from_str), Some(to_str), Some(act_str), Some(bin_hex)) = (
        option(args, "--chain-id"),
        option(args, "--from-version"),
        option(args, "--to-version"),
        option(args, "--activation-height"),
        option(args, "--binary-sha256"),
    ) else {
        let _ = writeln!(
            err,
            "upgrade: required --chain-id --from-version --to-version --activation-height --binary-sha256"
        );
        return EXIT_USAGE;
    };
    let from_version: u32 = match from_str.parse() {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(err, "upgrade: invalid from-version: {e}");
            return EXIT_ERROR;
        }
    };
    let to_version: u32 = match to_str.parse() {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(err, "upgrade: invalid to-version: {e}");
            return EXIT_ERROR;
        }
    };
    let activation_height: u64 = match act_str.parse() {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(err, "upgrade: invalid activation-height: {e}");
            return EXIT_ERROR;
        }
    };
    let binary_sha256 = match decode_hex_32(&bin_hex, "binary-sha256") {
        Ok(b) => b,
        Err(e) => {
            let _ = writeln!(err, "upgrade: {e}");
            return EXIT_ERROR;
        }
    };
    let plan_hash = match compute_upgrade_plan_hash(
        &chain_id,
        from_version,
        to_version,
        activation_height,
        &binary_sha256,
    ) {
        Ok(h) => h,
        Err(e) => {
            let _ = writeln!(err, "upgrade: {e}");
            return EXIT_ERROR;
        }
    };

    match args[0].as_str() {
        "plan" => {
            let _ = writeln!(out, "chain_id={chain_id}");
            let _ = writeln!(out, "from_version={from_version}");
            let _ = writeln!(out, "to_version={to_version}");
            let _ = writeln!(out, "activation_height={activation_height}");
            let _ = writeln!(out, "binary_sha256={}", encode_hex(&binary_sha256));
            let _ = writeln!(out, "plan_hash={}", plan_hash.to_hex());
            EXIT_OK
        }
        "verify" => {
            let Some(expect_hex) = option(args, "--expect") else {
                let _ = writeln!(err, "upgrade verify: missing --expect <64-hex>");
                return EXIT_USAGE;
            };
            let expect_bytes = match decode_hex_32(&expect_hex, "expect") {
                Ok(b) => b,
                Err(e) => {
                    let _ = writeln!(err, "upgrade verify: {e}");
                    return EXIT_ERROR;
                }
            };
            if constant_time_eq(plan_hash.as_bytes(), &expect_bytes) {
                let _ = writeln!(out, "OK {}", plan_hash.to_hex());
                EXIT_OK
            } else {
                let _ = writeln!(
                    err,
                    "MISMATCH expected {} got {}",
                    encode_hex(&expect_bytes),
                    plan_hash.to_hex()
                );
                EXIT_ERROR
            }
        }
        _ => {
            let _ = write!(err, "{HELP_TEXT}");
            EXIT_USAGE
        }
    }
}

fn json_rpc_call(addr: &str, method: &str, params: Value) -> Result<Value, String> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    })
    .to_string();
    let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let req = format!(
        "POST / HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        payload.len(),
        payload
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw).map_err(|e| e.to_string())?;
    let (_, body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| "malformed HTTP response".to_owned())?;
    let parsed: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    if let Some(error) = parsed.get("error")
        && !error.is_null()
    {
        return Err(error.to_string());
    }
    Ok(parsed["result"].clone())
}

fn http_get_json(addr: &str, path: &str) -> Result<(u16, Value), String> {
    let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw).map_err(|e| e.to_string())?;
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| "malformed HTTP response".to_owned())?;
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| "missing HTTP status code".to_owned())?;
    let parsed: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    Ok((status, parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoke(args: &[&str]) -> (i32, String, String) {
        let owned: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run(&owned, &mut out, &mut err);
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    #[test]
    fn help_documents_all_subcommands_and_flags() {
        let (code, out, err) = invoke(&["--help"]);
        assert_eq!(code, EXIT_OK);
        assert!(err.is_empty());
        for required in [
            "genesis",
            "keys",
            "tx",
            "query",
            "node",
            "snapshot",
            "upgrade",
            "--chain-id",
            "--seed",
            "--pubkey",
            "--rpc",
            "--state-root",
            "--activation-height",
        ] {
            assert!(
                out.contains(required),
                "aura --help missing `{required}`:\n{out}"
            );
        }
    }

    #[test]
    fn keys_verify_rejects_forged_signature() {
        let seed_hex = "11".repeat(32);
        let (_, sign_out, _) = invoke(&[
            "keys",
            "sign",
            "--seed",
            &seed_hex,
            "--chain-id",
            "auralite-devnet-1",
            "--message",
            "deadbeef",
        ]);
        let mut pubkey = String::new();
        let mut sig = String::new();
        for line in sign_out.lines() {
            if let Some(v) = line.strip_prefix("public_key=") {
                pubkey = v.to_owned();
            } else if let Some(v) = line.strip_prefix("signature=") {
                sig = v.to_owned();
            }
        }
        let (ok_code, ok_out, _) = invoke(&[
            "keys",
            "verify",
            "--pubkey",
            &pubkey,
            "--chain-id",
            "auralite-devnet-1",
            "--message",
            "deadbeef",
            "--signature",
            &sig,
        ]);
        assert_eq!(ok_code, EXIT_OK);
        assert_eq!(ok_out.trim(), "OK");

        // Tamper with the signature.
        let mut bad_sig_bytes = decode_hex(&sig).unwrap();
        bad_sig_bytes[0] ^= 0xFF;
        let bad_sig = encode_hex(&bad_sig_bytes);
        let (bad_code, _, bad_err) = invoke(&[
            "keys",
            "verify",
            "--pubkey",
            &pubkey,
            "--chain-id",
            "auralite-devnet-1",
            "--message",
            "deadbeef",
            "--signature",
            &bad_sig,
        ]);
        assert_eq!(bad_code, EXIT_ERROR);
        assert!(bad_err.contains("INVALID SIGNATURE"));
    }

    #[test]
    fn tx_transfer_rejects_below_gas_floor() {
        let seed_hex = "22".repeat(32);
        let recipient = Address::from_bytes([0x55; 20]).to_string();

        let (ok_code, ok_out, _) = invoke(&[
            "tx",
            "transfer",
            "--seed",
            &seed_hex,
            "--chain-id",
            "auralite-devnet-1",
            "--to",
            &recipient,
            "--amount",
            "1000",
            "--nonce",
            "0",
            "--gas-limit",
            "21000",
        ]);
        assert_eq!(ok_code, EXIT_OK);
        assert!(ok_out.contains("raw_tx=0x"));

        let (bad_code, _, bad_err) = invoke(&[
            "tx",
            "transfer",
            "--seed",
            &seed_hex,
            "--chain-id",
            "auralite-devnet-1",
            "--to",
            &recipient,
            "--amount",
            "1000",
            "--nonce",
            "0",
            "--gas-limit",
            "20999",
        ]);
        assert_eq!(bad_code, EXIT_ERROR);
        assert!(bad_err.contains("below transfer floor"));
    }

    #[test]
    fn node_config_check_rejects_unknown_field_and_missing_chain_id() {
        let dir = std::env::temp_dir().join(format!("aura-d8-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let valid_path = dir.join("valid.toml");
        std::fs::write(
            &valid_path,
            "chain_id = \"auralite-devnet-1\"\nrpc_addr = \"127.0.0.1:8545\"\n",
        )
        .unwrap();
        let (ok_code, ok_out, _) = invoke(&[
            "node",
            "config-check",
            "--file",
            valid_path.to_str().unwrap(),
        ]);
        assert_eq!(ok_code, EXIT_OK);
        assert!(ok_out.contains("status=valid"));

        let unknown_path = dir.join("unknown.toml");
        std::fs::write(
            &unknown_path,
            "chain_id = \"auralite-devnet-1\"\nbogus_field = true\n",
        )
        .unwrap();
        let (bad_code, _, bad_err) = invoke(&[
            "node",
            "config-check",
            "--file",
            unknown_path.to_str().unwrap(),
        ]);
        assert_eq!(bad_code, EXIT_ERROR);
        assert!(bad_err.contains("unknown configuration field"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_verify_rejects_tampered_digest() {
        let root_hex = "ab".repeat(32);
        let (code, create_out, _) = invoke(&[
            "snapshot",
            "create",
            "--chain-id",
            "auralite-devnet-1",
            "--height",
            "100",
            "--state-root",
            &root_hex,
        ]);
        assert_eq!(code, EXIT_OK);
        let snap_hash = create_out
            .lines()
            .find_map(|l| l.strip_prefix("snapshot_hash="))
            .unwrap()
            .to_owned();

        let (ok_code, _, _) = invoke(&[
            "snapshot",
            "verify",
            "--chain-id",
            "auralite-devnet-1",
            "--height",
            "100",
            "--state-root",
            &root_hex,
            "--expect",
            &snap_hash,
        ]);
        assert_eq!(ok_code, EXIT_OK);

        let wrong_expect = "cd".repeat(32);
        let (bad_code, _, bad_err) = invoke(&[
            "snapshot",
            "verify",
            "--chain-id",
            "auralite-devnet-1",
            "--height",
            "100",
            "--state-root",
            &root_hex,
            "--expect",
            &wrong_expect,
        ]);
        assert_eq!(bad_code, EXIT_ERROR);
        assert!(bad_err.contains("MISMATCH"));
    }

    #[test]
    fn upgrade_plan_rejects_downgrade_or_same_version() {
        let bin_sha = "ef".repeat(32);
        let (ok_code, ok_out, _) = invoke(&[
            "upgrade",
            "plan",
            "--chain-id",
            "auralite-devnet-1",
            "--from-version",
            "1",
            "--to-version",
            "2",
            "--activation-height",
            "500",
            "--binary-sha256",
            &bin_sha,
        ]);
        assert_eq!(ok_code, EXIT_OK);
        assert!(ok_out.contains("plan_hash="));

        let (bad_code, _, bad_err) = invoke(&[
            "upgrade",
            "plan",
            "--chain-id",
            "auralite-devnet-1",
            "--from-version",
            "2",
            "--to-version",
            "2",
            "--activation-height",
            "500",
            "--binary-sha256",
            &bin_sha,
        ]);
        assert_eq!(bad_code, EXIT_ERROR);
        assert!(bad_err.contains("strictly greater"));
    }
}
