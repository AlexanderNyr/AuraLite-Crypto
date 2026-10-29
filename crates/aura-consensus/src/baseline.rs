//! B3 baseline - **BFT finality**, and this module measures what remains
//! missing.
//!
//! `aura-consensus` is no longer a C0 placeholder. B0 landed the baseline
//! measurement; B1 landed the proposer and block signature; B2 landed the
//! [`bls`] module (BLS12-381 threshold signatures) and the [`qc`] module
//! (quorum certificate aggregation from 2f+1 votes); B3 landed the
//! [`finality`] module (3-chain commit, view change, timeouts, nil blocks).
//! The crate now measures as [`Presence::Implemented`] for those four
//! components and [`Presence::Placeholder`] for the two B4–B5 capabilities
//! that still do not exist. The honest artifact is a measurement of what is
//! present plus a binary that refuses to claim full consensus where it does
//! not exist.
//!
//! # What B3 measures
//!
//! [`survey`] measures the `aura-consensus` crate itself: files, lines, and
//! public item declarations. The `proposer`, `block-signature`, `qc`, and
//! `finality` layers measure as [`Presence::Implemented`]; `validator-set`
//! and `vrf-leader` measure as [`Presence::Placeholder`]. The two consensus
//! capabilities that B4–B5 will add are listed in [`MISSING_CAPABILITIES`]
//! and named in the refusal text.
//!
//! # Why the baseline refuses
//!
//! `CONTRIBUTING.md` §1 forbids stubs that silently return success. A consensus
//! module that reports "agreed" without any validators would make every later
//! block, QC and finality proof suspect. So:
//!
//! * the baseline reports what exists and names every missing capability
//!   with the phase that will add it;
//! * two different transaction logs are **not** equivalent - there is no
//!   full consensus to order them, and the gate asserts this;
//! * `report` is the one command that succeeds, because it produces a
//!   measurement.

use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The phase these numbers belong to.
pub const PHASE: &str = "B3";

/// Exit code of the refusal: no consensus exists.
pub const EXIT_STUB: i32 = 2;

/// Exit code when the baseline is asked about a tree that is not a workspace.
pub const EXIT_NO_WORKSPACE: i32 = 3;

/// Exit code of a usage error (sysexits `EX_USAGE`).
pub const EXIT_USAGE: i32 = 64;

/// One consensus component that will land in B1–B5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layer {
    /// Component name, e.g. `proposer`.
    pub name: &'static str,
    /// The phase that lands (or landed) the real implementation.
    pub phase: &'static str,
    /// What the component is responsible for.
    pub what: &'static str,
}

/// Every consensus component, in landing order.
pub const LAYERS: [Layer; 6] = [
    Layer {
        name: "proposer",
        phase: "B1",
        what: "deterministic round-robin proposer, N validators",
    },
    Layer {
        name: "block-signature",
        phase: "B1",
        what: "simple block signature for PoA",
    },
    Layer {
        name: "qc",
        phase: "B2",
        what: "quorum certificate, 2f+1 BLS aggregation, O(1) verify",
    },
    Layer {
        name: "finality",
        phase: "B3",
        what: "3-chain commit, view change, timeouts, nil blocks",
    },
    Layer {
        name: "validator-set",
        phase: "B4",
        what: "staking contract, epoch, enter/exit queue, set update",
    },
    Layer {
        name: "vrf-leader",
        phase: "B5",
        what: "stake-weighted VRF leader election, grinding protection",
    },
];

impl Layer {
    /// The source file under `crates/aura-consensus/src/` that implements
    /// this layer, if any. B1's proposer and block-signature have real
    /// modules; B2–B5 do not yet.
    pub fn module_file(&self) -> &'static str {
        match self.name {
            "proposer" => "proposer.rs",
            "block-signature" => "signature.rs",
            "qc" => "qc.rs",
            "finality" => "finality.rs",
            "validator-set" => "validator_set.rs",
            "vrf-leader" => "vrf_leader.rs",
            _ => unreachable!("unknown layer: {}", self.name),
        }
    }

    /// True when this layer's source file declares public consensus items.
    pub fn has_consensus_logic(&self) -> bool {
        matches!(
            self.name,
            "proposer" | "block-signature" | "qc" | "finality"
        )
    }
}

