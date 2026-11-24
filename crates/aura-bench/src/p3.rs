//! AuraLite-Crypto P3 — storage profile (phase P3,
//! `docs/plans/PERF_PLAN.md`): RocksDB-shaped tuning knobs — batch
//! writes, bloom filters, compaction — over a self-contained mini-LSM
//! commit path (decision **D-P7**: the P3 profile is a profiling
//! artifact of the bench crate; wiring the knobs into the RocksDB
//! backend of `aura-storage` is follow-up production work).
//!
//! The P2 DoD was an equality; the P3 DoD is a latency bound with a
//! durability contract:
//!
//! * **p99 commit latency < SLO** — the commit here is *persisting one
//!   block* (1000 writes of 350-byte values, the D-P1 payload). The
//!   deterministic cost model (D-P9) prices the commit path stages:
//!   WAL append (`WAL_BASE + value/64` per key), `FSYNC`, memtable
//!   publish (`MEMTABLE_PUT` per key) and amortized compaction
//!   (`COMPACT_PER_KEY` per key every `COMPACT_EVERY` blocks). The
//!   baseline commits write by write (an fsync per write — 311 000
//!   units per block); P3 commits the block as **one WriteBatch with
//!   one fsync** (12 300 units), under the published SLO of 15 000
//!   units (the wall-clock SLO is published in the report with its
//!   methodology — D-P3).
//! * **NC: fsync in the wrong place → S7 red** — the commit protocol
//!   is the S7 contract of `aura-storage`: append the WAL record,
//!   **fsync it, and only then publish** to the memtable. A crash
//!   loses the volatile memtable and every unsynced WAL record;
//!   recovery replays the synced tail. If fsync is moved out of the
//!   commit path (the `bad-fsync` mutation), a crash after a
//!   "committed" batch loses it — `P3: fsync missing in commit path`.
//! * **Bloom filters** (10 bits per key, k = 7 — the RocksDB default
//!   shape; false-positive rate ~1%): a per-run filter gates SSTable
//!   reads on the lookup path. Functional contract: **no false
//!   negatives** (`P3: bloom false negative`); cost pair: a miss costs
//!   `SST_READ = 40` units before vs `BLOOM_CHECK + fp·SST_READ/misses`
//!   after (`P3: bloom miss cost regression`).
//! * **Compaction**: immutable sorted runs merge into one (latest
//!   value wins) with a rebuilt filter; the gate is state preservation
//!   (`P3: compaction lost data`).
//!
//! Wall-clock numbers of a real file + real `fsync` land in the
//! measured layer only (`P3-MEASURED`, evidence — D-P9).

use std::collections::BTreeMap;
use std::fmt;

use crate::{MachineProfile, percentile, sha256_32, spread_pct};

/// Writes per committed block (the P3 workload: one block commit).
pub const BLOCK_WRITES: usize = 1000;
/// Value size of each write in bytes (the D-P1 payload).
pub const VALUE_BYTES: usize = 350;
/// Repeat runs of the deterministic model (P0–P2 convention).
pub const RUNS: usize = 5;

/// WAL append fixed cost per key.
pub const WAL_BASE: u64 = 4;
/// WAL append bytes divisor (value bytes / 64).
pub const WAL_DIV: u64 = 64;
/// One fsync of the WAL (the S7 durability boundary) in cost units.
pub const FSYNC: u64 = 300;
/// Memtable publish per key.
pub const MEMTABLE_PUT: u64 = 2;
/// One bloom filter probe (set of `BLOOM_K` bit checks).
pub const BLOOM_CHECK: u64 = 1;
/// One SSTable read on the lookup path.
pub const SST_READ: u64 = 40;
/// Compaction merge cost per key (amortized per `COMPACT_EVERY`
/// blocks).
pub const COMPACT_PER_KEY: u64 = 8;
/// Blocks between compactions (the amortization divisor).
pub const COMPACT_EVERY: u64 = 8;
/// Published SLO: p99 block-commit cost in deterministic units
/// (the wall-clock SLO is published in `docs/perf/p3-storage-report.md`
/// with its methodology — D-P3).
pub const COMMIT_SLO_UNITS: u64 = 15_000;

/// Bloom filter bits per key (the RocksDB default: ~1% FP at k ≈ 7).
pub const BLOOM_BITS_PER_KEY: usize = 10;
/// Bloom filter hash probes.
pub const BLOOM_K: usize = 7;
/// Lookup misses of the pinned fixture (absent keys).
pub const LOOKUP_MISSES: usize = 200;

