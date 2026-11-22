//! T1 issuance model: genesis supply, halving inflation curve and a fixed
//! max supply for `ALCRY` (ECONOMICS_PLAN T1, decision D-T1).
//!
//! The native asset `ALCRY` is a u128 amount with 18 decimals. The whole
//! issuance policy is declared here and fixed in genesis:
//!
//! * **genesis supply** — minted at year 0 (the T0 endowment table is its
//!   distribution: 3 000 000 `ALCRY`);
//! * **inflation curve** — a fixed annual emission that **halves** every
//!   [`HALVING_YEARS`] years (bitcoin-style disinflation);
//! * **max supply** — a hard cap ([`MAX_SUPPLY`]); issuance stops at the
//!   cap ("if fixed" — it is fixed);
//! * **target staked ratio** — a declared target in basis points for T4
//!   staking; T1 only pins and validates it.
//!
//! Changing any of these parameters later requires governance with a
//! timelock (decisions D-T1/D-T3) — not a simple commit. The ten-year
//! simulation gate requires the simulated supply to match the closed-form
//! formula within 1 base unit.

use crate::econ::Amount;

/// Base units in one `ALCRY` (18 decimals, decision D-T1).
pub const ALCRY: Amount = 10u128.pow(18);

/// Genesis supply: 3 000 000 `ALCRY`, distributed by the T0 table.
pub const GENESIS_SUPPLY: Amount = 3_000_000 * ALCRY;

/// Annual emission of the first halving period (year 1): 300 000 `ALCRY`.
pub const ANNUAL_EMISSION: Amount = 300_000 * ALCRY;

/// Halving period in years.
pub const HALVING_YEARS: u32 = 4;

/// Hard cap on the total supply: 6 000 000 `ALCRY`.
pub const MAX_SUPPLY: Amount = 6_000_000 * ALCRY;

/// Target staked ratio in basis points (60 %; consumed by T4).
pub const TARGET_STAKED_RATIO_BPS: u16 = 6_000;

/// Horizon of the phase gate, in years.
pub const T1_YEARS: u32 = 10;

/// The declared issuance parameters (genesis-fixed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IssuanceParams {
    /// Supply at year 0.
    pub genesis_supply: Amount,
    /// Emission of year 1; halved every `halving_years`.
    pub annual_emission: Amount,
    /// Halving period in years (nonzero).
    pub halving_years: u32,
    /// Hard cap on the total supply (>= genesis supply).
    pub max_supply: Amount,
    /// Target staked ratio in basis points (<= 10 000).
    pub target_staked_ratio_bps: u16,
}

/// Errors raised while building or running the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssuanceError {
    /// `halving_years` was zero.
    ZeroHalvingPeriod,
    /// `max_supply` was below `genesis_supply`.
    CapBelowGenesis,
    /// `target_staked_ratio_bps` exceeded 10 000.
    StakedRatioOutOfRange,
    /// Arithmetic overflow in the supply path.
    Overflow,
}

/// The T1 baseline parameters (mirrored by `sim/t1-issuance.json`).
pub const T1_PARAMS: IssuanceParams = IssuanceParams {
    genesis_supply: GENESIS_SUPPLY,
    annual_emission: ANNUAL_EMISSION,
    halving_years: HALVING_YEARS,
    max_supply: MAX_SUPPLY,
    target_staked_ratio_bps: TARGET_STAKED_RATIO_BPS,
};

/// Stepwise issuance simulator: advances whole years and stops at the cap.
#[derive(Clone, Debug)]
pub struct IssuanceModel {
    params: IssuanceParams,
    year: u32,
    supply: Amount,
}

impl IssuanceModel {
    /// Builds a simulator at year 0 with the genesis supply.
    pub fn new(params: IssuanceParams) -> Result<Self, IssuanceError> {
        if params.halving_years == 0 {
            return Err(IssuanceError::ZeroHalvingPeriod);
        }
        if params.max_supply < params.genesis_supply {
            return Err(IssuanceError::CapBelowGenesis);
        }
        if params.target_staked_ratio_bps > 10_000 {
            return Err(IssuanceError::StakedRatioOutOfRange);
        }
        Ok(Self {
            params,
            year: 0,
            supply: params.genesis_supply,
        })
    }

    /// Current year.
    pub fn year(&self) -> u32 {
        self.year
    }

    /// Current total supply.
    pub fn supply(&self) -> Amount {
        self.supply
    }

    /// The emission scheduled for `year` (1-based) before the cap:
    /// `annual_emission >> ((year - 1) / halving_years)`.
    pub fn emission_for_year(&self, year: u32) -> Amount {
        let shift = (year - 1) / self.params.halving_years;
        self.params.annual_emission >> shift
    }

    /// Advances one year: issues the scheduled emission, capped at
    /// `max_supply`. Returns the amount actually issued.
    pub fn step_year(&mut self) -> Result<Amount, IssuanceError> {
        self.year += 1;
        let raw = self.emission_for_year(self.year);
        let remaining = self.params.max_supply.saturating_sub(self.supply);
        let issued = raw.min(remaining);
        self.supply = self
            .supply
            .checked_add(issued)
            .ok_or(IssuanceError::Overflow)?;
        Ok(issued)
    }

    /// Advances `years` whole years and returns the supply path
    /// (year 0 first, one entry per year).
    pub fn run_years(&mut self, years: u32) -> Result<Vec<Amount>, IssuanceError> {
        let mut path = vec![self.supply];
        for _ in 0..years {
            self.step_year()?;
            path.push(self.supply);
        }
        Ok(path)
    }
}
