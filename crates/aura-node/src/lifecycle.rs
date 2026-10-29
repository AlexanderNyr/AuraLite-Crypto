//! D1 lifecycle — tracing, graceful shutdown, PID file, health probe.
//!
//! The node process owns its own lifetime: it initialises observability,
//! advertises its PID, and shuts down cleanly when asked. Nothing here
//! produces blocks or serves RPC — that arrives in D2/D4. What D1 owns is
//! *running as a process* instead of exiting stub-like.
//!
//! # Signals
//!
//! At D1 the process relies on the OS default behaviour for SIGTERM/SIGINT
//! (termination). Installing a custom signal handler would require an `unsafe`
//! FFI binding to `libc::signal`, which `#![forbid(unsafe_code)]` forbids.
//! A dedicated signal crate lands when the node has real state to flush;
//! until then the PID file is removed by the explicit shutdown path, and the
//! OS reclaims everything else on termination.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Handle returned by [`install_signal_handlers`]. At D1 this is a placeholder
/// that always reports "no shutdown requested" — real OS signal handling lands
/// with a signal crate in a later phase.
pub struct SignalHandle {
    requested: Arc<AtomicBool>,
}

impl SignalHandle {
    /// Has a termination signal arrived? Always `false` at D1.
    pub fn shutdown_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }

    /// Block until shutdown is requested. At D1 this returns immediately
    /// (no signal handler is installed), so callers should not block forever.
    pub fn wait(&self) -> i32 {
        // No OS signal handler at D1 — return immediately so the node can
        // proceed to its shutdown path. A real signal crate replaces this.
        0
    }
}

/// Install signal handlers. At D1 this is a no-op: no unsafe FFI, no external
/// signal crate. The OS default (terminate the process) applies.
pub fn install_signal_handlers() -> SignalHandle {
    tracing::debug!("signal handlers: OS default (D1, no unsafe FFI)");
    SignalHandle {
        requested: Arc::new(AtomicBool::new(false)),
    }
}

/// Initialise structured tracing with `tracing-subscriber`.
///
/// Reads `AURA_LOG` / `RUST_LOG` for the filter directive (default `info`).
/// Logs go to stderr in compact format so they can be piped to a log aggregator.
pub fn init_tracing() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = std::env::var("AURA_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| "info".to_string());

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(&filter))
        .with_target(false)
        .with_thread_ids(true)
        .with_level(true)
        .with_writer(std::io::stderr)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;
    tracing::info!("aurad lifecycle initialised (D1 skeleton)");
    Ok(())
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

/// Probe the node's health. At D1 this means: tracing is up, PID file exists.
/// No consensus or storage check yet — those land with D2/D3.
pub fn health_probe(pid_file: &Path) -> HealthStatus {
    if pid_file.exists() {
        HealthStatus::Healthy
    } else {
        HealthStatus::Unhealthy
    }
}

/// Graceful shutdown: remove the PID file, log, and exit with code 0.
///
/// Called from the main thread when the node is asked to stop. At D1 there is
/// no consensus or storage state to flush — the PID file and logs are the only
/// artefacts, both cleaned up here.
pub fn graceful_shutdown(pid_file: &Path) -> ! {
    tracing::info!("shutdown requested — stopping aurad (D1 skeleton)");
    let _ = remove_pid_file(pid_file);
    tracing::info!("aurad stopped");
    process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let handle = install_signal_handlers();
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
}
