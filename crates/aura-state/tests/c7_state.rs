//! C7 gate: account model and state key layout against independent Python.
//!
//! `fixtures/c7_golden.txt` comes from `tools/gen_c7_vectors.py`, which
//! self-checks pure-Python BLAKE3 against official vectors before emitting
//! key bytes, digests, account encodings and rejection vectors.

use std::collections::{BTreeMap, HashSet};

use aura_codec::CodecError;
use aura_crypto::blake3_hash;
use aura_state::{
    Account, KeySpace, MAX_STATE_KEY_LEN, STATE_KEY_CONTEXT, StateError, StateKey,
    assert_contract_may_write,
};
use aura_types::{Address, Amount, Hash, Nonce};

const GOLDEN: &str = include_str!("fixtures/c7_golden.txt");

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

fn bytes20(text: &str) -> [u8; 20] {
    hx(text).try_into().expect("20 bytes")
}

/// Same trivial deterministic patterns as `tools/gen_c7_vectors.py`.
fn addr(index: usize) -> Address {
    let mut bytes = [0u8; 20];
    bytes[..8].copy_from_slice(&(index as u64).to_le_bytes());
    for (j, byte) in bytes[8..].iter_mut().enumerate() {
        *byte = ((index as u64) * 11 + (j as u64) * 29 + 0x3C) as u8;
    }
    Address::from_bytes(bytes)
}

fn slot(index: usize) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&(index as u64).to_le_bytes());
    for (j, byte) in bytes[8..].iter_mut().enumerate() {
        *byte = ((index as u64) * 3 + (j as u64) * 17 + 0x77) as u8;
    }
    bytes
}

fn txid(index: usize) -> Hash {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&(index as u64).to_le_bytes());
    for (j, byte) in bytes[8..].iter_mut().enumerate() {
        *byte = ((index as u64) * 7 + (j as u64) * 13 + 0xAB) as u8;
    }
    Hash::from_bytes(bytes)
}

fn keys_for(index: usize) -> Vec<(&'static str, StateKey)> {
    vec![
        ("account", StateKey::account(&addr(index))),
        ("code", StateKey::code(&addr(index))),
        ("storage", StateKey::storage(&addr(index), &slot(index))),
        ("staking", StateKey::staking(&addr(index))),
        ("receipt", StateKey::receipt(&txid(index))),
    ]
}

#[test]
fn constants_are_pinned_by_the_independent_fixture() {
    let f = parse(GOLDEN);
    assert_eq!(to_hex(STATE_KEY_CONTEXT), f["state_key_context"]);
    assert_eq!(MAX_STATE_KEY_LEN.to_string(), f["max_state_key_len"]);
    assert_eq!(KeySpace::ALL.map(KeySpace::tag), [1, 2, 3, 4, 5]);
    assert_eq!(
        KeySpace::Account.expected_len().to_string(),
        f["len_account"]
    );
    assert_eq!(
        KeySpace::ContractCode.expected_len().to_string(),
        f["len_code"]
    );
    assert_eq!(
        KeySpace::ContractStorage.expected_len().to_string(),
        f["len_storage"]
    );
    assert_eq!(
        KeySpace::Staking.expected_len().to_string(),
        f["len_staking"]
    );
    assert_eq!(
        KeySpace::Receipt.expected_len().to_string(),
        f["len_receipt"]
    );
}

#[test]
fn state_keys_and_digests_match_independent_python() {
    let f = parse(GOLDEN);
    for index in 0..4 {
        assert_eq!(to_hex(addr(index).as_bytes()), f[&format!("addr_{index}")]);
        assert_eq!(to_hex(&slot(index)), f[&format!("slot_{index}")]);
        assert_eq!(txid(index).to_hex(), f[&format!("txid_{index}")]);
        for (name, key) in keys_for(index) {
            assert_eq!(
                to_hex(key.as_bytes()),
                f[&format!("key_{name}_{index}")],
                "key_{name}_{index}"
            );
            assert_eq!(
                to_hex(&key.to_bytes()),
                f[&format!("encoded_{name}_{index}")],
                "encoded_{name}_{index}"
            );
            assert_eq!(
                key.digest().to_hex(),
                f[&format!("digest_{name}_{index}")],
                "digest_{name}_{index}"
            );
            let decoded = StateKey::from_bytes(&hx(&f[&format!("encoded_{name}_{index}")]))
                .unwrap_or_else(|e| panic!("decode {name}_{index}: {e}"));
            assert_eq!(decoded, key);
        }
    }
}

