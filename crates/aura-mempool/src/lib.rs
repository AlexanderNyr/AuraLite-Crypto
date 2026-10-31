//! Bounded D3 transaction pool with nonce lanes, fee priority, replacement,
//! deterministic eviction and reorg reinsertion.
//!
//! The pool owns queueing policy, not chain validation. Callers must validate
//! transaction signatures, chain/expiry, gas, fee cap, account nonce and
//! solvency before insertion; after a reorg they must revalidate against the
//! new account state before calling [`Mempool::reinsert_reorged`].

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use aura_tx::Tx;
use aura_types::Hash;

/// Default maximum number of queued transactions.
pub const DEFAULT_CAPACITY: usize = 16_384;
/// Default replacement tip bump, in basis points (10%).
pub const DEFAULT_REPLACEMENT_BUMP_BPS: u16 = 1_000;
const BASIS_POINTS: u128 = 10_000;

type Sender = [u8; 20];
type TxKey = (Sender, u64);

/// Configuration for a [`Mempool`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MempoolConfig {
    /// Maximum number of queued transactions; must be nonzero.
    pub capacity: usize,
    /// Minimum tip increase for same-signer/same-nonce replacement, in basis
    /// points; valid range is `1..=10_000`.
    pub replacement_bump_bps: u16,
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_CAPACITY,
            replacement_bump_bps: DEFAULT_REPLACEMENT_BUMP_BPS,
        }
    }
}

/// Errors returned by pool construction and transaction insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MempoolError {
    /// A pool cannot have zero capacity.
    ZeroCapacity,
    /// Replacement bump must be between 1 and 10,000 basis points.
    InvalidReplacementBump(u16),
    /// This transaction identifier is already in the pool.
    DuplicateTransaction,
    /// Pool is full and the incoming transaction is not higher priority than
    /// the deterministic eviction candidate.
    PoolFull,
    /// Replacement did not satisfy the configured tip bump.
    ReplacementUnderpriced {
        /// Minimum accepted tip.
        required: u128,
        /// Incoming tip.
        got: u128,
    },
    /// Required replacement tip overflowed `u128`.
    ReplacementTipOverflow,
}

impl fmt::Display for MempoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCapacity => f.write_str("mempool capacity must be nonzero"),
            Self::InvalidReplacementBump(value) => {
                write!(f, "replacement bump {value} bps is outside 1..=10000")
            }
            Self::DuplicateTransaction => f.write_str("transaction already exists in mempool"),
            Self::PoolFull => f.write_str("mempool full; incoming tip is not competitive"),
            Self::ReplacementUnderpriced { required, got } => {
                write!(
                    f,
                    "replacement tip {got} is below required minimum {required}"
                )
            }
            Self::ReplacementTipOverflow => f.write_str("replacement tip arithmetic overflow"),
        }
    }
}

impl std::error::Error for MempoolError {}

/// Result of a successful pool insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertOutcome {
    /// Identifier of the newly inserted transaction.
    pub txid: Hash,
    /// Identifier replaced at the same signer and nonce, if any.
    pub replaced: Option<Hash>,
    /// Identifier evicted to make room, if any.
    pub evicted: Option<Hash>,
}

/// Bounded, deterministic transaction pool.
#[derive(Debug, Clone)]
pub struct Mempool {
    config: MempoolConfig,
    transactions: BTreeMap<TxKey, Tx>,
    txids: HashMap<Hash, TxKey>,
}

impl Mempool {
    /// Construct an empty pool with validated configuration.
    pub fn new(config: MempoolConfig) -> Result<Self, MempoolError> {
        if config.capacity == 0 {
            return Err(MempoolError::ZeroCapacity);
        }
        if !(1..=10_000).contains(&config.replacement_bump_bps) {
            return Err(MempoolError::InvalidReplacementBump(
                config.replacement_bump_bps,
            ));
        }
        Ok(Self {
            config,
            transactions: BTreeMap::new(),
            txids: HashMap::new(),
        })
    }

    /// Construct an empty pool using [`MempoolConfig::default`].
    pub fn with_defaults() -> Self {
        Self::new(MempoolConfig::default()).expect("default mempool configuration is valid")
    }

    /// Number of queued transactions.
    pub fn len(&self) -> usize {
        self.transactions.len()
    }

    /// Whether the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }

    /// Configured maximum number of transactions.
    pub fn capacity(&self) -> usize {
        self.config.capacity
    }

    /// Whether `txid` is currently queued.
    pub fn contains_txid(&self, txid: &Hash) -> bool {
        self.txids.contains_key(txid)
    }

    /// Last canonical sender/nonce key, used for the append-only admission fast path.
    pub fn last_key(&self) -> Option<TxKey> {
        self.transactions.last_key_value().map(|(key, _)| *key)
    }

    /// Return all transactions in deterministic sender/nonce order.
    pub fn canonical_transactions(&self) -> Vec<Tx> {
        self.transactions.values().cloned().collect()
    }

