//! D8 gate: 30 golden `aura` CLI vectors, full `--help` documentation check,
//! and live `aurad` node/query/tx operations.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use aura_node::cli::{self, EXIT_ERROR, EXIT_OK};
use aura_node::devnet::{
    DEVNET_BLS_SEEDS, DEVNET_CHAIN_ID, DEVNET_FAUCET_SEED, faucet_address,
    trigger_and_wait_devnet_round,
};
use aura_types::Address;

const FIXTURE: &str = include_str!("fixtures/d8_golden.txt");

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn invoke(args: &[&str]) -> (i32, String, String) {
    let owned: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = cli::run(&owned, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn parse_kv(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            map.insert(k.trim().to_owned(), v.trim().to_owned());
        }
    }
    map
}

fn parse_vectors(text: &str) -> (String, Vec<BTreeMap<String, String>>) {
    let mut chain_id = String::new();
    let mut vectors: Vec<BTreeMap<String, String>> = Vec::new();
    let mut current: Option<BTreeMap<String, String>> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (k, v) = trimmed.split_once('=').expect("key=value line");
        if k == "chain_id" && current.is_none() {
            chain_id = v.to_owned();
            continue;
        }
        if k == "vector" {
            if let Some(prev) = current.take() {
                vectors.push(prev);
            }
            let mut next = BTreeMap::new();
            next.insert("vector".to_owned(), v.to_owned());
            current = Some(next);
        } else if let Some(ref mut map) = current {
            map.insert(k.to_owned(), v.to_owned());
        }
    }
    if let Some(last) = current {
        vectors.push(last);
    }
    (chain_id, vectors)
}

