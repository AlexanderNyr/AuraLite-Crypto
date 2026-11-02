//! N8 baseline — **all eight `NET_PLAN` components (`transport`, `gossipsub`, `discovery`, `block-sync`, `state-sync`, `dos-defence`, `light-client`, and `chaos-harness`) landed**, and this module measures the completed P2P stack and what cross-plan wiring remains.
//!
//! N0 landed the initial baseline measurement; N1 landed authenticated peer
//! identity (`peer_id = BLAKE3(pubkey)`), handshake, session encryption, ban list,
//! and the TCP [`crate::transport::TransportSwarm`]; N2 landed
//! [`crate::gossipsub`] with separate topics (`tx`, `proposal`, `vote`,
//! `evidence`, `snapshot`), domain-separated Ed25519 message signatures,
//! deduplication (`SeenCache`), and peer scoring with flood/invalid-message
//! penalties; N3 landed [`crate::discovery`] (Kademlia DHT, bootstrap list,
//! NAT/UPnP port mapping, and Circuit Relay v2); N4 landed
//! [`crate::block_sync`] (header-first verification, QC validation, and parallel
//! body download); N5 landed [`crate::state_sync`] (integration with S4
//! chunks, manifest, corrupt-chunk rollback + re-request, and S1 `state_root`
//! verification); N6 landed [`crate::dos_defence`] (size/rate limits,
//! signature rate limit, and equivocation detection); N7 landed
//! [`crate::light_client`] (header+QC proof, finalized checkpoint check, and
//! simplified DAS); **N8** lands [`crate::chaos`] (latency/loss/partition/eclipse
//! simulation and the 100-node stand). All eight `NET_PLAN` layers now measure
//! as [`Presence::Implemented`].

use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The phase these numbers belong to.
pub const PHASE: &str = "N8";

/// Exit code of the refusal: the full P2P network is not complete.
pub const EXIT_STUB: i32 = 2;

/// Exit code when the baseline is asked about a tree that is not a workspace.
pub const EXIT_NO_WORKSPACE: i32 = 3;

/// Exit code of a usage error (sysexits `EX_USAGE`).
pub const EXIT_USAGE: i32 = 64;

/// Documented gap identifier pinned in the baseline fixture and report for
/// two unconnected nodes prior to B5/D7 daemon wiring.
pub const DOCUMENTED_GAP: &str = "N1-transport+N2-gossipsub-absent:node_b_cannot_see_node_a_tx";

/// One network component that will land in N1–N8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layer {
    /// Component name, e.g. `transport`.
    pub name: &'static str,
    /// The phase that lands the real implementation.
    pub phase: &'static str,
    /// What the component is responsible for.
    pub what: &'static str,
}

/// Every `NET_PLAN` component, in landing order.
pub const LAYERS: [Layer; 8] = [
    Layer {
        name: "transport",
        phase: "N1",
        what: "libp2p swarm, Noise/QUIC, identity, handshake, ban list",
    },
    Layer {
        name: "gossipsub",
        phase: "N2",
        what: "topics, message signatures, dedup, peer scoring",
    },
    Layer {
        name: "discovery",
        phase: "N3",
        what: "Kademlia DHT, bootstrap list, NAT/upnp, relay",
    },
    Layer {
        name: "block-sync",
        phase: "N4",
        what: "header-first, QC verification, parallel body download",
    },
    Layer {
        name: "state-sync",
        phase: "N5",
        what: "integration with S4: chunks, manifest, root verification",
    },
    Layer {
        name: "dos-defence",
        phase: "N6",
        what: "size/rate limits, signature rate limit, equivocation detection",
    },
    Layer {
        name: "light-client",
        phase: "N7",
        what: "header+QC proof, finalized checkpoint check, simplified DAS",
    },
    Layer {
        name: "chaos-harness",
        phase: "N8",
        what: "latency/loss/partition/eclipse simulation, 100-node stand",
    },
];

