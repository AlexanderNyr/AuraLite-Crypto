//! P6 SLO publication profile — the tamper-evident SLO ledger
//! (PERF_PLAN P6: methodology, logs, dashboards, honest "did not
//! work" items; DoD: **north star metrics confirmed; NC: rigged run →
//! log check red**).
//!
//! Self-contained profiling artifact (decision **D-P7**, the P3–P5
//! pattern): a registry of the six published north-star SLOs (the P0–P5
//! DoD numbers, pinned), a hash-chained run log (each entry commits to
//! the previous one), and verification gates. A rigged run — a log
//! value that disagrees with the registry — is rejected with a named
//! message; a broken hash chain is rejected at the first bad entry.
//! The methodology index and the honest "did not work" ledger are
//! gate-checked so a publication cannot silently drop either.

use std::fmt;

use crate::MachineProfile;

/// Deterministic model repeats.
pub const RUNS: usize = 5;

/// P0 north star: 5-run spread (percent), at most 5.
pub const P0_SPREAD_PCT: u64 = 0;
/// P0 bound.
pub const P0_SPREAD_LIMIT: u64 = 5;
/// P1 north star: batch verify cost (units at k=8), at most 38.
pub const P1_VERIFY_UNITS: u64 = 22;
/// P1 bound (the serialized baseline).
pub const P1_VERIFY_LIMIT: u64 = 38;
/// P2 north star: parallel engine cost (units per 1000 txs).
pub const P2_PARALLEL_UNITS: u64 = 15543;
/// P2 bound (the sequential engine).
pub const P2_PARALLEL_LIMIT: u64 = 99000;
/// P3 north star: p99 block commit (model units).
pub const P3_COMMIT_UNITS: u64 = 12300;
/// P3 bound (the published SLO).
pub const P3_COMMIT_LIMIT: u64 = 15000;
/// P4 north star: gossip traffic per validator (bytes/s).
pub const P4_TRAFFIC_BYTES: u64 = 86495;
/// P4 bound (the published budget).
pub const P4_TRAFFIC_LIMIT: u64 = 125_000;
/// P5 north star: p99 finality at 100 nodes (ms).
pub const P5_FINALITY_MS: u64 = 586;
/// P5 bound (the plan-fixed SLO).
pub const P5_FINALITY_LIMIT: u64 = 2000;

/// The methodology index: every published SLO must carry its
/// methodology section (D-P3).
pub const METHODOLOGY: [&str; 6] = [
    "p0-methodology",
    "p1-methodology",
    "p2-methodology",
    "p3-methodology",
    "p4-methodology",
    "p5-methodology",
];

/// The pinned expected methodology sections (an independent
/// literal: a mutation of [`METHODOLOGY`] must not move the pin).
pub const EXPECTED_METHODOLOGY: [&str; 6] = [
    "p0-methodology",
    "p1-methodology",
    "p2-methodology",
    "p3-methodology",
    "p4-methodology",
    "p5-methodology",
];

/// The honest "did not work" ledger: published negative results and
/// dead ends (P6 DoD: honest "did not work" items).
pub const HONEST_FAILURES: [&str; 4] = [
    "p2-measured-wall-clock",
    "p3-measured-sandbox-fsync",
    "p1-bls-model-only",
    "p2-lcg-fixture",
];

/// The pinned expected honest-failure items (an independent
/// literal: a mutation of [`HONEST_FAILURES`] must not move the pin).
pub const EXPECTED_HONEST_FAILURES: [&str; 4] = [
    "p2-measured-wall-clock",
    "p3-measured-sandbox-fsync",
    "p1-bls-model-only",
    "p2-lcg-fixture",
];

/// Run-log chain seed (domain-separated).
pub const LOG_SEED: &str = "p6log|seed|v1";

