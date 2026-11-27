//! O3 — closed-testnet ops receipts: uptime ledger, state losses and the
//! backup/restore drill (D-O3-1..D-O3-5).
//!
//! [`run`] validates the `testnet-ops.txt` receipt against the OPS_PLAN
//! O3 gate ("7 days uptime, 0 state losses, 1 restore drill") and rolls
//! up the uptime ledger. A receipt whose two `restore state_root` lines
//! differ — the restore did not reproduce the backed-up head — is the
//! named refusal `restore_mismatch_refused`
//! (`error: testnet-ops: restore mismatch`).
//!
//! Design of record: `docs/o3-testnet-ops-plan.md`. Vectors:
//! `tools/gen_o3_vectors.py` -> `tests/fixtures/o3_testnet_ops_vectors.json`,
//! asserted byte-for-byte by `tests/o3_testnet_ops.rs`. The measurements
//! themselves are `aurad-ops` (`crates/aura-node/src/bin/`), which runs
//! the scripted operator schedule and the live backup/restore drill;
//! this module only checks receipts and re-derives ledger rollups — it
//! never fabricates values.
//!
//! NC targets (`tools/nc_o3.py`):
//! 1. root-equality check dropped -> `restore_mismatch_refused` red (the
//!    named control: "0 state losses" starts with "the restore restored");
//! 2. state-losses check dropped -> `state_losses_refused` red;
//! 3. uptime-days check dropped -> `uptime_days_refused` red;
//! 4. restore-drills check dropped -> `restore_drills_refused` red;
//! 5. root-count check dropped -> `restore_root_count_refused` red;
//! 6. unknown-line strictness dropped -> `foreign_ops_line_refused` red.

use std::io::Write;

/// Exit code for policy refusals.
const EXIT_REFUSED: i32 = 65;

/// Exit code for usage errors.
const EXIT_USAGE: i32 = 64;

/// The receipt rule pinned by the format (D-O3-1).
const RULE: &str = "testnet-ops-v1";

/// Required uptime days (OPS_PLAN row O3).
const UPTIME_DAYS: i64 = 7;

/// Hours per day in the operator schedule (D-O3-3).
const HOURS_PER_DAY: i64 = 24;

// --------------------------------------------------------------------------- model

/// One recognized receipt line.
enum Item {
    /// `rule <name>`.
    Rule(String),
    /// `uptime days <n>`.
    UptimeDays(i64),
    /// `state losses <n>`.
    StateLosses(i64),
    /// `restore drills <n>`.
    RestoreDrills(i64),
    /// `restore state_root <hex-64>`.
    RestoreRoot(String),
}

/// The parsed ops receipt.
#[derive(Debug, Clone, Default)]
struct Receipt {
    /// The `rule` line value.
    rule: Option<String>,
    /// The `uptime days` value.
    days: Option<i64>,
    /// The `state losses` value.
    losses: Option<i64>,
    /// The `restore drills` value.
    drills: Option<i64>,
    /// The `restore state_root` values in file order (pre-backup,
    /// post-restore).
    roots: Vec<String>,
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
    if let Some(rest) = line.strip_prefix("uptime days ") {
        return Some(Item::UptimeDays(rest.parse().ok()?));
    }
    if let Some(rest) = line.strip_prefix("state losses ") {
        return Some(Item::StateLosses(rest.parse().ok()?));
    }
    if let Some(rest) = line.strip_prefix("restore drills ") {
        return Some(Item::RestoreDrills(rest.parse().ok()?));
    }
    if let Some(rest) = line.strip_prefix("restore state_root ") {
        if rest.len() != 64 || !rest.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return None;
        }
        return Some(Item::RestoreRoot(rest.to_string()));
    }
    None
}

/// Parse the strict receipt text (D-O3-1). Unknown lines are refused so
/// the format cannot carry commands.
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
            Item::UptimeDays(value) => receipt.days = Some(value),
            Item::StateLosses(value) => receipt.losses = Some(value),
            Item::RestoreDrills(value) => receipt.drills = Some(value),
            Item::RestoreRoot(root) => receipt.roots.push(root),
        }
    }
    Ok(receipt)
}

// --------------------------------------------------------------------------- ledger rollup

/// One heartbeat row (`uptime <day> <hour> <height> <root>`).
struct Heartbeat {
    /// Operator-schedule day index.
    day: i64,
    /// Operator-schedule hour index (0..23).
    hour: i64,
    /// Committed chain height at the poll.
    height: u64,
    /// State root at the poll (hex-64).
    root: String,
}

/// Classify one ledger row; `None` means the row is not part of the
/// format.
fn classify_heartbeat(line: &str) -> Option<Heartbeat> {
    let rest = line.strip_prefix("uptime ")?;
    let mut fields = rest.split(' ');
    let day = fields.next()?.parse().ok()?;
    let hour = fields.next()?.parse().ok()?;
    let height = fields.next()?.parse().ok()?;
    let root = fields.next()?;
    if fields.next().is_some() {
        return None;
    }
    if !(0..HOURS_PER_DAY).contains(&hour) {
        return None;
    }
    if root.len() != 64 || !root.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some(Heartbeat {
        day,
        hour,
        height,
        root: root.to_string(),
    })
}

