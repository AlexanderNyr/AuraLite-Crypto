//! O4 — public testnet (M2) receipts: the metrics ledger, the bug-bounty
//! triage journal and the 10-validator window (D-O4-1..D-O4-5).
//!
//! [`run`] validates the `testnet.txt` receipt against the OPS_PLAN O4
//! gate ("30 days, >= 99% uptime, >= 100 000 txs, 0 unfixed criticals")
//! and rolls up the two ledgers. A receipt carrying any unfixed critical
//! (the triage journal's open `critical`/`high` class) is the named
//! refusal `unfixed_criticals_refused` (`error: testnet: criticals: <n>`,
//! exit 65); the triage rollup refuses the same condition by finding id
//! (`open critical <id>`).
//!
//! Design of record: `docs/o4-testnet-public-plan.md`. Vectors:
//! `tools/gen_o4_vectors.py` -> `tests/fixtures/o4_testnet_public_vectors.json`,
//! asserted byte-for-byte by `tests/o4_testnet_public.rs`. The
//! measurements themselves are `aurad-testnet`
//! (`crates/aura-node/src/bin/`), which runs the scripted 30-day
//! schedule on the real pipeline and opens the ten validator nodes;
//! this module only checks receipts and re-derives ledger rollups — it
//! never fabricates values.
//!
//! NC targets (`tools/nc_o4.py`):
//! 1. criticals check dropped -> `unfixed_criticals_refused` red (the
//!    named control: "0 unfixed criticals");
//! 2. days check dropped -> `days_shortfall_refused` red;
//! 3. uptime check dropped -> `uptime_shortfall_refused` red;
//! 4. validators check dropped -> `validators_shortfall_refused` red;
//! 5. txs check dropped -> `txs_shortfall_refused` red;
//! 6. metrics regression check dropped -> `metrics_tx_regression_refused`
//!    red;
//! 7. triage open-critical check dropped -> `triage_open_critical_refused`
//!    red.

use std::io::Write;

/// Exit code for policy refusals.
const EXIT_REFUSED: i32 = 65;

/// Exit code for usage errors.
const EXIT_USAGE: i32 = 64;

/// The receipt rule pinned by the format (D-O4-1).
const RULE: &str = "testnet-v1";

/// Required window length in days (OPS_PLAN row O4).
const DAYS: i64 = 30;

/// Required uptime percent (OPS_PLAN row O4: ">= 99% uptime").
const UPTIME_PCT: i64 = 99;

/// Required independent validator nodes (OPS_PLAN row O4).
const VALIDATORS: i64 = 10;

/// Required total transactions (OPS_PLAN row O4: ">= 100 000 txs").
const TXS: i64 = 100_000;

/// Hours per day in the operator schedule (D-O4-2).
const HOURS_PER_DAY: i64 = 24;

// --------------------------------------------------------------------------- receipt

/// One recognized receipt line.
enum Item {
    /// `rule <name>`.
    Rule(String),
    /// `days <n>`.
    Days(i64),
    /// `uptime <n>`.
    Uptime(i64),
    /// `validators <n>`.
    Validators(i64),
    /// `txs <n>`.
    Txs(i64),
    /// `criticals <n>`.
    Criticals(i64),
}

/// The parsed public-testnet receipt.
#[derive(Debug, Clone, Default)]
struct Receipt {
    /// The `rule` line value.
    rule: Option<String>,
    /// The `days` value.
    days: Option<i64>,
    /// The `uptime` value (percent).
    uptime: Option<i64>,
    /// The `validators` value.
    validators: Option<i64>,
    /// The `txs` value.
    txs: Option<i64>,
    /// The `criticals` value.
    criticals: Option<i64>,
}

/// Classify one receipt line; `None` means the line is not part of the
/// format (every shape or value problem lands here).
fn classify(line: &str) -> Option<Item> {
    if let Some(name) = line.strip_prefix("rule ") {
        if name.is_empty() || name.contains(' ') {
            return None;
        }
        return Some(Item::Rule(name.to_string()));
    }
    if let Some(rest) = line.strip_prefix("days ") {
        return Some(Item::Days(rest.parse().ok()?));
    }
    if let Some(rest) = line.strip_prefix("uptime ") {
        return Some(Item::Uptime(rest.parse().ok()?));
    }
    if let Some(rest) = line.strip_prefix("validators ") {
        return Some(Item::Validators(rest.parse().ok()?));
    }
    if let Some(rest) = line.strip_prefix("txs ") {
        return Some(Item::Txs(rest.parse().ok()?));
    }
    if let Some(rest) = line.strip_prefix("criticals ") {
        return Some(Item::Criticals(rest.parse().ok()?));
    }
    None
}

