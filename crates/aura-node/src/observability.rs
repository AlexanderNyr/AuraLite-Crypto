//! D6 operational metrics and health routes.
//!
//! Prometheus exposition is rendered from low-cardinality process metrics and
//! a short-lived pipeline snapshot. Peer count remains zero until N1 P2P lands;
//! the endpoint labels that capability honestly rather than implying peers
//! exist.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aura_grpc_rest::RestGateway;
use aura_rpc::{HttpExtension, HttpExtensionResponse};

use crate::pipeline::Pipeline;

const GAS_WINDOW: Duration = Duration::from_secs(60);

/// Process-local node metrics. Gauges are refreshed from the pipeline on scrape;
/// counters and the rolling gas rate are updated when blocks are durably committed.
pub struct NodeMetrics {
    started_at_seconds: u64,
    committed_height: AtomicU64,
    finality_lag_blocks: AtomicU64,
    mempool_size: AtomicU64,
    mempool_capacity: AtomicU64,
    gas_used_total: AtomicU64,
    blocks_committed_total: AtomicU64,
    transactions_committed_total: AtomicU64,
    gas_samples: Mutex<VecDeque<(Instant, u64)>>,
    healthy: AtomicBool,
    otel_exporter_enabled: AtomicBool,
}

impl NodeMetrics {
    /// Create the registry for one daemon process.
    pub fn new(otel_exporter_enabled: bool) -> Self {
        Self {
            started_at_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            committed_height: AtomicU64::new(0),
            finality_lag_blocks: AtomicU64::new(0),
            mempool_size: AtomicU64::new(0),
            mempool_capacity: AtomicU64::new(0),
            gas_used_total: AtomicU64::new(0),
            blocks_committed_total: AtomicU64::new(0),
            transactions_committed_total: AtomicU64::new(0),
            gas_samples: Mutex::new(VecDeque::new()),
            healthy: AtomicBool::new(false),
            otel_exporter_enabled: AtomicBool::new(otel_exporter_enabled),
        }
    }

    fn refresh_from_pipeline(&self, pipeline: &Pipeline) {
        self.committed_height
            .store(pipeline.committed_height(), Ordering::Relaxed);
        self.finality_lag_blocks
            .store(pipeline.finality_lag_blocks(), Ordering::Relaxed);
        self.mempool_size
            .store(pipeline.mempool_len() as u64, Ordering::Relaxed);
        self.mempool_capacity
            .store(pipeline.mempool_capacity() as u64, Ordering::Relaxed);
    }

    /// Record one durably committed block and its executed gas/transaction count.
    pub fn record_committed_block(&self, gas_used: u64, transaction_count: usize) {
        saturating_add(&self.gas_used_total, gas_used);
        saturating_add(&self.blocks_committed_total, 1);
        saturating_add(&self.transactions_committed_total, transaction_count as u64);
        if let Ok(mut samples) = self.gas_samples.lock() {
            let now = Instant::now();
            samples.push_back((now, gas_used));
            prune_samples(&mut samples, now);
        }
    }

    fn gas_used_per_second(&self) -> f64 {
        let Ok(mut samples) = self.gas_samples.lock() else {
            return 0.0;
        };
        let now = Instant::now();
        prune_samples(&mut samples, now);
        let Some((first, _)) = samples.front() else {
            return 0.0;
        };
        let total = samples
            .iter()
            .fold(0u64, |sum, (_, value)| sum.saturating_add(*value));
        let elapsed = now.duration_since(*first).as_secs_f64().max(1.0);
        total as f64 / elapsed
    }

