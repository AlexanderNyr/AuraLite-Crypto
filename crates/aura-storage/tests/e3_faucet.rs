//! E3 faucet gate (ECOSYSTEM_PLAN E3): "1000 requests: 1
//! payout/address/day; SDK examples pass in CI" with named
//! negative-control messages. The SDK examples run in
//! tools/e3_examples_check.py; this gate covers the faucet policy.

use aura_storage::faucet::{Faucet, FaucetConfig, FaucetOutcome, Network};
use aura_storage::sdk::AuraLiteClient;
use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

const DAY: u64 = 86_400;

fn faucet(funding: u64) -> Faucet {
    Faucet::new(
        Network::Testnet,
        FaucetConfig {
            payout: 10,
            funding,
        },
    )
}

#[test]
fn e3_gate_1000_requests_pay_one_per_address_per_day() {
    let mut f = faucet(1_000_000);
    let mut paid_per_key: HashMap<(String, u64), u64> = HashMap::new();
    let mut payouts = 0u64;
    let mut rate_denied = 0u64;
    // 250 addresses x 3 days = 750 first-of-day requests, then 250
    // same-day repeats: exactly 1000 requests.
    for day in 0..3u64 {
        for i in 0..250u64 {
            let address = format!("addr-{i:03}");
            let outcome = f.request(day * DAY + 3600, Network::Testnet, &address);
            if let FaucetOutcome::Paid { day: paid_day, .. } = outcome {
                payouts += 1;
                *paid_per_key.entry((address, paid_day)).or_default() += 1;
            }
        }
    }
    for i in 0..250u64 {
        let address = format!("addr-{i:03}");
        let outcome = f.request(3600, Network::Testnet, &address);
        if outcome == FaucetOutcome::DeniedRateLimit {
            rate_denied += 1;
        }
    }
    assert_eq!(
        payouts, 750,
        "E3: faucet must pay 1 payout per address per day"
    );
    assert_eq!(
        rate_denied, 250,
        "E3: faucet must pay 1 payout per address per day"
    );
    assert_eq!(
        paid_per_key.len(),
        750,
        "E3: faucet must pay 1 payout per address per day"
    );
    for (key, count) in &paid_per_key {
        assert_eq!(
            *count, 1,
            "E3: faucet must pay 1 payout per address per day for {key:?}"
        );
    }
    assert_eq!(
        f.payouts(),
        750,
        "E3: faucet must pay 1 payout per address per day"
    );
}

#[test]
fn e3_faucet_refuses_non_testnet_requests() {
    let mut f = faucet(1000);
    assert_eq!(
        f.request(3600, Network::Mainnet, "alice"),
        FaucetOutcome::DeniedWrongNetwork,
        "E3: faucet must refuse non-testnet requests"
    );
    assert_eq!(
        f.request(3600, Network::Testnet, "alice"),
        FaucetOutcome::Paid { day: 0, payout: 10 },
        "E3: faucet must refuse non-testnet requests"
    );
    assert_eq!(
        f.payouts(),
        1,
        "E3: faucet must refuse non-testnet requests"
    );
}

#[test]
fn e3_faucet_stops_when_out_of_funds() {
    let mut f = faucet(25);
    assert_eq!(
        f.request(3600, Network::Testnet, "alice"),
        FaucetOutcome::Paid { day: 0, payout: 10 },
        "E3: faucet must stop when out of funds"
    );
    assert_eq!(
        f.request(3600, Network::Testnet, "bob"),
        FaucetOutcome::Paid { day: 0, payout: 10 },
        "E3: faucet must stop when out of funds"
    );
    assert_eq!(
        f.request(3600, Network::Testnet, "carol"),
        FaucetOutcome::DeniedOutOfFunds,
        "E3: faucet must stop when out of funds"
    );
    assert_eq!(f.funds(), 5, "E3: faucet must stop when out of funds");
}

