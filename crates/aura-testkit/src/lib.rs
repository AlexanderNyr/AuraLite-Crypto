//! AuraLite-Crypto test rig (C9).
//!
//! This crate owns the **portable** fuzz harness used by `make fuzz-smoke`
//! and the shared golden/property fixtures. It deliberately has no
//! `cargo-fuzz` dependency: the harness runs on stable Rust with a fixed
//! PRNG, so the 60-second gate is reproducible without nightly or sanitizers.
//!
//! `fuzz/` contains complementary `libfuzzer-sys` targets for H3's 24-hour
//! campaign; they call the same [`fuzz_one`] entry point.
//!
//! Specification: `docs/spec/test-rig.md`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use aura_block::{Block, MerkleProof};
use aura_codec::{CanonicalRecord, decode_canonical};
use aura_state::StateKey;
use aura_tx::Tx;

/// Fuzz target names understood by [`fuzz_one`].
pub const FUZZ_TARGETS: [&str; 6] = [
    "codec",
    "tx",
    "state-key",
    "genesis-json",
    "block",
    "merkle-proof",
];

/// Outcome of one fuzz input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuzzOutcome {
    /// The input was decoded (accepted or rejected) without a panic.
    Completed,
    /// The decoder panicked — a real crash.
    Panic,
    /// The target name is not registered.
    UnknownTarget,
}

/// Feed one arbitrary byte string to one decoder, catching panics.
///
/// Every consensus decoder must be total: no panic, no `unwrap`, no OOM on
/// arbitrary input. This is what both the portable harness and the
/// libFuzzer targets check.
pub fn fuzz_one(target: &str, data: &[u8]) -> FuzzOutcome {
    if !FUZZ_TARGETS.contains(&target) {
        return FuzzOutcome::UnknownTarget;
    }
    let result = catch_unwind(AssertUnwindSafe(|| match target {
        "codec" => {
            let _ = decode_canonical::<CanonicalRecord>(data);
        }
        "tx" => {
            let _ = Tx::from_canonical_bytes(data);
        }
        "state-key" => {
            let _ = StateKey::from_bytes(data);
        }
        "genesis-json" => {
            let _ = aura_genesis::from_json_str(&String::from_utf8_lossy(data));
        }
        "block" => {
            let _ = Block::from_canonical_bytes(data);
        }
        "merkle-proof" => {
            let _ = MerkleProof::from_canonical_bytes(data);
        }
        _ => unreachable!("checked above"),
    }));
    if result.is_ok() {
        FuzzOutcome::Completed
    } else {
        FuzzOutcome::Panic
    }
}

/// Measurements from one fuzz run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzReport {
    /// Target name.
    pub target: String,
    /// Number of inputs actually processed.
    pub iterations: u64,
    /// Number of panics observed.
    pub crashes: u64,
    /// Wall-clock time spent.
    pub elapsed_ms: u64,
    /// PRNG seed used.
    pub seed: u64,
}

/// Deterministic xorshift64* PRNG — no external randomness in the rig.
#[derive(Debug, Clone)]
pub struct Lcg {
    state: u64,
}

impl Lcg {
    /// Create a generator from a seed (zero is replaced with a fixed value).
    pub fn new(seed: u64) -> Self {
        Lcg {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    /// Next pseudo-random `u64`.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// Run a time- and iteration-bounded fuzz campaign for one target.
///
/// The loop ends when **both** the wall-clock deadline and
/// `min_iterations` are satisfied. `work` is separate from `iterations` so
/// a broken iteration counter cannot hang the harness (NC-5).
pub fn run_portable_fuzz(target: &str, seconds: u64, min_iterations: u64, seed: u64) -> FuzzReport {
    let mut rng = Lcg::new(seed ^ target.len() as u64);
    let start = Instant::now();
    let deadline = Duration::from_secs(seconds);
    let mut work = 0u64;
    let mut iterations = 0u64;
    let mut crashes = 0u64;

    while work < min_iterations || start.elapsed() < deadline {
        let length = (rng.next_u64() % 257) as usize;
        let mut data = Vec::with_capacity(length);
        for _ in 0..length {
            data.push((rng.next_u64() & 0xFF) as u8);
        }
        if fuzz_one(target, &data) == FuzzOutcome::Panic {
            crashes += 1;
        }
        work += 1;
        iterations += 1;
    }

    FuzzReport {
        target: target.to_owned(),
        iterations,
        crashes,
        elapsed_ms: start.elapsed().as_millis() as u64,
        seed,
    }
}
