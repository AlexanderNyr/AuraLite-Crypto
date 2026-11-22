//! T5 slashing and insurance: penalty parameters, insurance fund and
//! mass-slash protection (ECONOMICS_PLAN T5).
//!
//! A misbehaving validator causes a measurable **damage** to the network
//! and its users. The model books each incident as a [`SlashRecord`]:
//!
//! * **penalty parameters** — the offender pays
//!   `damage * `[`T5_PENALTY_BPS`]` / `[`T5_BPS`] (150 %), so the penalty
//!   always covers the damage;
//! * **insurance fund** — the victims are made whole from the
//!   [`InsuranceFund`] up to its available balance
//!   (`covered = min(damage, balance)`); the uncovered remainder is a
//!   recorded `shortfall`. The fund is replenished from penalties:
//!   [`T5_FUND_SHARE_BPS`] (50 %) of every applied penalty flows in.
//!   The fund balance must **never go negative**;
//! * **mass-slash protection** — no window may destroy more than
//!   [`T5_MASS_SLASH_CAP_BPS`] (5 %) of the registered bonded base
//!   ([`T5_REGISTERED_BONDED`]); excess penalty is **deferred**, not
//!   dropped, and is collected in later windows
//!   ([`InsuranceFund::advance`]) until the queue drains.
//!
//! The pinned scenario [`run_t5_scenario`] drives the seven gate
//! scenarios (single double-sign, repeat offender, correlated mass
//! slash, fund exhaustion, penalty margin floor, mass-slash cap over
//! windows, exhaustion and refill chain); [`scenario_digest`] is
//! cross-run against the independent Python oracle
//! (`tools/t5_slashing_check.py`).

use crate::econ::Amount;
use crate::issuance::ALCRY;

/// Penalty on top of the caused damage, in basis points (150 %).
pub const T5_PENALTY_BPS: u64 = 15_000;

/// Share of every applied penalty that replenishes the insurance fund,
/// in basis points (50 %).
pub const T5_FUND_SHARE_BPS: u64 = 5_000;

/// Mass-slash protection: maximum share of the registered bonded base
/// that one window may destroy, in basis points (5 %).
pub const T5_MASS_SLASH_CAP_BPS: u64 = 500;

/// Basis points denominator.
pub const T5_BPS: u64 = 10_000;

/// Registered bonded base of the epoch: the mass-slash cap is measured
/// against it (pinned policy parameter; per-validator stake accounting
/// is the T4 staking model).
pub const T5_REGISTERED_BONDED: Amount = 1_000_000 * ALCRY;

/// Initial insurance fund balance.
pub const T5_FUND_SEED: Amount = 200_000 * ALCRY;

/// Number of gate scenarios in [`run_t5_scenario`].
pub const T5_SCENARIOS: u32 = 7;

/// Names of the seven gate scenarios (ids 1..=7 in the step log).
pub const T5_SCENARIO_NAMES: [&str; 7] = [
    "single double-sign",
    "repeat offender",
    "correlated mass slash",
    "fund exhaustion",
    "penalty margin floor",
    "mass-slash cap over windows",
    "exhaustion and refill chain",
];

/// Penalty booked for one incident: `damage * T5_PENALTY_BPS / T5_BPS`.
fn penalty_of(damage: Amount) -> Amount {
    damage * Amount::from(T5_PENALTY_BPS) / Amount::from(T5_BPS)
}

/// Insurance income from an applied penalty:
/// `applied * T5_FUND_SHARE_BPS / T5_BPS`.
fn income_of(applied: Amount) -> Amount {
    applied * Amount::from(T5_FUND_SHARE_BPS) / Amount::from(T5_BPS)
}

/// Mass-slash cap of one window: 5 % of [`T5_REGISTERED_BONDED`].
fn window_cap() -> Amount {
    T5_REGISTERED_BONDED * Amount::from(T5_MASS_SLASH_CAP_BPS) / Amount::from(T5_BPS)
}

/// One booked slashing incident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlashRecord {
    /// Incident number within the run.
    pub index: u32,
    /// Slashing window the incident was booked in.
    pub window: u32,
    /// Scenario id of the pinned run (0 for ad-hoc incidents).
    pub scenario: u32,
    /// Damage caused to the network and its users.
    pub damage: Amount,
    /// Penalty taken from the offender (>= damage).
    pub penalty: Amount,
    /// Loss covered from the insurance fund (<= damage).
    pub covered: Amount,
    /// Part of the penalty applied within the window cap.
    pub applied: Amount,
    /// Insurance income: `T5_FUND_SHARE_BPS` of `applied`.
    pub income: Amount,
    /// Uncovered part of the damage: `damage - covered`.
    pub shortfall: Amount,
}

