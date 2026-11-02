//! D7 M1 acceptance gate: 4-validator multi-process `aurad` devnet finalizing
//! at least 100 blocks with identical head and `state_root`, RPC balance query
//! after faucet transfer, and continued finalization when 1 of 4 validators is stopped.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use aura_node::devnet::{
    DEVNET_BLS_SEEDS, DEVNET_CHAIN_ID, DEVNET_FAUCET_INITIAL_BALANCE, DEVNET_VALIDATOR_COUNT,
    build_faucet_transfer, faucet_address, trigger_and_wait_devnet_round,
};
use aura_node::pipeline::{D2_BASE_FEE, TRANSFER_GAS};
use aura_types::{Address, Amount};
use serde_json::{Value, json};

fn temp_devnet_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("aura-d7-m1-devnet-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create temp devnet dir");
    path
}

fn reserve_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

fn http_get(addr: &str, path: &str) -> Result<(u16, String), String> {
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
        .ok_or("malformed HTTP response")?;
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or("missing status code")?;
    Ok((status, body.to_owned()))
}

fn rpc_call(addr: &str, method: &str, params: Value) -> Value {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    })
    .to_string();
    let mut stream = TcpStream::connect(addr).expect("connect to validator RPC");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    let req = format!(
        "POST / HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        payload.len(),
        payload
    );
    stream.write_all(req.as_bytes()).expect("write RPC request");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("read RPC response");
    let (_, body) = raw.split_once("\r\n\r\n").expect("HTTP body separator");
    let parsed: Value = serde_json::from_str(body).expect("valid JSON-RPC response");
    if let Some(err) = parsed.get("error") {
        panic!("RPC {method} on {addr} returned error: {err}");
    }
    parsed["result"].clone()
}

struct RunningValidator {
    child: Child,
    rpc_addr: String,
    log_path: PathBuf,
}

