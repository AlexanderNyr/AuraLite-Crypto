//! D0 baseline survey — a historical starting-point measurement retained and
//! regenerated as the node grows through later phases.
//!
//! # What D0 measured
//!
//! At D0, `NODE_PLAN` measured the gap before a process existed: no config loader,
//! mempool, block production, RPC or devnet. That receipt is immutable historical
//! evidence. The live survey remains useful as crates gain APIs, but current node
//! behavior is owned by `aurad` and the D1-D4 libraries.
//!
//! # Why the stub refuses
//!
//! `CONTRIBUTING.md` §1 forbids stubs that silently return success. A node stub that
//! exits 0 would make `cargo run -p aura-node --bin aurad` look like a working
//! node in every script, CI job and demo. So:
//!
//! * `aurad` with no arguments prints the refusal and exits [`EXIT_STUB`] (2);
//! * a node flag (`--config`, `--data-dir`, …) is recognised, named, and refused
//!   with the phase that will implement it;
//! * anything else is a usage error, [`EXIT_USAGE`] (64);
//! * `report` is the one command that succeeds, because it produces a measurement;
//!   it returns [`EXIT_NO_WORKSPACE`] (3) when there is no workspace to measure.
//!
//! # What the survey does
//!
//! [`survey`] walks `<root>/crates/<dir>/src`, counts files, lines and public items,
//! and classifies each layer as [`Presence::Missing`], [`Presence::Placeholder`] or
//! [`Presence::Implemented`]. "Placeholder" is measured, not declared: a crate whose
//! every source file declares no public item is a placeholder, which is exactly the
//! C0 skeleton convention. The classification is pinned by
//! `crates/aura-node/tests/fixtures/d0_baseline.txt`, so landing consensus code (B1)
//! without updating the plan fails the gate instead of quietly changing the baseline.

#![deny(missing_docs)]

use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The phase these numbers belong to.
pub const PHASE: &str = "D0";

/// Exit code of the refusal: the node is not implemented.
pub const EXIT_STUB: i32 = 2;

/// Exit code when `report` is asked to measure a tree that is not a workspace.
pub const EXIT_NO_WORKSPACE: i32 = 3;

/// Exit code of a usage error (sysexits `EX_USAGE`).
pub const EXIT_USAGE: i32 = 64;

/// D1-D4 node capabilities currently implemented.
pub const IMPLEMENTED_NODE_CAPABILITIES: usize = 4;
/// Planned D1-D8 node capabilities in the local node roadmap.
pub const TOTAL_NODE_CAPABILITIES: usize = 8;

/// One library layer the node will have to compose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layer {
    /// Library name, e.g. `aura-exec`.
    pub name: &'static str,
    /// Directory under `crates/`.
    pub dir: &'static str,
    /// The phase that lands (or landed) the real implementation.
    pub phase: &'static str,
    /// What the layer is responsible for.
    pub what: &'static str,
}

/// Every library layer the node composes, in dependency order.
///
/// The node crate itself is deliberately not in this list: `aura-node` is the
/// composer, not a library layer. At D0 it contained this module and a stub binary.
pub const LAYERS: [Layer; 14] = [
    Layer {
        name: "aura-types",
        dir: "aura-types",
        phase: "C1",
        what: "addresses, hashes, amounts, chain id",
    },
    Layer {
        name: "aura-codec",
        dir: "aura-codec",
        phase: "C2",
        what: "canonical binary encoding",
    },
    Layer {
        name: "aura-crypto",
        dir: "aura-crypto",
        phase: "C3",
        what: "BLAKE3 hashing, Ed25519 signatures",
    },
    Layer {
        name: "aura-tx",
        dir: "aura-tx",
        phase: "C4",
        what: "transaction envelope and signing domain",
    },
    Layer {
        name: "aura-block",
        dir: "aura-block",
        phase: "C5",
        what: "block header and body commitment",
    },
    Layer {
        name: "aura-state",
        dir: "aura-state",
        phase: "S1",
        what: "JMT-like versioned tree, overlay, history",
    },
    Layer {
        name: "aura-storage",
        dir: "aura-storage",
        phase: "S2",
        what: "RocksDB + WAL, snapshots, pruning",
    },
    Layer {
        name: "aura-genesis",
        dir: "aura-genesis",
        phase: "C8",
        what: "genesis document, hash, `aura genesis` CLI",
    },
    Layer {
        name: "aura-exec",
        dir: "aura-exec",
        phase: "X1",
        what: "deterministic WASM execution, ABI v1-v3, verifier",
    },
    Layer {
        name: "aura-consensus",
        dir: "aura-consensus",
        phase: "B3",
        what: "BLS/QC and three-chain BFT finality; validator set and VRF remain open",
    },
    Layer {
        name: "aura-p2p",
        dir: "aura-p2p",
        phase: "N1",
        what: "gossip, peer management, sync",
    },
    Layer {
        name: "aura-mempool",
        dir: "aura-mempool",
        phase: "D3",
        what: "nonce/balance/gas admission, prioritisation",
    },
    Layer {
        name: "aura-rpc",
        dir: "aura-rpc",
        phase: "D4",
        what: "JSON-RPC 2.0 over HTTP/WebSocket",
    },
    Layer {
        name: "aura-grpc-rest",
        dir: "aura-grpc-rest",
        phase: "D5",
        what: "protobuf gRPC, versioned REST gateway and OpenAPI",
    },
];