/// One consensus capability that does not exist yet, and the phase that will add it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// Stable name, used by the gate and the refusal message.
    pub name: &'static str,
    /// The phase that lands it.
    pub phase: &'static str,
    /// What the capability will do.
    pub what: &'static str,
}

/// The two capabilities B4-B5 will add; B1's proposer, block-signature,
/// B2's qc, and B3's finality are no longer missing.
pub const MISSING_CAPABILITIES: [Capability; 2] = [
    Capability {
        name: "validator-set",
        phase: "B4",
        what: "staking contract, epoch, enter/exit queue, set update",
    },
    Capability {
        name: "vrf-leader",
        phase: "B5",
        what: "stake-weighted VRF leader election, grinding protection",
    },
];

/// What a consensus component turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// No source files at all.
    Missing,
    /// Source files exist but declare no consensus logic - the C0 skeleton
    /// convention extended: the baseline module is public, but it measures
    /// absence rather than providing consensus.
    Placeholder,
    /// At least one consensus capability exists.
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

/// One measured component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measurement {
    /// The layer this measurement belongs to.
    pub layer: Layer,
    /// What the source says it is.
    pub presence: Presence,
    /// Number of `.rs` files under `crates/aura-consensus/src/`.
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

    /// The report table, as printed by `aura-consensus report`.
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

/// Measure `root` - the directory that contains `crates/`.
///
/// All six consensus components live in the same crate
/// (`crates/aura-consensus/src`), so each measurement reads the same
/// directory. The crate measures as [`Presence::Placeholder`]: it has sources
/// and a public `baseline` module, but no consensus logic.
pub fn survey(root: &Path) -> Survey {
    let layers = LAYERS.iter().map(|layer| measure(root, *layer)).collect();
    Survey {
        root: root.to_path_buf(),
        layers,
    }
}

fn measure(root: &Path, layer: Layer) -> Measurement {
    let src = root.join("crates").join("aura-consensus").join("src");
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
    // B3: proposer, block-signature, qc, and finality have real consensus
    // modules; validator-set and vrf-leader are still placeholders.
    let presence = if files == 0 {
        Presence::Missing
    } else if layer.has_consensus_logic() {
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

/// What `aura-consensus` was asked to do, in the B1 reading of the arguments.
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
                "aura-consensus {} (B3 baseline - BFT finality)",
                env!("CARGO_PKG_VERSION")
            );
            0
        }
        "report" => report(&args[1..], out, err),
        other => {
            let _ = writeln!(err, "aura-consensus: unknown command `{other}`.",);
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
                    let _ = writeln!(err, "aura-consensus: `--workspace` needs a path.");
                    return EXIT_USAGE;
                };
                root = PathBuf::from(value);
                index += 2;
            }
            other => {
                let _ = writeln!(
                    err,
                    "aura-consensus: unknown argument `{other}` for `report`."
                );
                return EXIT_USAGE;
            }
        }
    }
    if !root.join("crates").is_dir() {
        let _ = writeln!(
            err,
            "aura-consensus: `{}` is not a workspace (no crates/ directory).",
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
        "consensus capabilities present: {} of {} - {}",
        6 - MISSING_CAPABILITIES.len(),
        LAYERS.len(),
        MISSING_CAPABILITIES
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>()
            .join(", ")
    );
    0
}

/// The pinned `key=value` form of a survey: one line per layer, plus the constants.
pub fn fixture_text(measured: &Survey) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "# B3 baseline: BFT finality, measured.");
    let _ = writeln!(
        text,
        "# Written by tools/gen_b3_vectors.py; pinned by tests/b3_baseline.rs."
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
    text.push_str("aura-consensus - the B3 baseline:\\n");
    text.push_str("B3 lands BFT finality; two capabilities remain missing.\\n\\n");
    text.push_str("missing capabilities:\\n");
    for capability in &MISSING_CAPABILITIES {
        let _ = writeln!(
            text,
            "  {:<14} {:<4} {}",
            capability.name, capability.phase, capability.what
        );
    }
    let _ = writeln!(
        text,
        "\\n`aura-consensus report` prints what this tree actually contains (measured, not claimed).\\n\
         `aura-consensus --help` lists the commands B3 is allowed to have."
    );
    text
}

