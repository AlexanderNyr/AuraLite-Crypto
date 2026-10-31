//! D1 configuration — TOML + env + CLI, `chain-id` mandatory.
//!
//! `aurad` is a thin composition of libraries; configuration is the first piece
//! of that composition. Nothing here touches consensus, mempool or storage —
//! those land in D2/D3. What D1 owns is the *loading* and *validation* of the
//! node's configuration, so that every later phase starts from a known, valid
//! state instead of guessing.
//!
//! # Sources (in priority order)
//
//! 1. **CLI flags** — `--config`, `--chain-id`, `--data-dir`, `--rpc-addr`,
//!    `--validator-key`. Highest priority; a flag overrides everything else.
//! 2. **Environment variables** — `AURA_CHAIN_ID`, `AURA_DATA_DIR`,
//!    `AURA_RPC_ADDR`, `AURA_VALIDATOR_KEY`.
//! 3. **TOML file** — `--config PATH` or, when absent, `./aurad.toml` next to
//!    the binary. Absent file is *not* an error; a missing `chain-id` is.
//!
//! # `chain-id` is mandatory
//!
//! `chain-id` identifies the network (`auralite-test-1` for devnet/testnet,
//! `auralite-1` reserved for mainnet). It must come from one of the three
//! sources above; `aurad` refuses to start without it rather than inventing one.
//!
//! # No `serde` here
//!
//! `serde` is a D4 dependency (JSON-RPC needs it). D1 parses TOML with the
//! `toml` crate's dynamic `Value` type instead, keeping the dependency surface
//! minimal and respecting the phase-layering rule.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Everything the node needs to start, before any subsystem is initialised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Network identifier — mandatory, never defaulted.
    pub chain_id: String,
    /// Directory for state, snapshots, PID file and logs.
    pub data_dir: PathBuf,
    /// HTTP JSON-RPC and REST listen address (`127.0.0.1:8545` style).
    pub rpc_addr: String,
    /// HTTP/2 gRPC listen address (`127.0.0.1:9090` style).
    pub grpc_addr: String,
    /// Optional validator key file (used by D7 devnet).
    pub validator_key: Option<PathBuf>,
}

