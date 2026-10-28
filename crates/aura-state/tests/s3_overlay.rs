//! S3 gate: execution overlay against an independent Python model.
//!
//! `fixtures/s3_golden.txt` comes from `tools/gen_s3_vectors.py` — pure-Python
//! BLAKE3, the C7 state-key digest and the S1 sparse Merkle tree.

use std::collections::BTreeMap;

use aura_state::{
    Account, DIFF_MAGIC, MAX_EVENTS, MAX_VALUE_LEN, MAX_WRITES, MerkleTree, OverlayError,
    S3_DIFF_CONTEXT, S3_EVENT_CONTEXT, StateDiff, StateKey, StateView, apply_diff, execute,
    state_path, versioned_root,
};
use aura_types::Hash;

const GOLDEN: &str = include_str!("fixtures/s3_golden.txt");

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
        assert!(map.insert(key.to_owned(), value.to_owned()).is_none());
        assert!(!key.is_empty());
    }
    map
}

fn hx(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks(2)
        .map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1]))
        .collect()
}

fn nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => panic!("invalid hex nibble"),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hash32(text: &str) -> Hash {
    Hash::from_slice(&hx(text)).expect("32-byte hash")
}

fn f_u64(f: &BTreeMap<String, String>, key: &str) -> u64 {
    f[key].parse().expect("u64")
}

fn f_usize(f: &BTreeMap<String, String>, key: &str) -> usize {
    f[key].parse().expect("usize")
}

fn accounts(f: &BTreeMap<String, String>) -> Vec<(StateKey, Hash, Vec<u8>)> {
    (0..3)
        .map(|index| {
            let raw = hx(&f[&format!("account_{index}_key")]);
            let key = StateKey::from_raw(&raw).expect("valid C7 account key");
            (
                key,
                hash32(&f[&format!("account_{index}_path")]),
                hx(&f[&format!("account_{index}_value")]),
            )
        })
        .collect()
}

fn base_tree(f: &BTreeMap<String, String>) -> MerkleTree {
    let entries: Vec<(Hash, Vec<u8>)> = accounts(f)
        .iter()
        .map(|(_, path, value)| (*path, value.clone()))
        .collect();
    MerkleTree::from_entries(f_u64(f, "base_version"), &entries)
}

fn decode(f: &BTreeMap<String, String>, key: &str) -> StateDiff {
    StateDiff::from_canonical_bytes(&hx(&f[key])).unwrap_or_else(|e| panic!("decode {key}: {e}"))
}

#[test]
fn constants_are_pinned_by_the_independent_fixture() {
    let f = parse(GOLDEN);
    assert_eq!(to_hex(DIFF_MAGIC), f["diff_magic"]);
    assert_eq!(to_hex(S3_DIFF_CONTEXT), f["s3_diff_context"]);
    assert_eq!(to_hex(S3_EVENT_CONTEXT), f["s3_event_context"]);
    assert_eq!(MAX_WRITES, f_usize(&f, "max_writes"));
    assert_eq!(MAX_EVENTS, f_usize(&f, "max_events"));
    assert_eq!(MAX_VALUE_LEN, f_usize(&f, "max_value_len"));
    assert_eq!(88, f_usize(&f, "account_len"));
}

#[test]
fn base_state_and_roots_match_independent_python() {
    let f = parse(GOLDEN);
    let version = f_u64(&f, "base_version");
    let tree = base_tree(&f);
    assert_eq!(tree.tree_root().to_hex(), f["base_tree_root"]);
    assert_eq!(
        versioned_root(version, &tree.tree_root()).to_hex(),
        f["base_root"]
    );
    assert_eq!(tree.len(), 3);

    for (key, path, value) in accounts(&f) {
        assert_eq!(
            state_path(&key),
            path,
            "state_path must match the Python digest"
        );
        assert_eq!(tree.get(&key), Some(value.as_slice()));
        let account = Account::from_canonical_bytes(&value).expect("account decodes");
        assert_eq!(account.code_hash, Account::NO_CODE);
    }
}

#[test]
fn canonical_diffs_match_independent_python() {
    let f = parse(GOLDEN);
    let version = f_u64(&f, "base_version") + 1;
    let base = base_tree(&f);

    for number in 1..=3 {
        let key = format!("diff_{number}_bytes");
        let expected_bytes = hx(&f[&key]);
        let diff = decode(&f, &key);
        assert_eq!(diff.to_bytes(), expected_bytes, "{key} re-encode");
        assert_eq!(diff.version, version);
        assert_eq!(diff.base_root.to_hex(), f["base_root"]);
        assert_eq!(diff.result_root.to_hex(), f[&format!("diff_{number}_root")]);
        assert_eq!(diff.gas_used.to_string(), f[&format!("diff_{number}_gas")]);
        assert_eq!(
            diff.events_root().to_hex(),
            f[&format!("diff_{number}_events_root")]
        );
        assert_eq!(
            diff.changes.len().to_string(),
            f[&format!("diff_{number}_changes")]
        );

        // Reconstruct the same diff through a real StateView.
        let mut view = StateView::new(base.clone(), 1_000);
        for (path, value) in &diff.changes {
            match value {
                Some(bytes) => view.set_path(*path, bytes.clone()),
                None => view.delete_path(*path),
            }
        }
        for event in &diff.events {
            view.emit(event.clone());
        }
        view.charge_gas(diff.gas_used).expect("gas fits");
        let rebuilt = view.diff(version);
        assert_eq!(rebuilt, diff, "diff_{number} rebuilt from StateView");
        assert_eq!(rebuilt.to_bytes(), expected_bytes);

        let applied =
            apply_diff(&base, &diff).unwrap_or_else(|e| panic!("diff_{number} apply: {e}"));
        assert_eq!(
            versioned_root(version, &applied.tree_root()),
            diff.result_root
        );
    }
}