/// Insurance fund with a slash ledger and mass-slash protection.
#[derive(Debug, Clone)]
pub struct InsuranceFund {
    balance: i128,
    window: u32,
    window_applied: Amount,
    pending: Amount,
    total_penalty: Amount,
    total_applied: Amount,
    total_covered: Amount,
    total_shortfall: Amount,
    records: Vec<SlashRecord>,
}

impl InsuranceFund {
    /// New fund seeded with [`T5_FUND_SEED`], window 0 open.
    pub fn new() -> Self {
        Self {
            balance: T5_FUND_SEED as i128,
            window: 0,
            window_applied: 0,
            pending: 0,
            total_penalty: 0,
            total_applied: 0,
            total_covered: 0,
            total_shortfall: 0,
            records: Vec::new(),
        }
    }

    /// Current fund balance (signed so an overdraw would be visible).
    pub fn balance(&self) -> i128 {
        self.balance
    }

    /// Penalty applied but not yet collected (mass-slash deferral).
    pub fn pending(&self) -> Amount {
        self.pending
    }

    /// Penalties booked so far.
    pub fn total_penalty(&self) -> Amount {
        self.total_penalty
    }

    /// Penalties applied so far (window-capped).
    pub fn total_applied(&self) -> Amount {
        self.total_applied
    }

    /// Damage covered from the fund so far.
    pub fn total_covered(&self) -> Amount {
        self.total_covered
    }

    /// Damage not covered so far.
    pub fn total_shortfall(&self) -> Amount {
        self.total_shortfall
    }

    /// Current window number.
    pub fn window(&self) -> u32 {
        self.window
    }

    /// Penalty applied within the current window.
    pub fn window_applied(&self) -> Amount {
        self.window_applied
    }

    /// The incident ledger.
    pub fn records(&self) -> &[SlashRecord] {
        &self.records
    }

    /// Book one incident: penalty from the offender, mass-slash cap on
    /// the applied part, insurance income, coverage of the damage.
    pub fn report(&mut self, damage: Amount, scenario: u32) -> &SlashRecord {
        let penalty = penalty_of(damage);
        let room = window_cap().saturating_sub(self.window_applied);
        let applied = penalty.min(room);
        self.pending += penalty - applied;
        self.window_applied += applied;
        self.total_penalty += penalty;
        self.total_applied += applied;
        let income = income_of(applied);
        self.balance += income as i128;
        let free = self.balance.max(0) as Amount;
        let covered = damage.min(free);
        self.balance -= covered as i128;
        let shortfall = damage - covered;
        self.total_covered += covered;
        self.total_shortfall += shortfall;
        let index = self.records.len() as u32;
        self.records.push(SlashRecord {
            index,
            window: self.window,
            scenario,
            damage,
            penalty,
            covered,
            applied,
            income,
            shortfall,
        });
        self.records.last().expect("just pushed")
    }

    /// Close the current window: apply up to one window cap of the
    /// deferred penalty (mass-slash protection) and open the next one.
    pub fn advance(&mut self, _scenario: u32) {
        self.window += 1;
        self.window_applied = 0;
        let taken = self.pending.min(window_cap());
        self.pending -= taken;
        self.window_applied = taken;
        self.total_applied += taken;
        let income = income_of(taken);
        self.balance += income as i128;
    }
}

impl Default for InsuranceFund {
    fn default() -> Self {
        Self::new()
    }
}

/// One action of the pinned scenario run (digest step).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct T5Step {
    /// Step number, starting at 0.
    pub index: u32,
    /// 0 = incident booked, 1 = window advanced, 9 = final step.
    pub kind: u8,
    /// Scenario id (1..=7).
    pub scenario: u32,
    /// Fund balance after the action.
    pub balance: i128,
    /// Deferred penalty after the action.
    pub pending: Amount,
    /// Cumulative applied penalty after the action.
    pub total_applied: Amount,
    /// Cumulative booked penalty after the action.
    pub total_penalty: Amount,
    /// Window number after the action.
    pub window: u32,
    /// Penalty applied within the window after the action.
    pub window_applied: Amount,
}

