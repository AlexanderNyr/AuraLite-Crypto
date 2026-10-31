//! D6 live health and Prometheus endpoint contract.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aura_grpc_rest::RestGateway;
use aura_node::observability::{NodeHttpExtension, NodeMetrics};
use aura_node::pipeline::{Pipeline, PipelineConfig};
use aura_node::rpc::PipelineRpcBackend;
use aura_rpc::{RpcServerHandle, RpcService};

static SERIAL: AtomicU64 = AtomicU64::new(0);

fn temporary_path(label: &str) -> PathBuf {
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("aura-d6-{label}-{}-{serial}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn http_get(address: std::net::SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn start_http_server() -> (RpcServerHandle, PathBuf) {
    let root = temporary_path("node");
    let pid_file = root.join("aurad.pid");
    fs::write(&pid_file, format!("{}\n", std::process::id())).unwrap();
    let pipeline = Pipeline::open(PipelineConfig {
        chain_id: "auralite-test-1".to_string(),
        data_dir: root.join("state"),
        validator_seed: [0xD6; 32],
        allocations: Vec::new(),
        genesis_time_ms: 1_700_000_000_000,
    })
    .unwrap();
    let pipeline = Arc::new(Mutex::new(pipeline));
    let backend = Arc::new(PipelineRpcBackend::new(Arc::clone(&pipeline)));
    let service = RpcService::new(backend);
    let rest = Arc::new(RestGateway::new(service.clone()));
    let metrics = Arc::new(NodeMetrics::new(false));
    let extension = Arc::new(NodeHttpExtension::new(
        rest,
        pipeline,
        metrics,
        pid_file.clone(),
        "auralite-test-1".to_string(),
    ));
    let server =
        RpcServerHandle::spawn_with_extension("127.0.0.1:0", service, Some(extension)).unwrap();
    (server, pid_file)
}

#[test]
fn health_endpoint_reports_ready_state_and_turns_unhealthy_without_pid() {
    let (mut server, pid_file) = start_http_server();
    let address = server.local_addr();
    let response = http_get(address, "/health");
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    let body = response.split_once("\r\n\r\n").unwrap().1;
    let health: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(health["status"], "healthy");
    assert_eq!(health["chainId"], "auralite-test-1");
    assert_eq!(health["peerCount"], 0);
    assert_eq!(health["p2p"], "not_implemented");

    fs::remove_file(&pid_file).unwrap();
    let response = http_get(address, "/health");
    assert!(
        response.starts_with("HTTP/1.1 503 Service Unavailable\r\n"),
        "{response}"
    );
    server.shutdown();
    let _ = fs::remove_dir_all(pid_file.parent().unwrap());
}

#[test]
fn metrics_endpoint_exposes_prometheus_dashboard_contract() {
    let (mut server, pid_file) = start_http_server();
    let response = http_get(server.local_addr(), "/metrics");
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.contains("Content-Type: text/plain; version=0.0.4; charset=utf-8"));
    let body = response.split_once("\r\n\r\n").unwrap().1;
    for metric in [
        "aurad_committed_height",
        "aurad_finality_lag_blocks",
        "aurad_mempool_size",
        "aurad_peer_count",
        "aurad_gas_used_per_second",
    ] {
        assert!(
            body.contains(&format!("# TYPE {metric}")),
            "missing {metric}"
        );
    }
    assert!(body.contains("aurad_peer_count 0\n"));
    assert!(body.contains("aurad_p2p_available 0\n"));
    server.shutdown();
    let _ = fs::remove_dir_all(pid_file.parent().unwrap());
}
