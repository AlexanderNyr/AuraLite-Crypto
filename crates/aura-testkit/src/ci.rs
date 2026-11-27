//! O0 — CI shard registry and the target registry check.
//!
//! The CI layout is a checked registry, not YAML folklore: `ci/registry.txt`
//! pins the ten lanes of `CONTRIBUTING.md` (core, state, exec, consensus,
//! net, node, wallet, ecosystem, security, perf) over the workspace's test
//! targets (`<pkg>/--lib` unit tests and `<pkg>/<stem>` integration tests)
//! and [`run`] validates it against the discovered tree ("target registry
//! check", D-O0-2). A disabled shard that holds targets, an empty shard, a
//! shard whose **main test** is not registered to it (the "dead gate" of the
//! MATURITY_AUDIT lesson), or a workflow matrix missing a shard are all
//! refusals — the O0 gate (`disabled_shard_refused`).
//!
//! Design of record: `docs/o0-ci-plan.md` (D-O0-1..D-O0-5). Vectors:
//! `tools/gen_o0_vectors.py` ->
//! `crates/aura-testkit/tests/fixtures/o0_ci_vectors.json`, asserted
//! byte-for-byte by `tests/o0_ci.rs`.
//!
//! NC targets (`tools/nc_o0.py`):
//! 1. disabled-shard check dropped -> `disabled_shard_refused` red (the
//!    named control: "disable a shard -> CI red");
//! 2. missing-target check dropped -> `missing_target_refused` red;
//! 3. duplicate-target check dropped -> `duplicate_target_refused` red;
//! 4. shard-set check dropped -> `shard_set_mismatch_refused` red;
//! 5. empty-shard check dropped -> `empty_shard_refused` red;
//! 6. unknown-line strictness dropped -> `foreign_registry_line_refused`
//!    red (D-O0-3: the format cannot carry commands — no secret shards).

use std::io::Write;
use std::path::{Path, PathBuf};

/// Exit code for policy refusals.
const EXIT_REFUSED: i32 = 65;

/// Exit code for usage errors.
const EXIT_USAGE: i32 = 64;

/// The assignment rule pinned by the registry format (D-O0-1).
const RULE: &str = "lanes-v1";

/// The ten CI lanes (CONTRIBUTING.md), in canonical order.
const SHARDS: [&str; 10] = [
    "core",
    "state",
    "exec",
    "consensus",
    "net",
    "node",
    "wallet",
    "ecosystem",
    "security",
    "perf",
];

/// The main test pinned by each lane (same order as [`SHARDS`], D-O0-1).
/// A shard that does not register its main test is a dead gate.
const MAIN: [&str; 10] = [
    "aura-codec/c2_codec",
    "aura-state/s0_baseline",
    "aura-exec/x0_census",
    "aura-consensus/b4_validator_set",
    "aura-p2p/n1_transport",
    "aura-node/d0_baseline",
    "aura-wallet/w1_keystore",
    "aura-storage/e1_indexer",
    "aura-consensus/h5_safety_model",
    "aura-bench/p0_bench",
];

// --------------------------------------------------------------------------- model

/// A discovered or registered test target (`<pkg>/<name>`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Target {
    /// The workspace package.
    pkg: String,
    /// `--lib` for unit tests, otherwise the integration test file stem.
    name: String,
}

impl Target {
    /// Parse `<pkg>/<name>`.
    fn parse(text: &str) -> Result<Target, String> {
        let (pkg, name) = text
            .split_once('/')
            .ok_or_else(|| format!("bad target: {text}"))?;
        if pkg.is_empty() || name.is_empty() || name.contains('/') {
            return Err(format!("bad target: {text}"));
        }
        Ok(Target {
            pkg: pkg.to_string(),
            name: name.to_string(),
        })
    }

    /// The `cargo test` command that runs this target (D-O0-3).
    fn command(&self) -> String {
        if self.name == "--lib" {
            format!("cargo test -p {} --lib", self.pkg)
        } else {
            format!("cargo test -p {} --test {}", self.pkg, self.name)
        }
    }

