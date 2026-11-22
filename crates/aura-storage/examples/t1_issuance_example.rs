//! T1 ten-year issuance simulation runner (ECONOMICS_PLAN T1).
//!
//! Prints a machine-readable summary consumed by the independent Python
//! oracle (`tools/t1_issuance_check.py`).

use aura_storage::issuance::{ALCRY, IssuanceModel, T1_PARAMS, T1_YEARS};

fn main() {
    let mut model = IssuanceModel::new(T1_PARAMS).expect("params");
    let path = model.run_years(T1_YEARS).expect("run");
    let mut text = String::new();
    for (year, supply) in path.iter().enumerate() {
        text.push_str(&year.to_string());
        text.push(':');
        text.push_str(&supply.to_string());
        text.push('\n');
    }
    let digest = aura_state::sha256_bytes(text.as_bytes()).to_hex();
    let alcry_path: Vec<String> = path.iter().map(|s| (s / ALCRY).to_string()).collect();
    println!(
        "T1-SIM-OK {{\"years\":{},\"final_supply\":{},\"final_alcry\":{},\"digest\":\"{}\",\"path_alcry\":[{}]}}",
        T1_YEARS,
        model.supply(),
        model.supply() / ALCRY,
        digest,
        alcry_path.join(",")
    );
}