    /// Render Prometheus text exposition 0.0.4, suitable for a scrape endpoint.
    pub fn render_prometheus(&self) -> String {
        let height = self.committed_height.load(Ordering::Relaxed);
        let lag = self.finality_lag_blocks.load(Ordering::Relaxed);
        let pending = self.mempool_size.load(Ordering::Relaxed);
        let capacity = self.mempool_capacity.load(Ordering::Relaxed);
        let gas_total = self.gas_used_total.load(Ordering::Relaxed);
        let blocks = self.blocks_committed_total.load(Ordering::Relaxed);
        let transactions = self.transactions_committed_total.load(Ordering::Relaxed);
        let up = u8::from(self.healthy.load(Ordering::Relaxed));
        let otel = u8::from(self.otel_exporter_enabled.load(Ordering::Relaxed));
        let rate = self.gas_used_per_second();
        let mut out = String::new();
        sample(
            &mut out,
            "aurad_committed_height",
            "Last block height durably committed after finality.",
            "gauge",
            height,
        );
        sample(
            &mut out,
            "aurad_finality_lag_blocks",
            "Proposed blocks not yet durably finalized.",
            "gauge",
            lag,
        );
        sample(
            &mut out,
            "aurad_mempool_size",
            "Transactions currently queued in the bounded mempool.",
            "gauge",
            pending,
        );
        sample(
            &mut out,
            "aurad_mempool_capacity",
            "Configured transaction capacity of the bounded mempool.",
            "gauge",
            capacity,
        );
        sample(
            &mut out,
            "aurad_peer_count",
            "Connected peers; zero because the N1 P2P layer is not implemented.",
            "gauge",
            0,
        );
        sample(
            &mut out,
            "aurad_p2p_available",
            "Whether the P2P capability is implemented (0 means unavailable).",
            "gauge",
            0,
        );
        sample(
            &mut out,
            "aurad_gas_used_total",
            "Gas in durably committed blocks since process start.",
            "counter",
            gas_total,
        );
        sample_f64(
            &mut out,
            "aurad_gas_used_per_second",
            "Rolling 60-second committed gas throughput.",
            "gauge",
            rate,
        );
        sample(
            &mut out,
            "aurad_blocks_committed_total",
            "Durably committed blocks since process start.",
            "counter",
            blocks,
        );
        sample(
            &mut out,
            "aurad_transactions_committed_total",
            "Transactions in durably committed blocks since process start.",
            "counter",
            transactions,
        );
        sample(
            &mut out,
            "aurad_up",
            "Whether the local pipeline and PID file are ready.",
            "gauge",
            u64::from(up),
        );
        sample(
            &mut out,
            "aurad_otel_exporter_enabled",
            "Whether OTLP trace export is configured.",
            "gauge",
            u64::from(otel),
        );
        sample(
            &mut out,
            "aurad_process_start_time_seconds",
            "Unix timestamp when this process started.",
            "gauge",
            self.started_at_seconds,
        );
        out
    }
}

fn saturating_add(counter: &AtomicU64, increment: u64) {
    let _ = counter.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(increment))
    });
}

fn prune_samples(samples: &mut VecDeque<(Instant, u64)>, now: Instant) {
    while samples
        .front()
        .is_some_and(|(timestamp, _)| now.duration_since(*timestamp) > GAS_WINDOW)
    {
        samples.pop_front();
    }
}

fn sample(out: &mut String, name: &str, help: &str, kind: &str, value: u64) {
    out.push_str(&format!(
        "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}\n"
    ));
}

fn sample_f64(out: &mut String, name: &str, help: &str, kind: &str, value: f64) {
    out.push_str(&format!(
        "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value:.3}\n"
    ));
}

/// Composite HTTP extension: operational endpoints first, then D5 REST routes.
pub struct NodeHttpExtension {
    rest: Arc<RestGateway>,
    pipeline: Arc<Mutex<Pipeline>>,
    metrics: Arc<NodeMetrics>,
    pid_file: PathBuf,
    chain_id: String,
}

impl NodeHttpExtension {
    /// Construct the HTTP extension used by the daemon's shared RPC/REST listener.
    pub fn new(
        rest: Arc<RestGateway>,
        pipeline: Arc<Mutex<Pipeline>>,
        metrics: Arc<NodeMetrics>,
        pid_file: PathBuf,
        chain_id: String,
    ) -> Self {
        Self {
            rest,
            pipeline,
            metrics,
            pid_file,
            chain_id,
        }
    }