    /// Insert a transaction that has already passed caller-side protocol and
    /// state validation. Higher-fee same-nonce transactions may replace an
    /// existing entry; a full pool evicts only its deterministic lowest-tip
    /// transaction, and only when the incoming tip is strictly higher.
    pub fn insert(&mut self, tx: Tx) -> Result<InsertOutcome, MempoolError> {
        let txid = tx.txid();
        if self.txids.contains_key(&txid) {
            return Err(MempoolError::DuplicateTransaction);
        }
        let key = (*tx.unsigned.signer.as_bytes(), tx.unsigned.nonce.value());
        let mut replaced = None;
        let mut evicted = None;

        if let Some(existing) = self.transactions.get(&key) {
            let required = replacement_tip(
                existing.unsigned.tip.units(),
                self.config.replacement_bump_bps,
            )?;
            let got = tx.unsigned.tip.units();
            if got < required {
                return Err(MempoolError::ReplacementUnderpriced { required, got });
            }
            let old = self.transactions.remove(&key).expect("entry was found");
            let old_id = old.txid();
            self.txids.remove(&old_id);
            replaced = Some(old_id);
        } else if self.transactions.len() >= self.config.capacity {
            let (weakest_key, weakest) = self
                .transactions
                .iter()
                .min_by_key(|(candidate_key, candidate)| {
                    (
                        candidate.unsigned.tip.units(),
                        candidate_key.0,
                        candidate_key.1,
                    )
                })
                .map(|(candidate_key, candidate)| (*candidate_key, candidate.clone()))
                .expect("a full pool is nonempty");
            if tx.unsigned.tip.units() <= weakest.unsigned.tip.units() {
                return Err(MempoolError::PoolFull);
            }
            self.transactions.remove(&weakest_key);
            let old_id = weakest.txid();
            self.txids.remove(&old_id);
            evicted = Some(old_id);
        }

        self.transactions.insert(key, tx);
        self.txids.insert(txid, key);
        Ok(InsertOutcome {
            txid,
            replaced,
            evicted,
        })
    }

    /// Reinsert a transaction removed from a discarded branch. The caller is
    /// responsible for validating it against the post-reorg chain, nonce and
    /// balance view before invoking this method.
    pub fn reinsert_reorged(&mut self, tx: Tx) -> Result<InsertOutcome, MempoolError> {
        self.insert(tx)
    }

    /// Remove transactions after they have been selected into a proposed block.
    /// Returns the entries that were actually present, in the requested id order.
    pub fn remove_included(&mut self, txids: &[Hash]) -> Vec<Tx> {
        let mut removed = Vec::with_capacity(txids.len());
        for txid in txids {
            if let Some(key) = self.txids.remove(txid)
                && let Some(tx) = self.transactions.remove(&key)
            {
                removed.push(tx);
            }
        }
        removed
    }

    /// Select up to `limit` transactions by highest tip while preserving each
    /// sender's contiguous nonce lane. The raw 20-byte address keys avoid
    /// requiring `Address: Ord`.
    pub fn select_ready(&self, base_nonces: &BTreeMap<Sender, u64>, limit: usize) -> Vec<Tx> {
        let mut next_nonce = base_nonces.clone();
        let mut ready = BTreeSet::<(Reverse<u128>, Sender, u64)>::new();
        for ((sender, nonce), tx) in &self.transactions {
            if *nonce == next_nonce.get(sender).copied().unwrap_or(0) {
                ready.insert((Reverse(tx.unsigned.tip.units()), *sender, *nonce));
            }
        }

        let mut selected = Vec::with_capacity(limit.min(self.transactions.len()));
        while selected.len() < limit {
            let Some((_, sender, nonce)) = ready.pop_first() else {
                break;
            };
            let Some(tx) = self.transactions.get(&(sender, nonce)) else {
                continue;
            };
            selected.push(tx.clone());
            let Some(next) = nonce.checked_add(1) else {
                continue;
            };
            next_nonce.insert(sender, next);
            if let Some(next_tx) = self.transactions.get(&(sender, next)) {
                ready.insert((Reverse(next_tx.unsigned.tip.units()), sender, next));
            }
        }
        selected
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::with_defaults()
    }
}

