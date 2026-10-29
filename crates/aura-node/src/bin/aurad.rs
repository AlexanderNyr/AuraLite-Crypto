//! `aurad` — the D1 node skeleton binary.
//!
//! D0 refused to run at all. D1 composes the libraries into a real process:
//! configuration (TOML + env + CLI, `chain-id` mandatory), tracing, signal
//! handling, PID file and graceful shutdown. The tx pipeline, mempool and RPC
//! do not exist yet (D2/D4) — this binary starts, waits for a termination
//! signal, and shuts down cleanly.
//!
//! The binary remains a thin shell: argument parsing, config loading and the
//! process lifecycle live in libraries (`aura_node::config`, `aura_node::lifecycle`,
//! `aura_node::baseline`), where the gate can call them directly.

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
                "aurad {} (D1 skeleton — the node starts, does not yet produce blocks)",
                env!("CARGO_PKG_VERSION")
            );
            0
        }
        Some("report") => aura_node::baseline::report(&args[1..], out, err),
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
    if let Err(e) = aura_node::lifecycle::init_tracing() {
        let _ = writeln!(err, "aurad: failed to initialise tracing: {e}");
        return baseline::EXIT_STUB;
    }

    tracing::info!(
        chain_id = %config.chain_id,
        data_dir = %config.data_dir.display(),
        rpc_addr = %config.rpc_addr,
        "aurad starting (D1 skeleton)"
    );

    // Write the PID file into the data directory.
    let pid_file = config.data_dir.join("aurad.pid");
    if let Err(e) = aura_node::lifecycle::write_pid_file(&pid_file) {
        let _ = writeln!(err, "aurad: failed to write PID file: {e}");
        return baseline::EXIT_STUB;
    }

    // Install SIGTERM/SIGINT handlers.
    let signals = aura_node::lifecycle::install_signal_handlers();

    // Log the health probe once at startup.
    let health = aura_node::lifecycle::health_probe(&pid_file);
    tracing::info!(health = ?health, "initial health probe");

    // Run until a termination signal arrives, then shut down gracefully.
    signals.wait();
    aura_node::lifecycle::graceful_shutdown(&pid_file)
}

/// Very small CLI parser for the node-start path.
///
/// Only the flags that matter at D1 are recognised here; every other flag is
/// reported as unknown by the match in `aurad_main`.
fn parse_cli(
    args: &[String],
    config_path: Option<&std::path::PathBuf>,
) -> aura_node::config::CliConfig {
    let mut chain_id: Option<String> = None;
    let mut data_dir: Option<PathBuf> = None;
    let mut rpc_addr: Option<String> = None;
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
        validator_key,
    }
}

fn usage() -> String {
    format!(
        "aurad {} (D1 skeleton)\n\
         \n\
         usage:\n\
         \x20 aurad [--config PATH] [--chain-id ID] [--data-dir PATH] \\\n\
         \x20         [--rpc-addr ADDR] [--validator-key PATH]\n\
         \n\
         The node skeleton starts, installs signal handlers, writes a PID file\n\
         and shuts down gracefully on SIGTERM/SIGINT. It does not yet produce\n\
         blocks or serve RPC — that lands in D2/D4.\n\
         \n\
         commands:\n\
         \x20 aurad report [--workspace PATH]  measure the tree and print what exists\n\
         \x20 aurad report --fixture           the same measurement as `key=value` lines\n\
         \x20 aurad --help                     this text\n\
         \x20 aurad --version                  version and phase\n\
         \n\
         exit codes: 0 = success, 2 = configuration/startup refusal, 64 = usage error.",
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
    fn help_and_version_succeed() {
        let (code, out, _) = run(&["--help"]);
        assert_eq!(code, 0);
        assert!(out.contains("usage:"));

        let (code, out, _) = run(&["--version"]);
        assert_eq!(code, 0);
        assert!(out.contains("aurad"));
        assert!(out.contains("D1 skeleton"));
    }

    #[test]
    fn run_node_refuses_without_chain_id() {
        let (code, _, err) = run(&[]);
        // No chain-id anywhere → configuration refusal (exit 2).
        assert_eq!(code, baseline::EXIT_STUB);
        assert!(err.contains("chain-id"));
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