impl RunningValidator {
    fn stop(&mut self) {
        if let Ok(Some(_)) = self.child.try_wait() {
            return;
        }
        let pid = self.child.id();
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for RunningValidator {
    fn drop(&mut self) {
        self.stop();
    }
}

#[test]
fn m1_four_validator_devnet_finalizes_100_blocks_serves_rpc_balance_and_survives_outage() {
    let devnet_dir = temp_devnet_dir();

    let aurad_bin = env!("CARGO_BIN_EXE_aurad");
    let mut validators: Vec<RunningValidator> = Vec::with_capacity(DEVNET_VALIDATOR_COUNT);

    for (idx, seed) in DEVNET_BLS_SEEDS.iter().enumerate() {
        let node_dir = devnet_dir.join(format!("validator-{idx}"));
        fs::create_dir_all(&node_dir).expect("create validator dir");
        let key_path = node_dir.join("validator.key");
        fs::write(&key_path, seed).expect("write validator key");

        let rpc_addr = format!("127.0.0.1:{}", reserve_port());
        let grpc_addr = format!("127.0.0.1:{}", reserve_port());
        let log_path = devnet_dir.join(format!("validator-{idx}.log"));
        let log_file = File::create(&log_path).expect("create log file");
        let log_err = log_file.try_clone().expect("clone log file");

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
            .expect("spawn aurad validator process");

        validators.push(RunningValidator {
            child,
            rpc_addr,
            log_path,
        });
    }

    // 1. Wait for all 4 independent aurad processes to report HTTP 200 on /health and /metrics.
    let deadline = Instant::now() + Duration::from_secs(15);
    for (idx, v) in validators.iter().enumerate() {
        loop {
            if let Ok((200, health_body)) = http_get(&v.rpc_addr, "/health")
                && let Ok((200, metrics_body)) = http_get(&v.rpc_addr, "/metrics")
            {
                assert!(health_body.contains("\"status\":\"healthy\""));
                assert!(metrics_body.contains("aurad_committed_height"));
                assert!(metrics_body.contains("aurad_peer_count 0"));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "validator-{idx} did not become healthy in time"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    // 2. Verify initial faucet balance over JSON-RPC on all 4 validators.
    let faucet = faucet_address();
    for v in &validators {
        let bal = rpc_call(
            &v.rpc_addr,
            "get_balance",
            json!({ "address": faucet.to_string() }),
        );
        assert_eq!(
            bal["balance"].as_str(),
            Some(DEVNET_FAUCET_INITIAL_BALANCE.to_string().as_str())
        );
    }

    // 3. Submit a faucet transfer to validator-0 via JSON-RPC.
    let recipient = Address::from_bytes([0xAB; 20]);
    let transfer_amount = 250_000u128;
    let tx1 = build_faucet_transfer(
        DEVNET_CHAIN_ID,
        recipient,
        Amount::from_units(transfer_amount),
        0,
    )
    .expect("build tx1");
    let tx1_hex = format!(
        "0x{}",
        tx1.to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let submit_res = rpc_call(
        &validators[0].rpc_addr,
        "submit_tx",
        json!({ "tx": tx1_hex }),
    );
    assert!(submit_res.as_str().unwrap().starts_with("0x"));

    // Allow the devnet tx gossip loop a brief moment to propagate tx1 to all 4 processes.
    std::thread::sleep(Duration::from_millis(40));

    // 4. Drive 103 consensus rounds across all 4 validators so committed height >= 100.
    let all_four = [0usize, 1, 2, 3];
    for round in 1..=103u64 {
        trigger_and_wait_devnet_round(&devnet_dir, round, &all_four, Duration::from_secs(10))
            .expect("4-validator round succeeds");
    }

    // 5. Verify all 4 nodes report the exact same finalized height (>= 100), block hash,
    //    state root, and recipient balance over JSON-RPC.
    let head0 = rpc_call(&validators[0].rpc_addr, "get_block", Value::Null);
    let height0 = head0["height"].as_u64().expect("height u64");
    let hash0 = head0["hash"].as_str().expect("block hash").to_owned();
    let root0 = head0["stateRoot"].as_str().expect("state root").to_owned();
    assert!(
        height0 >= 100,
        "expected finalized height >= 100, got {height0}"
    );

    let fee_per_tx = (D2_BASE_FEE.units() + 1) * (TRANSFER_GAS as u128);
    let expected_faucet_after_tx1 = DEVNET_FAUCET_INITIAL_BALANCE - transfer_amount - fee_per_tx;

    for (idx, v) in validators.iter().enumerate() {
        let head = rpc_call(&v.rpc_addr, "get_block", Value::Null);
        assert_eq!(
            head["height"].as_u64(),
            Some(height0),
            "validator-{idx} height"
        );
        assert_eq!(
            head["hash"].as_str(),
            Some(hash0.as_str()),
            "validator-{idx} hash"
        );
        assert_eq!(
            head["stateRoot"].as_str(),
            Some(root0.as_str()),
            "validator-{idx} stateRoot"
        );

        let rec_bal = rpc_call(
            &v.rpc_addr,
            "get_balance",
            json!({ "address": recipient.to_string() }),
        );
        assert_eq!(
            rec_bal["balance"].as_str(),
            Some(transfer_amount.to_string().as_str()),
            "validator-{idx} recipient balance"
        );

        let f_bal = rpc_call(
            &v.rpc_addr,
            "get_balance",
            json!({ "address": faucet.to_string() }),
        );
        assert_eq!(
            f_bal["balance"].as_str(),
            Some(expected_faucet_after_tx1.to_string().as_str()),
            "validator-{idx} faucet balance"
        );
    }

    // 6. Stop validator-3 (1 of 4 validators down) and verify the remaining 3 validators
    //    preserve the f=1 quorum of 3 and continue finalizing blocks and transfers.
    validators[3].stop();

    let tx2_amount = 125_000u128;
    let tx2 = build_faucet_transfer(
        DEVNET_CHAIN_ID,
        recipient,
        Amount::from_units(tx2_amount),
        1,
    )
    .expect("build tx2");
    let tx2_hex = format!(
        "0x{}",
        tx2.to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let _ = rpc_call(
        &validators[0].rpc_addr,
        "submit_tx",
        json!({ "tx": tx2_hex }),
    );
    std::thread::sleep(Duration::from_millis(40));

    let remaining_three = [0usize, 1, 2];
    for round in 104..=115u64 {
        trigger_and_wait_devnet_round(
            &devnet_dir,
            round,
            &remaining_three,
            Duration::from_secs(10),
        )
        .expect("3-of-4 quorum round succeeds with validator-3 down");
    }

    let post_head0 = rpc_call(&validators[0].rpc_addr, "get_block", Value::Null);
    let post_height0 = post_head0["height"].as_u64().expect("post-outage height");
    let post_hash0 = post_head0["hash"]
        .as_str()
        .expect("post-outage hash")
        .to_owned();
    let post_root0 = post_head0["stateRoot"]
        .as_str()
        .expect("post-outage stateRoot")
        .to_owned();
    assert!(
        post_height0 >= height0 + 10,
        "expected height to advance during 1-validator outage: {height0} -> {post_height0}"
    );

    let expected_recipient_total = transfer_amount + tx2_amount;
    for v in validators.iter().take(3) {
        let head = rpc_call(&v.rpc_addr, "get_block", Value::Null);
        assert_eq!(head["height"].as_u64(), Some(post_height0));
        assert_eq!(head["hash"].as_str(), Some(post_hash0.as_str()));
        assert_eq!(head["stateRoot"].as_str(), Some(post_root0.as_str()));

        let rec_bal = rpc_call(
            &v.rpc_addr,
            "get_balance",
            json!({ "address": recipient.to_string() }),
        );
        assert_eq!(
            rec_bal["balance"].as_str(),
            Some(expected_recipient_total.to_string().as_str())
        );
    }

    // 7. Stop remaining validators cleanly and verify per-node JSON logs.
    for v in &mut validators[0..3] {
        v.stop();
    }
    for (idx, v) in validators.iter().enumerate() {
        let log_text = fs::read_to_string(&v.log_path).expect("read validator log");
        assert!(
            log_text.contains("\"message\":\"devnet round finalized with 2f+1 QC\""),
            "validator-{idx} log missing finalized round entry"
        );
        assert!(
            log_text.contains(&root0),
            "validator-{idx} log missing common state_root {root0}"
        );
    }
    let _ = fs::remove_dir_all(&devnet_dir);
}