    /// The registry name form `<pkg>/<name>`.
    fn key(&self) -> String {
        format!("{}/{}", self.pkg, self.name)
    }

    /// The lane assignment used by `sync` (D-O0-1): unit tests follow the
    /// package's primary lane; integration tests follow their phase prefix
    /// (`c`/`o` -> core, `s`/`t` -> state, `x` -> exec, `b` -> consensus,
    /// `n` -> net, `d` -> node, `w` -> wallet, `e` -> ecosystem,
    /// `h` -> security, `p` -> perf), with the rig files pinned to core and
    /// `identity` to ecosystem.
    fn lane(&self) -> &'static str {
        if self.name == "--lib" {
            return match self.pkg.as_str() {
                "aura-state" | "aura-storage" => "state",
                "aura-exec" => "exec",
                "aura-consensus" => "consensus",
                "aura-p2p" => "net",
                "aura-node" | "aura-mempool" => "node",
                "aura-wallet" | "aura-genesis" => "wallet",
                "aura-sdk" | "aura-sdk-macros" | "aura-rpc" | "aura-grpc-rest" => "ecosystem",
                "aura-bench" => "perf",
                _ => "core",
            };
        }
        match self.name.chars().next() {
            Some('c' | 'o') => "core",
            Some('s' | 't') => "state",
            Some('x') => "exec",
            Some('b') => "consensus",
            Some('n') => "net",
            Some('d') => "node",
            Some('w') => "wallet",
            Some('e') => "ecosystem",
            Some('h') => "security",
            Some('p') => "perf",
            _ => match self.name.as_str() {
                "genesis_cli" | "harness" | "properties" | "vectors" => "core",
                "identity" => "ecosystem",
                _ => "core",
            },
        }
    }
}

/// The parsed shard registry.
#[derive(Debug, Clone)]
struct Registry {
    /// Assignment rule name from the `rule` line.
    rule: String,
    /// Shard declarations (`shard <name> <enabled|disabled>`) in file order.
    shards: Vec<(String, bool)>,
    /// Main-test pins (`main <shard> <target>`) in file order.
    mains: Vec<(String, Target)>,
    /// Registered targets (`target <pkg>/<name> shard <shard>`) in file order.
    targets: Vec<(Target, String)>,
}

// --------------------------------------------------------------------------- parsing

/// Parse an inventory text (`inventory v1` + `target pkg/name` lines).
fn parse_inventory(text: &str) -> Result<Vec<Target>, String> {
    let mut targets = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim_end();
        if index == 0 {
            if line != "inventory v1" {
                return Err(format!("bad inventory header: {line}"));
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.strip_prefix("target ") {
            Some(name) => targets.push(Target::parse(name)?),
            None => return Err(format!("unknown inventory line: {line}")),
        }
    }
    targets.sort();
    Ok(targets)
}

/// Parse a registry text (D-O0-1 format). Unknown lines are refused so the
/// format cannot carry commands (D-O0-3).
fn parse_registry(text: &str) -> Result<Registry, String> {
    let mut rule = None;
    let mut shards = Vec::new();
    let mut mains = Vec::new();
    let mut targets = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix("rule ") {
            rule = Some(name.to_string());
        } else if let Some(rest) = line.strip_prefix("shard ") {
            let (name, state) = rest
                .split_once(' ')
                .ok_or_else(|| format!("unknown registry line: {line}"))?;
            let disabled = match state {
                "enabled" => false,
                "disabled" => true,
                _ => return Err(format!("unknown registry line: {line}")),
            };
            shards.push((name.to_string(), disabled));
        } else if let Some(rest) = line.strip_prefix("main ") {
            let (shard, target) = rest
                .split_once(' ')
                .ok_or_else(|| format!("unknown registry line: {line}"))?;
            mains.push((shard.to_string(), Target::parse(target)?));
        } else if let Some(rest) = line.strip_prefix("target ") {
            let (target, shard) = rest
                .split_once(" shard ")
                .ok_or_else(|| format!("unknown registry line: {line}"))?;
            targets.push((Target::parse(target)?, shard.to_string()));
        } else {
            return Err(format!("unknown registry line: {line}"));
        }
    }
    Ok(Registry {
        rule: rule.ok_or("missing rule line")?,
        shards,
        mains,
        targets,
    })
}

