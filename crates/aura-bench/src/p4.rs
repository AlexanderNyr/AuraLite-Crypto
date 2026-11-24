//! P4 network profile — deterministic gossip-traffic cost model
//! (PERF_PLAN P4: vote batching, compression, gossipsub parameters,
//! mesh tuning; DoD: **100 nodes: traffic/validator within budget**).
//!
//! Self-contained profiling artifact (decision **D-P7**, the P3
//! pattern): a byte-accurate model of the consensus gossip path on a
//! 100-validator stand (D-P1 fixture), with four optimization levers,
//! each carrying a before/after pair (D-P2). The budget is published
//! with its methodology in `docs/perf/p4-network-report.md` (D-P3).
//!
//! Vocabulary follows the gossipsub v1.0 specification (D, D_low,
//! D_high, heartbeat, `GossipFactor`) and the N2 dictionary (`vote`,
//! `proposal`, `QC` topics).

use std::collections::BTreeMap;
use std::fmt;

use crate::{MachineProfile, hex, percentile, sha256_32, spread_pct};

/// Number of validator nodes on the stand (D-P1 fixture).
pub const N_NODES: usize = 100;
/// Consensus cadence: one view (round) per second. Bytes per view and
/// bytes per second are therefore the same number (published).
pub const VIEW_RATE: u64 = 1;
/// Wire size of one vote payload (height + round + block hash + sig).
pub const VOTE_PAYLOAD: u64 = 128;
/// Gossip envelope per message (topic + seqno + from + signature).
pub const ENV_BYTES: u64 = 64;
/// Proposal body (block) size before compression.
pub const PROPOSAL_BODY: u64 = 8192;
/// Proposal header size.
pub const PROPOSAL_HDR: u64 = 256;
/// QC payload (aggregated signature + bitmap).
pub const QC_PAYLOAD: u64 = 128;
/// Vote batching width (votes per batched forward message).
pub const BATCH_K: usize = 8;
/// RLE codec header bytes.
pub const RLE_HDR: u64 = 16;
/// Baseline mesh degree (a flood-ish mesh above the spec default).
pub const D_BEFORE: usize = 8;
/// Tuned mesh degree (below the spec default: bandwidth-class
/// validators; D_low..D_high keep the mesh stable).
pub const D_AFTER: usize = 5;
/// Tuned mesh lower bound (gossipsub `D_low`).
pub const D_LO: usize = 3;
/// Tuned mesh upper bound (gossipsub `D_high`).
pub const D_HI: usize = 8;
/// Tuned heartbeat in milliseconds (spec default 1000).
pub const HEARTBEAT_MS: u64 = 700;
/// Tuned mcache length in heartbeats (rust-libp2p `history_length` 5).
pub const MCACHE: u64 = 5;
/// Baseline gossip emission factor in percent (rust-libp2p
/// `gossip_factor` default 0.25).
pub const GOSSIP_FACTOR_BEFORE: u64 = 25;
/// Tuned gossip emission factor in percent (0.10).
pub const GOSSIP_FACTOR_AFTER: u64 = 10;
/// Advertisement hops accounted per message (IHAVE/IWANT overhead).
pub const ADV_HOPS: u64 = 2;
/// Published budget: bytes of gossip traffic per validator per second
/// (125 000 B/s = 1 Mbit/s = 10% of the per-validator share of the
/// D-P1 1 Gbit/s fabric; methodology in the report).
pub const BUDGET_BYTES: u64 = 125_000;
/// Deterministic model repeats.
pub const RUNS: usize = 5;

/// Named P4 messages (one line each; the NC contract).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum P4Error {
    /// Per-validator traffic exceeded the published budget.
    BudgetExceeded {
        /// Observed bytes per validator per second.
        bytes: u64,
        /// The published budget (bytes per second).
        budget: u64,
    },
    /// Reverting vote batching removed its win.
    BatchingWinReverted {
        /// Vote wire bytes before batching.
        before: u64,
        /// Vote wire bytes after (reverted) batching.
        after: u64,
    },
    /// The compression codec is not lossless.
    CompressionRoundtrip {
        /// Offset of the first mismatching byte.
        byte: u64,
    },
    /// The gossipsub parameter set is not a tuned set.
    ParamsInconsistent {
        /// Observed mesh degree D.
        d: usize,
        /// Observed D_low.
        lo: usize,
        /// Observed D_high.
        hi: usize,
    },
    /// The tuned mesh does not connect the stand.
    MeshDisconnected {
        /// Node count of the stand.
        nodes: usize,
    },
    /// Two identical runs disagreed (X7).
    DeterminismViolated {
        /// Index of the disagreeing run.
        run: usize,
    },
}