/// WAL append cost of one key with a `value_bytes`-long value.
pub fn wal_put(value_bytes: usize) -> u64 {
    WAL_BASE + value_bytes as u64 / WAL_DIV
}

/// Baseline block-commit cost (write by write: an fsync per write).
pub fn block_cost_before() -> u64 {
    BLOCK_WRITES as u64 * (wal_put(VALUE_BYTES) + FSYNC + MEMTABLE_PUT)
}

/// P3 block-commit cost (one WriteBatch, one fsync; compaction
/// amortized over `COMPACT_EVERY` blocks).
pub fn block_cost_after() -> u64 {
    let batch = BLOCK_WRITES as u64 * (wal_put(VALUE_BYTES) + MEMTABLE_PUT);
    batch + FSYNC + BLOCK_WRITES as u64 * COMPACT_PER_KEY / COMPACT_EVERY
}

/// Average cost of a lookup miss before bloom filters (every miss
/// reads an SSTable).
pub fn miss_cost_before() -> u64 {
    SST_READ
}

/// Average cost of a lookup miss with bloom filters: one probe plus
/// the false positives that still read an SSTable (ceil division).
pub fn miss_cost_after(false_positives: usize) -> u64 {
    BLOOM_CHECK + (false_positives as u64 * SST_READ).div_ceil(LOOKUP_MISSES as u64)
}

/// The bloom filter of one sorted run (m bits, k probes; double
/// hashing over sha256 of the key — deterministic, dependency-free).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bloom {
    bits: Vec<bool>,
    k: usize,
}

impl Bloom {
    /// Allocate a filter for `keys` expected keys (`BLOOM_BITS_PER_KEY`
    /// bits per key, rounded up to a power of two).
    pub fn for_keys(keys: usize) -> Bloom {
        let want = keys.max(1) * BLOOM_BITS_PER_KEY;
        let m = want.next_power_of_two();
        Bloom {
            bits: vec![false; m],
            k: BLOOM_K,
        }
    }

    /// The `k` probe positions of `key` (double hashing over sha256).
    fn probes(&self, key: u64) -> Vec<usize> {
        let d = sha256_32(&key.to_le_bytes());
        let h1 = u64::from_le_bytes(d[0..8].try_into().expect("8 bytes")) as usize;
        let h2 = (u64::from_le_bytes(d[8..16].try_into().expect("8 bytes")) as usize) | 1;
        (0..self.k)
            .map(|i| (h1.wrapping_add(i.wrapping_mul(h2))) % self.bits.len())
            .collect()
    }

    /// Insert `key` (sets all `k` bits).
    pub fn insert(&mut self, key: u64) {
        for p in self.probes(key) {
            self.bits[p] = true;
        }
    }

    /// Membership test: **never false-negatives** for inserted keys.
    pub fn maybe_contains(&self, key: u64) -> bool {
        self.probes(key).into_iter().all(|p| self.bits[p])
    }
}

/// One WAL record: key, value, synced flag (the S7 durability marker).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalRecord {
    /// Batch id of the committing block.
    pub batch: u64,
    /// Key.
    pub key: u64,
    /// Value bytes.
    pub value: Vec<u8>,
    /// Whether the record passed the fsync boundary.
    pub synced: bool,
}

/// The P3 mini-LSM store (S7 commit protocol: WAL append → fsync →
/// publish; volatile memtable + immutable sorted runs with bloom
/// filters; crash replays only the synced WAL tail).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    /// Write-ahead log (the durability authority).
    pub wal: Vec<WalRecord>,
    /// Volatile memtable.
    pub memtable: BTreeMap<u64, Vec<u8>>,
    /// Immutable sorted runs (newest first), each with its filter.
    pub runs: Vec<(BTreeMap<u64, Vec<u8>>, Bloom)>,
    /// Next batch id.
    pub next_batch: u64,
}

impl Store {
    /// Empty store (fresh database).
    pub fn new() -> Store {
        Store {
            wal: Vec::new(),
            memtable: BTreeMap::new(),
            runs: Vec::new(),
            next_batch: 1,
        }
    }

    /// Commit one batch (the block): WAL append, **fsync**, publish —
    /// the S7 order. Returns the batch id.
    pub fn commit(&mut self, batch: &[(u64, Vec<u8>)]) -> u64 {
        let id = self.next_batch;
        self.next_batch += 1;
        for (key, value) in batch {
            self.wal.push(WalRecord {
                batch: id,
                key: *key,
                value: value.clone(),
                synced: false,
            });
        }
        self.sync_wal();
        for (key, value) in batch {
            self.memtable.insert(*key, value.clone());
        }
        id
    }