/// Parse the strict receipt text (D-O4-1). Unknown lines are refused so
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
            Item::Days(value) => receipt.days = Some(value),
            Item::Uptime(value) => receipt.uptime = Some(value),
            Item::Validators(value) => receipt.validators = Some(value),
            Item::Txs(value) => receipt.txs = Some(value),
            Item::Criticals(value) => receipt.criticals = Some(value),
        }
    }
    Ok(receipt)
}

// --------------------------------------------------------------------------- checks

/// The O4 criterion check (D-O4-1), in the pinned order; returns the ok
/// line.
fn check(receipt: &Receipt) -> Result<String, String> {
    match receipt.rule.as_deref() {
        None => return Err("missing rule line".to_string()),
        Some(RULE) => {}
        Some(other) => return Err(format!("bad rule: {other}")),
    }

    let days = receipt
        .days
        .ok_or_else(|| "missing receipt line: days".to_string())?;
    let uptime = receipt
        .uptime
        .ok_or_else(|| "missing receipt line: uptime".to_string())?;
    let validators = receipt
        .validators
        .ok_or_else(|| "missing receipt line: validators".to_string())?;
    let txs = receipt
        .txs
        .ok_or_else(|| "missing receipt line: txs".to_string())?;
    let criticals = receipt
        .criticals
        .ok_or_else(|| "missing receipt line: criticals".to_string())?;

    // Thirty days of window.
    if days < DAYS {
        return Err(format!("days: {days}"));
    }

    // At least 99% uptime.
    if uptime < UPTIME_PCT {
        return Err(format!("uptime: {uptime}"));
    }

    // At least ten independent validators.
    if validators < VALIDATORS {
        return Err(format!("validators: {validators}"));
    }

    // At least 100 000 transactions.
    if txs < TXS {
        return Err(format!("txs: {txs}"));
    }

    // Zero unfixed criticals (the named gate).
    if criticals != 0 {
        return Err(format!("criticals: {criticals}"));
    }

    Ok("testnet=ok uptime=ok validators=ok txs=ok criticals=ok".to_string())
}

/// The normalized receipt report (`summary` command).
fn summary(receipt: &Receipt) -> Result<String, String> {
    check(receipt)?;
    Ok(format!(
        "window: days={} uptime={} validators={} txs={}\ncriticals: unfixed={}\n",
        receipt.days.unwrap_or(0),
        receipt.uptime.unwrap_or(0),
        receipt.validators.unwrap_or(0),
        receipt.txs.unwrap_or(0),
        receipt.criticals.unwrap_or(0),
    ))
}

// --------------------------------------------------------------------------- metrics ledger (D-O4-2)

/// One metrics row (`metrics <d> <h> <height> <txs_total> <root>`).
struct Metric {
    /// Schedule day index.
    day: i64,
    /// Schedule hour index (0..23).
    hour: i64,
    /// Committed chain height at the poll.
    height: u64,
    /// Cumulative transaction count at the poll.
    txs_total: u64,
    /// State root at the poll (hex-64).
    #[allow(dead_code)]
    root: String,
}

/// Classify one metrics row; `None` means the row is not part of the
/// format.
fn classify_metric(line: &str) -> Option<Metric> {
    let rest = line.strip_prefix("metrics ")?;
    let mut fields = rest.split(' ');
    let day = fields.next()?.parse().ok()?;
    let hour = fields.next()?.parse().ok()?;
    let height = fields.next()?.parse().ok()?;
    let txs_total = fields.next()?.parse().ok()?;
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
    Some(Metric {
        day,
        hour,
        height,
        txs_total,
        root: root.to_string(),
    })
}

/// Roll the metrics ledger up (D-O4-2): `days` counts day-buckets with
/// all 24 hourly polls present, `uptime` is
/// `floor(100 * present / (span_days * 24))`, `txs` is the last
/// transaction total. A regressing height or transaction total is
/// refused at its first occurrence (`tx regression at <d> <h>`).
fn rollup_metrics(text: &str) -> Result<(i64, i64, u64), String> {
    let mut rows: Vec<Metric> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(row) = classify_metric(line) else {
            return Err(format!("bad metric: {line}"));
        };
        rows.push(row);
    }
    if rows.is_empty() {
        return Err("bad metric: empty ledger".to_string());
    }
    rows.sort_by_key(|row| (row.day, row.hour));
    let mut previous: Option<&Metric> = None;
    for row in &rows {
        if let Some(last) = previous {
            if (row.day, row.hour) == (last.day, last.hour) {
                return Err(format!("bad metric: duplicate {} {}", row.day, row.hour));
            }
            // A regressing window is refused at its first occurrence.
            if row.height < last.height || row.txs_total < last.txs_total {
                return Err(format!("tx regression at {} {}", row.day, row.hour));
            }
        }
        previous = Some(row);
    }
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
    let first_day = rows.first().map(|row| row.day).unwrap_or(0);
    let last_day = rows.last().map(|row| row.day).unwrap_or(0);
    let span_days = last_day - first_day + 1;
    let present = rows.len() as i64;
    let scheduled = span_days * HOURS_PER_DAY;
    let uptime = if scheduled <= 0 {
        0
    } else {
        100 * present / scheduled
    };
    let txs = rows.last().map(|row| row.txs_total).unwrap_or(0);
    Ok((days, uptime, txs))
}

