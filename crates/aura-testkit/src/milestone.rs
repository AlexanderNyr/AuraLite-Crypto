//! O2 — M0/M1 milestone receipts (D-O2-1..D-O2-5).
//!
//! The milestones of `ROADMAP.md` §5 are measured, not asserted: M0 is
//! "genesis → 1000 tx → byte-identical state root on 2 runs", M1 is "4
//! validators, 1 block/s, 100 blocks without desync, RPC returns a
//! balance". [`run`] validates the measured `milestone.txt` receipt against
//! those criteria; a receipt whose two M0 state roots differ is the named
//! refusal `m0 state root drift` (`m0_root_drift_refused`).
//!
//! Design of record: `docs/o2-m0-m1-plan.md`. Vectors:
//! `tools/gen_o2_vectors.py` -> `tests/fixtures/o2_milestone_vectors.json`,
//! asserted byte-for-byte by `tests/o2_milestones.rs`. The measurements
//! themselves are `aurad-receipt` (`crates/aura-node/src/bin/`), which runs
//! the shipped pipeline/cluster; this module only checks the receipts and
//! never fabricates them.
//!
//! NC targets (`tools/nc_o2.py`):
//! 1. root-equality check dropped -> `m0_root_drift_refused` red (the
//!    named control: "byte-identical state root on 2 runs");
//! 2. validator-count check dropped -> `m1_validator_count_refused` red;
//! 3. desync check dropped -> `m1_desync_refused` red;
//! 4. height check dropped -> `m1_height_refused` red;
//! 5. rpc-balance check dropped -> `m1_rpc_balance_refused` red;
//! 6. unknown-line strictness dropped -> `foreign_receipt_line_refused`
//!    red (the format cannot carry commands).

use std::io::Write;

/// Exit code for policy refusals.
const EXIT_REFUSED: i32 = 65;

/// Exit code for usage errors.
const EXIT_USAGE: i32 = 64;

/// The receipt rule pinned by the format (D-O2-1).
const RULE: &str = "milestones-v1";

/// M0 replay transaction count (ROADMAP §5).
const M0_TXS: i64 = 1000;

/// M1 validator count (ROADMAP §5).
const M1_VALIDATORS: i64 = 4;

/// M1 minimum committed height (ROADMAP §5: 100 blocks).
const M1_MIN_HEIGHT: i64 = 100;

/// M1 maximum block interval in milliseconds (ROADMAP §5: 1 block/s).
const M1_MAX_BLOCK_MS: i64 = 1000;

// --------------------------------------------------------------------------- model

/// One recognized receipt line.
enum Item {
    /// `rule <name>`.
    Rule(String),
    /// `m0 txs <n>`.
    M0Txs(i64),
    /// `m0 state_root <hex-64>`.
    M0Root(String),
    /// `m1 <key> <n>`.
    M1(String, i64),
}

/// The parsed milestone receipt.
#[derive(Debug, Clone, Default)]
struct Receipt {
    /// The `rule` line value.
    rule: Option<String>,
    /// The `m0 txs` value.
    txs: Option<i64>,
    /// The `m0 state_root` values in file order.
    roots: Vec<String>,
    /// The `m1` keys (`validators`/`block_ms`/`height`/`desync`/
    /// `rpc_balance`).
    m1: Vec<(String, i64)>,
}

// --------------------------------------------------------------------------- parsing

/// Classify one receipt line; `None` means the line is not part of the
/// format (every shape or value problem lands here).
fn classify(line: &str) -> Option<Item> {
    if let Some(name) = line.strip_prefix("rule ") {
        if name.is_empty() || name.contains(' ') {
            return None;
        }
        return Some(Item::Rule(name.to_string()));
    }
    if let Some(rest) = line.strip_prefix("m0 txs ") {
        return Some(Item::M0Txs(rest.parse().ok()?));
    }
    if let Some(rest) = line.strip_prefix("m0 state_root ") {
        if rest.len() != 64 || !rest.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return None;
        }
        return Some(Item::M0Root(rest.to_string()));
    }
    if let Some(rest) = line.strip_prefix("m1 ") {
        let (key, value) = rest.split_once(' ')?;
        if !matches!(
            key,
            "validators" | "block_ms" | "height" | "desync" | "rpc_balance"
        ) {
            return None;
        }
        return Some(Item::M1(key.to_string(), value.parse().ok()?));
    }
    None
}