impl fmt::Display for P4Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BudgetExceeded { bytes, budget } => {
                write!(f, "P4: traffic/validator exceeds budget ({bytes}/{budget})")
            }
            Self::BatchingWinReverted { before, after } => {
                write!(f, "P4: vote batching win reverted ({before}/{after})")
            }
            Self::CompressionRoundtrip { byte } => {
                write!(f, "P4: compression roundtrip mismatch (byte {byte})")
            }
            Self::ParamsInconsistent { d, lo, hi } => {
                write!(f, "P4: gossip parameter set inconsistent (D={d}/{lo}/{hi})")
            }
            Self::MeshDisconnected { nodes } => {
                write!(f, "P4: mesh disconnected at {nodes} nodes")
            }
            Self::DeterminismViolated { run } => {
                write!(f, "P4: determinism violated (run {run})")
            }
        }
    }
}

impl std::error::Error for P4Error {}

/// Proposal body fixture: 8192 bytes with runs of 32 identical
/// values (deterministic; the RLE codec is exercised on it).
pub fn proposal_body() -> Vec<u8> {
    (0..PROPOSAL_BODY).map(|i| ((i / 32) % 251) as u8).collect()
}

/// Lossless RLE encoder: (value, count) pairs, count in 1..=255.
pub fn rle_encode(body: &[u8]) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    let mut iter = body.iter().copied();
    if let Some(first) = iter.next() {
        let mut value = first;
        let mut count: u8 = 1;
        for byte in iter {
            if byte == value && count < 255 {
                count += 1;
            } else {
                out.push((value, count));
                value = byte;
                count = 1;
            }
        }
        out.push((value, count));
    }
    out
}