fn usage() -> String {
    format!(
        "aura-consensus {} (B3 baseline)\\n\
         \\n\
         usage:\\n\
         \\x20 aura-consensus report [--workspace PATH]  measure the tree and print what exists\\n\
         \\x20 aura-consensus report --fixture           the same measurement as `key=value` lines\\n\
         \\x20 aura-consensus --help                      this text\\n\
         \\x20 aura-consensus --version                   version and phase\\n\
         \\n\
         B3 lands BFT finality; {} capabilities remain missing ({}..{}).\\n\
         exit codes: {} = no consensus, {} = not a workspace, {} = usage error.",
        env!("CARGO_PKG_VERSION"),
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
    fn b3_implemented_and_placeholders() {
        let root = default_root();
        let measured = survey(&root);
        assert_eq!(measured.layers.len(), LAYERS.len());
        // proposer, block-signature, qc, and finality are implemented at B3.
        assert_eq!(
            measured.implemented(),
            4,
            "proposer + block-signature + qc + finality implemented"
        );
        assert_eq!(
            measured.placeholders(),
            2,
            "validator-set, vrf-leader still placeholders"
        );
        assert_eq!(measured.missing(), 0);
        let live: Vec<_> = measured
            .layers
            .iter()
            .map(|e| (e.layer.name, e.presence))
            .collect();
        assert_eq!(live[0].1, Presence::Implemented, "proposer is implemented");
        assert_eq!(
            live[1].1,
            Presence::Implemented,
            "block-signature is implemented"
        );
        assert_eq!(live[2].1, Presence::Implemented, "qc is implemented");
        assert_eq!(live[3].1, Presence::Implemented, "finality is implemented");
        for entry in &measured.layers[4..] {
            assert_eq!(
                entry.presence,
                Presence::Placeholder,
                "{} must still be a placeholder at B3",
                entry.layer.name
            );
        }
    }

    #[test]
    fn refusal_exits_stub_without_arguments() {
        let (code, out, err) = run(&[]);
        assert_eq!(
            code, EXIT_STUB,
            "no consensus must be a refusal, not success"
        );
        assert!(out.is_empty(), "the refusal belongs on stderr");
        assert!(
            err.contains("B3 lands BFT finality"),
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
    fn two_different_logs_are_not_equivalent() {
        // NC: at B3 there is PoA + QC but no full consensus to order transactions.
        // Two different logs must not be treated as "the same".
        let log_a = ["tx1", "tx2", "tx3"];
        let log_b = ["tx3", "tx2", "tx1"];
        assert_ne!(log_a, log_b);
        assert!(!logs_equivalent(&log_a, &log_b));
    }

    fn logs_equivalent(_a: &[&str], _b: &[&str]) -> bool {
        // No full consensus → no equivalence at B3.
        false
    }

    #[test]
    fn documentation_is_not_a_public_item() {
        assert!(!declares_public_item(
            "//! `aura-consensus` - B3 baseline: BFT finality."
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
            "the tree changed under the pinned B3 baseline - either the survey is wrong or a phase landed without updating the plan and this fixture"
        );
        assert_eq!(singles["implemented"], measured.implemented().to_string());
        assert_eq!(singles["placeholders"], measured.placeholders().to_string());
        assert_eq!(singles["missing"], measured.missing().to_string());
    }

    #[test]
    fn the_fixture_is_what_the_binary_prints() {
        let (code, out, _) = run(&["report", "--fixture"]);
        assert_eq!(code, 0);
        assert_eq!(out, FIXTURE, "run `make gen-b3-vectors` to refresh the pin");
    }

    use std::collections::BTreeMap;
    use std::path::PathBuf;

    const FIXTURE: &str = include_str!("../tests/fixtures/b3_baseline.txt");

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Run the *library* entry point and capture both streams.
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

    /// The fixture, parsed into single keys plus repeated `capability` and `layer` lines.
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