#[test]
fn two_account_keys_never_collide() {
    // C7 DoD gate: keys of two accounts do not collide.
    let mut raw_keys: HashSet<Vec<u8>> = HashSet::new();
    let mut digests: HashSet<Hash> = HashSet::new();
    for index in 0..1000 {
        let address = addr(index);
        let key = StateKey::account(&address);
        assert_eq!(key.as_bytes().len(), 21);
        assert!(
            raw_keys.insert(key.as_bytes().to_vec()),
            "raw key collision at {index}"
        );
        assert!(digests.insert(key.digest()), "digest collision at {index}");
        // No cross-namespace aliasing for the same address either.
        for (name, other) in keys_for(index) {
            if name == "account" {
                continue;
            }
            assert!(
                raw_keys.insert(other.as_bytes().to_vec()),
                "{name} aliased account"
            );
            assert!(
                digests.insert(other.digest()),
                "{name} digest aliased account"
            );
        }
    }
    assert_eq!(raw_keys.len(), 5000);
    assert_eq!(digests.len(), 5000);
}

#[test]
fn reserved_space_is_unreachable_from_contracts() {
    // C7 DoD gate: reserved space unreachable from contracts.
    let f = parse(GOLDEN);
    for prefix in 0u8..=255 {
        let body_len = KeySpace::from_tag(prefix)
            .map(|space| space.expected_len() - 1)
            .unwrap_or(20);
        let mut raw = vec![prefix];
        raw.extend(std::iter::repeat_n(0u8, body_len));
        match KeySpace::from_tag(prefix) {
            Some(space) => {
                let key =
                    StateKey::from_raw(&raw).unwrap_or_else(|e| panic!("0x{prefix:02x}: {e}"));
                assert_eq!(key.space(), space);
            }
            None => {
                assert_eq!(
                    StateKey::from_raw(&raw),
                    Err(StateError::ReservedKeySpace(prefix)),
                    "prefix 0x{prefix:02x} must be reserved"
                );
            }
        }
    }
    assert_eq!(KeySpace::ALL.len(), 5, "only five namespaces are reachable");

    for key in [
        "bad_reserved_00",
        "bad_reserved_06",
        "bad_reserved_7f",
        "bad_reserved_80",
        "bad_reserved_ff",
    ] {
        assert!(
            StateKey::from_bytes(&hx(&f[key])).is_err(),
            "{key} must never decode into a StateKey"
        );
    }
}

#[test]
fn contract_storage_is_isolated() {
    let f = parse(GOLDEN);
    let contract_a = Address::from_bytes(bytes20(&f["contract_a"]));
    let contract_b = Address::from_bytes(bytes20(&f["contract_b"]));
    let key_a = StateKey::from_raw(&hx(&f["storage_key_a"])).unwrap();
    let key_b = StateKey::from_raw(&hx(&f["storage_key_b"])).unwrap();

    assert!(assert_contract_may_write(&contract_a, &key_a).is_ok());
    assert!(assert_contract_may_write(&contract_b, &key_b).is_ok());
    assert_eq!(
        assert_contract_may_write(&contract_a, &key_b),
        Err(StateError::WrongContractStorage)
    );
    assert_eq!(
        assert_contract_may_write(&contract_b, &key_a),
        Err(StateError::WrongContractStorage)
    );

    for (name, key) in keys_for(0) {
        if name == "storage" {
            continue;
        }
        assert_eq!(
            assert_contract_may_write(&contract_a, &key),
            Err(StateError::ForbiddenContractWrite),
            "{name} namespace must be unreachable"
        );
    }
}

#[test]
fn key_lengths_are_fixed_per_namespace() {
    for (name, key) in keys_for(0) {
        let space = key.space();
        let mut short = key.as_bytes().to_vec();
        short.pop();
        assert_eq!(
            StateKey::from_raw(&short),
            Err(StateError::InvalidKeyLength {
                space,
                expected: space.expected_len(),
                got: space.expected_len() - 1,
            }),
            "{name} short key"
        );
        let mut long = key.as_bytes().to_vec();
        long.push(0);
        assert_eq!(
            StateKey::from_raw(&long),
            Err(StateError::InvalidKeyLength {
                space,
                expected: space.expected_len(),
                got: space.expected_len() + 1,
            }),
            "{name} long key"
        );
    }
    assert_eq!(StateKey::from_raw(&[]), Err(StateError::EmptyKey));
    assert_eq!(
        StateKey::from_raw(&[0u8; MAX_STATE_KEY_LEN + 1]),
        Err(StateError::KeyTooLong(MAX_STATE_KEY_LEN + 1))
    );
}