    /// The fsync boundary: mark the WAL tail durable (S7: this must
    /// happen **before** publication).
    pub fn sync_wal(&mut self) {
        for rec in self.wal.iter_mut() {
            rec.synced = true;
        }
    }

    /// Crash and recover: the memtable and every unsynced WAL record
    /// are lost; the synced WAL tail replays into a fresh memtable;
    /// sorted runs (on disk) survive.
    pub fn crash(&mut self) {
        self.wal.retain(|rec| rec.synced);
        self.memtable = BTreeMap::new();
        let replay: Vec<(u64, Vec<u8>)> = self
            .wal
            .iter()
            .map(|rec| (rec.key, rec.value.clone()))
            .collect();
        for (key, value) in replay {
            self.memtable.insert(key, value);
        }
    }

    /// Flush the memtable into a new immutable sorted run (with its
    /// bloom filter) and clear the replayed WAL.
    pub fn flush(&mut self) {
        if self.memtable.is_empty() {
            return;
        }
        let mut filter = Bloom::for_keys(self.memtable.len());
        for key in self.memtable.keys() {
            filter.insert(*key);
        }
        self.runs
            .insert(0, (std::mem::take(&mut self.memtable), filter));
        self.wal.clear();
    }

    /// Compact all sorted runs into one (latest value wins) with a
    /// rebuilt filter — the state must be preserved exactly.
    pub fn compact(&mut self) {
        let mut merged: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
        // Oldest first so newer values overwrite older ones.
        for (run, _) in self.runs.iter().rev() {
            merged.extend(run.iter().map(|(k, v)| (*k, v.clone())));
        }
        // The memtable is newer than every run.
        merged.extend(self.memtable.iter().map(|(k, v)| (*k, v.clone())));
        let mut filter = Bloom::for_keys(merged.len());
        for key in merged.keys() {
            filter.insert(*key);
        }
        self.runs = vec![(merged, filter)];
    }

    /// Point lookup: memtable first, then runs newest-first with the
    /// bloom filter gating the SSTable read.
    pub fn get(&self, key: u64) -> Option<&Vec<u8>> {
        if let Some(v) = self.memtable.get(&key) {
            return Some(v);
        }
        for (run, filter) in &self.runs {
            if filter.maybe_contains(key)
                && let Some(v) = run.get(&key)
            {
                return Some(v);
            }
        }
        None
    }

    /// The full committed state (memtable over runs) as a map.
    pub fn state(&self) -> BTreeMap<u64, Vec<u8>> {
        let mut merged: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
        for (run, _) in self.runs.iter().rev() {
            merged.extend(run.iter().map(|(k, v)| (*k, v.clone())));
        }
        merged.extend(self.memtable.iter().map(|(k, v)| (*k, v.clone())));
        merged
    }

    /// Canonical state digest (`p3s|key:hexvalue|…`).
    pub fn digest(&self) -> String {
        let mut s = String::from("p3s");
        for (key, value) in self.state() {
            s.push_str(&format!("|{key}:{}", crate::hex(&value)));
        }
        crate::hex(&sha256_32(s.as_bytes()))
    }
}

impl Default for Store {
    fn default() -> Store {
        Store::new()
    }
}

/// The named P3 errors (NC targets; all Display literals live on
/// single lines in [`P3Error::fmt`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum P3Error {
    /// p99 block-commit cost at/above the published SLO (the P3 DoD;
    /// NC `bad-slo` reverts batching).
    SloViolated {
        /// Observed p99 commit cost.
        p99: u64,
        /// Published SLO (deterministic units).
        slo: u64,
    },
    /// A "committed" batch was lost by a crash (fsync missing in the
    /// commit path — the S7 durability contract; NC `bad-fsync`).
    FsyncMissing {
        /// Batch id that vanished.
        batch: u64,
    },
    /// The bloom filter missed an inserted key (NC `bad-bloom`).
    BloomFalseNegative {
        /// The key that must be found.
        key: u64,
    },
    /// The bloom lookup stop-loss regressed to SSTable-read cost
    /// (NC `bad-bloom-cost`).
    BloomMissCostRegression {
        /// Miss cost before bloom filters.
        before: u64,
        /// Miss cost with bloom filters.
        after: u64,
    },
    /// Compaction changed the committed state (NC `bad-compaction`).
    CompactionLostData,
    /// Repeat runs are not bit-identical (NC `bad-determinism`; X7).
    DeterminismViolated {
        /// Index of the first run that differs.
        run: usize,
    },
}