/// Lossless RLE decoder.
pub fn rle_decode(pairs: &[(u8, u8)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (value, count) in pairs {
        out.extend(std::iter::repeat_n(*value, *count as usize));
    }
    out
}

/// Wire size of one batched forward message: batched payloads plus
/// one envelope.
pub fn batch_wire_bytes(payloads: &[u64]) -> u64 {
    payloads.iter().sum::<u64>() + ENV_BYTES
}

/// Votes on the wire before batching: one envelope per vote.
pub fn votes_wire_before(votes: usize) -> u64 {
    votes as u64 * (VOTE_PAYLOAD + ENV_BYTES)
}

/// Votes on the wire after batching (`BATCH_K` votes per envelope).
pub fn votes_wire_after(votes: usize, batch_k: usize) -> u64 {
    let k = batch_k.max(1);
    let full = votes / k;
    let rest = votes % k;
    let mut total = 0u64;
    for _ in 0..full {
        total += batch_wire_bytes(&vec![VOTE_PAYLOAD; k]);
    }
    if rest > 0 {
        total += batch_wire_bytes(&vec![VOTE_PAYLOAD; rest]);
    }
    total
}

/// Proposal on the wire before compression.
pub fn proposal_wire_before() -> u64 {
    PROPOSAL_BODY + PROPOSAL_HDR + ENV_BYTES
}

/// Proposal on the wire after the RLE codec (actual encoded size).
pub fn proposal_wire_after() -> u64 {
    let pairs = rle_encode(&proposal_body());
    RLE_HDR + pairs.len() as u64 * 2 + ENV_BYTES
}

/// QC on the wire (not affected by the four levers).
pub fn qc_wire_bytes() -> u64 {
    QC_PAYLOAD + ENV_BYTES
}

/// Total message bytes per view.
pub fn msg_bytes(votes_wire: u64, proposal_wire: u64) -> u64 {
    votes_wire + proposal_wire + qc_wire_bytes()
}

/// Gossip advertisement overhead per view (IHAVE/IWANT accounting):
/// `factor` percent of message bytes over `ADV_HOPS` hops.
pub fn overhead_bytes(factor_pct: u64, msg: u64) -> u64 {
    factor_pct * msg * ADV_HOPS / 100
}

/// Per-validator gossip traffic per view (egress): each node forwards
/// every message to `degree` mesh peers, message plus overhead.
pub fn per_validator_bytes(degree: usize, msg: u64, overhead: u64) -> u64 {
    degree as u64 * (msg + overhead)
}

/// Conservative sensitivity (published in the report): per-validator
/// traffic if real blocks compress at only 2:1 instead of the RLE
/// fixture ratio. Must stay within [`BUDGET_BYTES`].
pub fn sensitivity_two_to_one() -> u64 {
    let proposal = (PROPOSAL_BODY + PROPOSAL_HDR) / 2 + 32 + ENV_BYTES;
    let msg = msg_bytes(votes_wire_after(N_NODES, BATCH_K), proposal);
    per_validator_bytes(D_AFTER, msg, overhead_bytes(GOSSIP_FACTOR_AFTER, msg))
}

/// One mesh ring + chords adjacency (N2 `GossipMesh` shape): the
/// backward/forward ring links plus a diameter chord.
pub fn build_mesh(n: usize, degree: usize) -> Vec<Vec<usize>> {
    let mut adj: Vec<Vec<usize>> = Vec::with_capacity(n);
    for i in 0..n {
        let back = |step: usize| (i + n - (step % n)) % n;
        let mut peers = vec![(i + 1) % n, back(1), (i + 2) % n, back(2), (i + n / 2) % n];
        peers.sort_unstable();
        peers.dedup();
        peers.truncate(degree);
        adj.push(peers);
    }
    adj
}

/// BFS reachability over the adjacency.
pub fn mesh_connected(adj: &[Vec<usize>]) -> bool {
    let n = adj.len();
    if n == 0 {
        return true;
    }
    let mut seen = vec![false; n];
    let mut queue = std::collections::VecDeque::new();
    seen[0] = true;
    queue.push_back(0);
    while let Some(node) = queue.pop_front() {
        for peer in &adj[node] {
            if !seen[*peer] {
                seen[*peer] = true;
                queue.push_back(*peer);
            }
        }
    }
    seen.iter().all(|s| *s)
}

/// The consensus gossip ledger of one view (state digest source).
pub fn ledger_lines() -> Vec<String> {
    let votes = votes_wire_after(N_NODES, BATCH_K);
    let proposal = proposal_wire_after();
    let msg = msg_bytes(votes, proposal);
    let over = overhead_bytes(GOSSIP_FACTOR_AFTER, msg);
    let per_val = per_validator_bytes(D_AFTER, msg, over);
    let mut lines = vec![
        format!("p4s|ledger|votes={votes}"),
        format!("p4s|ledger|proposal={proposal}"),
        format!("p4s|ledger|qc={}", qc_wire_bytes()),
        format!("p4s|ledger|overhead={over}"),
        format!("p4s|ledger|per_val={per_val}"),
    ];
    for (i, peers) in build_mesh(N_NODES, D_AFTER).iter().enumerate() {
        let joined = peers
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<String>>()
            .join(",");
        lines.push(format!("p4s|mesh|{i}:{joined}"));
    }
    lines
}

/// State digest over the ledger + mesh adjacency.
pub fn state_digest() -> String {
    let mut joined = ledger_lines().join("|");
    joined.push('|');
    hex(&sha256_32(joined.as_bytes()))
}

/// The four before/after pairs plus the budget verdict (D-P2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P4Pairs {
    /// Vote line bytes before/after batching.
    pub votes: (u64, u64),
    /// Proposal line bytes before/after compression.
    pub proposal: (u64, u64),
    /// Gossip overhead bytes before/after parameter tuning.
    pub overhead: (u64, u64),
    /// Mesh degree before/after mesh tuning.
    pub mesh: (usize, usize),
    /// Per-validator traffic before/after (all levers).
    pub traffic: (u64, u64),
}

impl P4Pairs {
    /// Compute the pairs from the frozen constants.
    pub fn compute() -> Self {
        let votes_before = votes_wire_before(N_NODES);
        let votes_after = votes_wire_after(N_NODES, BATCH_K);
        let proposal_before = proposal_wire_before();
        let proposal_after = proposal_wire_after();
        let msg_before = msg_bytes(votes_before, proposal_before);
        let msg_after = msg_bytes(votes_after, proposal_after);
        let over_before = overhead_bytes(GOSSIP_FACTOR_BEFORE, msg_before);
        let over_after = overhead_bytes(GOSSIP_FACTOR_AFTER, msg_after);
        let traffic_before = per_validator_bytes(D_BEFORE, msg_before, over_before);
        let traffic_after = per_validator_bytes(D_AFTER, msg_after, over_after);
        Self {
            votes: (votes_before, votes_after),
            proposal: (proposal_before, proposal_after),
            overhead: (over_before, over_after),
            mesh: (D_BEFORE, D_AFTER),
            traffic: (traffic_before, traffic_after),
        }
    }
}