/// Why configuration loading failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// No `chain-id` was found in any source.
    MissingChainId,
    /// The TOML file exists but could not be parsed.
    TomlParse(PathBuf, String),
    /// The TOML file contains a key this version does not understand.
    UnknownField(String),
    /// The TOML file was given but does not exist.
    TomlMissing(PathBuf),
    /// The data directory cannot be created or read.
    DataDir(PathBuf, String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingChainId => {
                write!(
                    f,
                    "chain-id is mandatory: pass --chain-id, set AURA_CHAIN_ID, or add chain_id to your TOML config"
                )
            }
            ConfigError::TomlParse(path, err) => {
                write!(f, "failed to parse TOML config {}: {}", path.display(), err)
            }
            ConfigError::UnknownField(field) => {
                write!(f, "unknown field in TOML config: {field}")
            }
            ConfigError::TomlMissing(path) => {
                write!(f, "config file not found: {}", path.display())
            }
            ConfigError::DataDir(path, err) => {
                write!(f, "data directory {} is unusable: {}", path.display(), err)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// Raw CLI view.
#[derive(Debug, Clone, Default)]
pub struct CliConfig {
    /// Path to the TOML config file (from `--config`).
    pub config: Option<PathBuf>,
    /// Chain identifier (from `--chain-id` or `AURA_CHAIN_ID`).
    pub chain_id: Option<String>,
    /// Data directory (from `--data-dir` or `AURA_DATA_DIR`).
    pub data_dir: Option<PathBuf>,
    /// HTTP JSON-RPC/REST listen address (from `--rpc-addr` or `AURA_RPC_ADDR`).
    pub rpc_addr: Option<String>,
    /// gRPC listen address (from `--grpc-addr` or `AURA_GRPC_ADDR`).
    pub grpc_addr: Option<String>,
    /// Validator key file (from `--validator-key` or `AURA_VALIDATOR_KEY`).
    pub validator_key: Option<PathBuf>,
}

/// Load configuration from TOML, env and CLI, in that priority order.
///
/// CLI overrides env overrides TOML. `chain-id` is mandatory from any source.
///
/// # Errors
///
/// Returns [`ConfigError::MissingChainId`] when no source provides a chain id.
pub fn load(cli: &CliConfig) -> Result<Config, ConfigError> {
    let toml_value = load_toml(cli)?;
    let env = load_env();
    let chain_id = cli
        .chain_id
        .as_deref()
        .or(env.chain_id.as_deref())
        .or(toml_chain_id(&toml_value))
        .ok_or(ConfigError::MissingChainId)?;

    let data_dir = cli
        .data_dir
        .clone()
        .or(env.data_dir.as_deref().map(PathBuf::from))
        .or(toml_data_dir(&toml_value).map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(".aura"));

    let rpc_addr = cli
        .rpc_addr
        .as_deref()
        .or(env.rpc_addr.as_deref())
        .or(toml_rpc_addr(&toml_value))
        .unwrap_or("127.0.0.1:8545")
        .to_string();

    let grpc_addr = cli
        .grpc_addr
        .as_deref()
        .or(env.grpc_addr.as_deref())
        .or(toml_grpc_addr(&toml_value))
        .unwrap_or("127.0.0.1:9090")
        .to_string();

    let validator_key = cli
        .validator_key
        .clone()
        .or(env.validator_key.as_deref().map(PathBuf::from))
        .or(toml_validator_key(&toml_value).map(PathBuf::from));

    // Validate the data directory early: a node that can't write state must fail
    // at startup, not mid-block.
    ensure_data_dir(&data_dir)?;

    Ok(Config {
        chain_id: chain_id.to_string(),
        data_dir,
        rpc_addr,
        grpc_addr,
        validator_key,
    })
}

/// Parse `--config` from CLI args (very early, before full parsing).
pub fn config_path_from_args(args: &[String]) -> Option<PathBuf> {
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => return args.get(i + 1).map(PathBuf::from),
            arg if arg.starts_with("--config=") => {
                return Some(PathBuf::from(&arg["--config=".len()..]));
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn load_toml(cli: &CliConfig) -> Result<toml::Value, ConfigError> {
    let path = cli
        .config
        .as_deref()
        .unwrap_or_else(|| Path::new("aurad.toml"));
    if !path.exists() {
        // Absent config file is not an error — env and CLI can still provide
        // everything. Return an empty table.
        return Ok(toml::Value::Table(Default::default()));
    }
    let text = fs::read_to_string(path)
        .map_err(|err| ConfigError::DataDir(path.to_path_buf(), err.to_string()))?;
    let value = toml::from_str::<toml::Value>(&text)
        .map_err(|err| ConfigError::TomlParse(path.to_path_buf(), err.to_string()))?;
    let allowed = [
        "chain_id",
        "data_dir",
        "rpc_addr",
        "grpc_addr",
        "validator_key",
    ];
    if let Some(table) = value.as_table() {
        for key in table.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(ConfigError::UnknownField(key.clone()));
            }
        }
    }
    Ok(value)
}

fn load_env() -> EnvConfig {
    EnvConfig {
        chain_id: env::var("AURA_CHAIN_ID").ok(),
        data_dir: env::var("AURA_DATA_DIR").ok(),
        rpc_addr: env::var("AURA_RPC_ADDR").ok(),
        grpc_addr: env::var("AURA_GRPC_ADDR").ok(),
        validator_key: env::var("AURA_VALIDATOR_KEY").ok(),
    }
}

/// Raw environment-variable view.
#[derive(Debug, Clone, Default)]
struct EnvConfig {
    chain_id: Option<String>,
    data_dir: Option<String>,
    rpc_addr: Option<String>,
    grpc_addr: Option<String>,
    validator_key: Option<String>,
}

/// Extract the `chain_id` string from a parsed TOML value.
///
/// Used by the config loader and exposed for testing.
pub fn toml_chain_id(value: &toml::Value) -> Option<&str> {
    value.get("chain_id").and_then(|v| v.as_str())
}

fn toml_data_dir(value: &toml::Value) -> Option<String> {
    value
        .get("data_dir")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

fn toml_rpc_addr(value: &toml::Value) -> Option<&str> {
    value.get("rpc_addr").and_then(|v| v.as_str())
}

fn toml_grpc_addr(value: &toml::Value) -> Option<&str> {
    value.get("grpc_addr").and_then(|v| v.as_str())
}

fn toml_validator_key(value: &toml::Value) -> Option<String> {
    value
        .get("validator_key")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

fn ensure_data_dir(path: &Path) -> Result<(), ConfigError> {
    if !path.exists() {
        fs::create_dir_all(path)
            .map_err(|err| ConfigError::DataDir(path.to_path_buf(), err.to_string()))?;
    }
    if !path.is_dir() {
        return Err(ConfigError::DataDir(
            path.to_path_buf(),
            "not a directory".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_id_is_mandatory() {
        let cli = CliConfig::default();
        let err = load(&cli).unwrap_err();
        assert!(matches!(err, ConfigError::MissingChainId));
        assert!(err.to_string().contains("chain-id"));
    }

    #[test]
    fn cli_chain_id_wins() {
        let cli = CliConfig {
            chain_id: Some("cli-chain".to_string()),
            ..Default::default()
        };
        let config = load(&cli).unwrap();
        assert_eq!(config.chain_id, "cli-chain");
    }

    #[test]
    fn data_dir_defaults_to_dot_aura() {
        let cli = CliConfig {
            chain_id: Some("test".to_string()),
            ..Default::default()
        };
        let config = load(&cli).unwrap();
        assert_eq!(config.data_dir, PathBuf::from(".aura"));
        assert_eq!(config.rpc_addr, "127.0.0.1:8545");
        assert_eq!(config.grpc_addr, "127.0.0.1:9090");
        assert!(config.validator_key.is_none());
    }

    #[test]
    fn config_path_from_args_parses_both_forms() {
        assert_eq!(
            config_path_from_args(&["--config".into(), "/path.toml".into()]),
            Some(PathBuf::from("/path.toml"))
        );
        assert_eq!(
            config_path_from_args(&["--config=/path.toml".into()]),
            Some(PathBuf::from("/path.toml"))
        );
        assert_eq!(config_path_from_args(&["--help".into()]), None);
    }

    #[test]
    fn toml_chain_id_can_be_read_from_a_known_field() {
        let value = toml::from_str::<toml::Value>("chain_id = \"test\"").unwrap();
        assert_eq!(toml_chain_id(&value), Some("test"));
    }

    #[test]
    fn config_toml_unknown_field_is_rejected() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "aurad-config-unit-{}-{}.toml",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let data_dir = path.with_extension("data");
        fs::write(
            &path,
            format!(
                "chain_id = \"test\"\ndata_dir = \"{}\"\nunknown_field = 1\n",
                data_dir.display()
            ),
        )
        .unwrap();
        let result = load(&CliConfig {
            config: Some(path.clone()),
            ..Default::default()
        });
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&data_dir);
        assert!(
            matches!(result, Err(ConfigError::UnknownField(field)) if field == "unknown_field")
        );
    }
}