/// Result of the pinned scenario run.
#[derive(Debug, Clone)]
pub struct T5Run {
    /// Digest steps of the run.
    pub steps: Vec<T5Step>,
    /// Incident ledger of the run.
    pub records: Vec<SlashRecord>,
    /// Final fund balance.
    pub final_balance: i128,
    /// Deferred penalty at the end (0: nothing dropped).
    pub pending: Amount,
    /// Cumulative booked penalty.
    pub total_penalty: Amount,
    /// Cumulative applied penalty.
    pub total_applied: Amount,
    /// Cumulative covered damage.
    pub total_covered: Amount,
    /// Cumulative uncovered damage.
    pub total_shortfall: Amount,
}

fn push_step(steps: &mut Vec<T5Step>, kind: u8, scenario: u32, fund: &InsuranceFund) {
    steps.push(T5Step {
        index: steps.len() as u32,
        kind,
        scenario,
        balance: fund.balance(),
        pending: fund.pending(),
        total_applied: fund.total_applied(),
        total_penalty: fund.total_penalty(),
        window: fund.window(),
        window_applied: fund.window_applied(),
    });
}

/// The pinned T5 scenario: seven gate scenarios over one insurance fund.
pub fn run_t5_scenario() -> T5Run {
    let mut fund = InsuranceFund::new();
    let mut steps: Vec<T5Step> = Vec::new();
    // s1: single double-sign.
    push_step(&mut steps, 0, 1, {
        fund.report(100 * ALCRY, 1);
        &fund
    });
    // s2: repeat offender.
    for _ in 0..10 {
        fund.report(50 * ALCRY, 2);
        push_step(&mut steps, 0, 2, &fund);
    }
    // s3: correlated mass slash, drained over three windows.
    for _ in 0..20 {
        fund.report(5_000 * ALCRY, 3);
        push_step(&mut steps, 0, 3, &fund);
    }
    for _ in 0..3 {
        fund.advance(3);
        push_step(&mut steps, 1, 3, &fund);
    }
    // s4: fund exhaustion pressure.
    for _ in 0..3 {
        fund.report(60_000 * ALCRY, 4);
        push_step(&mut steps, 0, 4, &fund);
    }
    for _ in 0..6 {
        fund.advance(4);
        push_step(&mut steps, 1, 4, &fund);
    }
    // s5: penalty margin floor (odd amounts).
    fund.report(ALCRY + 1, 5);
    push_step(&mut steps, 0, 5, &fund);
    fund.report(999_999_999_999_999_999, 5);
    push_step(&mut steps, 0, 5, &fund);
    // s6: mass-slash cap over multiple windows.
    for _ in 0..8 {
        fund.report(20_000 * ALCRY, 6);
        push_step(&mut steps, 0, 6, &fund);
    }
    for _ in 0..6 {
        fund.advance(6);
        push_step(&mut steps, 1, 6, &fund);
    }
    // s7: exhaustion and refill chain.
    fund.report(500_000 * ALCRY, 7);
    push_step(&mut steps, 0, 7, &fund);
    fund.report(250_000 * ALCRY, 7);
    push_step(&mut steps, 0, 7, &fund);
    for _ in 0..3 {
        fund.advance(7);
        push_step(&mut steps, 1, 7, &fund);
    }
    fund.report(1_000 * ALCRY, 7);
    push_step(&mut steps, 0, 7, &fund);
    fund.report(2_000 * ALCRY, 7);
    push_step(&mut steps, 0, 7, &fund);
    for _ in 0..20 {
        fund.advance(7);
        push_step(&mut steps, 1, 7, &fund);
    }
    push_step(&mut steps, 9, 7, &fund);
    T5Run {
        final_balance: fund.balance(),
        pending: fund.pending(),
        total_penalty: fund.total_penalty(),
        total_applied: fund.total_applied(),
        total_covered: fund.total_covered(),
        total_shortfall: fund.total_shortfall(),
        records: fund.records,
        steps,
    }
}

/// Digest of the scenario step log (sha256, hex).
pub fn scenario_digest(steps: &[T5Step]) -> String {
    let mut blob = String::new();
    for step in steps {
        blob.push_str(&format!(
            "{}:{}:{}:{}:{}:{}:{}\n",
            step.index,
            step.kind,
            step.scenario,
            step.balance,
            step.pending,
            step.total_applied,
            step.total_penalty
        ));
    }
    aura_state::sha256_bytes(blob.as_bytes()).to_hex()
}
