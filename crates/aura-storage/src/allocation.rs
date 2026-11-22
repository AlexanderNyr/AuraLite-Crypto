//! T2 allocation: the genesis supply split across validators, treasury,
//! community and team, with vesting cliffs and lockups (ECONOMICS_PLAN T2).
//!
//! The table below reconciles **exactly** with the T1 genesis supply:
//! `sum(pots) == GENESIS_SUPPLY` — any extra coin anywhere turns the gate
//! red (the phase NC is literal: "extra coins → red"). Vesting is linear
//! over `total_months` with a cliff gate; the unvested part of a pot is
//! **locked** and cannot be spent.

use crate::econ::Amount;
use crate::issuance::{ALCRY, GENESIS_SUPPLY};

/// A vesting schedule: linear over `total_months`, gated by a cliff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VestingSchedule {
    /// Months before which nothing is vested.
    pub cliff_months: u32,
    /// Total vesting duration in months (must exceed the cliff).
    pub total_months: u32,
    /// Amount covered by the schedule (<= the pot).
    pub amount: Amount,
}

/// One allocation line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationEntry {
    /// Name of the pot: `validators`, `treasury`, `community` or `team`.
    pub name: &'static str,
    /// Total amount of the pot.
    pub amount: Amount,
    /// Optional vesting; the unvested part is locked.
    pub vesting: Option<VestingSchedule>,
}

/// Errors raised by [`reconcile`] or the spend checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AllocError {
    /// The table lists the same pot twice.
    DuplicateName,
    /// The table sums to a different value than the genesis supply —
    /// the "extra coins" signature.
    SumMismatch {
        /// Sum of the table.
        table: Amount,
        /// Declared genesis supply.
        genesis: Amount,
    },
    /// A vesting schedule covers more than its pot.
    VestingExceedsAllocation,
    /// A schedule has `total_months <= cliff_months`.
    BadSchedule,
    /// A spend tried to touch locked (unvested) coins.
    LockedSpend,
}

/// The T2 baseline allocation of [`GENESIS_SUPPLY`] (mirrored by
/// `sim/t2-allocation.json`).
pub const T2_ALLOCATION: [AllocationEntry; 4] = [
    AllocationEntry {
        name: "validators",
        amount: 1_200_000 * ALCRY,
        vesting: None,
    },
    AllocationEntry {
        name: "treasury",
        amount: 600_000 * ALCRY,
        vesting: Some(VestingSchedule {
            cliff_months: 6,
            total_months: 24,
            amount: 600_000 * ALCRY,
        }),
    },
    AllocationEntry {
        name: "community",
        amount: 750_000 * ALCRY,
        vesting: None,
    },
    AllocationEntry {
        name: "team",
        amount: 450_000 * ALCRY,
        vesting: Some(VestingSchedule {
            cliff_months: 12,
            total_months: 36,
            amount: 450_000 * ALCRY,
        }),
    },
];

/// Vested amount of a schedule at `month`: zero before the cliff, then
/// `amount * month / total_months` truncated, capped at `amount`.
pub fn vested(schedule: &VestingSchedule, month: u32) -> Amount {
    if month < schedule.cliff_months {
        return 0;
    }
    if month >= schedule.total_months {
        return schedule.amount;
    }
    schedule.amount * Amount::from(month) / Amount::from(schedule.total_months)
}

/// Locked (unvested) part of an entry at `month`.
pub fn locked(entry: &AllocationEntry, month: u32) -> Amount {
    match &entry.vesting {
        Some(schedule) => schedule.amount - vested(schedule, month),
        None => 0,
    }
}

/// Liquid (spendable) part of an entry at `month`.
pub fn liquid(entry: &AllocationEntry, month: u32) -> Amount {
    entry.amount - locked(entry, month)
}

/// Reconciles the canonical [`T2_ALLOCATION`] table against the T1
/// [`GENESIS_SUPPLY`] — the T2 gate in one call.
pub fn reconcile_genesis() -> Result<(), AllocError> {
    reconcile(&T2_ALLOCATION, GENESIS_SUPPLY)
}

/// Checks a spend against the liquid part of the pot at `month`.
pub fn check_spend(entry: &AllocationEntry, month: u32, amount: Amount) -> Result<(), AllocError> {
    if amount > liquid(entry, month) {
        return Err(AllocError::LockedSpend);
    }
    Ok(())
}

/// Reconciles an allocation table with the genesis supply: unique names,
/// exact sum (**no extra coins**), schedules within their pots and valid.
pub fn reconcile(entries: &[AllocationEntry], genesis_supply: Amount) -> Result<(), AllocError> {
    let mut total: Amount = 0;
    let mut seen: Vec<&str> = Vec::new();
    for entry in entries {
        if seen.contains(&entry.name) {
            return Err(AllocError::DuplicateName);
        }
        seen.push(entry.name);
        total += entry.amount;
        if let Some(schedule) = &entry.vesting {
            if schedule.total_months <= schedule.cliff_months {
                return Err(AllocError::BadSchedule);
            }
            if schedule.amount > entry.amount {
                return Err(AllocError::VestingExceedsAllocation);
            }
        }
    }
    if total != genesis_supply {
        return Err(AllocError::SumMismatch {
            table: total,
            genesis: genesis_supply,
        });
    }
    Ok(())
}