impl fmt::Display for P3Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            P3Error::SloViolated { p99, slo } => {
                write!(f, "P3: p99 commit exceeds SLO ({p99}/{slo})")
            }
            P3Error::FsyncMissing { batch } => {
                write!(f, "P3: fsync missing in commit path (batch {batch})")
            }
            P3Error::BloomFalseNegative { key } => {
                write!(f, "P3: bloom false negative (key {key})")
            }
            P3Error::BloomMissCostRegression { before, after } => {
                write!(f, "P3: bloom miss cost regression ({before}/{after})")
            }
            P3Error::CompactionLostData => write!(f, "P3: compaction lost data"),
            P3Error::DeterminismViolated { run } => {
                write!(f, "P3: determinism violated (run {run})")
            }
        }
    }
}

impl std::error::Error for P3Error {}

/// The pinned P3 block-commit report (deterministic layer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P3Report {
    /// Workload digest (hex sha256 of the canonical workload form).
    pub workload_digest: String,
    /// Machine digest (hex sha256 of the canonical machine form).
    pub machine_digest: String,
    /// Repeat runs of the deterministic model.
    pub runs: usize,
    /// Per-run block-commit costs in deterministic cost units.
    pub run_costs: Vec<u64>,
    /// Spread across repeat runs in percent.
    pub spread_pct: u64,
    /// p50 of the per-run costs (nearest rank).
    pub p50: u64,
    /// p95 of the per-run costs (nearest rank).
    pub p95: u64,
    /// p99 of the per-run costs (nearest rank).
    pub p99: u64,
    /// Sum of the per-run costs.
    pub total: u64,
    /// Baseline block-commit cost (write by write).
    pub before: u64,
    /// P3 block-commit cost (one WriteBatch).
    pub after: u64,
    /// Published SLO (deterministic units).
    pub slo: u64,
    /// Throughput gain in percent (`before·100/after − 100`).
    pub speedup_pct: u64,
    /// Miss cost before bloom filters.
    pub miss_before: u64,
    /// Miss cost with bloom filters (fixture-averaged, ceil).
    pub miss_after: u64,
    /// False positives of the pinned fixture (exact, deterministic).
    pub fp: usize,
    /// Aggregate digest (hex sha256 of the canonical report form).
    pub bench_digest: String,
}

impl P3Report {
    /// Build the report from the fixture's exact false-positive count
    /// (deterministic: same inputs, same report).
    pub fn build(false_positives: usize, runs: usize) -> P3Report {
        assert!(runs > 0, "at least one run");
        let after = block_cost_after();
        let run_costs = vec![after; runs];
        let spread = spread_pct(&run_costs);
        let p50 = percentile(&run_costs, 50);
        let p95 = percentile(&run_costs, 95);
        let p99 = percentile(&run_costs, 99);
        let total: u64 = run_costs.iter().sum();
        let before = block_cost_before();
        let mut report = P3Report {
            workload_digest: crate::hex(&sha256_32(
                format!("p3w|block-commit|{BLOCK_WRITES}|{VALUE_BYTES}").as_bytes(),
            )),
            machine_digest: MachineProfile::REFERENCE.digest(),
            runs,
            run_costs,
            spread_pct: spread,
            p50,
            p95,
            p99,
            total,
            before,
            after,
            slo: COMMIT_SLO_UNITS,
            speedup_pct: before * 100 / after - 100,
            miss_before: miss_cost_before(),
            miss_after: miss_cost_after(false_positives),
            fp: false_positives,
            bench_digest: String::new(),
        };
        report.bench_digest = crate::hex(&sha256_32(report.canonical().as_bytes()));
        report
    }

    /// Canonical form of the report (the digest input).
    pub fn canonical(&self) -> String {
        format!(
            "p3b|{}|{}|runs={}|spread={}|p50={}|p95={}|p99={}|total={}\
             |before={}|after={}|slo={}|speedup_pct={}\
             |miss_before={}|miss_after={}|fp={}",
            self.workload_digest,
            self.machine_digest,
            self.runs,
            self.spread_pct,
            self.p50,
            self.p95,
            self.p99,
            self.total,
            self.before,
            self.after,
            self.slo,
            self.speedup_pct,
            self.miss_before,
            self.miss_after,
            self.fp
        )
    }