// --------------------------------------------------------------------------- checks

/// Count how many registry entries name `target`.
fn registrations(registry: &Registry, target: &Target) -> usize {
    registry
        .targets
        .iter()
        .filter(|(entry, _)| entry == target)
        .count()
}

/// How many registry targets sit in `shard`.
fn shard_load(registry: &Registry, shard: &str) -> usize {
    registry
        .targets
        .iter()
        .filter(|(_, name)| name == shard)
        .count()
}

/// The target registry check (D-O0-2); returns the inventory size on success.
fn check(inventory: &[Target], registry: &Registry) -> Result<usize, String> {
    // Shard set: the ten lanes of CONTRIBUTING.md (NC-4 target).
    for name in SHARDS {
        if !registry.shards.iter().any(|(declared, _)| declared == name) {
            return Err(format!("missing shard: {name}"));
        }
    }
    for (declared, _) in &registry.shards {
        if !SHARDS.contains(&declared.as_str()) {
            return Err(format!("unknown shard: {declared}"));
        }
    }

    // No empty shards (NC-5 target).
    for name in SHARDS {
        if shard_load(registry, name) == 0 {
            return Err(format!("shard {name} empty"));
        }
    }

    // Main-test pins: every shard must declare and register its main test
    // (the "dead gate" lesson of CONTRIBUTING.md).
    for name in SHARDS {
        let pinned = registry
            .mains
            .iter()
            .find(|(shard, _)| shard == name)
            .map(|(_, target)| target.clone());
        let Some(pinned) = pinned else {
            return Err(format!("missing main: {name}"));
        };
        let registered_here = registry
            .targets
            .iter()
            .any(|(target, shard)| target == &pinned && shard == name);
        if !registered_here || !inventory.binary_search(&pinned).is_ok() {
            return Err(format!("shard {name} main dead: {}", pinned.key()));
        }
    }

    // Stale entries: every registered target exists.
    for (target, _) in &registry.targets {
        if !inventory.binary_search(target).is_ok() {
            return Err(format!("unknown target: {}", target.key()));
        }
    }

    // No target in two shards (NC-3 target).
    for target in inventory {
        if registrations(registry, target) > 1 {
            return Err(format!("target in two shards: {}", target.key()));
        }
    }

    // Every discovered target is registered (NC-2 target).
    for target in inventory {
        if registrations(registry, target) == 0 {
            return Err(format!("unregistered target: {}", target.key()));
        }
    }

    // A disabled shard that holds targets makes CI red (NC-1 target, the
    // named control).
    for (name, disabled) in &registry.shards {
        if *disabled && shard_load(registry, name) > 0 {
            return Err(format!("shard {name} disabled"));
        }
    }

    Ok(inventory.len())
}

/// Check the workflow's matrix against the enabled shards (D-O0-2): every
/// enabled shard must appear in a `shard:` matrix list.
fn check_workflow(registry: &Registry, text: &str) -> Result<(), String> {
    let mut items: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("shard:") {
            let list = rest.trim().trim_start_matches('[').trim_end_matches(']');
            for item in list.split(',') {
                let name = item.trim().replace(['{', '}', '$'], "");
                if !name.is_empty() {
                    items.push(name);
                }
            }
        }
    }
    for (name, disabled) in &registry.shards {
        if !*disabled && !items.iter().any(|item| item == name) {
            return Err(format!("workflow matrix missing shard: {name}"));
        }
    }
    Ok(())
}

