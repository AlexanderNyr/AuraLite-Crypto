//! `aurad` — the node process, D5 API listeners and D6 observability endpoints.
//!
//! Configuration, transaction validation/execution, JSON-RPC and transport
//! are owned by libraries; this binary composes them, starts a local block
//! producer and shuts down on SIGTERM/SIGINT. The default validator seed is a
//! documented development-only key; production validator key management is a
//! later phase.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    let code = aurad_main(&args, &mut out, &mut err);
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

/// Entry point called by `main` and by integration tests.
///
/// Returns a `sysexits`-style exit code:
///   0 — success (help, version, report, clean shutdown)
///   2 — configuration or startup refusal (missing chain-id, unusable data dir)
///   64 — usage error (unknown command or argument)
fn aurad_main(args: &[String], out: &mut dyn std::io::Write, err: &mut dyn std::io::Write) -> i32 {
    match args.first().map(String::as_str) {
        None | Some("") => {
            // No subcommand: start the node skeleton.
            run_node(args, out, err)
        }
        Some("--help") | Some("-h") => {
            let _ = writeln!(out, "{}", usage());
            0
        }
        Some("--version") | Some("-V") => {
            let _ = writeln!(
                out,
                "aurad {} (D6 observability node — Prometheus, JSON logs, optional OTLP traces)",
                env!("CARGO_PKG_VERSION")
            );
            0
        }
        Some("report") => aura_node::baseline::report(&args[1..], out, err),
        Some(
            "--config" | "--data-dir" | "--rpc-addr" | "--grpc-addr" | "--chain-id"
            | "--validator-key",
        ) => run_node(args, out, err),
        Some(value)
            if value.starts_with("--config=")
                || value.starts_with("--data-dir=")
                || value.starts_with("--rpc-addr=")
                || value.starts_with("--grpc-addr=")
                || value.starts_with("--chain-id=")
                || value.starts_with("--validator-key=") =>
        {
            run_node(args, out, err)
        }
        Some(other) => {
            let _ = writeln!(err, "aurad: unknown command `{other}`.",);
            let _ = writeln!(err, "{}", usage());
            baseline::EXIT_USAGE
        }
    }
}