// --------------------------------------------------------------------------- triage ledger (D-O4-3)

/// One triage row (`triage <id> <severity> <status>`).
struct Triage {
    /// Finding id (kept for refusal messages).
    id: String,
    /// Severity class (validated).
    severity: String,
    /// Status (validated).
    status: String,
}

/// Classify one triage row; `None` means the row is not part of the
/// format.
fn classify_triage(line: &str) -> Option<Triage> {
    let rest = line.strip_prefix("triage ")?;
    let mut fields = rest.split(' ');
    let id = fields.next()?;
    let severity = fields.next()?;
    let status = fields.next()?;
    if fields.next().is_some() {
        return None;
    }
    if id.is_empty() || id.contains(' ') {
        return None;
    }
    if !matches!(
        severity,
        "critical" | "high" | "medium" | "low" | "informational"
    ) {
        return None;
    }
    if !matches!(status, "open" | "fixed" | "accepted" | "not-applicable") {
        return None;
    }
    Some(Triage {
        id: id.to_string(),
        severity: severity.to_string(),
        status: status.to_string(),
    })
}

/// An open finding in the critical class (`critical`/`high` — the H8
/// "0 unfixed critical/high" convention).
fn is_open_critical(row: &Triage) -> bool {
    row.status == "open" && (row.severity == "critical" || row.severity == "high")
}

/// Roll the triage journal up (D-O4-3): `items` counts findings, `open`
/// the unresolved ones, `criticals` the open critical-class ones. An
/// open critical is refused by its id (`open critical <id>`), first by
/// sorted id order.
fn rollup_triage(text: &str) -> Result<(i64, i64, i64), String> {
    let mut rows: Vec<Triage> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(row) = classify_triage(line) else {
            return Err(format!("bad triage: {line}"));
        };
        rows.push(row);
    }
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    let mut previous: Option<&str> = None;
    for row in &rows {
        if previous == Some(row.id.as_str()) {
            return Err(format!("bad triage: duplicate {}", row.id));
        }
        previous = Some(row.id.as_str());
    }
    for row in &rows {
        if is_open_critical(row) {
            return Err(format!("open critical {}", row.id));
        }
    }
    let items = rows.len() as i64;
    let open = rows.iter().filter(|row| row.status == "open").count() as i64;
    let criticals = rows.iter().filter(|row| is_open_critical(row)).count() as i64;
    Ok((items, open, criticals))
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

/// Execute one `aura-testnet` command against the given streams; returns
/// the exit code (0 ok, 64 usage, 65 refusal).
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut dispatch = || -> Result<(), String> {
        let (command, rest) = args.split_first().ok_or("usage:missing command")?;
        let map = flags(rest).map_err(|message| format!("usage:{message}"))?;
        match command.as_str() {
            "check" => {
                let receipts_path = map
                    .get("receipts")
                    .cloned()
                    .unwrap_or_else(|| "release/testnet.txt".to_string());
                let receipt = parse_receipt(&read_text(&receipts_path)?)?;
                let ok = check(&receipt)?;
                let _ = writeln!(out, "{ok}");
                Ok(())
            }
            "summary" => {
                let receipts_path = map
                    .get("receipts")
                    .cloned()
                    .unwrap_or_else(|| "release/testnet.txt".to_string());
                let receipt = parse_receipt(&read_text(&receipts_path)?)?;
                let report = summary(&receipt)?;
                let _ = write!(out, "{report}");
                Ok(())
            }
            "metrics" => {
                let ledger_path = map
                    .get("ledger")
                    .cloned()
                    .unwrap_or_else(|| "release/metrics-30d.txt".to_string());
                let (days, uptime, txs) = rollup_metrics(&read_text(&ledger_path)?)?;
                let _ = writeln!(out, "metrics ledger: days={days} uptime={uptime} txs={txs}");
                Ok(())
            }
            "triage" => {
                let ledger_path = map
                    .get("ledger")
                    .cloned()
                    .unwrap_or_else(|| "release/triage.txt".to_string());
                let (items, open, criticals) = rollup_triage(&read_text(&ledger_path)?)?;
                let _ = writeln!(
                    out,
                    "triage ledger: items={items} open={open} criticals={criticals}"
                );
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
                let _ = writeln!(err, "error: testnet: {message}");
                EXIT_REFUSED
            }
        },
    }
}
