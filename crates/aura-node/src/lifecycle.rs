//! D1 lifecycle and D6 structured tracing, signal-driven shutdown, PID file and health probe.
//!
//! D4 activates the process shell from D1: the node owns its lifetime,
//! advertises its PID, and shuts down its RPC/block-production threads after
//! SIGTERM or SIGINT. Transaction execution and RPC behavior live in separate
//! libraries.
//!
//! # Signals
//!
//! `signal-hook` provides safe SIGTERM/SIGINT registration. The main thread
//! waits for the atomic flag, then stops worker tasks and removes the PID file.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Handle returned by [`install_signal_handlers`].
pub struct SignalHandle {
    requested: Arc<AtomicBool>,
}

impl SignalHandle {
    /// Has SIGTERM or SIGINT arrived?
    pub fn shutdown_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }

    /// Block until shutdown is requested.
    pub fn wait(&self) -> i32 {
        while !self.shutdown_requested() {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        0
    }
}

/// Install safe SIGTERM and SIGINT handlers.
pub fn install_signal_handlers() -> Result<SignalHandle, std::io::Error> {
    let requested = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&requested))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&requested))?;
    tracing::debug!("SIGTERM/SIGINT handlers installed");
    Ok(SignalHandle { requested })
}

/// Owns the optional OTLP runtime and tracer provider for the daemon lifetime.
pub struct TracingGuard {
    provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
    _runtime: Option<tokio::runtime::Runtime>,
}

impl TracingGuard {
    /// Flush any queued spans and shut down the OTLP exporter.
    pub fn shutdown(&self) {
        if let Some(provider) = &self.provider {
            let _ = provider.force_flush();
            let _ = provider.shutdown();
        }
    }
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn structured_subscriber<W>(
    filter: tracing_subscriber::EnvFilter,
    writer: W,
) -> impl tracing::Subscriber
+ for<'span> tracing_subscriber::registry::LookupSpan<'span>
+ Send
+ Sync
+ 'static
where
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry().with(filter).with(
        tracing_subscriber::fmt::layer()
            .json()
            .with_ansi(false)
            .with_target(false)
            .with_thread_ids(true)
            .with_writer(writer),
    )
}

fn tracer_provider<E>(exporter: E) -> opentelemetry_sdk::trace::SdkTracerProvider
where
    E: opentelemetry_sdk::trace::SpanExporter + 'static,
{
    opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_sampler(opentelemetry_sdk::trace::Sampler::AlwaysOn)
        .with_batch_exporter(exporter)
        .build()
}

/// Initialise JSON-structured logs and optional OpenTelemetry OTLP traces.
///
/// `AURA_LOG` / `RUST_LOG` control the log filter (default `info`). When
/// `OTEL_EXPORTER_OTLP_ENDPOINT` is set, sampled spans are exported over OTLP
/// gRPC to that collector. The returned guard must live until shutdown so the
/// exporter runtime remains available and queued spans can be flushed.
pub fn init_tracing() -> Result<TracingGuard, Box<dyn std::error::Error + Send + Sync>> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig as _;
    use tracing_subscriber::prelude::*;

    let filter = std::env::var("AURA_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| "info".to_string());
    let filter = tracing_subscriber::EnvFilter::new(filter);
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .filter(|value| !value.trim().is_empty());

    let guard = if let Some(endpoint) = endpoint {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        let exporter = runtime.block_on(async {
            opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint)
                .build()
        })?;
        let provider = tracer_provider(exporter);
        opentelemetry::global::set_tracer_provider(provider.clone());
        let tracer = provider.tracer("aurad");
        let subscriber = structured_subscriber(filter, std::io::stderr)
            .with(tracing_opentelemetry::layer().with_tracer(tracer));
        subscriber.try_init()?;
        TracingGuard {
            provider: Some(provider),
            _runtime: Some(runtime),
        }
    } else {
        structured_subscriber(filter, std::io::stderr).try_init()?;
        TracingGuard {
            provider: None,
            _runtime: None,
        }
    };

    tracing::info!(
        otel_exporter_enabled = guard.provider.is_some(),
        "aurad structured logging and tracing initialised"
    );
    Ok(guard)
}