/// One node capability that does not exist yet, and the phase that will add it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// Stable name, used by the gate and the stub message.
    pub name: &'static str,
    /// The phase that lands it.
    pub phase: &'static str,
    /// What the capability will do.
    pub what: &'static str,
}

/// The two capabilities still absent after the D6 observability phase.
///
/// D1 supplies config/lifecycle, D2 the in-process transfer pipeline, D3 the
/// bounded mempool, D4 JSON-RPC, D5 versioned REST and protobuf gRPC, and D6
/// health/metrics plus structured logs and optional OTLP traces. The
/// multi-validator devnet remains later work.
pub const MISSING_CAPABILITIES: [Capability; 2] = [
    Capability {
        name: "devnet",
        phase: "D7",
        what: "`make devnet`: 4 validators, genesis, faucet key, logs",
    },
    Capability {
        name: "cli",
        phase: "D8",
        what: "`aura` node ops, query, tx, keys, snapshot, upgrade",
    },
];

/// What a layer turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// No source files at all.
    Missing,
    /// Source files exist but declare no public item — the C0 skeleton convention.
    Placeholder,
    /// At least one public item exists.
    Implemented,
}

impl Presence {
    /// Stable string form used by the fixture and the report.
    pub fn as_str(self) -> &'static str {
        match self {
            Presence::Missing => "missing",
            Presence::Placeholder => "placeholder",
            Presence::Implemented => "implemented",
        }
    }
}

/// One measured layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measurement {
    /// The layer this measurement belongs to.
    pub layer: Layer,
    /// What the source says it is.
    pub presence: Presence,
    /// Number of `.rs` files under `src/`.
    pub files: usize,
    /// Number of lines in those files.
    pub lines: usize,
    /// Number of public item declarations in those files.
    pub public_items: usize,
}

/// The result of [`survey`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Survey {
    /// Root that was measured.
    pub root: PathBuf,
    /// One entry per layer of [`LAYERS`], in that order.
    pub layers: Vec<Measurement>,
}

impl Survey {
    /// Layers that report `Implemented`.
    pub fn implemented(&self) -> usize {
        self.count(Presence::Implemented)
    }

    /// Layers that are still placeholders.
    pub fn placeholders(&self) -> usize {
        self.count(Presence::Placeholder)
    }

    /// Layers with no source at all.
    pub fn missing(&self) -> usize {
        self.count(Presence::Missing)
    }

    fn count(&self, presence: Presence) -> usize {
        self.layers
            .iter()
            .filter(|measurement| measurement.presence == presence)
            .count()
    }

    /// Total `.rs` files across the measured layers.
    pub fn total_files(&self) -> usize {
        self.layers
            .iter()
            .map(|measurement| measurement.files)
            .sum()
    }

    /// Total lines across the measured layers.
    pub fn total_lines(&self) -> usize {
        self.layers
            .iter()
            .map(|measurement| measurement.lines)
            .sum()
    }