#[test]
fn revert_and_out_of_gas_do_not_change_root() {
    let f = parse(GOLDEN);
    let version = f_u64(&f, "base_version") + 1;
    let base = base_tree(&f);
    let (key, _, original) = accounts(&f)[0].clone();
    let base_root = base.state_root();
    assert_eq!(base_root.to_hex(), f["revert_root"]);

    // Explicit revert after writes, gas and events.
    let mut view = StateView::new(base.clone(), 100);
    let outcome = execute(&mut view, version, |view| -> Result<(), OverlayError> {
        view.set(&key, b"attacker-balance".to_vec());
        view.charge_gas(7)?;
        view.emit(b"event".to_vec());
        Err(OverlayError::Revert("nope".to_owned()))
    });
    assert_eq!(
        outcome,
        Err(OverlayError::Revert("nope".to_owned())),
        "revert error"
    );
    assert_eq!(
        view.base_root().to_hex(),
        f["revert_root"],
        "revert must not change root"
    );
    assert_eq!(
        view.get(&key),
        Some(original.as_slice()),
        "revert must not change balance"
    );
    assert_eq!(
        view.pending_len(),
        0,
        "NC target: rollback must clear changes"
    );
    assert_eq!(view.gas_used(), 0);
    assert!(view.events().is_empty());
    assert_eq!(
        view.root(version),
        versioned_root(version, &base.tree_root())
    );

    // OutOfGas path.
    let mut view = StateView::new(base.clone(), 10);
    let outcome = execute(&mut view, version, |view| -> Result<(), OverlayError> {
        view.set(&key, b"attacker-balance".to_vec());
        view.charge_gas(11)
    });
    assert_eq!(
        outcome,
        Err(OverlayError::OutOfGas {
            limit: 10,
            used: 0,
            needed: 11
        }),
    );
    assert_eq!(view.base_root().to_hex(), f["out_of_gas_root"]);
    assert_eq!(
        view.get(&key),
        Some(original.as_slice()),
        "OutOfGas must not change balance"
    );
    assert_eq!(
        view.pending_len(),
        0,
        "NC target: rollback must clear changes"
    );
    assert_eq!(view.gas_used(), 0);
    assert!(view.events().is_empty());
}

#[test]
fn successful_execution_commits_only_the_diff() {
    let f = parse(GOLDEN);
    let version = f_u64(&f, "base_version") + 1;
    let base = base_tree(&f);
    let (key, _, original) = accounts(&f)[0].clone();

    let mut view = StateView::new(base.clone(), 100);
    let (value, diff) = execute(&mut view, version, |view| -> Result<u64, OverlayError> {
        view.set(&key, original.clone());
        view.charge_gas(1)?;
        Ok(7)
    })
    .expect("execution succeeds");
    assert_eq!(value, 7);
    assert_eq!(diff.gas_used, 1);
    assert_eq!(diff.base_root.to_hex(), f["base_root"]);
    assert_eq!(diff.result_root, view.root(version));
    assert_eq!(diff.to_bytes(), view.commit(version).to_bytes());
}

#[test]
fn every_bad_vector_is_rejected() {
    let f = parse(GOLDEN);
    let expected: Vec<(&str, OverlayError)> = vec![
        ("bad_checksum_diff", OverlayError::BadChecksum),
        ("bad_trailing_diff", OverlayError::TrailingBytes(1)),
        ("bad_truncated_diff", OverlayError::UnexpectedEof),
        ("bad_magic_diff", OverlayError::BadMagic),
        ("bad_option_tag_diff", OverlayError::InvalidOptionTag(2)),
        ("bad_unsorted_diff", OverlayError::UnsortedChanges),
        ("bad_duplicate_diff", OverlayError::UnsortedChanges),
        (
            "bad_count_diff",
            OverlayError::TooManyWrites(MAX_WRITES + 1),
        ),
        ("bad_nonminimal_diff", OverlayError::NonCanonicalVarint),
    ];
    for (key, error) in &expected {
        let bytes = hx(&f[*key]);
        assert_eq!(
            StateDiff::from_canonical_bytes(&bytes),
            Err(error.clone()),
            "{key}"
        );
    }

    // bad_root_diff is structurally valid; only apply() catches it.
    let tampered = decode(&f, "bad_root_diff");
    assert_eq!(
        tampered.apply(&base_tree(&f)).unwrap_err(),
        OverlayError::RootMismatch
    );
}
