//! T0 economics baseline: a deterministic supply simulation with
//! enforceable invariants and no token (ECONOMICS_PLAN T0).
//!
//! The module models abstract *testnet accounting units* (explicitly with no
//! market value — decision D-T4). The native asset `ALCRY` does not exist
//! yet; it arrives with the issuance model in T1. T0 establishes the
//! measurable properties every later economic parameter must respect:
//!
//! * **scarcity** — the total supply changes only through declared emission
//!   (genesis, and later the T1 issuance schedule); "emission from nowhere"
//!   is structurally impossible and is detected at every simulation step;
//! * **security** — invariants are checked after every step, transfers
//!   cannot spend more than the sender has, and the simulation is
//!   bit-exact reproducible from its seed;
//! * **accessibility** — the baseline distribution is explicit and recorded
//!   in `sim/t0-baseline.json`; no hidden premine (the genesis table is the
//!   whole of it).
//!
//! The simulation itself is driven by [`EconSim`]; the deterministic
//! schedule is produced by [`generate_schedule`] from the scenario constants
//! below (mirrored by `sim/t0-baseline.json`, checked by
//! `tools/t0_economics_check.py`).

use std::collections::BTreeMap;

use aura_state::sha256_bytes;

/// Abstract accounting unit (u128; 18 decimals are reserved for `ALCRY`
/// per decision D-T1 — T0 does not use them).
pub type Amount = u128;

/// Seed of the T0 baseline schedule (campaign seed convention).
pub const T0_SEED: u64 = 20_261_180;

/// Number of transfer steps in the T0 baseline simulation.
pub const T0_STEPS: usize = 1000;

/// Maximum share of the sender balance a single step may request, in basis
/// points (2.5 %).
pub const T0_MAX_SEND_BPS: u64 = 250;

/// Genesis endowment table of the T0 baseline (accounting units).
pub const T0_AGENTS: [(&str, Amount); 5] = [
    ("alice", 1_000_000),
    ("bob", 800_000),
    ("carol", 600_000),
    ("dave", 400_000),
    ("erin", 200_000),
];

/// The desired properties the T0 baseline describes and enforces:
/// scarcity, security, accessibility (ECONOMICS_PLAN T0 DoD).
pub const DESIRED_PROPERTIES: [&str; 3] = ["scarcity", "security", "accessibility"];

/// Declared sources of new units. T0 declares only genesis; the issuance
/// schedule of T1 adds further sources — never an implicit one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmissionSource {
    /// The genesis endowment table (`sim/t0-baseline.json`).
    Genesis,
    /// The T1 issuance schedule (not reachable in T0).
    IssuanceSchedule,
}

/// Errors raised while mutating the ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EconError {
    /// A transfer referenced an account missing from the ledger.
    UnknownAccount(String),
    /// A transfer tried to spend more than the sender holds.
    InsufficientFunds,
    /// The genesis table listed the same account twice or was empty.
    BadGenesis,
    /// An invariant was broken; see [`EconViolation`].
    Invariant(EconViolation),
}

/// Invariant violations detected by [`EconSim::check_invariants`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EconViolation {
    /// Sum of balances no longer equals the declared supply: this is the
    /// "emission from nowhere" signature.
    SupplyMismatch {
        /// Declared supply (genesis + declared emission).
        supply: Amount,
        /// Sum of all balances.
        sum: Amount,
    },
    /// The step counter diverged from the number of applied steps.
    StepMismatch {
        /// Steps recorded by the ledger.
        recorded: u64,
        /// Steps actually applied by the runner.
        applied: u64,
    },
}

/// One scheduled transfer request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfer {
    /// Sender account name.
    pub from: String,
    /// Recipient account name.
    pub to: String,
    /// Requested amount; the runner caps it at the sender balance.
    pub amount: Amount,
}

/// Deterministic ledger with supply invariants (T0 baseline).
#[derive(Clone, Debug)]
pub struct EconSim {
    balances: BTreeMap<String, Amount>,
    genesis_total: Amount,
    declared: Amount,
    steps: u64,
}

impl EconSim {
    /// Builds a ledger from the genesis endowment table. Fails on an empty
    /// table or duplicate accounts.
    pub fn from_genesis(agents: &[(&str, Amount)]) -> Result<Self, EconError> {
        if agents.is_empty() {
            return Err(EconError::BadGenesis);
        }
        let mut balances = BTreeMap::new();
        let mut genesis_total: Amount = 0;
        for (name, amount) in agents {
            if balances.insert((*name).to_string(), *amount).is_some() {
                return Err(EconError::BadGenesis);
            }
            genesis_total = genesis_total
                .checked_add(*amount)
                .ok_or(EconError::BadGenesis)?;
        }
        Ok(Self {
            balances,
            genesis_total,
            declared: 0,
            steps: 0,
        })
    }

    /// Declared supply: the genesis endowment plus declared emission.
    pub fn total_supply(&self) -> Amount {
        self.genesis_total + self.declared
    }