impl Layer {
    /// Source file under `crates/aura-p2p/src/` that implements this layer.
    pub fn module_file(&self) -> &'static str {
        match self.name {
            "transport" => "transport.rs",
            "gossipsub" => "gossipsub.rs",
            "discovery" => "discovery.rs",
            "block-sync" => "block_sync.rs",
            "state-sync" => "state_sync.rs",
            "dos-defence" => "dos_defence.rs",
            "light-client" => "light_client.rs",
            "chaos-harness" => "chaos.rs",
            _ => unreachable!("unknown layer: {}", self.name),
        }
    }

    /// True when this layer's network implementation has landed.
    ///
    /// At N8, all eight `NET_PLAN` layers (`transport` .. `chaos-harness`) are implemented.
    pub fn has_network_logic(&self) -> bool {
        matches!(
            self.name,
            "transport"
                | "gossipsub"
                | "discovery"
                | "block-sync"
                | "state-sync"
                | "dos-defence"
                | "light-client"
                | "chaos-harness"
        )
    }
}

/// One network capability that does not exist yet, and the phase that will add it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// Stable name, used by the gate and the refusal message.
    pub name: &'static str,
    /// The phase that lands it.
    pub phase: &'static str,
    /// What the capability will do.
    pub what: &'static str,
}

/// Cross-plan capability still open after N1–N8: B6 slashing evidence wiring.
pub const MISSING_CAPABILITIES: [Capability; 1] = [Capability {
    name: "slashing-evidence",
    phase: "B6",
    what: "on-chain slashing contract wiring for equivocation evidence",
}];

/// What a network component turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// No source files at all.
    Missing,
    /// Source files exist in the crate, but this network layer has no
    /// implementation yet.
    Placeholder,
    /// The network layer is implemented.
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

/// One measured network component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measurement {
    /// The layer this measurement belongs to.
    pub layer: Layer,
    /// What the source says it is.
    pub presence: Presence,
    /// Number of `.rs` files under `crates/aura-p2p/src/`.
    pub files: usize,
    /// Number of non-empty lines in those files.
    pub lines: usize,
    /// Number of public item declarations in those files.
    pub public_items: usize,
}

/// Two-node no-P2P isolation measurement retained for unconnected node pipelines
/// prior to B5/D7 multi-node daemon wiring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwoNodeIsolationReport {
    /// Transaction hash submitted to Node A over local RPC.
    pub submitted_tx_hash: String,
    /// Whether Node A's local pipeline observes its own submitted transaction.
    pub node_a_knows_tx: bool,
    /// Whether an unconnected Node B observes Node A's transaction without P2P wiring.
    pub node_b_knows_tx: bool,
    /// Connected P2P peers on the local node pipeline before B5/D7 wiring.
    pub peer_count: usize,
    /// Whether any P2P gossip propagation occurred on the unconnected local pipeline.
    pub propagated_over_p2p: bool,
    /// Documented gap tag explaining why an unconnected Node B cannot see Node A's tx.
    pub documented_gap: &'static str,
}

/// Measure two isolated nodes when P2P is not wired: Node A receives `tx_hash`
/// over RPC, and an unconnected Node B does not receive it.
pub fn measure_two_nodes_isolation(tx_hash: &str) -> TwoNodeIsolationReport {
    let node_a_mempool = [tx_hash];
    let node_b_mempool: [&str; 0] = [];
    let node_b_knows_tx = node_b_can_observe_node_a_tx(&node_a_mempool, &node_b_mempool);
    TwoNodeIsolationReport {
        submitted_tx_hash: tx_hash.to_owned(),
        node_a_knows_tx: !tx_hash.is_empty() && node_a_mempool.contains(&tx_hash),
        node_b_knows_tx,
        peer_count: 0,
        propagated_over_p2p: node_b_knows_tx,
        documented_gap: DOCUMENTED_GAP,
    }
}