/// Deterministic traffic report (the P3 skeleton).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P4Report {
    /// Workload digest.
    pub workload_digest: String,
    /// Machine digest (D-P1).
    pub machine_digest: String,
    /// Model repeats.
    pub runs: usize,
    /// Spread across repeats (percent).
    pub spread_pct: u64,
    /// Median per-validator traffic (bytes/s).
    pub p50: u64,
    /// p95 per-validator traffic (bytes/s).
    pub p95: u64,
    /// p99 per-validator traffic (bytes/s) — the DoD number.
    pub p99: u64,
    /// Sum over repeats.
    pub total: u64,
    /// Per-validator traffic before (bytes/s).
    pub before: u64,
    /// Per-validator traffic after (bytes/s).
    pub after: u64,
    /// Published budget (bytes/s).
    pub budget: u64,
    /// Traffic reduction (percent, honest integer).
    pub reduction_pct: u64,
    /// The four optimization pairs (D-P2).
    pub pairs: P4Pairs,
    /// State digest.
    pub state_digest: String,
    /// Bench digest over the canonical line.
    pub bench_digest: String,
}

impl P4Report {
    /// Build the deterministic report from the frozen constants.
    pub fn build(runs: usize) -> Self {
        let pairs = P4Pairs::compute();
        let (before, after) = pairs.traffic;
        let run_costs = vec![after; runs];
        let p50 = percentile(&run_costs, 50);
        let p95 = percentile(&run_costs, 95);
        let p99 = percentile(&run_costs, 99);
        let total: u64 = run_costs.iter().sum();
        let spread_pct = spread_pct(&run_costs);
        let reduction_pct = (before - after) * 100 / before;
        let workload_digest = hex(&sha256_32(
            format!(
                "p4w|net-gossip|nodes={N_NODES}|views={VIEW_RATE}|vote={VOTE_PAYLOAD}|body={PROPOSAL_BODY}|k={BATCH_K}"
            )
            .as_bytes(),
        ));
        let machine_digest = MachineProfile::REFERENCE.digest();
        let sd = state_digest();
        let canonical = format!(
            "p4b|{}|{}|runs={runs}|spread={spread_pct}|p50={p50}|p95={p95}|p99={p99}|total={total}|before={before}|after={after}|budget={}|reduction_pct={reduction_pct}|votes={}/{}|proposal={}/{}|overhead={}/{}|mesh={}/{}",
            workload_digest,
            machine_digest,
            BUDGET_BYTES,
            pairs.votes.0,
            pairs.votes.1,
            pairs.proposal.0,
            pairs.proposal.1,
            pairs.overhead.0,
            pairs.overhead.1,
            pairs.mesh.0,
            pairs.mesh.1,
        );
        Self {
            workload_digest,
            machine_digest,
            runs,
            spread_pct,
            p50,
            p95,
            p99,
            total,
            before,
            after,
            budget: BUDGET_BYTES,
            reduction_pct,
            pairs,
            state_digest: sd,
            bench_digest: hex(&sha256_32(canonical.as_bytes())),
        }
    }

    /// The canonical line (digest source).
    pub fn canonical(&self) -> String {
        format!(
            "p4b|{}|{}|runs={}|spread={}|p50={}|p95={}|p99={}|total={}|before={}|after={}|budget={}|reduction_pct={}|votes={}/{}|proposal={}/{}|overhead={}/{}|mesh={}/{}",
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
            self.budget,
            self.reduction_pct,
            self.pairs.votes.0,
            self.pairs.votes.1,
            self.pairs.proposal.0,
            self.pairs.proposal.1,
            self.pairs.overhead.0,
            self.pairs.overhead.1,
            self.pairs.mesh.0,
            self.pairs.mesh.1,
        )
    }