    /// Compact JSON of the pinned fields (byte-stable key order).
    pub fn json(&self) -> String {
        format!(
            "{{\"workload_digest\": \"{}\", \"machine_digest\": \"{}\", \"runs\": {}, \
             \"spread_pct\": {}, \"p50\": {}, \"p95\": {}, \"p99\": {}, \
             \"total\": {}, \"before\": {}, \"after\": {}, \"slo\": {}, \
             \"speedup_pct\": {}, \"miss_before\": {}, \"miss_after\": {}, \
             \"fp\": {}, \"block_writes\": {}, \"value_bytes\": {}, \
             \"bench_digest\": \"{}\"}}",
            self.workload_digest,
            self.machine_digest,
            self.runs,
            self.spread_pct,
            self.p50,
            self.p95,
            self.p99,
            self.total,
            self.before,
            self.after,
            self.slo,
            self.speedup_pct,
            self.miss_before,
            self.miss_after,
            self.fp,
            BLOCK_WRITES,
            VALUE_BYTES,
            self.bench_digest
        )
    }

    /// Validate the optimization gates in the fixed order
    /// determinism → SLO (the DoD) → bloom miss-cost pair. (Durability,
    /// bloom correctness and compaction live in the gate functions.)
    pub fn validate(&self) -> Result<(), P3Error> {
        if let Some(first) = self.run_costs.first() {
            for (i, c) in self.run_costs.iter().enumerate() {
                if c != first {
                    return Err(P3Error::DeterminismViolated { run: i });
                }
            }
        }
        if self.p99 >= self.slo {
            return Err(P3Error::SloViolated {
                p99: self.p99,
                slo: self.slo,
            });
        }
        if self.miss_after >= self.miss_before {
            return Err(P3Error::BloomMissCostRegression {
                before: self.miss_before,
                after: self.miss_after,
            });
        }
        Ok(())
    }
}

/// Functional gate (the S7 half of the P3 DoD): a committed batch
/// must survive a crash — fsync belongs in the commit path
/// (`P3: fsync missing in commit path (batch {n})`).
pub fn gate_durability() -> Result<(), P3Error> {
    let mut store = Store::new();
    let batch: Vec<(u64, Vec<u8>)> = (0..8u64).map(|i| (i, vec![i as u8; 4])).collect();
    let id = store.commit(&batch);
    store.crash();
    for (key, value) in &batch {
        match store.get(*key) {
            Some(v) if v == value => {}
            _ => return Err(P3Error::FsyncMissing { batch: id }),
        }
    }
    Ok(())
}

/// Functional gate: the bloom filter must never miss an inserted key
/// (`P3: bloom false negative (key {n})`); returns the fixture's exact
/// false-positive count over `LOOKUP_MISSES` absent keys.
pub fn gate_bloom() -> Result<usize, P3Error> {
    let mut filter = Bloom::for_keys(1000);
    for key in 0..1000u64 {
        filter.insert(key);
    }
    for key in 0..1000u64 {
        if !filter.maybe_contains(key) {
            return Err(P3Error::BloomFalseNegative { key });
        }
    }
    let mut fp = 0usize;
    for key in 1_000_000..1_000_000 + LOOKUP_MISSES as u64 {
        if filter.maybe_contains(key) {
            fp += 1;
        }
    }
    Ok(fp)
}

/// Functional gate: one batch commit must equal the same writes
/// committed one by one (batching changes cost, never results).
pub fn gate_batch_equiv(batch: &[(u64, Vec<u8>)]) -> Result<(), P3Error> {
    let mut one = Store::new();
    one.commit(batch);
    let mut many = Store::new();
    for (key, value) in batch {
        many.commit(&[(*key, value.clone())]);
    }
    if one.state() == many.state() {
        Ok(())
    } else {
        Err(P3Error::CompactionLostData)
    }
}

/// Functional gate: compaction must preserve the committed state
/// exactly (`P3: compaction lost data`).
pub fn gate_compact() -> Result<(), P3Error> {
    let mut store = Store::new();
    for block in 0..3u64 {
        let batch: Vec<(u64, Vec<u8>)> = (0..16u64)
            .map(|i| (block * 16 + i, vec![(block + i) as u8; 8]))
            .collect();
        store.commit(&batch);
        store.flush();
    }
    // Overwrite a few keys in the memtable (latest wins through
    // compaction).
    let over: Vec<(u64, Vec<u8>)> = (0..4u64).map(|i| (i, vec![0xEE; 8])).collect();
    store.commit(&over);
    let before = store.state();
    store.compact();
    if store.state() == before {
        Ok(())
    } else {
        Err(P3Error::CompactionLostData)
    }
}

