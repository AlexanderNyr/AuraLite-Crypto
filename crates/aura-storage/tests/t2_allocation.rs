//! T2 allocation gate (ECONOMICS_PLAN T2).
//!
//! The gate is the literal phase gate: the reconciliation script must
//! match the genesis balances with the table — and "extra coins" anywhere
//! must turn it red. Every control below carries its named T2 message.

use aura_storage::allocation::{
    AllocError, AllocationEntry, T2_ALLOCATION, VestingSchedule, check_spend, liquid, locked,
    reconcile, reconcile_genesis, vested,
};
use aura_storage::issuance::{ALCRY, GENESIS_SUPPLY};

/// Golden table digest (sha256 over the canonical table lines), computed
/// by `tools/t2_allocation_check.py`.
const GOLDEN_TABLE_DIGEST: &str =
    "5fe7417a134011e7028f255cf7cf180335cc23c773ee9657a68de260fcc9ea41";

fn team() -> AllocationEntry {
    T2_ALLOCATION[3]
}

fn treasury() -> AllocationEntry {
    T2_ALLOCATION[1]
}

#[test]
fn t2_allocation_sums_to_the_genesis_supply() {
    assert_eq!(
        reconcile_genesis(),
        Ok(()),
        "T2: the allocation must sum to the genesis supply"
    );
    let total: u128 = T2_ALLOCATION.iter().map(|entry| entry.amount).sum();
    assert_eq!(
        total, GENESIS_SUPPLY,
        "T2: the allocation must sum to the genesis supply"
    );
    assert_eq!(
        total,
        3_000_000 * ALCRY,
        "T2: the allocation must sum to the genesis supply"
    );
    // Literally: one extra coin must be rejected.
    let mut extra = T2_ALLOCATION;
    extra[0].amount += 1;
    assert_eq!(
        reconcile(&extra, GENESIS_SUPPLY),
        Err(AllocError::SumMismatch {
            table: GENESIS_SUPPLY + 1,
            genesis: GENESIS_SUPPLY
        }),
        "T2: the allocation must sum to the genesis supply"
    );
}

#[test]
fn t2_vesting_must_not_exceed_its_allocation() {
    let bad = [AllocationEntry {
        name: "team",
        amount: 450_000 * ALCRY,
        vesting: Some(VestingSchedule {
            cliff_months: 12,
            total_months: 36,
            amount: 450_001 * ALCRY,
        }),
    }];
    assert_eq!(
        reconcile(&bad, 450_000 * ALCRY),
        Err(AllocError::VestingExceedsAllocation),
        "T2: vesting must not exceed its allocation"
    );
    let bad_schedule = [AllocationEntry {
        name: "team",
        amount: 450_000 * ALCRY,
        vesting: Some(VestingSchedule {
            cliff_months: 12,
            total_months: 12,
            amount: 450_000 * ALCRY,
        }),
    }];
    assert_eq!(
        reconcile(&bad_schedule, 450_000 * ALCRY),
        Err(AllocError::BadSchedule),
        "T2: vesting must not exceed its allocation"
    );
}

#[test]
fn t2_the_cliff_locks_everything_before_it() {
    let schedule = team().vesting.expect("team vests");
    for month in 0..schedule.cliff_months {
        assert_eq!(
            vested(&schedule, month),
            0,
            "T2: the cliff must lock everything before it"
        );
    }
    assert_eq!(
        locked(&team(), 6),
        450_000 * ALCRY,
        "T2: the cliff must lock everything before it"
    );
    assert_eq!(
        liquid(&team(), 6),
        0,
        "T2: the cliff must lock everything before it"
    );
}

#[test]
fn t2_vesting_completes_exactly_at_the_end() {
    let schedule = team().vesting.expect("team vests");
    assert_eq!(
        vested(&schedule, schedule.total_months),
        schedule.amount,
        "T2: vesting must complete exactly at the end"
    );
    for month in [37u32, 48, 120] {
        assert_eq!(
            vested(&schedule, month),
            schedule.amount,
            "T2: vesting must complete exactly at the end"
        );
    }
    assert_eq!(
        vested(&schedule, 12),
        150_000 * ALCRY,
        "T2: vesting must complete exactly at the end"
    );
    let treasury_schedule = treasury().vesting.expect("treasury vests");
    assert_eq!(
        vested(&treasury_schedule, 6),
        150_000 * ALCRY,
        "T2: vesting must complete exactly at the end"
    );
    assert_eq!(
        vested(&treasury_schedule, 24),
        600_000 * ALCRY,
        "T2: vesting must complete exactly at the end"
    );
}