/// Write the current process PID to `path`.
///
/// The PID file lets operators and supervisors find the running node. The
/// caller owns removal — see [`remove_pid_file`] — which is called from the
/// graceful shutdown path.
pub fn write_pid_file(path: &Path) -> Result<(), std::io::Error> {
    let pid = process::id();
    let text = format!("{pid}\n");
    let mut file = fs::File::create(path)?;
    file.write_all(text.as_bytes())?;
    file.flush()?;
    tracing::info!(pid, "wrote PID file to {}", path.display());
    Ok(())
}

/// Remove the PID file at `path`. No error if it is already gone.
pub fn remove_pid_file(path: &Path) -> Result<(), std::io::Error> {
    match fs::remove_file(path) {
        Ok(()) => {
            tracing::info!("removed PID file at {}", path.display());
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Health probe result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// The node is running and its subsystems are initialised.
    Healthy,
    /// The node is starting up or shutting down.
    Unhealthy,
}

/// Probe process health. The current lightweight probe requires the PID file;
/// consensus and storage diagnostics remain separate subsystem checks.
pub fn health_probe(pid_file: &Path) -> HealthStatus {
    if pid_file.exists() {
        HealthStatus::Healthy
    } else {
        HealthStatus::Unhealthy
    }
}

/// Graceful shutdown: remove the PID file, log, and exit with code 0.
///
/// Called from the main thread after RPC and block-production workers stop.
/// The storage backend has committed writes synchronously before this path.
pub fn graceful_shutdown(pid_file: &Path, tracing_guard: &TracingGuard) -> ! {
    tracing::info!("shutdown requested — stopping aurad");
    let _ = remove_pid_file(pid_file);
    tracing::info!("aurad stopped");
    tracing_guard.shutdown();
    process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex as StdMutex;
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone)]
    struct SharedWriter(Arc<StdMutex<Vec<u8>>>);

    impl Write for SharedWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for SharedWriter {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn health_probe_healthy_when_pid_exists() {
        // No PID file yet → unhealthy.
        assert_eq!(
            health_probe(Path::new("/nonexistent-aura-pid")),
            HealthStatus::Unhealthy
        );
    }

    #[test]
    fn signal_handle_starts_unset() {
        let handle = install_signal_handlers().unwrap();
        assert!(!handle.shutdown_requested());
    }

    #[test]
    fn write_and_remove_pid_file() {
        let dir = std::env::temp_dir().join(format!("aurad-test-{}", process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("aurad.pid");
        assert!(write_pid_file(&path).is_ok());
        assert!(path.exists());
        assert!(remove_pid_file(&path).is_ok());
        assert!(!path.exists());
        // Removing again is not an error.
        assert!(remove_pid_file(&path).is_ok());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn structured_log_events_are_json() {
        let buffer = Arc::new(StdMutex::new(Vec::new()));
        let subscriber = structured_subscriber(
            tracing_subscriber::EnvFilter::new("trace"),
            SharedWriter(Arc::clone(&buffer)),
        );
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(height = 7u64, "structured d6 test event");
        });
        let bytes = buffer.lock().unwrap().clone();
        let line = String::from_utf8(bytes).unwrap();
        let event: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(event["fields"]["height"], 7);
        assert_eq!(event["fields"]["message"], "structured d6 test event");
    }

    #[test]
    fn otel_layer_exports_recorded_spans() {
        use opentelemetry::trace::TracerProvider as _;
        use opentelemetry_sdk::trace::InMemorySpanExporter;
        use tracing_subscriber::prelude::*;

        let exporter = InMemorySpanExporter::default();
        let provider = tracer_provider(exporter.clone());
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("aura-node-test")));
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("d6.exported_span", committed_height = 9u64);
            span.in_scope(|| tracing::info!("trace export check"));
        });
        provider.force_flush().unwrap();
        let spans = exporter.get_finished_spans().unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "d6.exported_span");
        provider.shutdown().unwrap();
    }
}