    fn health(&self) -> HttpExtensionResponse {
        let lock = self.pipeline.try_lock();
        let pipeline = match lock {
            Ok(pipeline) => pipeline,
            Err(_) => {
                self.metrics.healthy.store(false, Ordering::Relaxed);
                return HttpExtensionResponse::new(
                    503,
                    "application/json",
                    br#"{"status":"unhealthy","reason":"pipeline_unavailable"}"#.to_vec(),
                );
            }
        };
        if !self.pid_file.is_file() {
            self.metrics.healthy.store(false, Ordering::Relaxed);
            return HttpExtensionResponse::new(
                503,
                "application/json",
                br#"{"status":"unhealthy","reason":"pid_file_missing"}"#.to_vec(),
            );
        }
        self.metrics.refresh_from_pipeline(&pipeline);
        self.metrics.healthy.store(true, Ordering::Relaxed);
        let body = serde_json::json!({
            "status": "healthy",
            "chainId": self.chain_id,
            "committedHeight": pipeline.committed_height(),
            "finalityLagBlocks": pipeline.finality_lag_blocks(),
            "mempoolSize": pipeline.mempool_len(),
            "peerCount": 0,
            "p2p": "not_implemented",
        });
        HttpExtensionResponse::new(
            200,
            "application/json",
            serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec()),
        )
    }

    fn metrics(&self) -> HttpExtensionResponse {
        let lock = self.pipeline.try_lock();
        let pipeline = match lock {
            Ok(pipeline) => pipeline,
            Err(_) => {
                self.metrics.healthy.store(false, Ordering::Relaxed);
                return HttpExtensionResponse::new(
                    503,
                    "text/plain; charset=utf-8",
                    b"# pipeline unavailable\n".to_vec(),
                );
            }
        };
        self.metrics.refresh_from_pipeline(&pipeline);
        self.metrics
            .healthy
            .store(self.pid_file.is_file(), Ordering::Relaxed);
        HttpExtensionResponse::new(
            200,
            "text/plain; version=0.0.4; charset=utf-8",
            self.metrics.render_prometheus().into_bytes(),
        )
    }
}

impl HttpExtension for NodeHttpExtension {
    fn handle(
        &self,
        method: &str,
        path: &str,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Option<HttpExtensionResponse> {
        match (method, path) {
            ("GET", "/health") => Some(self.health()),
            ("GET", "/metrics") => Some(self.metrics()),
            _ => self.rest.handle(method, path, content_type, body),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    fn extension(pid_file: PathBuf) -> (NodeHttpExtension, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "aura-d6-unit-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let pipeline = Pipeline::open(crate::pipeline::PipelineConfig {
            chain_id: "auralite-test-1".to_string(),
            data_dir: root.join("state"),
            validator_seed: [0xD6; 32],
            allocations: Vec::new(),
            genesis_time_ms: 1_700_000_000_000,
        })
        .unwrap();
        let pipeline = Arc::new(Mutex::new(pipeline));
        let backend = Arc::new(crate::rpc::PipelineRpcBackend::new(Arc::clone(&pipeline)));
        let service = aura_rpc::RpcService::new(backend);
        let rest = Arc::new(RestGateway::new(service));
        let metrics = Arc::new(NodeMetrics::new(false));
        (
            NodeHttpExtension::new(
                rest,
                pipeline,
                metrics,
                pid_file,
                "auralite-test-1".to_string(),
            ),
            root,
        )
    }

    #[test]
    fn health_endpoint_requires_pid_file() {
        let root = std::env::temp_dir().join(format!(
            "aura-d6-health-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        let pid_file = root.join("aurad.pid");
        let (extension, root) = extension(pid_file);
        let response = extension.health();
        assert_eq!(response.status, 503);
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["reason"], "pid_file_missing");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn metrics_include_finality_gauge_and_dashboard_contract() {
        let metrics = NodeMetrics::new(false);
        let text = metrics.render_prometheus();
        assert!(text.contains("# TYPE aurad_finality_lag_blocks gauge"));
        assert!(text.contains("# TYPE aurad_committed_height gauge"));
        assert!(text.contains("# TYPE aurad_mempool_size gauge"));

        let dashboard: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/observability/grafana-dashboard-d6.json"
        ))
        .unwrap();
        let expressions = dashboard["panels"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|panel| panel["targets"][0]["expr"].as_str())
            .collect::<Vec<_>>();
        for expected in [
            "aurad_committed_height",
            "aurad_finality_lag_blocks",
            "aurad_mempool_size",
            "aurad_peer_count",
            "aurad_gas_used_per_second",
        ] {
            assert!(
                expressions.contains(&expected),
                "dashboard missing {expected}"
            );
        }
    }

    #[test]
    fn gas_rate_counts_committed_samples() {
        let metrics = NodeMetrics::new(false);
        assert_eq!(metrics.gas_used_per_second(), 0.0);
        metrics.record_committed_block(21_000, 1);
        let text = metrics.render_prometheus();
        assert!(text.contains("aurad_gas_used_total 21000\n"));
        assert!(text.contains("aurad_transactions_committed_total 1\n"));
        assert!(metrics.gas_used_per_second() > 0.0);
    }

    #[test]
    fn peer_count_is_zero_until_p2p_exists() {
        let text = NodeMetrics::new(false).render_prometheus();
        assert!(text.contains("aurad_peer_count 0\n"));
        assert!(text.contains("aurad_p2p_available 0\n"));
    }
}
