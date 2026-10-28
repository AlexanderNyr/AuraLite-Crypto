//! S0 gate: the naive baseline must MATCH an independent Python model and
//! must expose its three measured defects (no root, no persistence, order
//! dependence). These are documented defects that S1 has to remove.

use std::collections::BTreeMap;
use std::path::Path;

use aura_state::{
    Account, BaselineError, BaselineState, BaselineWrite, FINAL_VIEW_CONTEXT, KeySpace, StateKey,
};
use aura_types::{Address, Amount, Nonce};

const GOLDEN: &str = include_str!("fixtures/s0_golden.txt");

fn parse(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = trimmed
            .split_once('=')
            .unwrap_or_else(|| panic!("malformed golden line: {trimmed}"));
        assert!(
            map.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate golden key {key}"
        );
    }
    map
}

fn hx(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0, "even-length hex");
    text.as_bytes()
        .chunks(2)
        .map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1]))
        .collect()
}

fn nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => panic!("invalid hex nibble in fixture"),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn account_key(index: u8) -> StateKey {
    StateKey::account(&Address::from_bytes([index; 20]))
}

fn w(index: u8, nonce: u64, balance: u128) -> BaselineWrite {
    BaselineWrite::new(
        account_key(index),
        Nonce::new(nonce),
        Amount::from_units(balance),
    )
}

fn scenarios() -> Vec<Vec<BaselineWrite>> {
    vec![
        vec![w(1, 0, 100), w(1, 0, 200), w(1, 0, 300)],
        vec![w(1, 1, 10), w(2, 2, 20), w(1, 3, 30), w(2, 4, 40)],
        vec![w(1, 0, 11), w(2, 0, 22), w(3, 0, 33)],
        vec![
            w(1, 0, u128::MAX),
            w(1, 0, 0),
            w(1, 0, 1),
            w(1, 0, 1u128 << 64),
            w(1, 0, u128::MAX - 1),
        ],
    ]
}

fn state_from(writes: &[BaselineWrite]) -> BaselineState {
    let mut state = BaselineState::new();
    for write in writes {
        state.apply(write.clone());
    }
    state
}

#[test]
fn constants_are_pinned_by_the_independent_fixture() {
    let f = parse(GOLDEN);
    assert_eq!(to_hex(FINAL_VIEW_CONTEXT), f["final_view_context"]);
    assert_eq!(f["no_root"], "1");
    assert_eq!(f["no_persistence"], "1");
    assert_eq!(f["deterministic"], "0");
    assert_eq!(f["scenario_count"], "4");
    assert_eq!(KeySpace::Account.tag(), 0x01);
}

#[test]
fn golden_hex_vectors_are_well_formed() {
    let f = parse(GOLDEN);
    let mut checked = 0;
    for key in f.keys() {
        if !key.ends_with("_writes_a")
            && !key.ends_with("_writes_b")
            && !key.ends_with("_final_a")
            && !key.ends_with("_final_b")
        {
            continue;
        }
        let bytes = hx(&f[key]);
        assert!(!bytes.is_empty(), "{key} must decode to non-empty bytes");
        checked += 1;
    }
    assert_eq!(checked, 4 * 4, "four vectors per scenario");
}