    /// JSON line for the evidence / sim output.
    pub fn json(&self) -> String {
        let p = &self.pairs;
        format!(
            "{{\"workload_digest\": \"{}\", \"machine_digest\": \"{}\", \"runs\": {}, \"spread_pct\": {}, \"p50\": {}, \"p95\": {}, \"p99\": {}, \"total\": {}, \"before\": {}, \"after\": {}, \"budget\": {}, \"reduction_pct\": {}, \"votes_before\": {}, \"votes_after\": {}, \"proposal_before\": {}, \"proposal_after\": {}, \"overhead_before\": {}, \"overhead_after\": {}, \"mesh_before\": {}, \"mesh_after\": {}, \"nodes\": {N_NODES}, \"view_rate\": {VIEW_RATE}, \"sensitivity_2to1_bytes\": {}, \"bench_digest\": \"{}\"}}",
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
            self.budget,
            self.reduction_pct,
            p.votes.0,
            p.votes.1,
            p.proposal.0,
            p.proposal.1,
            p.overhead.0,
            p.overhead.1,
            p.mesh.0,
            p.mesh.1,
            sensitivity_two_to_one(),
            self.bench_digest,
        )
    }

    /// The DoD gates: budget → batching win → determinism.
    pub fn validate(&self) -> Result<(), P4Error> {
        if self.after > self.budget {
            return Err(P4Error::BudgetExceeded {
                bytes: self.after,
                budget: self.budget,
            });
        }
        if self.pairs.votes.1 >= self.pairs.votes.0 {
            return Err(P4Error::BatchingWinReverted {
                before: self.pairs.votes.0,
                after: self.pairs.votes.1,
            });
        }
        let again = Self::build(self.runs);
        if again.canonical() != self.canonical() {
            return Err(P4Error::DeterminismViolated { run: 1 });
        }
        Ok(())
    }
}

/// Functional gate: batching changes the wire, never the votes —
/// unpacking the batches yields exactly the input votes in order.
pub fn gate_vote_batching(votes: &[(u64, Vec<u8>)]) -> Result<(u64, u64), P4Error> {
    let k = BATCH_K;
    let mut batches: Vec<Vec<(u64, Vec<u8>)>> = Vec::new();
    let mut index = 0;
    while index < votes.len() {
        let end = (index + k).min(votes.len());
        batches.push(votes[index..end].to_vec());
        index = end;
    }
    let flat: Vec<(u64, Vec<u8>)> = batches.into_iter().flatten().collect();
    if flat.len() != votes.len() {
        return Err(P4Error::BatchingWinReverted {
            before: votes.len() as u64,
            after: flat.len() as u64,
        });
    }
    for (i, (key, payload)) in votes.iter().enumerate() {
        if flat[i].0 != *key || flat[i].1 != *payload {
            return Err(P4Error::BatchingWinReverted {
                before: votes.len() as u64,
                after: i as u64,
            });
        }
    }
    Ok((
        votes_wire_before(votes.len()),
        votes_wire_after(votes.len(), k),
    ))
}

/// Functional gate: the compression codec is lossless on the fixture.
pub fn gate_compression() -> Result<(u64, u64), P4Error> {
    let body = proposal_body();
    let pairs = rle_encode(&body);
    let back = rle_decode(&pairs);
    if back.len() != body.len() {
        return Err(P4Error::CompressionRoundtrip {
            byte: back.len() as u64,
        });
    }
    for (i, byte) in body.iter().enumerate() {
        if back[i] != *byte {
            return Err(P4Error::CompressionRoundtrip { byte: i as u64 });
        }
    }
    Ok((proposal_wire_before(), proposal_wire_after()))
}

/// Functional gate: the gossipsub parameter set is tuned and its
/// overhead win is real.
pub fn gate_gossip_params() -> Result<(u64, u64), P4Error> {
    if D_LO > D_AFTER || D_AFTER > D_HI || HEARTBEAT_MS > 1000 || MCACHE < 3 {
        return Err(P4Error::ParamsInconsistent {
            d: D_AFTER,
            lo: D_LO,
            hi: D_HI,
        });
    }
    if GOSSIP_FACTOR_AFTER >= GOSSIP_FACTOR_BEFORE {
        return Err(P4Error::ParamsInconsistent {
            d: D_AFTER,
            lo: D_LO,
            hi: D_HI,
        });
    }
    let msg_before = msg_bytes(votes_wire_before(N_NODES), proposal_wire_before());
    let msg_after = msg_bytes(votes_wire_after(N_NODES, BATCH_K), proposal_wire_after());
    Ok((
        overhead_bytes(GOSSIP_FACTOR_BEFORE, msg_before),
        overhead_bytes(GOSSIP_FACTOR_AFTER, msg_after),
    ))
}

/// Functional gate: the tuned mesh keeps the 100-node stand
/// connected.
pub fn gate_mesh() -> Result<(usize, usize), P4Error> {
    let adj = build_mesh(N_NODES, D_AFTER);
    if !mesh_connected(&adj) {
        return Err(P4Error::MeshDisconnected { nodes: N_NODES });
    }
    Ok((D_BEFORE, D_AFTER))
}