/// Named P6 messages (one line each; the NC contract).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum P6Error {
    /// A north-star metric does not satisfy its published bound.
    NorthStarNotConfirmed {
        /// Metric name.
        name: String,
        /// Observed value.
        observed: u64,
        /// Published bound.
        bound: u64,
    },
    /// The run-log hash chain is broken.
    LogChainBroken {
        /// Index of the first bad entry.
        at: u64,
    },
    /// A rigged run: the log disagrees with the registry.
    RiggedRun {
        /// The offending `key=value` pair.
        detail: String,
    },
    /// A methodology section is missing.
    MethodologyGap {
        /// Missing section name.
        section: String,
    },
    /// A pinned honest-failure item is missing.
    HonestFailureMissing {
        /// Missing item name.
        name: String,
    },
    /// Two identical runs disagreed (X7).
    DeterminismViolated {
        /// Index of the disagreeing run.
        run: usize,
    },
}

impl fmt::Display for P6Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NorthStarNotConfirmed {
                name,
                observed,
                bound,
            } => {
                write!(
                    f,
                    "P6: north star metric not confirmed ({name} {observed}/{bound})"
                )
            }
            Self::LogChainBroken { at } => {
                write!(f, "P6: run log hash chain broken at entry {at}")
            }
            Self::RiggedRun { detail } => {
                write!(f, "P6: rigged run detected ({detail})")
            }
            Self::MethodologyGap { section } => {
                write!(f, "P6: methodology gap ({section})")
            }
            Self::HonestFailureMissing { name } => {
                write!(f, "P6: honest-failure item missing ({name})")
            }
            Self::DeterminismViolated { run } => {
                write!(f, "P6: determinism violated (run {run})")
            }
        }
    }
}

impl std::error::Error for P6Error {}

/// One published north-star SLO (all six are upper-bound relations).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SloEntry {
    /// Metric name (`phase.metric`).
    pub name: &'static str,
    /// Observed value (pinned golden).
    pub observed: u64,
    /// Published bound.
    pub bound: u64,
}

impl SloEntry {
    /// The relation holds when the observation is within the bound.
    pub fn confirmed(&self) -> bool {
        self.observed <= self.bound
    }
}

/// The six published north-star SLOs (P0–P5 DoD numbers).
pub fn north_star() -> Vec<SloEntry> {
    vec![
        SloEntry {
            name: "p0.spread_pct",
            observed: P0_SPREAD_PCT,
            bound: P0_SPREAD_LIMIT,
        },
        SloEntry {
            name: "p1.verify_units",
            observed: P1_VERIFY_UNITS,
            bound: P1_VERIFY_LIMIT,
        },
        SloEntry {
            name: "p2.parallel_units",
            observed: P2_PARALLEL_UNITS,
            bound: P2_PARALLEL_LIMIT,
        },
        SloEntry {
            name: "p3.commit_units",
            observed: P3_COMMIT_UNITS,
            bound: P3_COMMIT_LIMIT,
        },
        SloEntry {
            name: "p4.traffic_bytes",
            observed: P4_TRAFFIC_BYTES,
            bound: P4_TRAFFIC_LIMIT,
        },
        SloEntry {
            name: "p5.finality_ms",
            observed: P5_FINALITY_MS,
            bound: P5_FINALITY_LIMIT,
        },
    ]
}

/// The recorded run rows (`key=value` pairs written to the run log).
/// Kept separate from [`north_star`] so a rigged run can be modeled:
/// verification compares the log against the registry.
pub fn log_rows() -> Vec<(&'static str, u64)> {
    north_star()
        .into_iter()
        .map(|e| (e.name, e.observed))
        .collect()
}

/// One hash-chained log entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Metric key.
    pub key: String,
    /// Recorded value.
    pub value: u64,
    /// Chain digest after this entry.
    pub chain: String,
}

/// Chain step: `sha256(prev | "|" | line)` in lowercase hex.
pub fn chain_step(prev: &str, line: &str) -> String {
    crate::hex(&crate::sha256_32(format!("{prev}|{line}").as_bytes()))
}

/// Build the hash-chained run log from [`log_rows`].
pub fn build_log() -> Vec<LogEntry> {
    let mut prev = LOG_SEED.to_string();
    let mut out = Vec::new();
    for (key, value) in log_rows() {
        let line = format!("{key}={value}");
        prev = chain_step(&prev, &line);
        out.push(LogEntry {
            key: key.to_string(),
            value,
            chain: prev.clone(),
        });
    }
    out
}

