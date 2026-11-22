//! T2 allocation reconciliation runner (ECONOMICS_PLAN T2).
//!
//! Prints a machine-readable summary consumed by the independent Python
//! oracle (`tools/t2_allocation_check.py`).

use aura_storage::allocation::{T2_ALLOCATION, liquid, locked, reconcile, vested};
use aura_storage::issuance::{ALCRY, GENESIS_SUPPLY};

fn main() {
    reconcile(&T2_ALLOCATION, GENESIS_SUPPLY).expect("reconcile");
    let mut text = String::new();
    let mut rows = Vec::new();
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
        rows.push(format!(
            "\"{}\":{{\"amount\":{},\"liquid12\":{},\"locked12\":{}}}",
            entry.name,
            entry.amount,
            liquid(entry, 12),
            locked(entry, 12)
        ));
    }
    let digest = aura_state::sha256_bytes(text.as_bytes()).to_hex();
    let team_vested_36 = vested(&T2_ALLOCATION[3].vesting.expect("team vests"), 36);
    println!(
        "T2-SIM-OK {{\"genesis\":{},\"digest\":\"{}\",\"team_vested_36\":{},\"entries\":{{{}}}}}",
        GENESIS_SUPPLY,
        digest,
        team_vested_36,
        rows.join(",")
    );
    let _ = ALCRY;
}