/// Start the node skeleton: load config, init tracing, write PID, install
/// signal handlers, wait for termination, shut down gracefully.
fn run_node(args: &[String], _out: &mut dyn std::io::Write, err: &mut dyn std::io::Write) -> i32 {
    // Very early: extract --config so the rest of config loading can use it.
    let config_path = aura_node::config::config_path_from_args(args);
    let cli = parse_cli(args, config_path.as_ref());

    // Load configuration: TOML + env + CLI. `chain-id` is mandatory.
    let config = match aura_node::config::load(&cli) {
        Ok(config) => config,
        Err(e) => {
            let _ = writeln!(err, "aurad: configuration error: {e}");
            let _ = writeln!(
                err,
                "  the `config` capability lands with D1 (TOML + env + CLI, `chain-id` mandatory)"
            );
            let _ = writeln!(
                err,
                "  pass --chain-id <id>, set AURA_CHAIN_ID, or add chain_id to aurad.toml"
            );
            return baseline::EXIT_STUB;
        }
    };

    // Initialise structured tracing before anything else.
    let tracing_guard = match aura_node::lifecycle::init_tracing() {
        Ok(guard) => guard,
        Err(error) => {
            let _ = writeln!(
                err,
                "aurad: failed to initialise structured tracing: {error}"
            );
            return baseline::EXIT_STUB;
        }
    };

    tracing::info!(
        chain_id = %config.chain_id,
        data_dir = %config.data_dir.display(),
        rpc_addr = %config.rpc_addr,
        grpc_addr = %config.grpc_addr,
        "aurad starting (D6 observability enabled)"
    );

    // Write the PID file into the data directory.
    let pid_file = config.data_dir.join("aurad.pid");
    if let Err(e) = aura_node::lifecycle::write_pid_file(&pid_file) {
        let _ = writeln!(err, "aurad: failed to write PID file: {e}");
        return baseline::EXIT_STUB;
    }

    let validator_seed = match load_validator_seed(config.validator_key.as_deref()) {
        Ok(seed) => seed,
        Err(error) => {
            let _ = writeln!(err, "aurad: invalid validator key: {error}");
            let _ = aura_node::lifecycle::remove_pid_file(&pid_file);
            return baseline::EXIT_STUB;
        }
    };
    let pipeline = match aura_node::pipeline::Pipeline::open(aura_node::pipeline::PipelineConfig {
        chain_id: config.chain_id.clone(),
        data_dir: config.data_dir.clone(),
        validator_seed,
        allocations: Vec::new(),
        genesis_time_ms: 0,
    }) {
        Ok(pipeline) => pipeline,
        Err(error) => {
            let _ = writeln!(
                err,
                "aurad: failed to initialize transaction pipeline: {error}"
            );
            let _ = aura_node::lifecycle::remove_pid_file(&pid_file);
            return baseline::EXIT_STUB;
        }
    };
    let pipeline = std::sync::Arc::new(std::sync::Mutex::new(pipeline));
    let backend = std::sync::Arc::new(aura_node::rpc::PipelineRpcBackend::new(
        std::sync::Arc::clone(&pipeline),
    ));
    let service = aura_rpc::RpcService::new(backend);
    let rest = std::sync::Arc::new(aura_grpc_rest::RestGateway::new(service.clone()));
    let otel_enabled = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .is_some_and(|endpoint| !endpoint.trim().is_empty());
    let metrics = std::sync::Arc::new(aura_node::observability::NodeMetrics::new(otel_enabled));
    let http_extension = std::sync::Arc::new(aura_node::observability::NodeHttpExtension::new(
        rest,
        std::sync::Arc::clone(&pipeline),
        std::sync::Arc::clone(&metrics),
        pid_file.clone(),
        config.chain_id.clone(),
    ));
    let mut server = match aura_rpc::RpcServerHandle::spawn_with_extension(
        &config.rpc_addr,
        service.clone(),
        Some(http_extension),
    ) {
        Ok(server) => server,
        Err(error) => {
            let _ = writeln!(
                err,
                "aurad: cannot bind HTTP JSON-RPC/REST address {}: {error}",
                config.rpc_addr
            );
            let _ = aura_node::lifecycle::remove_pid_file(&pid_file);
            return baseline::EXIT_STUB;
        }
    };
    let mut grpc_server = match aura_grpc_rest::GrpcServerHandle::spawn(&config.grpc_addr, service)
    {
        Ok(server) => server,
        Err(error) => {
            let _ = writeln!(
                err,
                "aurad: cannot bind gRPC address {}: {error}",
                config.grpc_addr
            );
            server.shutdown();
            let _ = aura_node::lifecycle::remove_pid_file(&pid_file);
            return baseline::EXIT_STUB;
        }
    };

    let stop_producer = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let producer_stop = std::sync::Arc::clone(&stop_producer);
    let producer_pipeline = std::sync::Arc::clone(&pipeline);
    let producer_metrics = std::sync::Arc::clone(&metrics);
    let producer = std::thread::Builder::new()
        .name("aura-local-producer".to_owned())
        .spawn(move || {
            while !producer_stop.load(std::sync::atomic::Ordering::Acquire) {
                if let Ok(mut pipeline) = producer_pipeline.lock()
                    && pipeline.needs_block_production()
                {
                    match pipeline.produce_block() {
                        Ok(production) => {
                            for committed in production.committed {
                                producer_metrics.record_committed_block(
                                    committed.block.header.gas_used.value(),
                                    committed.receipts.len(),
                                );
                            }
                        }
                        Err(error) => tracing::warn!(%error, "local block production failed"),
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        });
    let producer = match producer {
        Ok(producer) => producer,
        Err(error) => {
            let _ = writeln!(err, "aurad: cannot start local block producer: {error}");
            server.shutdown();
            grpc_server.shutdown();
            let _ = aura_node::lifecycle::remove_pid_file(&pid_file);
            return baseline::EXIT_STUB;
        }
    };

    let signals = match aura_node::lifecycle::install_signal_handlers() {
        Ok(signals) => signals,
        Err(error) => {
            let _ = writeln!(
                err,
                "aurad: cannot install SIGTERM/SIGINT handlers: {error}"
            );
            stop_producer.store(true, std::sync::atomic::Ordering::Release);
            server.shutdown();
            grpc_server.shutdown();
            let _ = producer.join();
            let _ = aura_node::lifecycle::remove_pid_file(&pid_file);
            return baseline::EXIT_STUB;
        }
    };

    tracing::info!(
        rpc = %server.local_addr(),
        grpc = %grpc_server.local_addr(),
        "JSON-RPC, REST and gRPC endpoints ready"
    );
    let health = aura_node::lifecycle::health_probe(&pid_file);
    tracing::info!(health = ?health, "initial health probe");

    signals.wait();
    stop_producer.store(true, std::sync::atomic::Ordering::Release);
    server.shutdown();
    grpc_server.shutdown();
    let _ = producer.join();
    aura_node::lifecycle::graceful_shutdown(&pid_file, &tracing_guard)
}

fn load_validator_seed(path: Option<&std::path::Path>) -> Result<[u8; 32], String> {
    let Some(path) = path else {
        tracing::warn!(
            "using development-only deterministic validator seed; configure --validator-key for local signing"
        );
        return Ok([0xD4; 32]);
    };
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| "validator key file must contain exactly 32 raw bytes".to_owned())
}

/// Very small CLI parser for the node-start path.
///
/// Node listener and identity flags are parsed here; every other flag is
/// reported as unknown by the match in `aurad_main`.
fn parse_cli(
    args: &[String],
    config_path: Option<&std::path::PathBuf>,
) -> aura_node::config::CliConfig {
    let mut chain_id: Option<String> = None;
    let mut data_dir: Option<PathBuf> = None;
    let mut rpc_addr: Option<String> = None;
    let mut grpc_addr: Option<String> = None;
    let mut validator_key: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--chain-id" => {
                chain_id = args.get(i + 1).cloned();
                i += 2;
            }
            arg if arg.starts_with("--chain-id=") => {
                chain_id = Some(arg["--chain-id=".len()..].to_string());
                i += 1;
            }
            "--data-dir" => {
                data_dir = args.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            arg if arg.starts_with("--data-dir=") => {
                data_dir = Some(PathBuf::from(&arg["--data-dir=".len()..]));
                i += 1;
            }
            "--rpc-addr" => {
                rpc_addr = args.get(i + 1).cloned();
                i += 2;
            }
            arg if arg.starts_with("--rpc-addr=") => {
                rpc_addr = Some(arg["--rpc-addr=".len()..].to_string());
                i += 1;
            }
            "--grpc-addr" => {
                grpc_addr = args.get(i + 1).cloned();
                i += 2;
            }
            arg if arg.starts_with("--grpc-addr=") => {
                grpc_addr = Some(arg["--grpc-addr=".len()..].to_string());
                i += 1;
            }
            "--validator-key" => {
                validator_key = args.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            arg if arg.starts_with("--validator-key=") => {
                validator_key = Some(PathBuf::from(&arg["--validator-key=".len()..]));
                i += 1;
            }
            // `--config` is handled by `config_path_from_args` before this.
            _ => i += 1,
        }
    }

    aura_node::config::CliConfig {
        config: config_path.map(|p| p.to_path_buf()),
        chain_id,
        data_dir,
        rpc_addr,
        grpc_addr,
        validator_key,
    }
}

fn usage() -> String {
    format!(
        concat!(
            "aurad {} (D6 observability node)\n\n",
            "usage:\n",
            "  aurad [--config PATH] [--chain-id ID] [--data-dir PATH] \\\
",
            "        [--rpc-addr ADDR] [--grpc-addr ADDR] [--validator-key PATH]\n\n",
            "Starts JSON-RPC, versioned REST, gRPC, /health and Prometheus /metrics.\n",
            "Logs are JSON; OTEL_EXPORTER_OTLP_ENDPOINT enables OTLP traces.\n",
            "SIGTERM/SIGINT shuts all listeners down.\n\n",
            "commands:\n",
            "  aurad report [--workspace PATH]  measure the tree and print what exists\n",
            "  aurad report --fixture           the same measurement as key=value lines\n",
            "  aurad --help                     this text\n",
            "  aurad --version                  version and phase\n\n",
            "exit codes: 0 = success, 2 = configuration/startup refusal, 64 = usage error."
        ),
        env!("CARGO_PKG_VERSION")
    )
}

use aura_node::baseline;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command_is_usage_error() {
        let (code, _, err) = run(&["serve"]);
        assert_eq!(code, baseline::EXIT_USAGE);
        assert!(err.contains("unknown command `serve`"));
    }

    #[test]
    fn grpc_address_flag_is_routed_and_parsed() {
        let args = vec!["--grpc-addr=127.0.0.1:9191".to_string()];
        let cli = parse_cli(&args, None);
        assert_eq!(cli.grpc_addr.as_deref(), Some("127.0.0.1:9191"));
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(aurad_main(&args, &mut out, &mut err), baseline::EXIT_STUB);
        assert!(String::from_utf8(err).unwrap().contains("chain-id"));
    }

    #[test]
    fn help_and_version_succeed() {
        let (code, out, _) = run(&["--help"]);
        assert_eq!(code, 0);
        assert!(out.contains("usage:"));

        let (code, out, _) = run(&["--version"]);
        assert_eq!(code, 0);
        assert!(out.contains("aurad"));
        assert!(out.contains("D6 observability"));
    }

    #[test]
    fn run_node_refuses_without_chain_id() {
        let (code, _, err) = run(&[]);
        // No chain-id anywhere → configuration refusal (exit 2).
        assert_eq!(code, baseline::EXIT_STUB);
        assert!(err.contains("chain-id"));
        assert!(err.contains("`config` capability lands with D1"));
    }

    fn run(args: &[&str]) -> (i32, String, String) {
        let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        let mut out: Vec<u8> = Vec::new();
        let mut err: Vec<u8> = Vec::new();
        let code = aurad_main(&args, &mut out, &mut err);
        (
            code,
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err).into_owned(),
        )
    }
}