/// Parse the strict receipt text (D-O2-1). Unknown lines are refused so the
/// format cannot carry commands.
fn parse_receipt(text: &str) -> Result<Receipt, String> {
    let mut receipt = Receipt::default();
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(item) = classify(line) else {
            return Err(format!("unknown receipt line: {line}"));
        };
        match item {
            Item::Rule(name) => receipt.rule = Some(name),
            Item::M0Txs(value) => receipt.txs = Some(value),
            Item::M0Root(root) => receipt.roots.push(root),
            Item::M1(key, value) => {
                if let Some(slot) = receipt.m1.iter_mut().find(|(name, _)| *name == key) {
                    slot.1 = value;
                } else {
                    receipt.m1.push((key, value));
                }
            }
        }
    }
    Ok(receipt)
}

/// Fetch a required `m1` value.
fn need(receipt: &Receipt, key: &str) -> Result<i64, String> {
    receipt
        .m1
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| *value)
        .ok_or_else(|| format!("missing receipt line: m1 {key}"))
}

// --------------------------------------------------------------------------- checks

/// The M0/M1 criterion check (D-O2-1), in the pinned order; returns the ok
/// line.
fn check(receipt: &Receipt) -> Result<String, String> {
    match receipt.rule.as_deref() {
        None => return Err("missing rule line".to_string()),
        Some(RULE) => {}
        Some(other) => return Err(format!("bad rule: {other}")),
    }

    // Every criterion value must be present.
    let txs = receipt
        .txs
        .ok_or_else(|| "missing receipt line: m0 txs".to_string())?;
    if receipt.roots.len() != 2 {
        return Err(format!("m0 state root count: {}", receipt.roots.len()));
    }
    let validators = need(receipt, "validators")?;
    let block_ms = need(receipt, "block_ms")?;
    let height = need(receipt, "height")?;
    let desync = need(receipt, "desync")?;
    let rpc_balance = need(receipt, "rpc_balance")?;

    // M0: the two replays must agree byte-for-byte (the named gate).
    if receipt.roots[0] != receipt.roots[1] {
        return Err("m0 state root drift".to_string());
    }

    // M0: exactly 1000 replayed transactions.
    if txs != M0_TXS {
        return Err(format!("m0 tx count: {txs}"));
    }

    // M1: 4 validators.
    if validators != M1_VALIDATORS {
        return Err(format!("m1 validators: {validators}"));
    }

    // M1: 100 blocks.
    if height < M1_MIN_HEIGHT {
        return Err(format!("m1 height: {height}"));
    }

    // M1: without desync.
    if desync != 0 {
        return Err(format!("m1 desync: {desync}"));
    }

    // M1: RPC returns a (positive) balance.
    if rpc_balance <= 0 {
        return Err(format!("m1 rpc balance: {rpc_balance}"));
    }

    // M1: at least 1 block/s.
    if block_ms > M1_MAX_BLOCK_MS {
        return Err(format!("m1 block rate: {block_ms}"));
    }

    Ok("milestones=ok m0=ok m1=ok".to_string())
}

/// The normalized receipt report (`summary` command).
fn summary(receipt: &Receipt) -> Result<String, String> {
    check(receipt)?;
    let roots = if receipt.roots[0] == receipt.roots[1] {
        "identical"
    } else {
        "drift"
    };
    let value = |key: &str| -> i64 {
        receipt
            .m1
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| *value)
            .unwrap_or(0)
    };
    Ok(format!(
        "m0 replay: txs={} roots={roots}\nm1 devnet: validators={} height={} desync={} rpc_balance={} block_ms={}\n",
        receipt.txs.unwrap_or(0),
        value("validators"),
        value("height"),
        value("desync"),
        value("rpc_balance"),
        value("block_ms"),
    ))
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

/// Execute one `aura-milestone` command against the given streams; returns
/// the exit code (0 ok, 64 usage, 65 refusal).
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut dispatch = || -> Result<(), String> {
        let (command, rest) = args.split_first().ok_or("usage:missing command")?;
        let map = flags(rest).map_err(|message| format!("usage:{message}"))?;
        let receipts_path = map
            .get("receipts")
            .cloned()
            .unwrap_or_else(|| "release/milestone.txt".to_string());
        let receipt = parse_receipt(&read_text(&receipts_path)?)?;
        match command.as_str() {
            "check" => {
                let ok = check(&receipt)?;
                let _ = writeln!(out, "{ok}");
                Ok(())
            }
            "summary" => {
                let report = summary(&receipt)?;
                let _ = write!(out, "{report}");
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
                let _ = writeln!(err, "error: milestone: {message}");
                EXIT_REFUSED
            }
        },
    }
}