/// Verify the run log: the hash chain first (named entry index on
/// break), then every value against the registry (the rigged-run
/// check). Returns the chain tip on success.
pub fn verify_log(entries: &[LogEntry]) -> Result<String, P6Error> {
    let mut prev = LOG_SEED.to_string();
    let registry = north_star();
    for (i, entry) in entries.iter().enumerate() {
        let line = format!("{}={}", entry.key, entry.value);
        prev = chain_step(&prev, &line);
        if entry.chain != prev {
            return Err(P6Error::LogChainBroken { at: i as u64 });
        }
        match registry.iter().find(|r| r.name == entry.key) {
            Some(r) if r.observed == entry.value => {}
            _ => {
                return Err(P6Error::RiggedRun { detail: line });
            }
        }
    }
    Ok(prev)
}

/// Gate: every published SLO carries its methodology section (D-P3).
pub fn gate_methodology() -> Result<(), P6Error> {
    for section in EXPECTED_METHODOLOGY {
        if !METHODOLOGY.contains(&section) {
            return Err(P6Error::MethodologyGap {
                section: section.to_string(),
            });
        }
    }
    Ok(())
}

/// Gate: the honest "did not work" ledger keeps every pinned item.
pub fn gate_honest_failures() -> Result<(), P6Error> {
    for name in EXPECTED_HONEST_FAILURES {
        if !HONEST_FAILURES.contains(&name) {
            return Err(P6Error::HonestFailureMissing {
                name: name.to_string(),
            });
        }
    }
    Ok(())
}

/// Gate: all six north-star relations hold.
pub fn gate_north_star() -> Result<(), P6Error> {
    for entry in north_star() {
        if !entry.confirmed() {
            return Err(P6Error::NorthStarNotConfirmed {
                name: entry.name.to_string(),
                observed: entry.observed,
                bound: entry.bound,
            });
        }
    }
    Ok(())
}

/// The state digest over registry + log + ledgers.
pub fn state_digest() -> String {
    let mut parts = Vec::new();
    for entry in north_star() {
        parts.push(format!(
            "p6s|slo|{}={}/{}",
            entry.name, entry.observed, entry.bound
        ));
    }
    for entry in build_log() {
        parts.push(format!(
            "p6s|log|{}={}|{}",
            entry.key, entry.value, entry.chain
        ));
    }
    for section in METHODOLOGY {
        parts.push(format!("p6s|methodology|{section}"));
    }
    for name in HONEST_FAILURES {
        parts.push(format!("p6s|honest|{name}"));
    }
    let mut joined = parts.join("|");
    joined.push('|');
    crate::hex(&crate::sha256_32(joined.as_bytes()))
}

/// Deterministic SLO-publication report (the P3–P5 skeleton shape,
/// mapped to the registry: `p50/p95/p99` are the confirmed counts of
/// the last three deterministic runs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P6Report {
    /// Workload digest.
    pub workload_digest: String,
    /// Machine digest (D-P1).
    pub machine_digest: String,
    /// Model repeats.
    pub runs: usize,
    /// Spread across repeats (percent).
    pub spread_pct: u64,
    /// Confirmed north-star metrics.
    pub confirmed: u64,
    /// Total north-star metrics.
    pub total: u64,
    /// Run-log entries.
    pub log_entries: u64,
    /// Run-log chain tip (hex).
    pub log_tip: String,
    /// Methodology sections.
    pub methodology: u64,
    /// Honest-failure items.
    pub honest: u64,
    /// State digest.
    pub state_digest: String,
    /// Bench digest over the canonical line.
    pub bench_digest: String,
}

impl P6Report {
    /// Build the deterministic report from the frozen registry.
    pub fn build(runs: usize) -> Self {
        let entries = north_star();
        let confirmed = entries.iter().filter(|e| e.confirmed()).count() as u64;
        let log = build_log();
        let tip = log
            .last()
            .map(|e| e.chain.clone())
            .unwrap_or_else(|| crate::hex(&crate::sha256_32(LOG_SEED.as_bytes())));
        let workload_digest = crate::hex(&crate::sha256_32(
            format!("p6w|slo-publication|metrics={}", entries.len()).as_bytes(),
        ));
        let machine_digest = MachineProfile::REFERENCE.digest();
        let sd = state_digest();
        let canonical = format!(
            "p6b|{}|{}|runs={runs}|spread=0|confirmed={confirmed}|total={}|log_entries={}|tip={tip}|methodology={}|honest={}",
            workload_digest,
            machine_digest,
            entries.len(),
            log.len(),
            METHODOLOGY.len(),
            HONEST_FAILURES.len(),
        );
        Self {
            workload_digest,
            machine_digest,
            runs,
            spread_pct: 0,
            confirmed,
            total: entries.len() as u64,
            log_entries: log.len() as u64,
            log_tip: tip,
            methodology: METHODOLOGY.len() as u64,
            honest: HONEST_FAILURES.len() as u64,
            state_digest: sd,
            bench_digest: crate::hex(&crate::sha256_32(canonical.as_bytes())),
        }
    }