/// Roll the ledger up (D-O3-3): `days` counts day-buckets with all 24
/// hourly polls present, `losses` counts head regressions (height went
/// backwards, or the same height with a different root). The first loss
/// is refused with `state loss at <day> <hour>`.
fn rollup_ledger(text: &str) -> Result<(i64, i64), String> {
    let mut rows: Vec<Heartbeat> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(row) = classify_heartbeat(line) else {
            return Err(format!("bad heartbeat: {line}"));
        };
        rows.push(row);
    }
    rows.sort_by_key(|row| (row.day, row.hour));
    let mut previous: Option<&Heartbeat> = None;
    for row in &rows {
        if let Some(last) = previous {
            if (row.day, row.hour) == (last.day, last.hour) {
                return Err(format!("bad heartbeat: duplicate {} {}", row.day, row.hour));
            }
            // A head regression is a state loss: refused at its first
            // occurrence, with the slot that recorded it.
            if row.height < last.height || (row.height == last.height && row.root != last.root) {
                return Err(format!("state loss at {} {}", row.day, row.hour));
            }
        }
        previous = Some(row);
    }
    // A completed ledger that passed the regression scan has zero losses.
    let losses = 0i64;
    let mut days = 0i64;
    let mut index = 0;
    while index < rows.len() {
        let day = rows[index].day;
        let mut slots: Vec<i64> = Vec::new();
        while index < rows.len() && rows[index].day == day {
            slots.push(rows[index].hour);
            index += 1;
        }
        slots.sort_unstable();
        slots.dedup();
        if slots.len() as i64 == HOURS_PER_DAY && slots.first() == Some(&0) {
            days += 1;
        }
    }
    Ok((days, losses))
}

// --------------------------------------------------------------------------- checks

/// The O3 criterion check (D-O3-1), in the pinned order; returns the ok
/// line.
fn check(receipt: &Receipt) -> Result<String, String> {
    match receipt.rule.as_deref() {
        None => return Err("missing rule line".to_string()),
        Some(RULE) => {}
        Some(other) => return Err(format!("bad rule: {other}")),
    }

    let days = receipt
        .days
        .ok_or_else(|| "missing receipt line: uptime days".to_string())?;
    let losses = receipt
        .losses
        .ok_or_else(|| "missing receipt line: state losses".to_string())?;
    let drills = receipt
        .drills
        .ok_or_else(|| "missing receipt line: restore drills".to_string())?;
    if receipt.roots.len() != 2 {
        return Err(format!("restore state root count: {}", receipt.roots.len()));
    }

    // The restore must reproduce the backed-up head byte-for-byte (the
    // named gate — "0 state losses" starts here).
    if receipt.roots[0] != receipt.roots[1] {
        return Err("restore mismatch".to_string());
    }

    // Zero state losses across the whole window.
    if losses != 0 {
        return Err(format!("state losses: {losses}"));
    }

    // At least one completed restore drill.
    if drills < 1 {
        return Err(format!("restore drills: {drills}"));
    }

    // Seven days of uptime.
    if days < UPTIME_DAYS {
        return Err(format!("uptime days: {days}"));
    }

    Ok("testnet-ops=ok uptime=ok state=ok drills=ok".to_string())
}

/// The normalized receipt report (`summary` command).
fn summary(receipt: &Receipt) -> Result<String, String> {
    check(receipt)?;
    let roots = if receipt.roots[0] == receipt.roots[1] {
        "identical"
    } else {
        "drift"
    };
    Ok(format!(
        "uptime: days={} state_losses={}\nrestore: drills={} roots={roots}\n",
        receipt.days.unwrap_or(0),
        receipt.losses.unwrap_or(0),
        receipt.drills.unwrap_or(0),
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

/// Execute one `aura-ops` command against the given streams; returns the
/// exit code (0 ok, 64 usage, 65 refusal).
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut dispatch = || -> Result<(), String> {
        let (command, rest) = args.split_first().ok_or("usage:missing command")?;
        let map = flags(rest).map_err(|message| format!("usage:{message}"))?;
        match command.as_str() {
            "check" => {
                let receipts_path = map
                    .get("receipts")
                    .cloned()
                    .unwrap_or_else(|| "release/testnet-ops.txt".to_string());
                let receipt = parse_receipt(&read_text(&receipts_path)?)?;
                let ok = check(&receipt)?;
                let _ = writeln!(out, "{ok}");
                Ok(())
            }
            "summary" => {
                let receipts_path = map
                    .get("receipts")
                    .cloned()
                    .unwrap_or_else(|| "release/testnet-ops.txt".to_string());
                let receipt = parse_receipt(&read_text(&receipts_path)?)?;
                let report = summary(&receipt)?;
                let _ = write!(out, "{report}");
                Ok(())
            }
            "ledger" => {
                let ledger_path = map
                    .get("ledger")
                    .cloned()
                    .unwrap_or_else(|| "release/uptime-7d.txt".to_string());
                let (days, losses) = rollup_ledger(&read_text(&ledger_path)?)?;
                let _ = writeln!(out, "uptime ledger: days={days} state_losses={losses}");
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
                let _ = writeln!(err, "error: testnet-ops: {message}");
                EXIT_REFUSED
            }
        },
    }
}