#[test]
fn e3_daily_window_rolls_over() {
    let mut f = faucet(1000);
    assert_eq!(
        f.request(3600, Network::Testnet, "alice"),
        FaucetOutcome::Paid { day: 0, payout: 10 },
        "E3: the daily window must roll over"
    );
    assert_eq!(
        f.request(DAY + 3600, Network::Testnet, "alice"),
        FaucetOutcome::Paid { day: 1, payout: 10 },
        "E3: the daily window must roll over"
    );
    assert_eq!(
        f.request(2 * DAY + 3600, Network::Testnet, "alice"),
        FaucetOutcome::Paid { day: 2, payout: 10 },
        "E3: the daily window must roll over"
    );
    assert_eq!(f.payouts(), 3, "E3: the daily window must roll over");
}

#[test]
fn e3_faucet_validates_addresses() {
    let mut f = faucet(1000);
    assert_eq!(
        f.request(3600, Network::Testnet, ""),
        FaucetOutcome::DeniedBadAddress,
        "E3: the faucet must validate addresses"
    );
    let oversized = "a".repeat(65);
    assert_eq!(
        f.request(3600, Network::Testnet, &oversized),
        FaucetOutcome::DeniedBadAddress,
        "E3: the faucet must validate addresses"
    );
    assert_eq!(
        f.request(3600, Network::Testnet, "a".repeat(64).as_str()),
        FaucetOutcome::Paid { day: 0, payout: 10 },
        "E3: the faucet must validate addresses"
    );
}

#[test]
fn e3_http_api_applies_the_same_policy_as_the_library() {
    fn post(f: &mut Faucet, address: &str, now: u64) -> (u16, &'static str, String) {
        let body = format!("{{\"now\":{now},\"network\":\"testnet\",\"address\":\"{address}\"}}");
        let request = format!(
            "POST /faucet HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        f.handle(&request)
    }
    let mut f = faucet(1000);
    let (s1, _c1, b1) = post(&mut f, "alice", 3600);
    assert_eq!(
        s1, 200,
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    assert!(
        b1.contains("\"outcome\":\"paid\""),
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    let (s2, _c2, b2) = post(&mut f, "alice", 4000);
    assert_eq!(
        s2, 200,
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    assert!(
        b2.contains("\"outcome\":\"denied_rate_limit\""),
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    let body = "{\"now\":3600,\"network\":\"mainnet\",\"address\":\"bob\"}".to_string();
    let request = format!(
        "POST /faucet HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (s3, _c3, b3) = f.handle(&request);
    assert_eq!(
        s3, 200,
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    assert!(
        b3.contains("\"outcome\":\"denied_wrong_network\""),
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    let (s4, _c4, b4) =
        f.handle("GET /faucet/status HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
    assert_eq!(
        s4, 200,
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    assert!(
        b4.contains("\"payouts\":1"),
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
}

#[test]
fn e3_sdk_client_reaches_the_live_faucet() {
    let shared = Arc::new(Mutex::new(faucet(1000)));
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let serve_faucet = Arc::clone(&shared);
    std::thread::spawn(move || aura_storage::faucet::serve(serve_faucet, listener));

    let client = AuraLiteClient::new(&format!("http://127.0.0.1:{port}"));
    let (s1, b1) = client
        .faucet_request(3600, "testnet", "alice")
        .expect("rpc");
    assert_eq!(
        s1, 200,
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    assert!(
        b1.contains("\"outcome\":\"paid\""),
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    let (s2, b2) = client
        .faucet_request(3700, "testnet", "alice")
        .expect("rpc");
    assert_eq!(
        s2, 200,
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    assert!(
        b2.contains("\"outcome\":\"denied_rate_limit\""),
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    let (s3, b3) = client.faucet_status().expect("rpc");
    assert_eq!(
        s3, 200,
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    assert!(
        b3.contains("\"payouts\":1"),
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
    assert!(
        b3.contains("\"funds\":990"),
        "E3: the faucet HTTP API must apply the same policy as the library"
    );
}