    /// The names of the layers that are still placeholders, in survey order.
    pub fn placeholder_names(&self) -> Vec<&'static str> {
        self.layers
            .iter()
            .filter(|measurement| measurement.presence == Presence::Placeholder)
            .map(|measurement| measurement.layer.name)
            .collect()
    }

    /// The report table, as printed by `aurad report` and written into the evidence.
    pub fn table(&self) -> String {
        let mut out = String::new();
        out.push_str("layer             owner  presence      files  lines  public\n");
        for measurement in &self.layers {
            let _ = writeln!(
                out,
                "{:<17} {:<6} {:<13} {:>5}  {:>5}  {:>6}",
                measurement.layer.name,
                measurement.layer.phase,
                measurement.presence.as_str(),
                measurement.files,
                measurement.lines,
                measurement.public_items
            );
        }
        let _ = writeln!(
            out,
            "{:<17} {:<6} {:<13} {:>5}  {:>5}",
            "total",
            PHASE,
            format!("{} impl", self.implemented()),
            self.total_files(),
            self.total_lines()
        );
        out
    }
}

/// Measure `root` — the directory that contains `crates/`.
pub fn survey(root: &Path) -> Survey {
    let layers = LAYERS.iter().map(|layer| measure(root, *layer)).collect();
    Survey {
        root: root.to_path_buf(),
        layers,
    }
}

fn measure(root: &Path, layer: Layer) -> Measurement {
    let src = root.join("crates").join(layer.dir).join("src");
    let mut files = 0usize;
    let mut lines = 0usize;
    let mut public_items = 0usize;
    for path in rust_files(&src) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        files += 1;
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                continue;
            }
            lines += 1;
            if declares_public_item(trimmed) {
                public_items += 1;
            }
        }
    }
    let presence = if files == 0 {
        Presence::Missing
    } else if public_items == 0 {
        Presence::Placeholder
    } else {
        Presence::Implemented
    };
    Measurement {
        layer,
        presence,
        files,
        lines,
        public_items,
    }
}

/// Every `.rs` file under `dir`, sorted so the survey is deterministic.
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|e| e.path())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

/// True when a source line declares a public item.
///
/// Documentation (`//!`, `///`), attributes and comments are not declarations, so a
/// crate whose entire body is a module doc comment measures as a placeholder.
fn declares_public_item(line: &str) -> bool {
    if line.starts_with("//") || line.starts_with('*') || line.starts_with("/*") {
        return false;
    }
    const KINDS: [&str; 10] = [
        "pub fn ",
        "pub struct ",
        "pub enum ",
        "pub const ",
        "pub mod ",
        "pub use ",
        "pub trait ",
        "pub type ",
        "pub static ",
        "pub union ",
    ];
    if KINDS.iter().any(|kind| line.contains(kind)) {
        return true;
    }
    // A macro can declare an item too (`pub(crate)`, `pub(crate) use`, …).
    line.starts_with("pub(crate) ") || line.starts_with("macro_rules! pub ")
}

/// Where the workspace lives when `--workspace` is not passed.
///
/// `CARGO_MANIFEST_DIR` is baked in at compile time, so a binary built from this
/// repository measures this repository. A binary shipped elsewhere must be given
/// `--workspace` — and D0 refuses rather than guessing.
pub fn default_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// What `aurad` was asked to do, in the D0 reading of the arguments.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let Some(command) = args.first().map(String::as_str) else {
        return refuse(err);
    };
    match command {
        "--help" | "-h" => {
            let _ = writeln!(out, "{}", usage());
            0
        }
        "--version" | "-V" => {
            let _ = writeln!(
                out,
                "aurad {} ({} historical baseline survey)",
                env!("CARGO_PKG_VERSION"),
                PHASE
            );
            0
        }
        "report" => report(&args[1..], out, err),
        "--config" | "--data-dir" | "--rpc-addr" | "--chain-id" | "--validator-key" => {
            let capability = match command {
                "--config" | "--chain-id" => "config",
                _ => "lifecycle",
            };
            let phase = MISSING_CAPABILITIES
                .iter()
                .find(|entry| entry.name == capability)
                .map(|entry| entry.phase)
                .unwrap_or("D1");
            let _ = writeln!(
                err,
                "aurad: `{command}` is not implemented — the `{capability}` capability lands with {phase}."
            );
            let _ = writeln!(err, "{}", refusal_lines());
            EXIT_STUB
        }
        other => {
            let _ = writeln!(err, "aurad: unknown command `{other}`.");
            let _ = writeln!(err, "{}", usage());
            EXIT_USAGE
        }
    }
}