#[test]
fn bad_key_vectors_are_rejected() {
    let f = parse(GOLDEN);
    let bad: Vec<&str> = f
        .keys()
        .filter(|k| k.starts_with("bad_"))
        .map(String::as_str)
        .collect();
    assert_eq!(bad.len(), 12);
    for key in bad {
        assert!(
            StateKey::from_bytes(&hx(&f[key])).is_err(),
            "{key} must be rejected"
        );
    }
    assert_eq!(
        StateKey::from_bytes(&hx(&f["bad_nonminimal_len"])),
        Err(StateError::Decode(CodecError::NonCanonicalVarint))
    );
    assert_eq!(
        StateKey::from_bytes(&hx(&f["bad_trailing"])),
        Err(StateError::Decode(CodecError::TrailingBytes(1)))
    );
    assert!(StateKey::from_bytes(&hx(&f["bad_short_account"])).is_err());
}

#[test]
fn key_digest_is_domain_separated() {
    let key = StateKey::account(&addr(0));
    assert_ne!(key.digest(), blake3_hash(key.as_bytes()));
    assert_ne!(key.digest(), StateKey::code(&addr(0)).digest());
    let short = blake3_hash(&[STATE_KEY_CONTEXT, key.as_bytes()].concat());
    assert_ne!(key.digest(), short, "explicit length prefix matters");
}

#[test]
fn account_canonical_encoding_matches_independent_python() {
    let f = parse(GOLDEN);
    let accounts = [
        Account::new_empty(),
        Account::new(
            Nonce::new(7),
            Amount::from_units(5 * 10u128.pow(18)),
            Hash::from_bytes([0xAA; 32]),
            Hash::from_bytes([0xBB; 32]),
        ),
        Account::new(
            Nonce::new(u64::MAX),
            Amount::MAX,
            Hash::from_bytes([0xFF; 32]),
            Hash::from_bytes([0xEE; 32]),
        ),
    ];
    for (index, account) in accounts.iter().enumerate() {
        let key = format!("account_{}_bytes", index + 1);
        assert_eq!(to_hex(&account.to_bytes()), f[&key], "{key}");
        assert_eq!(account.to_bytes().len(), 88);
        let decoded = Account::from_canonical_bytes(&hx(&f[&key]))
            .unwrap_or_else(|e| panic!("decode {key}: {e}"));
        assert_eq!(decoded, *account);
    }
}

#[test]
fn account_arithmetic_is_checked() {
    let mut account = Account::new_empty();
    assert!(!account.has_code());
    assert_eq!(account.nonce.value(), 0);

    account.credit(Amount::from_units(100)).unwrap();
    account.debit(Amount::from_units(40)).unwrap();
    assert_eq!(account.balance.units(), 60);
    account.bump_nonce().unwrap();
    assert_eq!(account.nonce.value(), 1);

    assert_eq!(
        account.debit(Amount::from_units(61)),
        Err(StateError::InsufficientBalance)
    );
    assert_eq!(
        account.balance.units(),
        60,
        "failed debit must not change state"
    );

    let mut full = Account::new(
        Nonce::new(0),
        Amount::MAX,
        Account::NO_CODE,
        Account::EMPTY_STORAGE_ROOT,
    );
    assert_eq!(full.credit(Amount::ONE), Err(StateError::BalanceOverflow));

    let mut last = Account::new(
        Nonce::new(u64::MAX),
        Amount::ZERO,
        Account::NO_CODE,
        Account::EMPTY_STORAGE_ROOT,
    );
    assert_eq!(last.bump_nonce(), Err(StateError::NonceOverflow));

    let with_code = Account::new(
        Nonce::new(0),
        Amount::ZERO,
        Hash::from_bytes([1; 32]),
        Account::EMPTY_STORAGE_ROOT,
    );
    assert!(with_code.has_code());
}

#[test]
fn account_encoding_is_canonical() {
    let f = parse(GOLDEN);
    let bytes = hx(&f["account_2_bytes"]);
    assert_eq!(
        Account::from_canonical_bytes(&bytes).unwrap().to_bytes(),
        bytes
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(Account::from_canonical_bytes(&trailing).is_err());
    assert!(Account::from_canonical_bytes(&bytes[..bytes.len() - 1]).is_err());
}
