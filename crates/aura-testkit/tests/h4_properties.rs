//! H4 properties: balance conservation, canonical round-trip, signature
//! properties and credit/debit algebra (docs/h4-property-plan.md §2).
//!
//! The deterministic part guarantees that every mutation trigger used by
//! `tools/nc_h4.py` (amounts `0..=64`, notably 7) fires; proptest adds
//! randomised surface. Every failure carries its named H4 message.

use aura_codec::{decode_canonical, encode_to_bytes};
use aura_crypto::ed25519_public_key;
use aura_state::Account;
use aura_testkit::Lcg;
use aura_tx::{Tx, TxKind, TxParams, UnsignedTx};
use aura_types::{Amount, ChainId, Gas, Hash, Height, Nonce};
use proptest::prelude::*;

const START: u128 = 1_000_000;
const ACCOUNTS: usize = 8;

fn fresh_accounts() -> Vec<Account> {
    (0..ACCOUNTS)
        .map(|_| {
            Account::new(
                Nonce::new(0),
                Amount::from_units(START),
                Hash::ZERO,
                Hash::ZERO,
            )
        })
        .collect()
}

fn total(accounts: &[Account]) -> u128 {
    accounts.iter().map(|account| account.balance.units()).sum()
}

/// Mirror of the production transfer semantics (detcheck `Node::transfer`):
/// debit + credit + nonce bump commit together or not at all.
fn apply_transfer(accounts: &mut [Account], from: usize, to: usize, amount: u128) -> bool {
    let mut payer = accounts[from];
    let mut payee = accounts[to];
    let ok = payer
        .debit(Amount::from_units(amount))
        .and_then(|()| payee.credit(Amount::from_units(amount)))
        .is_ok()
        && payer.bump_nonce().is_ok();
    if ok {
        accounts[from] = payer;
        accounts[to] = payee;
    }
    ok
}