/// The deterministic target-coverage report (D-O0-4); refuses on an invalid
/// registry.
fn coverage_report(inventory: &[Target], registry: &Registry) -> Result<String, String> {
    let total = check(inventory, registry)?;
    let mut out = String::new();
    out.push_str("ci coverage report v1\n");
    out.push_str(&format!("rule {}\n", registry.rule));
    out.push_str("shards 10\n");
    out.push_str(&format!("targets {total}\n"));
    let mut covered = 0usize;
    for name in SHARDS {
        let count = shard_load(registry, name);
        out.push_str(&format!("shard {name} targets={count}\n"));
        covered += count;
    }
    let per_mille = (covered * 1000).checked_div(total).unwrap_or(0);
    out.push_str(&format!(
        "target_coverage {covered}/{total} ({}.{:01}%)\n",
        per_mille / 10,
        per_mille % 10
    ));
    Ok(out)
}

/// The `cargo test` commands of one shard, in sorted target order
/// (D-O0-3: plain `cargo test` forms only).
fn shard_commands(registry: &Registry, shard: &str) -> Result<Vec<String>, String> {
    if !SHARDS.contains(&shard) {
        return Err(format!("bad shard: {shard}"));
    }
    let mut targets: Vec<&Target> = registry
        .targets
        .iter()
        .filter(|(_, name)| name == shard)
        .map(|(target, _)| target)
        .collect();
    targets.sort();
    targets.dedup();
    Ok(targets.iter().map(|target| target.command()).collect())
}

// --------------------------------------------------------------------------- filesystem

/// Discover the workspace's test targets: every `crates/*/src/lib.rs` and
/// `crates/*/tests/*.rs` (documented scope of the registry — D-O0-1).
fn discover(workspace: &Path) -> Result<Vec<Target>, String> {
    let crates = workspace.join("crates");
    let mut targets = Vec::new();
    let entries = std::fs::read_dir(&crates).map_err(|_| "bad workspace".to_string())?;
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    dirs.sort();
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        let Some(pkg) = dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        else {
            continue;
        };
        if dir.join("src/lib.rs").is_file() {
            targets.push(Target {
                pkg: pkg.clone(),
                name: "--lib".to_string(),
            });
        }
        let tests = dir.join("tests");
        if let Ok(entries) = std::fs::read_dir(&tests) {
            let mut files: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
            files.sort();
            for file in files {
                if file.extension().is_some_and(|ext| ext == "rs") {
                    let stem = file
                        .file_stem()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default();
                    targets.push(Target {
                        pkg: pkg.clone(),
                        name: stem,
                    });
                }
            }
        }
    }
    targets.sort();
    Ok(targets)
}

/// Render a fresh registry: lane-table assignment (D-O0-1), preserving
/// disabled flags and the canonical main pins.
fn sync_registry(inventory: &[Target], previous: Option<&Registry>) -> String {
    let mut out = String::new();
    out.push_str("# AuraLite CI shard registry (O0) — the target registry check reads this.\n");
    out.push_str("# Shards are the ten lanes of CONTRIBUTING.md; each pins its main test\n");
    out.push_str("# (a shard that does not run its main test is a dead gate).\n");
    out.push_str("# Regenerate with `make ci-sync`. Format: docs/o0-ci-plan.md (D-O0-1).\n");
    out.push_str(&format!("rule {RULE}\n"));
    for name in SHARDS {
        let disabled = previous
            .map(|registry| {
                registry
                    .shards
                    .iter()
                    .any(|(declared, off)| declared == name && *off)
            })
            .unwrap_or(false);
        out.push_str(&format!(
            "shard {name} {}\n",
            if disabled { "disabled" } else { "enabled" }
        ));
    }
    for (index, name) in SHARDS.iter().enumerate() {
        out.push_str(&format!("main {name} {}\n", MAIN[index]));
    }
    let mut sorted: Vec<&Target> = inventory.iter().collect();
    sorted.sort();
    sorted.dedup();
    for target in sorted {
        out.push_str(&format!(
            "target {} shard {}\n",
            target.key(),
            target.lane()
        ));
    }
    out
}

// --------------------------------------------------------------------------- CLI driver