    /// The canonical line (digest source).
    pub fn canonical(&self) -> String {
        format!(
            "p6b|{}|{}|runs={}|spread={}|confirmed={}|total={}|log_entries={}|tip={}|methodology={}|honest={}",
            self.workload_digest,
            self.machine_digest,
            self.runs,
            self.spread_pct,
            self.confirmed,
            self.total,
            self.log_entries,
            self.log_tip,
            self.methodology,
            self.honest,
        )
    }

    /// JSON line for the evidence / sim output.
    pub fn json(&self) -> String {
        format!(
            "{{\"workload_digest\": \"{}\", \"machine_digest\": \"{}\", \"runs\": {}, \"spread_pct\": {}, \"confirmed\": {}, \"total\": {}, \"log_entries\": {}, \"log_tip\": \"{}\", \"methodology\": {}, \"honest\": {}, \"bench_digest\": \"{}\"}}",
            self.workload_digest,
            self.machine_digest,
            self.runs,
            self.spread_pct,
            self.confirmed,
            self.total,
            self.log_entries,
            self.log_tip,
            self.methodology,
            self.honest,
            self.bench_digest,
        )
    }

    /// The DoD gates: north star → run log (chain + values) →
    /// methodology → honest failures → determinism.
    pub fn validate(&self) -> Result<(), P6Error> {
        gate_north_star()?;
        verify_log(&build_log())?;
        gate_methodology()?;
        gate_honest_failures()?;
        let again = Self::build(self.runs);
        if again.canonical() != self.canonical() {
            return Err(P6Error::DeterminismViolated { run: 1 });
        }
        Ok(())
    }
}

/// Honest measured layer (D-P9, evidence only — never a gate): real
/// build+verify of a long hash-chained log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P6Measured {
    /// Building `len` chained entries, nanoseconds.
    pub build_ns: u64,
    /// Verifying the chain, nanoseconds.
    pub verify_ns: u64,
    /// Entries processed.
    pub len: u64,
}

impl P6Measured {
    /// JSON line.
    pub fn json(&self) -> String {
        format!(
            "{{\"build_ns\": {}, \"verify_ns\": {}, \"len\": {}}}",
            self.build_ns, self.verify_ns, self.len
        )
    }
}

/// Measure real log throughput (medians of `reps`).
pub fn measure(reps: usize) -> P6Measured {
    const LEN: usize = 4096;
    let mut build = Vec::new();
    let mut verify = Vec::new();
    for _ in 0..reps {
        let start = std::time::Instant::now();
        let mut prev = LOG_SEED.to_string();
        let mut entries = Vec::with_capacity(LEN);
        for i in 0..LEN {
            let line = format!("m{i}={i}");
            prev = chain_step(&prev, &line);
            entries.push(LogEntry {
                key: format!("m{i}"),
                value: i as u64,
                chain: prev.clone(),
            });
        }
        build.push(start.elapsed().as_nanos() as u64);

        let start = std::time::Instant::now();
        let mut prev = LOG_SEED.to_string();
        for entry in &entries {
            let line = format!("{}={}", entry.key, entry.value);
            prev = chain_step(&prev, &line);
            assert_eq!(entry.chain, prev);
        }
        verify.push(start.elapsed().as_nanos() as u64);
    }
    build.sort_unstable();
    verify.sort_unstable();
    P6Measured {
        build_ns: build[build.len() / 2],
        verify_ns: verify[verify.len() / 2],
        len: LEN as u64,
    }
}
