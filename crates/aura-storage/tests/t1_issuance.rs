//! T1 issuance model gate (ECONOMICS_PLAN T1).
//!
//! The gate is the literal phase gate: a 10-year simulation whose supply
//! must match the closed-form formula within 1 base unit. Every control
//! below carries its named T1 message; the golden path digest is pinned
//! from the independent Python oracle.

use aura_storage::issuance::{
    ALCRY, ANNUAL_EMISSION, GENESIS_SUPPLY, HALVING_YEARS, IssuanceError, IssuanceModel,
    IssuanceParams, MAX_SUPPLY, T1_PARAMS, T1_YEARS, TARGET_STAKED_RATIO_BPS,
};

/// Golden supply-path digest (sha256 over `year:supply` lines), computed
/// by `tools/t1_issuance_check.py`.
const GOLDEN_PATH_DIGEST: &str = "c2a4669192ddde5d9cc4e7cb32a76ecd06356022bce7c7230826f068854c1a35";

/// Independent closed-form recomputation (the "formula" side of the gate).
fn closed_form_supply(years: u32) -> u128 {
    let mut supply = GENESIS_SUPPLY;
    for year in 1..=years {
        let raw = ANNUAL_EMISSION >> ((year - 1) / HALVING_YEARS);
        let remaining = MAX_SUPPLY.saturating_sub(supply);
        supply += raw.min(remaining);
    }
    supply
}

fn path_digest(path: &[u128]) -> String {
    let mut text = String::new();
    for (year, supply) in path.iter().enumerate() {
        text.push_str(&year.to_string());
        text.push(':');
        text.push_str(&supply.to_string());
        text.push('\n');
    }
    aura_state::sha256_bytes(text.as_bytes()).to_hex()
}

#[test]
fn t1_ten_year_supply_matches_the_formula_within_one_unit() {
    let mut model = IssuanceModel::new(T1_PARAMS).expect("params");
    let path = model.run_years(T1_YEARS).expect("run");
    assert_eq!(
        path.len() as u32,
        T1_YEARS + 1,
        "T1: the supply must match the issuance formula"
    );
    for year in 0..=T1_YEARS {
        let formula = closed_form_supply(year);
        let simulated = path[year as usize];
        let diff = simulated.abs_diff(formula);
        assert!(
            diff <= 1,
            "T1: the supply must match the issuance formula (year {}: sim {} vs formula {})",
            year,
            simulated,
            formula
        );
    }
    assert_eq!(
        model.supply(),
        4_950_000 * ALCRY,
        "T1: the supply must match the issuance formula"
    );
}

#[test]
fn t1_halving_cuts_the_emission_in_half_every_period() {
    let model = IssuanceModel::new(T1_PARAMS).expect("params");
    assert_eq!(
        model.emission_for_year(1),
        ANNUAL_EMISSION,
        "T1: the halving must cut the emission in half every period"
    );
    assert_eq!(
        model.emission_for_year(HALVING_YEARS),
        ANNUAL_EMISSION,
        "T1: the halving must cut the emission in half every period"
    );
    assert_eq!(
        model.emission_for_year(HALVING_YEARS + 1),
        ANNUAL_EMISSION / 2,
        "T1: the halving must cut the emission in half every period"
    );
    assert_eq!(
        model.emission_for_year(2 * HALVING_YEARS + 1),
        ANNUAL_EMISSION / 4,
        "T1: the halving must cut the emission in half every period"
    );
}

#[test]
fn t1_max_supply_is_never_exceeded() {
    let params = IssuanceParams {
        max_supply: 3_500_000 * ALCRY,
        ..T1_PARAMS
    };
    let mut model = IssuanceModel::new(params).expect("params");
    let path = model.run_years(100).expect("run");
    for supply in &path {
        assert!(
            *supply <= params.max_supply,
            "T1: the max supply must never be exceeded"
        );
    }
    assert_eq!(
        model.supply(),
        params.max_supply,
        "T1: the max supply must never be exceeded"
    );
    assert_eq!(
        IssuanceModel::new(IssuanceParams {
            max_supply: GENESIS_SUPPLY - 1,
            ..T1_PARAMS
        })
        .err(),
        Some(IssuanceError::CapBelowGenesis),
        "T1: the max supply must never be exceeded"
    );
}

#[test]
fn t1_genesis_supply_matches_the_declared_endowment() {
    let model = IssuanceModel::new(T1_PARAMS).expect("params");
    assert_eq!(
        model.supply(),
        GENESIS_SUPPLY,
        "T1: the genesis supply must match the declared endowment"
    );
    assert_eq!(
        GENESIS_SUPPLY,
        3_000_000 * ALCRY,
        "T1: the genesis supply must match the declared endowment"
    );
    assert_eq!(
        model.year(),
        0,
        "T1: the genesis supply must match the declared endowment"
    );
}

#[test]
// Intentional constant property: NC-5 removes the validation in the model;
// this test pins the declared constant itself.
#[allow(clippy::assertions_on_constants)]
fn t1_target_staked_ratio_stays_within_basis_points() {
    assert!(
        TARGET_STAKED_RATIO_BPS <= 10_000,
        "T1: the target staked ratio must stay within basis points"
    );
    assert_eq!(
        IssuanceModel::new(IssuanceParams {
            target_staked_ratio_bps: 10_001,
            ..T1_PARAMS
        })
        .err(),
        Some(IssuanceError::StakedRatioOutOfRange),
        "T1: the target staked ratio must stay within basis points"
    );
    assert_eq!(
        IssuanceModel::new(T1_PARAMS).expect("params").supply() % ALCRY,
        0,
        "T1: the target staked ratio must stay within basis points"
    );
}

#[test]
fn t1_reference_model_agrees_with_simulation() {
    let mut model = IssuanceModel::new(T1_PARAMS).expect("params");
    let path = model.run_years(T1_YEARS).expect("run");
    assert_eq!(
        path_digest(&path),
        GOLDEN_PATH_DIGEST,
        "T1: the reference model must agree with the simulation"
    );
    let expected: Vec<u128> = vec![
        3_000_000 * ALCRY,
        3_300_000 * ALCRY,
        3_600_000 * ALCRY,
        3_900_000 * ALCRY,
        4_200_000 * ALCRY,
        4_350_000 * ALCRY,
        4_500_000 * ALCRY,
        4_650_000 * ALCRY,
        4_800_000 * ALCRY,
        4_875_000 * ALCRY,
        4_950_000 * ALCRY,
    ];
    assert_eq!(
        path, expected,
        "T1: the reference model must agree with the simulation"
    );
}

#[test]
fn t1_stepping_year_by_year_matches_the_closed_form() {
    // Metamorphism: the stepwise simulator and the closed-form loop must
    // agree everywhere; splitting the horizon must not change the result.
    let mut one = IssuanceModel::new(T1_PARAMS).expect("params");
    let full = one.run_years(T1_YEARS).expect("run");
    let mut a = IssuanceModel::new(T1_PARAMS).expect("params");
    let first = a.run_years(4).expect("run");
    let second = a.run_years(6).expect("run");
    let mut spliced = first.clone();
    spliced.extend_from_slice(&second[1..]);
    assert_eq!(
        full, spliced,
        "T1: stepping year by year must match the closed form"
    );
    assert_eq!(
        *full.last().expect("nonempty"),
        closed_form_supply(T1_YEARS),
        "T1: stepping year by year must match the closed form"
    );
}
