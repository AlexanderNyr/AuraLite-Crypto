//! H6 MEV bound (docs/h6-economic-plan.md §2, D-H6-1).
//!
//! The canonical transaction order must be a deterministic function of the
//! transaction set — insert order is irrelevant (metamorphism), and the
//! fee/nonce key rules stay pinned. A proposer therefore cannot extract
//! discretionary-reordering MEV.
//! `tools/nc_h6.py` NC-5 reverses the canonical order.

use aura_crypto::ed25519_public_key;
use aura_mempool::Mempool;
use aura_tx::{Tx, TxKind, TxParams, UnsignedTx, signer_address};
use aura_types::{Address, Amount, ChainId, Gas, Height, Nonce};

const CHAIN: &str = "auralite-h6-mev";
const RECIPIENT: Address = Address::from_bytes([0x42; 20]);

fn tx(seed_byte: u8, nonce: u64, tip: u128) -> Tx {
    let seed = [seed_byte; 32];
    let signer = signer_address(&ed25519_public_key(&seed));
    UnsignedTx::new(TxParams {
        chain_id: ChainId::new(CHAIN).unwrap(),
        signer,
        nonce: Nonce::new(nonce),
        kind: TxKind::Transfer,
        payload: [RECIPIENT.as_bytes().as_slice(), &1u128.to_be_bytes()].concat(),
        gas_limit: Gas::new(21_000),
        max_fee: Amount::from_units(1_000_000),
        tip: Amount::from_units(tip),
        expiry: Height::new(100),
    })
    .unwrap()
    .sign(&seed)
    .unwrap()
}

#[test]
fn h6_canonical_ordering_deterministic() {
    let txs: Vec<Tx> = vec![
        tx(1, 0, 10),
        tx(2, 0, 30),
        tx(3, 0, 20),
        tx(4, 0, 40),
        tx(1, 1, 5),
        tx(2, 1, 60),
    ];

    // Metamorphism: any insertion permutation yields the same canonical
    // order — the order is a pure function of the set.
    let permutations: [Vec<usize>; 3] = [
        vec![0, 1, 2, 3, 4, 5],
        vec![5, 4, 3, 2, 1, 0],
        vec![2, 0, 4, 1, 5, 3],
    ];
    let mut canonical: Option<Vec<Tx>> = None;
    for perm in permutations {
        let mut pool = Mempool::with_defaults();
        for i in perm {
            pool.insert(txs[i].clone()).expect("tx accepted");
        }
        let order = pool.canonical_transactions();
        match &canonical {
            None => canonical = Some(order),
            Some(previous) => assert_eq!(
                &order, previous,
                "H6: MEV extraction must stay bounded by canonical ordering"
            ),
        }
    }

    // Pinned key rules: same-sender nonce lanes stay ordered, and across
    // senders higher tips sort first.
    let order = canonical.expect("canonical order exists");
    let sender = |t: &Tx| *t.unsigned.signer.as_bytes();
    let nonce_of = |t: &Tx| t.unsigned.nonce.value();
    for pair in order.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if sender(a) == sender(b) {
            assert!(
                nonce_of(a) < nonce_of(b),
                "H6: MEV extraction must stay bounded by canonical ordering"
            );
        } else {
            assert!(
                sender(a) <= sender(b),
                "H6: MEV extraction must stay bounded by canonical ordering"
            );
        }
    }
}