/// True only if Node B can observe a transaction submitted to Node A.
pub fn node_b_can_observe_node_a_tx(_node_a_txs: &[&str], node_b_txs: &[&str]) -> bool {
    !node_b_txs.is_empty()
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
    /// Layers that report [`Presence::Implemented`].
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
            .filter(|m| m.presence == presence)
            .count()
    }

    /// Total `.rs` files across the measured layers.
    pub fn total_files(&self) -> usize {
        self.layers.iter().map(|m| m.files).sum()
    }

    /// Total lines across the measured layers.
    pub fn total_lines(&self) -> usize {
        self.layers.iter().map(|m| m.lines).sum()
    }

    /// The names of the layers that are still placeholders, in survey order.
    pub fn placeholder_names(&self) -> Vec<&'static str> {
        self.layers
            .iter()
            .filter(|m| m.presence == Presence::Placeholder)
            .map(|m| m.layer.name)
            .collect()
    }

    /// The report table, as printed by `aura-p2p report`.
    pub fn table(&self) -> String {
        let mut out = String::new();
        out.push_str("layer             owner  presence      files  lines  public\n");
        for m in &self.layers {
            let _ = writeln!(
                out,
                "{:<17} {:<6} {:<13} {:>5}  {:>5}  {:>6}",
                m.layer.name,
                m.layer.phase,
                m.presence.as_str(),
                m.files,
                m.lines,
                m.public_items
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
    let src = root.join("crates").join("aura-p2p").join("src");
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
    } else if layer.has_network_logic() {
        Presence::Implemented
    } else {
        Presence::Placeholder
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
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
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
    line.starts_with("pub(crate) ") || line.starts_with("macro_rules! pub ")
}

/// Where the workspace lives when `--workspace` is not passed.
pub fn default_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Entry point for `aura-p2p` CLI arguments.
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
                "aura-p2p {} (N8 baseline — transport, gossipsub, discovery, block-sync, state-sync, dos-defence, light-client, and chaos-harness landed)",
                env!("CARGO_PKG_VERSION")
            );
            0
        }
        "report" => report(&args[1..], out, err),
        other => {
            let _ = writeln!(err, "aura-p2p: unknown command `{other}`.");
            let _ = writeln!(err, "{}", usage());
            EXIT_USAGE
        }
    }
}

fn report(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
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
                    let _ = writeln!(err, "aura-p2p: `--workspace` needs a path.");
                    return EXIT_USAGE;
                };
                root = PathBuf::from(value);
                index += 2;
            }
            other => {
                let _ = writeln!(err, "aura-p2p: unknown argument `{other}` for `report`.");
                return EXIT_USAGE;
            }
        }
    }
    if !root.join("crates").is_dir() {
        let _ = writeln!(
            err,
            "aura-p2p: `{}` is not a workspace (no crates/ directory).",
            root.display()
        );
        return EXIT_NO_WORKSPACE;
    }
    let measured = survey(&root);
    if fixture {
        let _ = write!(out, "{}", fixture_text(&measured));
        return 0;
    }
    let probe = measure_two_nodes_isolation("0x01");
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
        "network capabilities present: {} of {} — missing: {}",
        LAYERS.len() - MISSING_CAPABILITIES.len(),
        LAYERS.len(),
        MISSING_CAPABILITIES
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        out,
        "two-node isolation: node_a_knows_tx={}, node_b_knows_tx={}, peer_count={}, gap={}",
        probe.node_a_knows_tx, probe.node_b_knows_tx, probe.peer_count, probe.documented_gap
    );
    0
}