#[test]
fn h4_balance_conservation() {
    let mut accounts = fresh_accounts();
    let start_total = total(&accounts);
    // Deterministic sweep: every amount 0..=64 between distinct accounts, so
    // any amount-pattern mutation (tools/nc_h4.py NC-1) is guaranteed to fire.
    for amount in 0u128..=64u128 {
        let from = (amount as usize) % ACCOUNTS;
        let to = (amount as usize + 1) % ACCOUNTS;
        let before = total(&accounts);
        let ok = apply_transfer(&mut accounts, from, to, amount);
        assert_eq!(
            total(&accounts),
            before,
            "H4: balance conservation violated"
        );
        assert!(ok, "H4: rejected transfer must not change state");
    }
    // Deterministic stream, including overdrafts (rejections must not move
    // the total and must not mutate state).
    let mut lcg = Lcg::new(20261180);
    for round in 0..2000u64 {
        let from = (lcg.next_u64() as usize) % ACCOUNTS;
        let to = (lcg.next_u64() as usize) % ACCOUNTS;
        if from == to {
            continue;
        }
        let amount = lcg.next_u64() as u128 % 200_000;
        let mut trial = accounts.clone();
        let before = total(&accounts);
        if apply_transfer(&mut trial, from, to, amount) {
            assert_eq!(
                total(&trial),
                before,
                "H4: balance conservation violated (round {round})"
            );
            accounts = trial;
        } else {
            assert_eq!(
                trial, accounts,
                "H4: rejected transfer must not change state (round {round})"
            );
        }
        assert_eq!(
            total(&accounts),
            start_total,
            "H4: balance conservation violated (round {round})"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Random transfer sequences conserve the total whatever happens.
    #[test]
    fn h4_balance_conservation_random(transfers in proptest::collection::vec((0..8usize, 0..8usize, 0..1_000_000u128), 1..64)) {
        let mut accounts = fresh_accounts();
        let start_total = total(&accounts);
        for (from, to, amount) in transfers {
            if from == to {
                continue;
            }
            let mut trial = accounts.clone();
            let before = total(&accounts);
            if apply_transfer(&mut trial, from, to, amount) {
                prop_assert_eq!(total(&trial), before);
                accounts = trial;
            } else {
                prop_assert_eq!(trial.clone(), accounts.clone());
            }
            prop_assert_eq!(total(&accounts), start_total, "H4: balance conservation violated");
        }
    }
}

#[test]
fn h4_canonical_roundtrip() {
    // Trailing bytes must be rejected by the canonicality guard — this is the
    // NC-2 mutation target (tools/nc_h4.py disables the re-encode comparison).
    let nonce = Nonce::new(7);
    let mut trailing = encode_to_bytes(&nonce);
    trailing.push(0x00);
    assert!(
        decode_canonical::<Nonce>(&trailing).is_err(),
        "H4: canonical roundtrip violated"
    );
    // Padded varints must be rejected by the varint layer (C9 padding: set
    // the continuation bit on the last minimal byte and append a zero byte).
    let padded = [0x87u8, 0x00];
    let mut reader = aura_codec::Reader::new(&padded);
    assert!(
        aura_codec::read_varint(&mut reader).is_err(),
        "H4: canonical roundtrip violated"
    );
    // Round-trip of real transactions over the deterministic sweep.
    for amount in 0u128..=64u128 {
        let seed = [amount as u8; 32];
        let tx = signed_tx(&seed, Nonce::new(amount as u64), amount);
        let bytes = tx.to_bytes();
        let parsed = Tx::from_canonical_bytes(&bytes).expect("H4: canonical roundtrip violated");
        assert_eq!(parsed, tx, "H4: canonical roundtrip violated");
        assert_eq!(parsed.to_bytes(), bytes, "H4: canonical roundtrip violated");
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Any structurally valid transaction survives a canonical round-trip.
    #[test]
    fn h4_canonical_roundtrip_random(nonce in any::<u64>(), payload in proptest::collection::vec(any::<u8>(), 0..64)) {
        let seed = [0x42u8; 32];
        let pk = ed25519_public_key(&seed);
        let params = TxParams {
            chain_id: ChainId::new("h4-chain").unwrap(),
            signer: aura_tx::signer_address(&pk),
            nonce: Nonce::new(nonce),
            kind: TxKind::Transfer,
            payload,
            gas_limit: Gas::new(1_000),
            max_fee: Amount::from_units(10),
            tip: Amount::from_units(1),
            expiry: Height::new(100),
        };
        let tx = UnsignedTx::new(params).unwrap().sign(&seed).unwrap();
        let bytes = tx.to_bytes();
        let parsed = Tx::from_canonical_bytes(&bytes).expect("H4: canonical roundtrip violated");
        prop_assert_eq!(parsed.clone(), tx);
        prop_assert_eq!(parsed.to_bytes(), bytes.clone());
    }
}

fn signed_tx(seed: &[u8; 32], nonce: Nonce, amount: u128) -> Tx {
    let pk = ed25519_public_key(seed);
    let params = TxParams {
        chain_id: ChainId::new("h4-chain").unwrap(),
        signer: aura_tx::signer_address(&pk),
        nonce,
        kind: TxKind::Transfer,
        payload: amount.to_le_bytes().to_vec(),
        gas_limit: Gas::new(1_000),
        max_fee: Amount::from_units(10),
        tip: Amount::from_units(1),
        expiry: Height::new(100),
    };
    UnsignedTx::new(params).unwrap().sign(seed).unwrap()
}

#[test]
fn h4_signature_properties() {
    let seed = [7u8; 32];
    let tx = signed_tx(&seed, Nonce::new(7), 7);
    tx.verify_signature()
        .expect("H4: signature property violated");

    // Flipping any byte of the canonical encoding must break verification:
    // either the decode fails or the signature check does.
    let bytes = tx.to_bytes();
    for index in 0..bytes.len() {
        let mut bad = bytes.clone();
        bad[index] ^= 0x01;
        if let Ok(tampered) = Tx::from_canonical_bytes(&bad) {
            assert!(
                tampered.verify_signature().is_err(),
                "H4: signature property violated (byte {index})"
            );
        }
    }

    // The expiry field is signed: changing it after signing must be rejected.
    // (tools/nc_h4.py NC-3 drops `expiry` from `signing_bytes` and this case
    // must go red.)
    let mut late = tx.clone();
    late.unsigned.expiry = Height::new(101);
    assert!(
        late.verify_signature().is_err(),
        "H4: signature property violated"
    );

    // Signer ↔ public-key binding: a foreign key must not verify.
    let other_seed = [9u8; 32];
    let other_pk = ed25519_public_key(&other_seed);
    let mut forged = tx.clone();
    forged.sig.public_key = other_pk;
    assert!(
        forged.verify_signature().is_err(),
        "H4: signature property violated"
    );
}

#[test]
fn h4_credit_debit_algebra() {
    // Exact add/subtract identities over 0..=64 (amount 7 is the NC-6 trigger).
    for amount in 0u128..=64u128 {
        let mut account = Account::new(
            Nonce::new(0),
            Amount::from_units(START),
            Hash::ZERO,
            Hash::ZERO,
        );
        account
            .credit(Amount::from_units(amount))
            .expect("credit within capacity");
        assert_eq!(
            account.balance.units(),
            START + amount,
            "H4: credit/debit algebra violated"
        );
        account
            .debit(Amount::from_units(amount))
            .expect("debit within balance");
        assert_eq!(
            account.balance.units(),
            START,
            "H4: credit/debit algebra violated"
        );
        account.bump_nonce().expect("nonce bump");
        assert_eq!(
            account.nonce,
            Nonce::new(1),
            "H4: credit/debit algebra violated"
        );
    }
    // Overdraft is rejected without mutation.
    let mut poor = Account::new(Nonce::new(0), Amount::from_units(5), Hash::ZERO, Hash::ZERO);
    let before = poor;
    assert!(poor.debit(Amount::from_units(6)).is_err());
    assert_eq!(poor, before, "H4: credit/debit algebra violated");
    // Checked arithmetic: credit must not wrap u128.
    let mut whale = Account::new(
        Nonce::new(0),
        Amount::from_units(u128::MAX),
        Hash::ZERO,
        Hash::ZERO,
    );
    assert!(whale.credit(Amount::from_units(1)).is_err());
    assert_eq!(
        whale.balance.units(),
        u128::MAX,
        "H4: credit/debit algebra violated"
    );
}