#[test]
fn write_logs_and_final_views_match_independent_python() {
    let f = parse(GOLDEN);
    for (index, order_a) in scenarios().into_iter().enumerate() {
        let n = index + 1;
        let order_b: Vec<BaselineWrite> = order_a.iter().rev().cloned().collect();
        let state_a = state_from(&order_a);
        let state_b = state_from(&order_b);

        assert_eq!(
            to_hex(&state_a.to_bytes()),
            f[&format!("scenario_{n}_writes_a")]
        );
        assert_eq!(
            to_hex(&state_b.to_bytes()),
            f[&format!("scenario_{n}_writes_b")]
        );
        assert_eq!(
            to_hex(&state_a.final_view_bytes()),
            f[&format!("scenario_{n}_final_a")]
        );
        assert_eq!(
            to_hex(&state_b.final_view_bytes()),
            f[&format!("scenario_{n}_final_b")]
        );

        let defect = f[&format!("scenario_{n}_defect")] == "1";
        assert_eq!(
            state_a.final_view_bytes() != state_b.final_view_bytes(),
            defect,
            "scenario {n} defect flag"
        );
        assert_eq!(
            state_a.len().to_string(),
            f[&format!("scenario_{n}_writes_count")]
        );
        assert_eq!(
            state_a.final_view().len().to_string(),
            f[&format!("scenario_{n}_accounts_a")]
        );
        assert_eq!(
            state_b.final_view().len().to_string(),
            f[&format!("scenario_{n}_accounts_b")]
        );

        let key = account_key(1);
        assert_eq!(
            state_a.get(&key).unwrap().balance.units().to_string(),
            f[&format!("scenario_{n}_balance_key1_a")]
        );
        assert_eq!(
            state_b.get(&key).unwrap().balance.units().to_string(),
            f[&format!("scenario_{n}_balance_key1_b")]
        );
    }
}

#[test]
fn insertion_order_changes_balances_documented_defect() {
    // S0 DoD: two "nodes" with the same writes in a different order end with
    // different balances. Do NOT "fix" this here — S1 removes the defect.
    let writes = scenarios()[0].clone();
    let reversed: Vec<BaselineWrite> = writes.iter().rev().cloned().collect();
    let a = state_from(&writes);
    let b = state_from(&reversed);

    assert_eq!(a.get(&account_key(1)).unwrap().balance.units(), 300);
    assert_eq!(b.get(&account_key(1)).unwrap().balance.units(), 100);
    assert_ne!(a.final_view_bytes(), b.final_view_bytes());
    assert_ne!(a.get(&account_key(1)), b.get(&account_key(1)));
}

#[test]
fn disjoint_writes_are_order_independent() {
    // Guard against a false positive: with no conflicting key, order must NOT
    // change the final view. The defect is specifically last-write-wins.
    let writes = scenarios()[2].clone();
    let reversed: Vec<BaselineWrite> = writes.iter().rev().cloned().collect();
    let a = state_from(&writes);
    let b = state_from(&reversed);
    assert_eq!(a.final_view_bytes(), b.final_view_bytes());
    assert_eq!(a.final_view().len(), 3);
}

#[test]
fn there_is_no_state_root_and_no_persistence() {
    let state = state_from(&scenarios()[0]);
    assert_eq!(state.state_root(), Err(BaselineError::NoRoot));
    assert_eq!(
        state.persist(Path::new("/tmp/aura-s0-does-not-exist")),
        Err(BaselineError::NoPersistence)
    );
    assert_eq!(
        BaselineState::load(Path::new("/tmp/aura-s0-does-not-exist")),
        Err(BaselineError::NoPersistence)
    );
    // The file must genuinely not be created.
    assert!(!Path::new("/tmp/aura-s0-does-not-exist").exists());
}

#[test]
fn last_write_wins_lookup_and_account_defaults() {
    let state = state_from(&scenarios()[1]);
    let account = state.get(&account_key(1)).unwrap();
    assert_eq!(account.balance.units(), 30);
    assert_eq!(account.nonce.value(), 3);
    assert_eq!(account.code_hash, Account::NO_CODE);
    assert_eq!(account.storage_root, Account::EMPTY_STORAGE_ROOT);
    assert_eq!(state.get(&account_key(99)), None);
    assert!(BaselineState::new().is_empty());
}

#[test]
fn measured_report_is_honest() {
    let f = parse(GOLDEN);
    let state = state_from(&scenarios()[0]);
    let report = state.report();
    assert_eq!(report.writes.to_string(), f["report_writes"]);
    assert_eq!(report.accounts.to_string(), f["report_accounts"]);
    assert_eq!(report.has_root, f["report_has_root"] == "1");
    assert_eq!(report.has_persistence, f["report_has_persistence"] == "1");
    assert_eq!(report.deterministic, f["report_deterministic"] == "1");
    assert!(!report.deterministic, "S0 must not claim determinism");
    assert_eq!(state.report(), report, "report is a pure function");
}