/// Honest wall-clock of the real commit path (evidence only, D-P9):
/// real file appends with a real `fsync` per batch vs per write, and
/// bloom-gated vs raw lookups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P3Measured {
    /// Median ns to commit one 1000-write block, write-by-write path
    /// (an fsync per write).
    pub per_write_ns: u64,
    /// Median ns to commit one 1000-write block, one batch + fsync.
    pub batch_ns: u64,
    /// Wall-clock speedup in percent (can be negative).
    pub speedup_pct: i64,
    /// Median ns per bloom-gated miss lookup.
    pub bloom_miss_ns: u64,
    /// Median ns per raw SSTable-style miss lookup (no filter).
    pub raw_miss_ns: u64,
}

impl P3Measured {
    /// Compact JSON (byte-stable key order).
    pub fn json(&self) -> String {
        format!(
            "{{\"per_write_ns\": {}, \"batch_ns\": {}, \"speedup_pct\": {}, \
             \"bloom_miss_ns\": {}, \"raw_miss_ns\": {}}}",
            self.per_write_ns,
            self.batch_ns,
            self.speedup_pct,
            self.bloom_miss_ns,
            self.raw_miss_ns
        )
    }
}

/// Median of a sample (integer arithmetic).
fn median_u64(sample: &mut [u64]) -> u64 {
    sample.sort_unstable();
    let n = sample.len();
    if n % 2 == 1 {
        sample[n / 2]
    } else {
        (sample[n / 2 - 1] + sample[n / 2]) / 2
    }
}

/// Measure the honest wall-clock of the real commit path on a real
/// file in the temp dir (evidence layer, D-P9). Each block commit is
/// 250 writes of 8 KiB on a real file with real `File::sync_all()`.
pub fn measure(reps: usize) -> P3Measured {
    use std::io::Write;
    assert!(reps > 0, "at least one repetition");
    let path = std::env::temp_dir().join("aura-p3-measure.wal");
    let payload = [0xA5u8; 8192];
    let writes = 250usize;

    let mut per_write_samples = Vec::with_capacity(reps);
    let mut batch_samples = Vec::with_capacity(reps);
    for _ in 0..reps {
        // Baseline: an fsync per write.
        let mut f = std::fs::File::create(&path).expect("temp wal");
        let start = std::time::Instant::now();
        for _ in 0..writes {
            f.write_all(&payload).expect("wal write");
            f.sync_all().expect("fsync per write");
        }
        per_write_samples.push(start.elapsed().as_nanos() as u64);
        // P3: one append burst + one fsync.
        let mut f = std::fs::File::create(&path).expect("temp wal");
        let start = std::time::Instant::now();
        for _ in 0..writes {
            f.write_all(&payload).expect("wal write");
        }
        f.sync_all().expect("fsync per batch");
        batch_samples.push(start.elapsed().as_nanos() as u64);
    }
    let _ = std::fs::remove_file(&path);

    // Lookup path: bloom-gated miss vs raw map miss (in-memory proxy
    // for the SSTable read).
    let mut filter = Bloom::for_keys(1000);
    for key in 0..1000u64 {
        filter.insert(key);
    }
    let table: BTreeMap<u64, u64> = (0..1000u64).map(|k| (k, k)).collect();
    let mut bloom_samples = Vec::with_capacity(reps);
    let mut raw_samples = Vec::with_capacity(reps);
    for _ in 0..reps {
        let start = std::time::Instant::now();
        for key in 1_000_000..1_000_000 + 200 {
            if filter.maybe_contains(key) {
                let _ = table.get(&key);
            }
        }
        bloom_samples.push(start.elapsed().as_nanos() as u64 / 200);
        let start = std::time::Instant::now();
        for key in 1_000_000..1_000_000 + 200 {
            let _ = table.get(&key);
        }
        raw_samples.push(start.elapsed().as_nanos() as u64 / 200);
    }

    let per_write = median_u64(&mut per_write_samples);
    let batch = median_u64(&mut batch_samples);
    P3Measured {
        per_write_ns: per_write,
        batch_ns: batch,
        speedup_pct: if batch == 0 {
            0
        } else {
            (per_write as i64) * 100 / (batch as i64) - 100
        },
        bloom_miss_ns: median_u64(&mut bloom_samples),
        raw_miss_ns: median_u64(&mut raw_samples),
    }
}