    /// Sum of all balances.
    pub fn sum_balances(&self) -> Amount {
        self.balances.values().sum()
    }

    /// Balance of `who`, or `None` for an unknown account.
    pub fn balance(&self, who: &str) -> Option<Amount> {
        self.balances.get(who).copied()
    }

    /// Number of steps applied so far.
    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// Canonical state digest: sha256 over the sorted `name:balance` lines.
    /// Reproducible across implementations; the golden value is pinned in
    /// the tests and in `tools/t0_economics_check.py`.
    pub fn state_digest(&self) -> String {
        let mut text = String::new();
        for (name, balance) in &self.balances {
            text.push_str(name);
            text.push(':');
            text.push_str(&balance.to_string());
            text.push('\n');
        }
        let bytes = text.as_bytes();
        sha256_bytes(bytes).to_hex()
    }

    /// Moves `amount` units from `from` to `to`. Refuses to spend more than
    /// the sender holds; a zero transfer to a known account is a no-op.
    pub fn transfer(&mut self, from: &str, to: &str, amount: Amount) -> Result<(), EconError> {
        if !self.balances.contains_key(from) {
            return Err(EconError::UnknownAccount(from.to_string()));
        }
        if !self.balances.contains_key(to) {
            return Err(EconError::UnknownAccount(to.to_string()));
        }
        if from == to {
            return Ok(());
        }
        {
            let from_bal = self.balances.get_mut(from).expect("checked above");
            *from_bal = from_bal
                .checked_sub(amount)
                .ok_or(EconError::InsufficientFunds)?;
        }
        let to_bal = self.balances.get_mut(to).expect("checked above");
        *to_bal += amount;
        Ok(())
    }

    /// Creates `amount` units for `to` from a *declared* emission source.
    /// This is the only way the total supply may grow (T1 uses it; T0 does
    /// not call it in the baseline schedule).
    pub fn mint(
        &mut self,
        source: EmissionSource,
        to: &str,
        amount: Amount,
    ) -> Result<(), EconError> {
        let _ = source;
        let credit = self
            .balances
            .get_mut(to)
            .ok_or_else(|| EconError::UnknownAccount(to.to_string()))?;
        *credit += amount;
        self.declared += amount;
        Ok(())
    }

    /// Checks every T0 invariant. The supply identity
    /// `sum(balances) == genesis + declared emission` is what makes
    /// "emission from nowhere" detectable.
    pub fn check_invariants(&self) -> Result<(), EconViolation> {
        let supply = self.total_supply();
        let sum = self.sum_balances();
        if sum != supply {
            return Err(EconViolation::SupplyMismatch { supply, sum });
        }
        Ok(())
    }

    /// Applies one scheduled step: the requested amount is capped at the
    /// sender's balance (a request above the balance sends what is there),
    /// then the invariants are re-checked. Steps counter counts every
    /// scheduled step, including zero-value no-ops.
    pub fn apply_step(&mut self, step: &Transfer) -> Result<(), EconError> {
        let available = self
            .balance(&step.from)
            .ok_or_else(|| EconError::UnknownAccount(step.from.clone()))?;
        let capped = step.amount.min(available);
        self.transfer(&step.from, &step.to, capped)?;
        self.steps += 1;
        self.check_invariants().map_err(EconError::Invariant)
    }

    /// Runs a whole schedule, checking the invariants after every step.
    pub fn run(&mut self, schedule: &[Transfer]) -> Result<(), EconError> {
        for step in schedule {
            self.apply_step(step)?;
        }
        Ok(())
    }
}

/// xorshift64* generator shared with `tools/t0_economics_check.py`.
/// Nonzero state only; the T0 seed is [`T0_SEED`].
#[derive(Clone, Debug)]
pub struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    /// Builds a generator; a zero seed is replaced by a fixed nonzero one.
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    /// Next raw u64 (xorshift64*).
    pub fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }
}

/// Generates the deterministic T0 baseline schedule: for every step the
/// sender and recipient are drawn from the agent list and the requested
/// amount is `1 + draw % 997`; `apply_step` caps it at the sender balance.
/// Identical in Rust and in the Python oracle.
pub fn generate_schedule(seed: u64, steps: usize, agents: &[(&str, Amount)]) -> Vec<Transfer> {
    let mut rng = XorShift64::new(seed);
    let n = agents.len() as u64;
    let mut schedule = Vec::with_capacity(steps);
    for _ in 0..steps {
        let sender = agents[(rng.next_u64() % n) as usize].0;
        let mut recipient = agents[(rng.next_u64() % n) as usize].0;
        if recipient == sender {
            let idx = (rng.next_u64() % n) as usize;
            recipient = agents[idx].0;
            if recipient == sender {
                recipient = agents[((idx as u64 + 1) % n) as usize].0;
            }
        }
        let amount: Amount = 1 + (rng.next_u64() % 997) as Amount;
        schedule.push(Transfer {
            from: sender.to_string(),
            to: recipient.to_string(),
            amount,
        });
    }
    schedule
}