#[test]
fn thirty_golden_cli_vectors_verify_end_to_end() {
    let (chain_id, vectors) = parse_vectors(FIXTURE);
    assert_eq!(vectors.len(), 30, "D8 gate requires 30 CLI vectors");
    let root = repo_root();

    for vec_map in &vectors {
        let label = &vec_map["vector"];
        match vec_map["kind"].as_str() {
            "keys_sign" => {
                let seed = &vec_map["seed"];
                let msg = &vec_map["message"];
                let (code, derive_out, _) = invoke(&["keys", "derive", "--seed", seed]);
                assert_eq!(code, EXIT_OK, "{label}");
                let d_kv = parse_kv(&derive_out);
                assert_eq!(&d_kv["public_key"], &vec_map["public_key"], "{label}");
                assert_eq!(&d_kv["address"], &vec_map["address"], "{label}");

                let (code, inspect_out, _) =
                    invoke(&["keys", "inspect", "--pubkey", &vec_map["public_key"]]);
                assert_eq!(code, EXIT_OK, "{label}");
                let i_kv = parse_kv(&inspect_out);
                assert_eq!(&i_kv["address"], &vec_map["address"], "{label}");

                let (code, sign_out, _) = invoke(&[
                    "keys",
                    "sign",
                    "--seed",
                    seed,
                    "--chain-id",
                    &chain_id,
                    "--message",
                    msg,
                ]);
                assert_eq!(code, EXIT_OK, "{label}");
                let s_kv = parse_kv(&sign_out);
                assert_eq!(&s_kv["signature"], &vec_map["signature"], "{label}");

                let (code, ver_out, _) = invoke(&[
                    "keys",
                    "verify",
                    "--pubkey",
                    &vec_map["public_key"],
                    "--chain-id",
                    &chain_id,
                    "--message",
                    msg,
                    "--signature",
                    &vec_map["signature"],
                ]);
                assert_eq!(code, EXIT_OK, "{label}");
                assert_eq!(ver_out.trim(), "OK", "{label}");
            }
            "tx_transfer" => {
                let (code, tx_out, _) = invoke(&[
                    "tx",
                    "transfer",
                    "--seed",
                    &vec_map["seed"],
                    "--chain-id",
                    &chain_id,
                    "--to",
                    &vec_map["recipient"],
                    "--amount",
                    &vec_map["amount"],
                    "--nonce",
                    &vec_map["nonce"],
                    "--gas-limit",
                    &vec_map["gas_limit"],
                    "--max-fee",
                    &vec_map["max_fee"],
                    "--tip",
                    &vec_map["tip"],
                    "--expiry",
                    &vec_map["expiry"],
                ]);
                assert_eq!(code, EXIT_OK, "{label}");
                let kv = parse_kv(&tx_out);
                assert_eq!(&kv["txid"], &vec_map["txid"], "{label}");
                assert_eq!(&kv["signer"], &vec_map["signer"], "{label}");
                assert_eq!(&kv["raw_tx"], &vec_map["raw_tx"], "{label}");

                let (code, dec_out, _) = invoke(&[
                    "tx",
                    "decode",
                    "--tx",
                    &vec_map["raw_tx"],
                    "--chain-id",
                    &chain_id,
                    "--height",
                    "1",
                ]);
                assert_eq!(code, EXIT_OK, "{label}");
                let d_kv = parse_kv(&dec_out);
                assert_eq!(&d_kv["txid"], &vec_map["txid"], "{label}");
                assert_eq!(&d_kv["recipient"], &vec_map["recipient"], "{label}");
                assert_eq!(&d_kv["amount"], &vec_map["amount"], "{label}");

                let (code, hash_out, _) = invoke(&["tx", "hash", "--tx", &vec_map["raw_tx"]]);
                assert_eq!(code, EXIT_OK, "{label}");
                assert_eq!(hash_out.trim(), vec_map["txid"], "{label}");
            }
            "genesis_hash" => {
                let file_path = root.join(&vec_map["file"]);
                let file_str = file_path.to_str().unwrap();
                let (code, out, _) = invoke(&["genesis", "hash", "--file", file_str]);
                assert_eq!(code, EXIT_OK, "{label}");
                assert_eq!(out.trim(), vec_map["genesis_hash"], "{label}");

                let (code, ver_out, _) = invoke(&[
                    "genesis",
                    "verify",
                    "--file",
                    file_str,
                    "--expect",
                    &vec_map["genesis_hash"],
                ]);
                assert_eq!(code, EXIT_OK, "{label}");
                assert!(ver_out.starts_with("OK "), "{label}");
            }
            "genesis_show" => {
                let file_path = root.join(&vec_map["file"]);
                let file_str = file_path.to_str().unwrap();
                let (code, out, _) = invoke(&["genesis", "show", "--file", file_str]);
                assert_eq!(code, EXIT_OK, "{label}");
                let kv = parse_kv(&out);
                assert_eq!(&kv["genesis_hash"], &vec_map["genesis_hash"], "{label}");
                assert_eq!(
                    &kv["validator_set_hash"], &vec_map["validator_set_hash"],
                    "{label}"
                );
                assert_eq!(
                    &kv["allocation_root"], &vec_map["allocation_root"],
                    "{label}"
                );
                assert_eq!(&kv["total_supply"], &vec_map["total_supply"], "{label}");
            }
            "snapshot" => {
                let (code, out, _) = invoke(&[
                    "snapshot",
                    "create",
                    "--chain-id",
                    &chain_id,
                    "--height",
                    &vec_map["height"],
                    "--state-root",
                    &vec_map["state_root"],
                ]);
                assert_eq!(code, EXIT_OK, "{label}");
                let kv = parse_kv(&out);
                assert_eq!(&kv["snapshot_hash"], &vec_map["snapshot_hash"], "{label}");

                let (code, ver_out, _) = invoke(&[
                    "snapshot",
                    "verify",
                    "--chain-id",
                    &chain_id,
                    "--height",
                    &vec_map["height"],
                    "--state-root",
                    &vec_map["state_root"],
                    "--expect",
                    &vec_map["snapshot_hash"],
                ]);
                assert_eq!(code, EXIT_OK, "{label}");
                assert!(ver_out.starts_with("OK "), "{label}");
            }
            "upgrade" => {
                let (code, out, _) = invoke(&[
                    "upgrade",
                    "plan",
                    "--chain-id",
                    &chain_id,
                    "--from-version",
                    &vec_map["from_version"],
                    "--to-version",
                    &vec_map["to_version"],
                    "--activation-height",
                    &vec_map["activation_height"],
                    "--binary-sha256",
                    &vec_map["binary_sha256"],
                ]);
                assert_eq!(code, EXIT_OK, "{label}");
                let kv = parse_kv(&out);
                assert_eq!(&kv["plan_hash"], &vec_map["plan_hash"], "{label}");

                let (code, ver_out, _) = invoke(&[
                    "upgrade",
                    "verify",
                    "--chain-id",
                    &chain_id,
                    "--from-version",
                    &vec_map["from_version"],
                    "--to-version",
                    &vec_map["to_version"],
                    "--activation-height",
                    &vec_map["activation_height"],
                    "--binary-sha256",
                    &vec_map["binary_sha256"],
                    "--expect",
                    &vec_map["plan_hash"],
                ]);
                assert_eq!(code, EXIT_OK, "{label}");
                assert!(ver_out.starts_with("OK "), "{label}");
            }
            other => panic!("unexpected vector kind `{other}`"),
        }
    }
}

fn reserve_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

struct GuardedChildren(Vec<Child>, PathBuf);