/// Handle the `report` subcommand: measure the workspace and print what exists.
///
/// Returns [`EXIT_NO_WORKSPACE`] (3) when the directory has no `crates/`,
/// [`EXIT_USAGE`] (64) on a bad argument, and 0 on success.
pub fn report(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut root = default_root();
    let mut fixture = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--fixture" => {
                fixture = true;
                index += 1;
            }
            "--workspace" => {
                let Some(value) = args.get(index + 1) else {
                    let _ = writeln!(err, "aurad: `--workspace` needs a path.");
                    return EXIT_USAGE;
                };
                root = PathBuf::from(value);
                index += 2;
            }
            other => {
                let _ = writeln!(err, "aurad: unknown argument `{other}` for `report`.");
                return EXIT_USAGE;
            }
        }
    }
    if !root.join("crates").is_dir() {
        let _ = writeln!(
            err,
            "aurad: `{}` is not a workspace (no crates/ directory).",
            root.display()
        );
        return EXIT_NO_WORKSPACE;
    }
    let measured = survey(&root);
    if fixture {
        let _ = write!(out, "{}", fixture_text(&measured));
        return 0;
    }
    let _ = writeln!(out, "{}", measured.table());
    let _ = writeln!(
        out,
        "measured: {} implemented, {} placeholders, {} missing ({} files, {} lines)",
        measured.implemented(),
        measured.placeholders(),
        measured.missing(),
        measured.total_files(),
        measured.total_lines()
    );
    let _ = writeln!(
        out,
        "node capabilities present: {} of {} — future work: {}",
        IMPLEMENTED_NODE_CAPABILITIES,
        TOTAL_NODE_CAPABILITIES,
        MISSING_CAPABILITIES
            .iter()
            .map(|entry| entry.phase)
            .collect::<Vec<_>>()
            .join(", ")
    );
    0
}

/// The pinned `key=value` form of a survey: one line per layer, plus the constants.
///
/// `tools/gen_d0_vectors.py` writes `crates/aura-node/tests/fixtures/d0_baseline.txt`
/// from this, and the gate compares the live survey against that file — so the
/// baseline cannot change without a phase saying so.
pub fn fixture_text(measured: &Survey) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "# D0 baseline: what exists, what a node needs.");
    let _ = writeln!(
        text,
        "# Written by tools/gen_d0_vectors.py; pinned by tests/d0_baseline.rs."
    );
    let _ = writeln!(text, "phase={PHASE}");
    let _ = writeln!(text, "exit_stub={EXIT_STUB}");
    let _ = writeln!(text, "exit_usage={EXIT_USAGE}");
    let _ = writeln!(text, "exit_no_workspace={EXIT_NO_WORKSPACE}");
    let _ = writeln!(text, "layers={}", measured.layers.len());
    let _ = writeln!(text, "capabilities={}", MISSING_CAPABILITIES.len());
    for capability in &MISSING_CAPABILITIES {
        let _ = writeln!(
            text,
            "capability={} {} {}",
            capability.name, capability.phase, capability.what
        );
    }
    let _ = writeln!(text, "implemented={}", measured.implemented());
    let _ = writeln!(text, "placeholders={}", measured.placeholders());
    let _ = writeln!(text, "missing={}", measured.missing());
    for measurement in &measured.layers {
        let _ = writeln!(
            text,
            "layer={} {} {}",
            measurement.layer.name,
            measurement.layer.phase,
            measurement.presence.as_str()
        );
    }
    text
}

fn refuse(err: &mut dyn Write) -> i32 {
    let _ = writeln!(err, "{}", refusal_lines());
    EXIT_STUB
}

/// The refusal text: what is missing, and which phase adds it.
fn refusal_lines() -> String {
    let mut text = String::new();
    text.push_str(
        "aurad — AuraLite-Crypto local node. This is the D0 baseline refusal template:\n",
    );
    text.push_str("D1-D4 are implemented; the following roadmap capabilities remain:\n\n");
    text.push_str("missing capabilities:\n");
    for capability in &MISSING_CAPABILITIES {
        let _ = writeln!(
            text,
            "  {:<12} {:<4} {}",
            capability.name, capability.phase, capability.what
        );
    }
    let _ = writeln!(
        text,
        "\n`aurad report` prints what this tree actually contains (measured, not claimed).\n\
         `aurad --help` lists the commands D0 is allowed to have."
    );
    text
}