fn replacement_tip(old_tip: u128, bump_bps: u16) -> Result<u128, MempoolError> {
    let numerator = old_tip
        .checked_mul(u128::from(bump_bps))
        .and_then(|value| value.checked_add(BASIS_POINTS - 1))
        .ok_or(MempoolError::ReplacementTipOverflow)?;
    let proportional = numerator / BASIS_POINTS;
    let bump = proportional.max(1);
    old_tip
        .checked_add(bump)
        .ok_or(MempoolError::ReplacementTipOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use aura_crypto::ed25519_public_key;
    use aura_tx::{TxKind, TxParams, UnsignedTx, signer_address};
    use aura_types::{Address, Amount, ChainId, Gas, Height, Nonce};

    const CHAIN: &str = "auralite-mempool-test-1";
    const RECIPIENT: Address = Address::from_bytes([0x42; 20]);

    fn tx(seed_byte: u8, nonce: u64, tip: u128) -> Tx {
        tx_with_amount(seed_byte, nonce, tip, 1)
    }

    fn tx_with_amount(seed_byte: u8, nonce: u64, tip: u128, amount: u128) -> Tx {
        let seed = [seed_byte; 32];
        let signer = signer_address(&ed25519_public_key(&seed));
        UnsignedTx::new(TxParams {
            chain_id: ChainId::new(CHAIN).unwrap(),
            signer,
            nonce: Nonce::new(nonce),
            kind: TxKind::Transfer,
            payload: [RECIPIENT.as_bytes().as_slice(), &amount.to_be_bytes()].concat(),
            gas_limit: Gas::new(21_000),
            max_fee: Amount::from_units(1_000_000),
            tip: Amount::from_units(tip),
            expiry: Height::new(100),
        })
        .unwrap()
        .sign(&seed)
        .unwrap()
    }

    fn sender(tx: &Tx) -> [u8; 20] {
        *tx.unsigned.signer.as_bytes()
    }

    #[test]
    fn ready_transactions_follow_fee_priority_and_nonce_dependencies() {
        let mut pool = Mempool::with_defaults();
        let one = tx(1, 0, 1);
        let two = tx(2, 0, 50);
        let (low_seed, high_seed) = if sender(&one) < sender(&two) {
            (1, 2)
        } else {
            (2, 1)
        };
        let a0 = tx(low_seed, 0, 1);
        let a1 = tx(low_seed, 1, 100);
        let b0 = tx(high_seed, 0, 50);
        pool.insert(a0.clone()).unwrap();
        pool.insert(a1.clone()).unwrap();
        pool.insert(b0.clone()).unwrap();

        let base = BTreeMap::from([(sender(&a0), 0), (sender(&b0), 0)]);
        let selected = pool.select_ready(&base, 3);
        assert_eq!(selected[0].txid(), b0.txid());
        assert_eq!(selected[1].txid(), a0.txid());
        assert_eq!(selected[2].txid(), a1.txid());
    }

    #[test]
    fn replacement_requires_fee_bump_and_replaces_same_nonce() {
        let mut pool = Mempool::with_defaults();
        let original = tx(1, 0, 10);
        let original_id = original.txid();
        pool.insert(original).unwrap();
        assert!(matches!(
            pool.insert(tx_with_amount(1, 0, 10, 2)),
            Err(MempoolError::ReplacementUnderpriced {
                required: 11,
                got: 10
            })
        ));
        let replacement = tx_with_amount(1, 0, 11, 2);
        let replacement_id = replacement.txid();
        let outcome = pool.insert(replacement).unwrap();
        assert_eq!(outcome.replaced, Some(original_id));
        assert_eq!(pool.len(), 1);
        assert!(pool.contains_txid(&replacement_id));
        assert!(!pool.contains_txid(&original_id));
    }

    #[test]
    fn capacity_evicts_only_lower_priority() {
        let mut pool = Mempool::new(MempoolConfig {
            capacity: 2,
            ..MempoolConfig::default()
        })
        .unwrap();
        let low = tx(1, 0, 1);
        let low_id = low.txid();
        pool.insert(low).unwrap();
        pool.insert(tx(2, 0, 2)).unwrap();
        let incoming = tx(3, 0, 3);
        let incoming_id = incoming.txid();
        let outcome = pool.insert(incoming).unwrap();
        assert_eq!(outcome.evicted, Some(low_id));
        assert!(!pool.contains_txid(&low_id));
        assert!(pool.contains_txid(&incoming_id));
        assert!(matches!(
            pool.insert(tx(4, 0, 2)),
            Err(MempoolError::PoolFull)
        ));
    }

    #[test]
    fn duplicate_transaction_id_is_rejected() {
        let mut pool = Mempool::with_defaults();
        let transaction = tx(1, 0, 1);
        pool.insert(transaction.clone()).unwrap();
        assert_eq!(
            pool.insert(transaction),
            Err(MempoolError::DuplicateTransaction)
        );
    }

    #[test]
    fn reorged_transactions_can_be_reinserted_at_new_head() {
        let mut pool = Mempool::with_defaults();
        let transaction = tx(1, 0, 1);
        let txid = transaction.txid();
        pool.insert(transaction.clone()).unwrap();
        assert_eq!(pool.remove_included(&[txid]).len(), 1);
        assert!(pool.is_empty());
        pool.reinsert_reorged(transaction).unwrap();
        assert!(pool.contains_txid(&txid));
        let base = BTreeMap::from([(sender(&tx(1, 0, 1)), 0)]);
        assert_eq!(pool.select_ready(&base, 1)[0].txid(), txid);
    }

    #[test]
    fn nonce_gaps_are_not_selected_until_the_lane_is_contiguous() {
        let mut pool = Mempool::with_defaults();
        let future = tx(1, 1, 100);
        let base = BTreeMap::from([(sender(&future), 0)]);
        pool.insert(future).unwrap();
        assert!(pool.select_ready(&base, 10).is_empty());
    }
}