/// Read a whole file to text.
fn read_text(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|_| format!("cannot read {path}"))
}

/// Collect `--flag value` pairs from the argument tail.
fn flags(rest: &[String]) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut map = std::collections::BTreeMap::new();
    let mut index = 0;
    while index < rest.len() {
        let flag = &rest[index];
        let name = flag
            .strip_prefix("--")
            .ok_or_else(|| format!("expected --flag, got {flag}"))?;
        if index + 1 >= rest.len() {
            return Err(format!("missing value for --{name}"));
        }
        map.insert(name.to_string(), rest[index + 1].clone());
        index += 2;
    }
    Ok(map)
}

/// Load the inventory: `--inventory FILE`, else discover `--workspace DIR`
/// (default `.`).
fn load_inventory(map: &std::collections::BTreeMap<String, String>) -> Result<Vec<Target>, String> {
    match map.get("inventory") {
        Some(path) => parse_inventory(&read_text(path)?),
        None => discover(Path::new(
            map.get("workspace").map(String::as_str).unwrap_or("."),
        )),
    }
}

/// Execute one `aura-ci` command against the given streams; returns the exit
/// code (0 ok, 64 usage, 65 refusal).
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut dispatch = || -> Result<(), String> {
        let (command, rest) = args.split_first().ok_or("usage:missing command")?;
        let map = flags(rest).map_err(|message| format!("usage:{message}"))?;
        let registry_path = map
            .get("registry")
            .cloned()
            .unwrap_or_else(|| "ci/registry.txt".to_string());
        let inventory = load_inventory(&map)?;
        match command.as_str() {
            "check" => {
                let registry = parse_registry(&read_text(&registry_path)?)?;
                let total = check(&inventory, &registry)?;
                if let Some(path) = map.get("workflow") {
                    check_workflow(&registry, &read_text(path)?)?;
                }
                let _ = writeln!(out, "registry=ok shards=10 targets={total}");
                Ok(())
            }
            "coverage" => {
                let registry = parse_registry(&read_text(&registry_path)?)?;
                let report = coverage_report(&inventory, &registry)?;
                let _ = write!(out, "{report}");
                Ok(())
            }
            "run" => {
                let registry = parse_registry(&read_text(&registry_path)?)?;
                let shard = map
                    .get("shard")
                    .ok_or_else(|| "usage:missing --shard".to_string())?;
                check(&inventory, &registry)?;
                let commands = shard_commands(&registry, shard)?;
                let _ = writeln!(out, "ci run shard {shard}");
                for command in &commands {
                    let _ = writeln!(out, "{command}");
                }
                if map.contains_key("exec") {
                    for command in &commands {
                        let mut parts = command.split_whitespace();
                        let program = parts.next().unwrap_or("true");
                        let status = std::process::Command::new(program)
                            .args(parts)
                            .status()
                            .map_err(|_| format!("cannot run {command}"))?;
                        if !status.success() {
                            return Err(format!("command failed: {command}"));
                        }
                    }
                }
                Ok(())
            }
            "sync" => {
                let previous = read_text(&registry_path)
                    .ok()
                    .and_then(|text| parse_registry(&text).ok());
                let text = sync_registry(&inventory, previous.as_ref());
                if let Some(parent) = Path::new(&registry_path).parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|_| "cannot write registry".to_string())?;
                }
                std::fs::write(&registry_path, text)
                    .map_err(|_| "cannot write registry".to_string())?;
                let _ = writeln!(out, "synced targets={} shards=10", inventory.len());
                Ok(())
            }
            other => Err(format!("usage:unknown command {other}")),
        }
    };
    match dispatch() {
        Ok(()) => 0,
        Err(message) => match message.strip_prefix("usage:") {
            Some(detail) => {
                let _ = writeln!(err, "error: usage: {detail}");
                EXIT_USAGE
            }
            None => {
                let _ = writeln!(err, "error: ci: {message}");
                EXIT_REFUSED
            }
        },
    }
}