impl Drop for GuardedChildren {
    fn drop(&mut self) {
        for child in &mut self.0 {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        let _ = fs::remove_dir_all(&self.1);
    }
}

#[test]
fn cli_queries_and_submits_against_live_aurad_devnet() {
    let devnet_dir = std::env::temp_dir().join(format!("aura-d8-live-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&devnet_dir);
    fs::create_dir_all(&devnet_dir).unwrap();

    let aurad_bin = env!("CARGO_BIN_EXE_aurad");
    let mut children = Vec::new();
    let mut rpc_addrs = Vec::new();

    // Start 3 validators (sufficient for 3-of-4 devnet quorum).
    for (idx, seed) in DEVNET_BLS_SEEDS.iter().enumerate().take(3) {
        let node_dir = devnet_dir.join(format!("validator-{idx}"));
        fs::create_dir_all(&node_dir).unwrap();
        let key_path = node_dir.join("validator.key");
        fs::write(&key_path, seed).unwrap();

        let rpc_addr = format!("127.0.0.1:{}", reserve_port());
        let grpc_addr = format!("127.0.0.1:{}", reserve_port());
        let log_file = File::create(devnet_dir.join(format!("v-{idx}.log"))).unwrap();
        let log_err = log_file.try_clone().unwrap();

        let child = Command::new(aurad_bin)
            .args([
                "--chain-id",
                DEVNET_CHAIN_ID,
                "--data-dir",
                node_dir.to_str().unwrap(),
                "--rpc-addr",
                &rpc_addr,
                "--grpc-addr",
                &grpc_addr,
                "--validator-key",
                key_path.to_str().unwrap(),
            ])
            .env("AURA_DEVNET_DIR", &devnet_dir)
            .env("AURA_VALIDATOR_INDEX", idx.to_string())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_err))
            .spawn()
            .unwrap();
        children.push(child);
        rpc_addrs.push(rpc_addr);
    }
    let _guard = GuardedChildren(children, devnet_dir.clone());

    let deadline = Instant::now() + Duration::from_secs(15);
    for addr in &rpc_addrs {
        loop {
            let (code, _, _) = invoke(&["node", "health", "--rpc", addr]);
            if code == EXIT_OK {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "validator {addr} did not become healthy"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    // 1. Submit a transfer via `aura tx transfer --rpc`
    let faucet_seed_hex = cli::encode_hex(&DEVNET_FAUCET_SEED);
    let recipient = Address::from_bytes([0xCC; 20]).to_string();
    let (code, submit_out, submit_err) = invoke(&[
        "tx",
        "transfer",
        "--seed",
        &faucet_seed_hex,
        "--chain-id",
        DEVNET_CHAIN_ID,
        "--to",
        &recipient,
        "--amount",
        "77700",
        "--nonce",
        "0",
        "--rpc",
        &rpc_addrs[0],
    ]);
    assert_eq!(code, EXIT_OK, "submit failed: {submit_err}");
    let submit_kv = parse_kv(&submit_out);
    let txid_hex = format!("0x{}", submit_kv["txid"]);
    assert_eq!(submit_kv["submitted_txid"], txid_hex);

    std::thread::sleep(Duration::from_millis(40));
    for r in 1..=4 {
        trigger_and_wait_devnet_round(&devnet_dir, r, &[0, 1, 2], Duration::from_secs(10)).unwrap();
    }

    // 2. Query recipient balance, faucet balance, tx status, block, and node status via `aura`.
    let (code, bal_out, _) = invoke(&[
        "query",
        "balance",
        "--rpc",
        &rpc_addrs[0],
        "--address",
        &recipient,
    ]);
    assert_eq!(code, EXIT_OK);
    let bal_kv = parse_kv(&bal_out);
    assert_eq!(bal_kv["balance"], "77700");

    let (code, f_out, _) = invoke(&[
        "query",
        "balance",
        "--rpc",
        &rpc_addrs[0],
        "--address",
        &faucet_address().to_string(),
    ]);
    assert_eq!(code, EXIT_OK);
    assert_eq!(parse_kv(&f_out)["nonce"], "1");

    let (code, tx_out, _) = invoke(&["query", "tx", "--rpc", &rpc_addrs[0], "--txid", &txid_hex]);
    assert_eq!(code, EXIT_OK);
    assert_eq!(parse_kv(&tx_out)["status"], "committed");

    let (code, blk_out, _) = invoke(&["query", "block", "--rpc", &rpc_addrs[0]]);
    assert_eq!(code, EXIT_OK);
    let blk_kv = parse_kv(&blk_out);
    assert!(blk_kv["height"].parse::<u64>().unwrap() >= 2);

    let (code, st_out, _) = invoke(&["node", "status", "--rpc", &rpc_addrs[0]]);
    assert_eq!(code, EXIT_OK);
    let st_kv = parse_kv(&st_out);
    assert_eq!(st_kv["status"], "healthy");
    assert_eq!(st_kv["state_root"], blk_kv["state_root"]);

    // Negative query check: invalid address is rejected with EXIT_ERROR.
    let (bad_code, _, _) = invoke(&[
        "query",
        "balance",
        "--rpc",
        &rpc_addrs[0],
        "--address",
        "not-a-bech32-address",
    ]);
    assert_eq!(bad_code, EXIT_ERROR);
}