fn usage() -> String {
    format!(
        "aurad {} ({} baseline)\n\
         \n\
         usage:\n\
         \x20 aurad report [--workspace PATH]  measure the tree and print what exists\n\
         \x20 aurad report --fixture           the same measurement as `key=value` lines\n\
         \x20 aurad --help                      this text\n\
         \x20 aurad --version                   version and phase\n\
         \n\
         the local node implements D1-D4; {} roadmap capabilities remain ({}..{}).\n\
         exit codes: {} = node not implemented, {} = not a workspace, {} = usage error.",
        env!("CARGO_PKG_VERSION"),
        PHASE,
        MISSING_CAPABILITIES.len(),
        MISSING_CAPABILITIES[0].phase,
        MISSING_CAPABILITIES[MISSING_CAPABILITIES.len() - 1].phase,
        EXIT_STUB,
        EXIT_NO_WORKSPACE,
        EXIT_USAGE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspaces_are_classified_by_their_source() {
        let root = default_root();
        let measured = survey(&root);
        assert_eq!(measured.layers.len(), LAYERS.len());
        for entry in &measured.layers {
            match entry.presence {
                Presence::Missing => assert_eq!(entry.files, 0),
                Presence::Placeholder => {
                    assert!(entry.files > 0);
                    assert_eq!(entry.public_items, 0);
                }
                Presence::Implemented => {
                    assert!(entry.files > 0);
                    assert!(entry.public_items > 0);
                }
            }
        }
    }

    #[test]
    fn documentation_is_not_a_public_item() {
        assert!(!declares_public_item(
            "//! `aura-node` — placeholder crate."
        ));
        assert!(!declares_public_item("/// public fn in a doc comment"));
        assert!(!declares_public_item("#![deny(missing_docs)]"));
        assert!(declares_public_item("pub fn run() {}"));
        assert!(declares_public_item("    pub struct Thing;"));
    }

    #[test]
    fn stub_refuses_without_arguments() {
        let (code, out, err) = run(&[]);
        assert_eq!(code, EXIT_STUB);
        assert!(out.is_empty());
        assert!(err.contains("missing capabilities"));
    }

    #[test]
    fn the_unlanded_layers_are_still_placeholders() {
        let measured = survey(&default_root());
        assert_eq!(measured.placeholder_names(), vec!["aura-p2p"]);
        assert_eq!(measured.implemented(), 13);
        assert_eq!(measured.placeholders(), 1);
        assert_eq!(measured.missing(), 0);
    }

    #[test]
    fn report_refuses_a_tree_that_is_not_a_workspace() {
        let (code, out, err) = run(&["report", "--workspace", "/nonexistent-auralite"]);
        assert_eq!(code, EXIT_NO_WORKSPACE);
        assert!(out.is_empty());
        assert!(err.contains("is not a workspace"));
    }

    #[test]
    fn unknown_command_is_a_usage_error() {
        let (code, _, err) = run(&["serve"]);
        assert_eq!(code, EXIT_USAGE);
        assert!(err.contains("unknown command `serve`"));
    }

    #[test]
    fn refusal_names_every_missing_capability() {
        let (code, _, err) = run(&["--chain-id", "test"]);
        assert_eq!(code, EXIT_STUB);
        for capability in &MISSING_CAPABILITIES {
            assert!(
                err.contains(capability.name),
                "{} missing in {err}",
                capability.name
            );
            assert!(
                err.contains(capability.phase),
                "{} missing in {err}",
                capability.phase
            );
        }
    }

    #[test]
    fn node_flag_is_a_capability_refusal() {
        let (code, _, err) = run(&["--config", "aurad.toml"]);
        assert_eq!(code, EXIT_STUB);
        assert!(err.contains("config"));
        assert!(err.contains("D1"));
    }

    fn run(args: &[&str]) -> (i32, String, String) {
        let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = super::run(&args, &mut out, &mut err);
        (
            code,
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err).into_owned(),
        )
    }
}