#[test]
fn t2_locked_coins_are_not_spendable() {
    assert_eq!(
        check_spend(&team(), 6, 1),
        Err(AllocError::LockedSpend),
        "T2: locked coins must not be spendable"
    );
    assert_eq!(
        check_spend(&team(), 12, 150_001 * ALCRY),
        Err(AllocError::LockedSpend),
        "T2: locked coins must not be spendable"
    );
    assert_eq!(
        check_spend(&team(), 12, 150_000 * ALCRY),
        Ok(()),
        "T2: locked coins must not be spendable"
    );
    assert_eq!(
        check_spend(&team(), 36, 450_000 * ALCRY),
        Ok(()),
        "T2: locked coins must not be spendable"
    );
}

#[test]
fn t2_reference_table_agrees_with_the_simulation() {
    let mut text = String::new();
    for entry in &T2_ALLOCATION {
        text.push_str(entry.name);
        text.push(':');
        text.push_str(&entry.amount.to_string());
        if let Some(schedule) = &entry.vesting {
            text.push_str(":v=");
            text.push_str(&schedule.cliff_months.to_string());
            text.push(',');
            text.push_str(&schedule.total_months.to_string());
            text.push(',');
            text.push_str(&schedule.amount.to_string());
        }
        text.push('\n');
    }
    let digest = aura_state::sha256_bytes(text.as_bytes()).to_hex();
    assert_eq!(
        digest, GOLDEN_TABLE_DIGEST,
        "T2: the reconciliation must agree with the reference table"
    );
    assert_eq!(
        T2_ALLOCATION.len(),
        4,
        "T2: the reconciliation must agree with the reference table"
    );
    let names: Vec<&str> = T2_ALLOCATION.iter().map(|entry| entry.name).collect();
    assert_eq!(
        names,
        ["validators", "treasury", "community", "team"],
        "T2: the reconciliation must agree with the reference table"
    );
}

#[test]
fn t2_splitting_an_entry_preserves_the_total() {
    // Metamorphism: splitting a pot in two (with proportional vesting)
    // keeps the reconciliation green and the total identical.
    let mut split = Vec::new();
    for entry in &T2_ALLOCATION {
        let half = entry.amount / 2;
        let rest = entry.amount - half;
        let split_vesting = |amount: u128| {
            entry.vesting.as_ref().map(|schedule| VestingSchedule {
                cliff_months: schedule.cliff_months,
                total_months: schedule.total_months,
                // Proportional split of the schedule. All table amounts
                // are whole `ALCRY` units, so scaling in ALCRY units is
                // exact and keeps `amount * pot` inside u128.
                amount: schedule.amount * (amount / ALCRY) / (entry.amount / ALCRY),
            })
        };
        split.push(AllocationEntry {
            name: entry.name,
            amount: half,
            vesting: split_vesting(half),
        });
        split.push(AllocationEntry {
            name: entry.name,
            amount: rest,
            vesting: split_vesting(rest),
        });
    }
    let total: u128 = split.iter().map(|entry| entry.amount).sum();
    assert_eq!(
        total, GENESIS_SUPPLY,
        "T2: the allocation must sum to the genesis supply"
    );
    for month in [0u32, 12, 24, 36] {
        let original_locked: u128 = T2_ALLOCATION.iter().map(|entry| locked(entry, month)).sum();
        let split_locked: u128 = split.iter().map(|entry| locked(entry, month)).sum();
        assert!(
            original_locked.abs_diff(split_locked) <= ALCRY,
            "T2: the allocation must sum to the genesis supply"
        );
    }
}

#[test]
fn t2_monthly_deltas_sum_to_the_vested_amount() {
    let schedule = team().vesting.expect("team vests");
    let mut previous = 0;
    let mut total = 0;
    for month in 1..=schedule.total_months {
        let current = vested(&schedule, month);
        total += current - previous;
        previous = current;
    }
    assert_eq!(
        total, schedule.amount,
        "T2: vesting must complete exactly at the end"
    );
}