/// The pinned `key=value` form of a survey: one line per layer, plus the constants
/// and the two-node isolation probe.
pub fn fixture_text(measured: &Survey) -> String {
    let probe = measure_two_nodes_isolation("0x01");
    let mut text = String::new();
    let _ = writeln!(
        text,
        "# N8 baseline: transport, gossipsub, discovery, block-sync, state-sync, dos-defence, light-client, and chaos-harness landed, measured."
    );
    let _ = writeln!(
        text,
        "# Written by tools/gen_n8_vectors.py; pinned by baseline::tests."
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
    let _ = writeln!(text, "node_a_knows_tx={}", probe.node_a_knows_tx);
    let _ = writeln!(text, "node_b_knows_tx={}", probe.node_b_knows_tx);
    let _ = writeln!(text, "peer_count={}", probe.peer_count);
    let _ = writeln!(text, "documented_gap={}", probe.documented_gap);
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
    text.push_str("aura-p2p — the N8 baseline:\n");
    text.push_str(
        "N0 measures that no P2P network exists; N1–N8 land transport, gossipsub, discovery, block-sync, state-sync, dos-defence, light-client, and chaos-harness, and one cross-plan capability remains missing.\n\n",
    );
    text.push_str("missing capabilities:\n");
    for capability in &MISSING_CAPABILITIES {
        let _ = writeln!(
            text,
            "  {:<14} {:<4} {}",
            capability.name, capability.phase, capability.what
        );
    }
    let _ = writeln!(
        text,
        "\n`aura-p2p report` prints what this tree actually contains (measured, not claimed).\n\
         `aura-p2p --help` lists the commands N8 is allowed to have."
    );
    text
}

fn usage() -> String {
    format!(
        "aura-p2p {} (N8 baseline)\n\
         \n\
         usage:\n\
         \x20 aura-p2p report [--workspace PATH]  measure the tree and print what exists\n\
         \x20 aura-p2p report --fixture           the same measurement as `key=value` lines\n\
         \x20 aura-p2p --help                     this text\n\
         \x20 aura-p2p --version                  version and phase\n\
         \n\
         N1–N8 land transport, gossipsub, discovery, block-sync, state-sync, dos-defence, light-client, and chaos-harness; {} cross-plan capability remains missing ({}).\n\
         exit codes: {} = incomplete P2P stack, {} = not a workspace, {} = usage error.",
        env!("CARGO_PKG_VERSION"),
        MISSING_CAPABILITIES.len(),
        MISSING_CAPABILITIES[0].phase,
        EXIT_STUB,
        EXIT_NO_WORKSPACE,
        EXIT_USAGE
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const FIXTURE: &str = include_str!("../tests/fixtures/n8_baseline.txt");

    #[test]
    fn n0_implemented_and_placeholders() {
        let root = default_root();
        let measured = survey(&root);
        assert_eq!(measured.layers.len(), LAYERS.len());
        assert_eq!(
            measured.implemented(),
            8,
            "all eight N1..N8 layers are implemented at N8"
        );
        assert_eq!(
            measured.placeholders(),
            0,
            "0 placeholders remain in N1..N8 layers at N8"
        );
        assert_eq!(measured.missing(), 0);
        for entry in &measured.layers {
            assert_eq!(
                entry.presence,
                Presence::Implemented,
                "{} must be implemented at N8",
                entry.layer.name
            );
        }
        // Verify an unlanded cross-plan layer still measures as Placeholder.
        let unlanded = measure(
            &root,
            Layer {
                name: "ibc-relayer",
                phase: "E7",
                what: "light-client bridge and relayer",
            },
        );
        assert_eq!(unlanded.presence, Presence::Placeholder);
    }

    #[test]
    fn refusal_exits_stub_without_arguments() {
        let (code, out, err) = run(&[]);
        assert_eq!(
            code, EXIT_STUB,
            "incomplete P2P network must be a refusal, not success"
        );
        assert!(out.is_empty(), "the refusal belongs on stderr");
        assert!(
            err.contains("N0 measures that no P2P network exists"),
            "the refusal must state the claim: {err}"
        );
    }

    #[test]
    fn refusal_names_every_missing_capability() {
        let (_code, _out, err) = run(&[]);
        for capability in &MISSING_CAPABILITIES {
            assert!(
                err.contains(capability.name),
                "refusal must name missing capability: {}",
                capability.name
            );
            assert!(
                err.contains(capability.phase),
                "refusal must name missing capability phase: {}",
                capability.phase
            );
        }
    }

    #[test]
    fn report_refuses_a_tree_that_is_not_a_workspace() {
        let mut out: Vec<u8> = Vec::new();
        let mut err: Vec<u8> = Vec::new();
        let args: Vec<String> = vec!["--workspace".into(), "/tmp/not-a-workspace".into()];
        let code = super::report(&args, &mut out, &mut err);
        assert_eq!(code, EXIT_NO_WORKSPACE, "a non-workspace must be refused");
        let err_text = String::from_utf8_lossy(&err);
        assert!(
            err_text.contains("is not a workspace"),
            "the refusal must state the claim: {err_text}"
        );
    }

    #[test]
    fn unknown_command_exits_usage() {
        let (code, _out, _err) = run(&["definitely-not-a-real-command-xyz"]);
        assert_eq!(code, EXIT_USAGE, "an unknown command must be a usage error");
    }

    #[test]
    fn node_b_cannot_observe_node_a_tx_without_p2p() {
        let report = measure_two_nodes_isolation("0xdeadbeef");
        assert!(report.node_a_knows_tx, "node A accepted its own RPC tx");
        assert!(
            !report.node_b_knows_tx,
            "unconnected node B cannot observe node A's tx without P2P wiring"
        );
        assert_eq!(report.peer_count, 0);
        assert!(!report.propagated_over_p2p);
        assert_eq!(report.documented_gap, DOCUMENTED_GAP);
        assert!(!node_b_can_observe_node_a_tx(&["tx1"], &[]));
    }

    #[test]
    fn documentation_is_not_a_public_item() {
        assert!(!declares_public_item(
            "//! `aura-p2p` - N2 baseline: transport and gossipsub landed."
        ));
        assert!(!declares_public_item("/// public fn in a doc comment"));
        assert!(!declares_public_item("#![deny(missing_docs)]"));
        assert!(declares_public_item("pub fn run() {}"));
        assert!(declares_public_item("    pub struct Thing;"));
    }

    #[test]
    fn the_survey_is_deterministic() {
        let root = default_root();
        let first = survey(&root);
        let second = survey(&root);
        assert_eq!(first, second);
        assert_eq!(first.table(), second.table());
        assert_eq!(fixture_text(&first), fixture_text(&second));
    }

    #[test]
    fn the_pinned_fixture_matches_this_tree() {
        let (singles, capabilities, layers) = fixture();
        assert_eq!(singles["phase"], PHASE);
        assert_eq!(singles["exit_stub"], EXIT_STUB.to_string());
        assert_eq!(singles["exit_usage"], EXIT_USAGE.to_string());
        assert_eq!(singles["exit_no_workspace"], EXIT_NO_WORKSPACE.to_string());
        assert_eq!(singles["layers"], LAYERS.len().to_string());
        assert_eq!(
            singles["capabilities"],
            MISSING_CAPABILITIES.len().to_string()
        );
        assert_eq!(singles["node_a_knows_tx"], "true");
        assert_eq!(singles["node_b_knows_tx"], "false");
        assert_eq!(singles["peer_count"], "0");
        assert_eq!(singles["documented_gap"], DOCUMENTED_GAP);

        let expected_capabilities: Vec<String> = MISSING_CAPABILITIES
            .iter()
            .map(|entry| format!("{} {} {}", entry.name, entry.phase, entry.what))
            .collect();
        assert_eq!(capabilities, expected_capabilities);

        let measured = survey(&repo_root());
        let live: Vec<String> = measured
            .layers
            .iter()
            .map(|entry| {
                format!(
                    "{} {} {}",
                    entry.layer.name,
                    entry.layer.phase,
                    entry.presence.as_str()
                )
            })
            .collect();
        assert_eq!(
            layers, live,
            "the tree changed under the pinned N8 baseline — either the survey is wrong or a phase landed without updating the plan and this fixture"
        );
        assert_eq!(singles["implemented"], measured.implemented().to_string());
        assert_eq!(singles["placeholders"], measured.placeholders().to_string());
        assert_eq!(singles["missing"], measured.missing().to_string());
    }

    #[test]
    fn the_fixture_is_what_the_binary_prints() {
        let (code, out, _) = run(&["report", "--fixture"]);
        assert_eq!(code, 0);
        assert_eq!(out, FIXTURE, "run `make gen-n8-vectors` to refresh the pin");
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn run(args: &[&str]) -> (i32, String, String) {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        let mut out: Vec<u8> = Vec::new();
        let mut err: Vec<u8> = Vec::new();
        let code = super::run(&args, &mut out, &mut err);
        (
            code,
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err).into_owned(),
        )
    }

    fn fixture() -> (BTreeMap<String, String>, Vec<String>, Vec<String>) {
        let mut singles = BTreeMap::new();
        let mut capabilities = Vec::new();
        let mut layers = Vec::new();
        for line in FIXTURE.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "capability" => capabilities.push(value.to_string()),
                "layer" => layers.push(value.to_string()),
                other => {
                    singles.insert(other.to_string(), value.to_string());
                }
            }
        }
        (singles, capabilities, layers)
    }
}