/// Budget gate: the tuned path fits the published budget while the
/// baseline honestly fails it.
pub fn gate_budget() -> Result<u64, P4Error> {
    let pairs = P4Pairs::compute();
    if pairs.traffic.1 > BUDGET_BYTES {
        return Err(P4Error::BudgetExceeded {
            bytes: pairs.traffic.1,
            budget: BUDGET_BYTES,
        });
    }
    Ok(pairs.traffic.1)
}

/// Honest measured layer (D-P9, evidence only — never a gate): real
/// RLE encode/decode throughput and a real batching loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P4Measured {
    /// RLE encode of the 8 KiB fixture, nanoseconds.
    pub rle_encode_ns: u64,
    /// RLE decode back, nanoseconds.
    pub rle_decode_ns: u64,
    /// Batching 100 vote records, nanoseconds.
    pub batch_ns: u64,
    /// Copying 100 vote records uncompressed, nanoseconds.
    pub raw_copy_ns: u64,
}

impl P4Measured {
    /// JSON line.
    pub fn json(&self) -> String {
        format!(
            "{{\"rle_encode_ns\": {}, \"rle_decode_ns\": {}, \"batch_ns\": {}, \"raw_copy_ns\": {}}}",
            self.rle_encode_ns, self.rle_decode_ns, self.batch_ns, self.raw_copy_ns
        )
    }
}

/// Measure the real codec and batching loops (medians of `reps`).
pub fn measure(reps: usize) -> P4Measured {
    let body = proposal_body();
    let votes: Vec<(u64, Vec<u8>)> = (0..100u64).map(|i| (i, vec![7u8; 128])).collect();
    let mut enc = Vec::new();
    let mut dec = Vec::new();
    let mut bat = Vec::new();
    let mut raw = Vec::new();
    for _ in 0..reps {
        let start = std::time::Instant::now();
        let pairs = rle_encode(&body);
        enc.push(start.elapsed().as_nanos() as u64);
        let start = std::time::Instant::now();
        let back = rle_decode(&pairs);
        dec.push(start.elapsed().as_nanos() as u64);
        assert_eq!(back.len(), body.len());
        let start = std::time::Instant::now();
        let mut batches = Vec::new();
        for chunk in votes.chunks(BATCH_K) {
            batches.push(chunk.to_vec());
        }
        bat.push(start.elapsed().as_nanos() as u64);
        assert_eq!(batches.iter().map(Vec::len).sum::<usize>(), 100);
        let start = std::time::Instant::now();
        let mut sink = 0u64;
        for (key, payload) in &votes {
            sink ^= key.wrapping_add(payload.len() as u64);
        }
        raw.push(start.elapsed().as_nanos() as u64);
        assert_ne!(sink, u64::MAX);
    }
    enc.sort_unstable();
    dec.sort_unstable();
    bat.sort_unstable();
    raw.sort_unstable();
    P4Measured {
        rle_encode_ns: enc[enc.len() / 2],
        rle_decode_ns: dec[dec.len() / 2],
        batch_ns: bat[bat.len() / 2],
        raw_copy_ns: raw[raw.len() / 2],
    }
}

/// Convenience: the vote fixture used by gates and tests.
pub fn vote_fixture() -> Vec<(u64, Vec<u8>)> {
    (0..N_NODES as u64)
        .map(|i| (i, vec![(i % 251) as u8; VOTE_PAYLOAD as usize]))
        .collect()
}

/// Map over named run costs (kept for test parity with P3).
pub fn run_costs(after: u64, runs: usize) -> Vec<u64> {
    vec![after; runs]
}

/// Histogram of the ledger state (BTreeMap for determinism).
pub fn ledger_state() -> BTreeMap<String, u64> {
    let mut map = BTreeMap::new();
    for line in ledger_lines() {
        let mut parts = line.split('|').skip(2);
        if let Some(pair) = parts.next()
            && let Some((key, value)) = pair.split_once('=')
            && let Ok(parsed) = value.parse::<u64>()
        {
            map.insert(key.to_string(), parsed);
        }
    }
    map
}

/// Digest helper exposed for tests.
pub fn digest_hex(text: &str) -> String {
    hex(&sha256_32(text.as_bytes()))
}

/// Hex of raw bytes (test helper).
pub fn hex_of(bytes: &[u8]) -> String {
    hex(bytes)
}
