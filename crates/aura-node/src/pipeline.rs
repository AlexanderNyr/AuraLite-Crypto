//! D2 deterministic transaction pipeline.
//!
//! This library composes the D1 node with canonical transactions, native
//! transfer execution, C5 blocks, the B3 finality engine, S2 storage, receipts
//! and commit events. The D3 mempool bounds ingress, validates projected
//! nonce/balance/gas state, prioritises sender nonce lanes by tip, and supports
//! replacement, eviction and reorg reinsertion. Network submission belongs to
//! D4. Only native `Transfer` is executable; contract deployment and calls are
//! rejected rather than acknowledged without execution.
//!
//! A local one-validator pipeline uses the real BLS vote/QC and three-chain
//! commit implementation. This is a development/test composition, not a
//! claim that the D2 node is a multi-validator network. The same finalized
//! block is written to durable storage before its receipt event is published.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};

use aura_block::{Block, BlockError, HeaderParams, MAX_BLOCK_TXS};
use aura_codec::Writer;
use aura_consensus::bls::{BlsSecretKey, address_from_public_key};
use aura_consensus::finality::{FinalityBlock, FinalityEngine, FinalizedBlock};
use aura_consensus::qc::{QcAggregator, QuorumCertificate, VoteType};
use aura_crypto::hash_domain;
use aura_mempool::{Mempool, MempoolConfig, MempoolError};
use aura_state::{Account, StateKey};
use aura_storage::{
    Receipt as StorageReceipt, StateChange, Storage, StorageError, key_path, state_root,
};
use aura_tx::{Tx, TxError, TxKind};
use aura_types::{Address, Amount, ChainId, Domain, Gas, Hash, Height, Nonce, PROTOCOL_VERSION};

/// Native transfer payload size: 20-byte recipient followed by 16-byte amount.
pub const TRANSFER_PAYLOAD_LEN: usize = 36;
/// D2's deterministic gas schedule for one native transfer.
pub const TRANSFER_GAS: u64 = 21_000;
/// Per-gas base fee for the local D2 pipeline.
pub const D2_BASE_FEE: Amount = Amount::ONE;
/// Default D3 transaction-pool capacity.
pub const D3_MEMPOOL_CAPACITY: usize = 16_384;
const QC_MAGIC: &[u8] = b"ALQC1";
const RECEIPT_MAGIC: &[u8] = b"ALR2";

/// Inputs needed to construct a local transaction pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Network chain identifier.
    pub chain_id: String,
    /// Directory containing the S2 WAL and state backend.
    pub data_dir: PathBuf,
    /// Local BLS seed for the one-validator D2 composition.
    pub validator_seed: [u8; 32],
    /// Genesis balances for a new store. Duplicate addresses are rejected.
    pub allocations: Vec<(Address, Amount)>,
    /// Deterministic genesis timestamp in milliseconds.
    pub genesis_time_ms: u64,
}

/// Errors from transaction admission, execution, consensus or persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    /// Chain identifier is not a valid canonical `ChainId`.
    InvalidChainId(String),
    /// Transaction failed canonical validation or signature verification.
    InvalidTransaction(TxError),
    /// The transaction kind is not implemented by D2.
    UnsupportedTransactionKind(TxKind),
    /// Native transfer payload does not have its required fixed length.
    InvalidTransferPayload(usize),
    /// Transfer gas limit is below the deterministic D2 cost.
    GasLimitTooLow {
        /// Required gas amount.
        required: u64,
        /// Gas amount supplied by the transaction.
        got: u64,
    },
    /// The fee cap does not cover base fee plus tip.
    FeeCapTooLow,
    /// Account nonce does not match the next expected nonce.
    NonceMismatch {
        /// Expected nonce from current projected state.
        expected: u64,
        /// Nonce carried by the submitted transaction.
        got: u64,
    },
    /// Sender cannot cover transfer value and execution fee.
    InsufficientBalance,
    /// Checked amount arithmetic overflowed.
    AmountOverflow,
    /// The transaction identifier is already queued or pending.
    DuplicateTransaction,
    /// The bounded D3 mempool rejected an insertion.
    Mempool(MempoolError),
    /// The legacy D2 queue-capacity error (retained for source compatibility).
    QueueFull,
    /// More than one genesis allocation was supplied for an address.
    DuplicateGenesisAccount,
    /// One-validator consensus unexpectedly failed to produce a QC.
    NoQuorum,
    /// A finalized block has no matching staged transaction batch.
    FinalityInvariant,
    /// Committed state exists without a replayable finality checkpoint.
    RecoveryCheckpointRequired,
    /// Durable storage produced a different root from the proposed block.
    StateRootMismatch,
    /// Storage operation failed.
    Storage(StorageError),
    /// The C5 block failed validation.
    Block(BlockError),
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidChainId(message) => write!(f, "invalid chain id: {message}"),
            Self::InvalidTransaction(error) => write!(f, "invalid transaction: {error}"),
            Self::UnsupportedTransactionKind(kind) => {
                write!(f, "transaction kind {kind:?} is not executable at D2")
            }
            Self::InvalidTransferPayload(len) => {
                write!(
                    f,
                    "transfer payload is {len} bytes, expected {TRANSFER_PAYLOAD_LEN}"
                )
            }
            Self::GasLimitTooLow { required, got } => {
                write!(
                    f,
                    "transfer requires {required} gas, transaction allows {got}"
                )
            }
            Self::FeeCapTooLow => f.write_str("max_fee does not cover base_fee plus tip"),
            Self::NonceMismatch { expected, got } => {
                write!(f, "nonce {got} does not match expected nonce {expected}")
            }
            Self::InsufficientBalance => f.write_str("insufficient balance for transfer and fee"),
            Self::AmountOverflow => f.write_str("amount arithmetic overflow"),
            Self::DuplicateTransaction => f.write_str("transaction id already queued or pending"),
            Self::Mempool(error) => write!(f, "mempool rejected transaction: {error}"),
            Self::QueueFull => f.write_str("D2 ingress queue is full"),
            Self::DuplicateGenesisAccount => f.write_str("duplicate genesis allocation address"),
            Self::NoQuorum => f.write_str("one-validator consensus did not produce a QC"),
            Self::FinalityInvariant => f.write_str("finality output did not match a staged block"),
            Self::RecoveryCheckpointRequired => {
                f.write_str("committed transaction state requires a finality checkpoint to reopen")
            }
            Self::StateRootMismatch => f.write_str("storage root differs from proposed block root"),
            Self::Storage(error) => write!(f, "storage error: {error}"),
            Self::Block(error) => write!(f, "block error: {error}"),
        }
    }
}

impl std::error::Error for PipelineError {}

impl From<StorageError> for PipelineError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<MempoolError> for PipelineError {
    fn from(error: MempoolError) -> Self {
        Self::Mempool(error)
    }
}

impl From<TxError> for PipelineError {
    fn from(error: TxError) -> Self {
        Self::InvalidTransaction(error)
    }
}

impl From<BlockError> for PipelineError {
    fn from(error: BlockError) -> Self {
        Self::Block(error)
    }
}

/// Receipt for one successfully executed native transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionReceipt {
    /// Transaction identifier.
    pub txid: Hash,
    /// Sender address.
    pub signer: Address,
    /// Sender nonce consumed by this transaction.
    pub nonce: Nonce,
    /// Transfer recipient.
    pub recipient: Address,
    /// Transferred smallest-unit amount.
    pub amount: Amount,
    /// Gas consumed by the D2 transfer executor.
    pub gas_used: Gas,
    /// Base-fee portion burned by this transaction.
    pub base_fee_burned: Amount,
    /// Priority tip credited to the local proposer.
    pub tip_paid: Amount,
    /// Block height at which the transfer was included.
    pub height: Height,
}

impl TransactionReceipt {
    /// Decode the fixed-width D2 receipt record stored under the C7 receipt key.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PipelineError> {
        const LEN: usize = 4 + 32 + 20 + 8 + 20 + 16 + 8 + 16 + 16 + 8;
        if bytes.len() != LEN || &bytes[..4] != RECEIPT_MAGIC {
            return Err(PipelineError::FinalityInvariant);
        }
        let mut cursor = 4;
        let mut take = |length: usize| {
            let start = cursor;
            cursor += length;
            &bytes[start..cursor]
        };
        let txid = Hash::from_slice(take(32)).map_err(|_| PipelineError::FinalityInvariant)?;
        let signer = Address::from_slice(take(20)).map_err(|_| PipelineError::FinalityInvariant)?;
        let nonce = Nonce::new(u64::from_le_bytes(
            take(8)
                .try_into()
                .map_err(|_| PipelineError::FinalityInvariant)?,
        ));
        let recipient =
            Address::from_slice(take(20)).map_err(|_| PipelineError::FinalityInvariant)?;
        let amount = Amount::from_units(u128::from_le_bytes(
            take(16)
                .try_into()
                .map_err(|_| PipelineError::FinalityInvariant)?,
        ));
        let gas_used = Gas::new(u64::from_le_bytes(
            take(8)
                .try_into()
                .map_err(|_| PipelineError::FinalityInvariant)?,
        ));
        let base_fee_burned = Amount::from_units(u128::from_le_bytes(
            take(16)
                .try_into()
                .map_err(|_| PipelineError::FinalityInvariant)?,
        ));
        let tip_paid = Amount::from_units(u128::from_le_bytes(
            take(16)
                .try_into()
                .map_err(|_| PipelineError::FinalityInvariant)?,
        ));
        let height = Height::new(u64::from_le_bytes(
            take(8)
                .try_into()
                .map_err(|_| PipelineError::FinalityInvariant)?,
        ));
        Ok(Self {
            txid,
            signer,
            nonce,
            recipient,
            amount,
            gas_used,
            base_fee_burned,
            tip_paid,
            height,
        })
    }

    /// Canonical fixed-width receipt bytes stored under the C7 receipt key.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + 32 + 20 + 8 + 20 + 16 + 8 + 16 + 16 + 8);
        out.extend_from_slice(RECEIPT_MAGIC);
        out.extend_from_slice(self.txid.as_bytes());
        out.extend_from_slice(self.signer.as_bytes());
        out.extend_from_slice(&self.nonce.value().to_le_bytes());
        out.extend_from_slice(self.recipient.as_bytes());
        out.extend_from_slice(&self.amount.units().to_le_bytes());
        out.extend_from_slice(&self.gas_used.value().to_le_bytes());
        out.extend_from_slice(&self.base_fee_burned.units().to_le_bytes());
        out.extend_from_slice(&self.tip_paid.units().to_le_bytes());
        out.extend_from_slice(&self.height.value().to_le_bytes());
        out
    }
}

/// One block that has passed B3 finality and was persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedBlock {
    /// Canonical C5 block.
    pub block: Block,
    /// Successful transaction receipts, in canonical block order.
    pub receipts: Vec<TransactionReceipt>,
    /// S2 storage commit receipt.
    pub storage: StorageReceipt,
}

/// Result of one proposal attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockProduction {
    /// Newly proposed canonical block.
    pub block: Block,
    /// Whether it had at least one transaction.
    pub transaction_count: usize,
    /// Blocks newly finalized and durably committed by this proposal.
    pub committed: Vec<CommittedBlock>,
}

/// Event published only after the matching storage commit succeeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineEvent {
    /// Persisted block and its receipts.
    pub committed: CommittedBlock,
}

struct ManagedSubscriber {
    sender: Sender<PipelineEvent>,
    active: Arc<AtomicBool>,
}

/// Drop-aware commit stream used by transport adapters so disconnected clients
/// are removed even when no later block is committed.
pub(crate) struct ManagedSubscription {
    receiver: Receiver<PipelineEvent>,
    active: Arc<AtomicBool>,
}

impl ManagedSubscription {
    pub(crate) fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<PipelineEvent, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
}

impl Drop for ManagedSubscription {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

struct PendingBlock {
    finality_hash: Hash,
    committed: CommittedBlock,
    changes: Vec<StateChange>,
    after_accounts: BTreeMap<[u8; 20], Account>,
}

/// Single-process D2 transaction sequencer.
pub struct Pipeline {
    chain_id: String,
    data_dir: PathBuf,
    storage: Storage,
    validator: BlsSecretKey,
    proposer: Address,
    validator_set_hash: Hash,
    consensus: FinalityEngine,
    genesis_hash: Hash,
    committed_blocks: VecDeque<Block>,
    latest_qc: Option<QuorumCertificate>,
    committed_accounts: BTreeMap<[u8; 20], Account>,
    projected_accounts: BTreeMap<[u8; 20], Account>,
    queue_accounts: BTreeMap<[u8; 20], Account>,
    mempool: Mempool,
    pending: BTreeMap<Hash, PendingBlock>,
    pending_order: VecDeque<Hash>,
    proposed_height: u64,
    proposed_tip: Hash,
    subscribers: Vec<Sender<PipelineEvent>>,
    managed_subscribers: Vec<ManagedSubscriber>,
}

impl Pipeline {
    /// Open a D2 pipeline with a one-validator BFT engine and S2 storage.
    ///
    /// New stores are initialized from `allocations`. Existing stores are
    /// opened and their accounts restored from the WAL-backed state. D2 keeps
    /// the consensus tip in process; restart checkpointing is a later node
    /// integration task, so a non-empty store with committed receipts is
    /// refused rather than silently forking from a synthetic genesis.
    pub fn open(config: PipelineConfig) -> Result<Self, PipelineError> {
        let chain = ChainId::new(&config.chain_id)
            .map_err(|error| PipelineError::InvalidChainId(error.to_string()))?;
        let validator = BlsSecretKey::from_seed(&config.validator_seed);
        let proposer = address_from_public_key(&validator.public_key());
        let validator_set_hash = hash_domain(
            &config.chain_id,
            Domain::Genesis,
            &validator.public_key().to_bytes(),
        );
        let mut storage = Storage::open_memory(&config.data_dir)?;
        let accounts = if storage.head().version == 0 {
            let mut allocations = BTreeMap::new();
            for (address, balance) in config.allocations {
                if allocations
                    .insert(*address.as_bytes(), (address, balance))
                    .is_some()
                {
                    return Err(PipelineError::DuplicateGenesisAccount);
                }
            }
            let changes = allocations
                .values()
                .map(|(address, balance)| StateChange {
                    key: StateKey::account(address).as_bytes().to_vec(),
                    value: Some(
                        Account::new(
                            Nonce::new(0),
                            *balance,
                            Account::NO_CODE,
                            Account::EMPTY_STORAGE_ROOT,
                        )
                        .to_bytes(),
                    ),
                    nonce: 0,
                })
                .collect::<Vec<_>>();
            if !changes.is_empty() {
                storage.commit(changes)?;
            }
            allocations
                .into_iter()
                .map(|(address_key, (_, balance))| {
                    (
                        address_key,
                        Account::new(
                            Nonce::new(0),
                            balance,
                            Account::NO_CODE,
                            Account::EMPTY_STORAGE_ROOT,
                        ),
                    )
                })
                .collect()
        } else {
            let mut loaded = BTreeMap::new();
            for (key, value) in storage.entries()? {
                if key.first() == Some(&0x05) {
                    return Err(PipelineError::RecoveryCheckpointRequired);
                }
                if key.len() == 21 && key.first() == Some(&0x01) {
                    let raw: [u8; 20] = key[1..]
                        .try_into()
                        .map_err(|_| PipelineError::FinalityInvariant)?;
                    let account = Account::from_canonical_bytes(&value)
                        .map_err(|_| PipelineError::FinalityInvariant)?;
                    loaded.insert(raw, account);
                }
            }
            loaded
        };
        // Ensure an empty default account exists for a fresh empty genesis.
        let genesis_root = storage.head().root;
        let genesis_block = Block::new(
            HeaderParams {
                version: PROTOCOL_VERSION,
                chain_id: chain,
                height: Height::new(0),
                parent_hash: Hash::ZERO,
                state_root: genesis_root,
                validator_set_hash,
                timestamp_ms: config.genesis_time_ms,
                proposer,
                qc: None,
                gas_used: Gas::new(0),
                base_fee: D2_BASE_FEE,
            },
            Vec::new(),
        )?;
        genesis_block.validate(&config.chain_id)?;
        let genesis_hash = genesis_block.block_hash();
        let finality_genesis = FinalityBlock {
            view: 0,
            round: 0,
            parent_hash: Hash::ZERO,
            timestamp_ms: config.genesis_time_ms,
            block_hash: genesis_hash,
            qc: None,
            transactions: Vec::new(),
        };
        let mut consensus = FinalityEngine::new(&config.chain_id, 1, genesis_hash);
        consensus.init_with_genesis(finality_genesis);
        let projected_accounts = accounts.clone();
        let queue_accounts = accounts.clone();
        let mempool = Mempool::new(MempoolConfig {
            capacity: D3_MEMPOOL_CAPACITY,
            ..MempoolConfig::default()
        })?;
        Ok(Self {
            chain_id: config.chain_id,
            data_dir: config.data_dir,
            storage,
            validator,
            proposer,
            validator_set_hash,
            consensus,
            genesis_hash,
            committed_blocks: VecDeque::from([genesis_block]),
            latest_qc: None,
            committed_accounts: accounts,
            projected_accounts,
            queue_accounts,
            mempool,
            pending: BTreeMap::new(),
            pending_order: VecDeque::new(),
            proposed_height: 0,
            proposed_tip: genesis_hash,
            subscribers: Vec::new(),
            managed_subscribers: Vec::new(),
        })
    }

    /// Validate and submit a native transfer to the bounded D3 mempool.
    ///
    /// Chain, expiry, signature, nonce, balance, gas and fee checks run before
    /// insertion. Same-nonce replacements require the configured tip bump;
    /// otherwise-valid candidates are ordered by sender/nonce for projected
    /// solvency checks. The append-only path avoids re-executing the whole pool
    /// for the common sequential-nonce case.
    pub fn submit(&mut self, tx: Tx) -> Result<Hash, PipelineError> {
        let _span = tracing::debug_span!("aura.tx.submit").entered();
        let height = Height::new(self.proposed_height.saturating_add(1));
        tx.validate(&self.chain_id, height)?;
        let sort_key = (tx.unsigned.signer.into_bytes(), tx.unsigned.nonce.value());
        let append = self.mempool.last_key().is_none_or(|last| last < sort_key);
        if append && self.mempool.len() < self.mempool.capacity() {
            let mut projected = self.queue_accounts.clone();
            let _ = execute_transfer(&tx, height, self.proposer, &mut projected)?;
            let outcome = self.mempool.insert(tx)?;
            self.queue_accounts = projected;
            return Ok(outcome.txid);
        }
        let mut candidate_pool = self.mempool.clone();
        let outcome = candidate_pool
            .insert(tx.clone())
            .map_err(|error| match error {
                MempoolError::DuplicateTransaction => PipelineError::DuplicateTransaction,
                other => PipelineError::Mempool(other),
            })?;

        let projected = if append && outcome.replaced.is_none() && outcome.evicted.is_none() {
            let mut projected = self.queue_accounts.clone();
            let _ = execute_transfer(&tx, height, self.proposer, &mut projected)?;
            projected
        } else {
            project_mempool_accounts(
                &self.chain_id,
                height,
                self.proposer,
                &self.projected_accounts,
                &candidate_pool,
            )?
        };

        self.mempool = candidate_pool;
        self.queue_accounts = projected;
        Ok(outcome.txid)
    }

    /// Propose one block, run its transactions through the D2 executor and
    /// B3 one-validator QC path, then persist any blocks newly finalized.
    ///
    /// Empty calls produce nil blocks. This lets a local pipeline advance the
    /// three-chain rule after a transaction block without inventing work.
    pub fn produce_block(&mut self) -> Result<BlockProduction, PipelineError> {
        let _span = tracing::info_span!("aura.block.produce").entered();
        let height = self
            .proposed_height
            .checked_add(1)
            .ok_or(PipelineError::AmountOverflow)?;
        let base_nonces = self
            .projected_accounts
            .iter()
            .map(|(address, account)| (*address, account.nonce.value()))
            .collect::<BTreeMap<_, _>>();
        let mut txs = self.mempool.select_ready(&base_nonces, MAX_BLOCK_TXS);
        let count = txs.len();
        // The mempool picks fee-priority sender lanes. C5 fixes a canonical
        // body order, so execute and commit those selected transactions by key.
        txs.sort_by_key(|tx| (tx.unsigned.signer, tx.unsigned.nonce));
        let included_txids = txs.iter().map(Tx::txid).collect::<Vec<_>>();
        let mut after_accounts = self.projected_accounts.clone();
        let mut receipts = Vec::with_capacity(count);
        let mut total_gas = 0u64;
        for tx in &txs {
            tx.validate(&self.chain_id, Height::new(height))?;
            let receipt =
                execute_transfer(tx, Height::new(height), self.proposer, &mut after_accounts)?;
            total_gas = total_gas
                .checked_add(receipt.gas_used.value())
                .ok_or(PipelineError::AmountOverflow)?;
            receipts.push(receipt);
        }

        let block_changes = account_changes(&self.projected_accounts, &after_accounts);
        let mut receipt_changes = receipts
            .iter()
            .map(|receipt| StateChange {
                key: StateKey::receipt(&receipt.txid).as_bytes().to_vec(),
                value: Some(receipt.to_bytes()),
                nonce: self.storage.head().version.saturating_add(1),
            })
            .collect::<Vec<_>>();
        let mut changes = block_changes;
        changes.append(&mut receipt_changes);
        changes.sort_by(|a, b| a.key.cmp(&b.key));
        let projected_root = self.root_after_pending(&changes)?;
        let timestamp_ms = self
            .consensus
            .tip
            .timestamp_ms
            .checked_add(1_000)
            .ok_or(PipelineError::AmountOverflow)?;
        let mut finality_txs = txs.iter().map(Tx::to_bytes).collect::<Vec<_>>();
        finality_txs.sort();
        let finality_block = self.consensus.propose_at(timestamp_ms, finality_txs);
        let qc_bytes = self.latest_qc.as_ref().map(encode_qc);
        let block = Block::new(
            HeaderParams {
                version: PROTOCOL_VERSION,
                chain_id: ChainId::new(&self.chain_id)
                    .map_err(|error| PipelineError::InvalidChainId(error.to_string()))?,
                height: Height::new(height),
                parent_hash: self.proposed_tip,
                state_root: projected_root,
                validator_set_hash: self.validator_set_hash,
                timestamp_ms,
                proposer: self.proposer,
                qc: qc_bytes,
                gas_used: Gas::new(total_gas),
                base_fee: D2_BASE_FEE,
            },
            txs,
        )?;
        block.validate(&self.chain_id)?;
        block.validate_transactions(&self.chain_id)?;

        let vote = self.consensus.vote_for(
            &self.validator,
            self.proposer,
            &finality_block,
            VoteType::Precommit,
        );
        let mut aggregator = QcAggregator::new(1);
        let (qc, newly_finalized) =
            self.consensus
                .aggregate_and_commit(&mut aggregator, self.validator.public_key(), vote);
        let qc = qc.ok_or(PipelineError::NoQuorum)?;
        let finality_hash = finality_block.block_hash;
        let committed = CommittedBlock {
            block: block.clone(),
            receipts,
            storage: StorageReceipt {
                version: 0,
                root: Hash::ZERO,
                wal_offset: 0,
            },
        };
        self.pending.insert(
            finality_hash,
            PendingBlock {
                finality_hash,
                committed,
                changes,
                after_accounts,
            },
        );
        self.pending_order.push_back(finality_hash);
        self.projected_accounts = self
            .pending_order
            .back()
            .and_then(|hash| self.pending.get(hash))
            .map(|pending| pending.after_accounts.clone())
            .unwrap_or_else(|| self.committed_accounts.clone());
        self.proposed_height = height;
        self.proposed_tip = block.block_hash();
        self.latest_qc = Some(qc);
        self.mempool.remove_included(&included_txids);
        self.queue_accounts = project_mempool_accounts(
            &self.chain_id,
            Height::new(self.proposed_height.saturating_add(1)),
            self.proposer,
            &self.projected_accounts,
            &self.mempool,
        )?;

        let mut committed_blocks = Vec::new();
        for finalized in newly_finalized {
            if finalized.block.block_hash == self.genesis_hash {
                continue;
            }
            committed_blocks.push(self.persist_finalized(finalized)?);
        }
        Ok(BlockProduction {
            block,
            transaction_count: count,
            committed: committed_blocks,
        })
    }

    /// Subscribe to durable commit events. Failed or merely proposed blocks
    /// never produce events.
    pub fn subscribe(&mut self) -> Receiver<PipelineEvent> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers.push(sender);
        receiver
    }

    /// Create a drop-aware commit subscription for network/transport adapters.
    pub(crate) fn subscribe_managed(&mut self) -> ManagedSubscription {
        self.managed_subscribers
            .retain(|subscriber| subscriber.active.load(Ordering::Acquire));
        let (sender, receiver) = mpsc::channel();
        let active = Arc::new(AtomicBool::new(true));
        self.managed_subscribers.push(ManagedSubscriber {
            sender,
            active: Arc::clone(&active),
        });
        ManagedSubscription { receiver, active }
    }

    /// Read a current account from the committed state.
    pub fn account(&self, address: &Address) -> Option<Account> {
        self.committed_accounts.get(address.as_bytes()).copied()
    }

    /// Read a durable receipt by transaction id.
    pub fn stored_receipt(&self, txid: &Hash) -> Result<Option<Vec<u8>>, PipelineError> {
        Ok(self.storage.get(StateKey::receipt(txid).as_bytes())?)
    }

    /// True when `txid` is currently queued in the D3 mempool.
    pub fn contains_pending_txid(&self, txid: &Hash) -> bool {
        self.mempool.contains_txid(txid)
    }

    /// Read one of the most recent committed blocks retained in memory.
    ///
    /// D4 keeps a bounded recent-block window; older history is not persisted
    /// until a later storage/checkpoint phase. `None` selects the newest block.
    pub fn committed_block(&self, height: Option<u64>) -> Option<Block> {
        match height {
            Some(height) => self
                .committed_blocks
                .iter()
                .find(|block| block.header.height.value() == height)
                .cloned(),
            None => self.committed_blocks.back().cloned(),
        }
    }

    /// Whether the local producer should advance to include or finalize transactions.
    ///
    /// Empty nil blocks are not generated when the pool is idle and no
    /// transaction-bearing proposal awaits the B3 three-chain commit.
    pub fn needs_block_production(&self) -> bool {
        !self.mempool.is_empty()
            || self
                .pending
                .values()
                .any(|pending| !pending.committed.receipts.is_empty())
    }

    /// Number of transactions currently waiting in the D3 mempool.
    pub fn mempool_len(&self) -> usize {
        self.mempool.len()
    }

    /// Configured maximum number of queued D3 transactions.
    pub fn mempool_capacity(&self) -> usize {
        self.mempool.capacity()
    }

    /// Last C5 block hash proposed by this process.
    pub fn proposed_tip(&self) -> Hash {
        self.proposed_tip
    }

    /// Last S2 storage head.
    pub fn storage_head(&self) -> aura_storage::Head {
        self.storage.head()
    }

    /// Highest block height committed to durable storage after finality.
    pub fn committed_height(&self) -> u64 {
        self.committed_blocks
            .back()
            .map(|block| block.header.height.value())
            .unwrap_or_default()
    }

    /// Number of proposed blocks that have not yet reached durable finality.
    pub fn finality_lag_blocks(&self) -> u64 {
        self.pending_order.len() as u64
    }

    /// Path backing the S2 WAL.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn root_after_pending(&self, changes: &[StateChange]) -> Result<Hash, PipelineError> {
        let mut entries = self
            .storage
            .entries()?
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        for hash in &self.pending_order {
            let pending = self
                .pending
                .get(hash)
                .ok_or(PipelineError::FinalityInvariant)?;
            apply_changes(&mut entries, &pending.changes);
        }
        apply_changes(&mut entries, changes);
        let version = self
            .storage
            .head()
            .version
            .checked_add(self.pending_order.len() as u64)
            .and_then(|value| value.checked_add(1))
            .ok_or(PipelineError::AmountOverflow)?;
        let leaves = entries
            .into_iter()
            .map(|(key, value)| (key_path(&key), value))
            .collect::<Vec<_>>();
        Ok(state_root(version, &leaves))
    }

    fn persist_finalized(
        &mut self,
        finalized: FinalizedBlock,
    ) -> Result<CommittedBlock, PipelineError> {
        let finality_hash = finalized.block.block_hash;
        let pending = self
            .pending
            .remove(&finality_hash)
            .ok_or(PipelineError::FinalityInvariant)?;
        if pending.finality_hash != finality_hash
            || self.pending_order.pop_front() != Some(finality_hash)
        {
            return Err(PipelineError::FinalityInvariant);
        }
        let storage_receipt = self.storage.commit(pending.changes)?;
        if storage_receipt.root != pending.committed.block.header.state_root {
            return Err(PipelineError::StateRootMismatch);
        }
        self.committed_accounts = pending.after_accounts;
        let committed = CommittedBlock {
            storage: storage_receipt,
            ..pending.committed
        };
        self.committed_blocks.push_back(committed.block.clone());
        while self.committed_blocks.len() > 256 {
            self.committed_blocks.pop_front();
        }
        let event = PipelineEvent {
            committed: committed.clone(),
        };
        self.subscribers
            .retain(|subscriber| subscriber.send(event.clone()).is_ok());
        self.managed_subscribers.retain(|subscriber| {
            subscriber.active.load(Ordering::Acquire)
                && subscriber.sender.send(event.clone()).is_ok()
        });
        Ok(committed)
    }
}

fn apply_changes(entries: &mut BTreeMap<Vec<u8>, Vec<u8>>, changes: &[StateChange]) {
    for change in changes {
        match &change.value {
            Some(value) => {
                entries.insert(change.key.clone(), value.clone());
            }
            None => {
                entries.remove(&change.key);
            }
        }
    }
}

fn account_changes(
    before: &BTreeMap<[u8; 20], Account>,
    after: &BTreeMap<[u8; 20], Account>,
) -> Vec<StateChange> {
    let addresses = before
        .keys()
        .chain(after.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut changes = Vec::new();
    for raw_address in addresses {
        let old = before.get(&raw_address).copied().unwrap_or_default();
        let new = after.get(&raw_address).copied().unwrap_or_default();
        if old != new {
            let address = Address::from_bytes(raw_address);
            changes.push(StateChange {
                key: StateKey::account(&address).as_bytes().to_vec(),
                value: Some(new.to_bytes()),
                nonce: 0,
            });
        }
    }
    changes
}

fn project_mempool_accounts(
    chain_id: &str,
    height: Height,
    proposer: Address,
    base_accounts: &BTreeMap<[u8; 20], Account>,
    mempool: &Mempool,
) -> Result<BTreeMap<[u8; 20], Account>, PipelineError> {
    let mut projected = base_accounts.clone();
    for tx in mempool.canonical_transactions() {
        tx.validate(chain_id, height)?;
        let _ = execute_transfer(&tx, height, proposer, &mut projected)?;
    }
    Ok(projected)
}

fn execute_transfer(
    tx: &Tx,
    height: Height,
    proposer: Address,
    accounts: &mut BTreeMap<[u8; 20], Account>,
) -> Result<TransactionReceipt, PipelineError> {
    if tx.unsigned.kind != TxKind::Transfer {
        return Err(PipelineError::UnsupportedTransactionKind(tx.unsigned.kind));
    }
    if tx.unsigned.payload.len() != TRANSFER_PAYLOAD_LEN {
        return Err(PipelineError::InvalidTransferPayload(
            tx.unsigned.payload.len(),
        ));
    }
    if tx.unsigned.gas_limit.value() < TRANSFER_GAS {
        return Err(PipelineError::GasLimitTooLow {
            required: TRANSFER_GAS,
            got: tx.unsigned.gas_limit.value(),
        });
    }
    let recipient = Address::from_bytes(
        tx.unsigned.payload[..20]
            .try_into()
            .map_err(|_| PipelineError::InvalidTransferPayload(tx.unsigned.payload.len()))?,
    );
    let amount = Amount::from_units(u128::from_be_bytes(
        tx.unsigned.payload[20..]
            .try_into()
            .map_err(|_| PipelineError::InvalidTransferPayload(tx.unsigned.payload.len()))?,
    ));
    let effective_price = D2_BASE_FEE
        .checked_add(tx.unsigned.tip)
        .ok_or(PipelineError::AmountOverflow)?;
    if tx.unsigned.max_fee < effective_price {
        return Err(PipelineError::FeeCapTooLow);
    }
    let total_fee = effective_price
        .checked_mul(TRANSFER_GAS as u128)
        .ok_or(PipelineError::AmountOverflow)?;
    let base_fee_burned = D2_BASE_FEE
        .checked_mul(TRANSFER_GAS as u128)
        .ok_or(PipelineError::AmountOverflow)?;
    let tip_paid = tx
        .unsigned
        .tip
        .checked_mul(TRANSFER_GAS as u128)
        .ok_or(PipelineError::AmountOverflow)?;
    let total_debit = amount
        .checked_add(total_fee)
        .ok_or(PipelineError::AmountOverflow)?;
    let sender = tx.unsigned.signer;
    let sender_key = *sender.as_bytes();
    let mut sender_account = accounts.get(&sender_key).copied().unwrap_or_default();
    if sender_account.nonce.value() != tx.unsigned.nonce.value() {
        return Err(PipelineError::NonceMismatch {
            expected: sender_account.nonce.value(),
            got: tx.unsigned.nonce.value(),
        });
    }
    if sender_account.balance < total_debit {
        return Err(PipelineError::InsufficientBalance);
    }
    sender_account
        .debit(total_debit)
        .map_err(|_| PipelineError::InsufficientBalance)?;
    sender_account
        .bump_nonce()
        .map_err(|_| PipelineError::AmountOverflow)?;
    accounts.insert(sender_key, sender_account);

    let recipient_key = *recipient.as_bytes();
    let mut recipient_account = accounts.get(&recipient_key).copied().unwrap_or_default();
    recipient_account
        .credit(amount)
        .map_err(|_| PipelineError::AmountOverflow)?;
    accounts.insert(recipient_key, recipient_account);

    if tip_paid != Amount::ZERO {
        let proposer_key = *proposer.as_bytes();
        let mut proposer_account = accounts.get(&proposer_key).copied().unwrap_or_default();
        proposer_account
            .credit(tip_paid)
            .map_err(|_| PipelineError::AmountOverflow)?;
        accounts.insert(proposer_key, proposer_account);
    }
    Ok(TransactionReceipt {
        txid: tx.txid(),
        signer: sender,
        nonce: tx.unsigned.nonce,
        recipient,
        amount,
        gas_used: Gas::new(TRANSFER_GAS),
        base_fee_burned,
        tip_paid,
        height,
    })
}

/// Encode the D2 native-transfer payload: `recipient[20] || amount_be[16]`.
pub fn encode_transfer_payload(recipient: Address, amount: Amount) -> Vec<u8> {
    let mut payload = Vec::with_capacity(TRANSFER_PAYLOAD_LEN);
    payload.extend_from_slice(recipient.as_bytes());
    payload.extend_from_slice(&amount.units().to_be_bytes());
    payload
}

fn encode_qc(qc: &QuorumCertificate) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.push_bytes(QC_MAGIC);
    writer.push_bytes(&qc.view.to_le_bytes());
    writer.push_bytes(&qc.round.to_le_bytes());
    writer.push_bytes(qc.block_hash.as_bytes());
    writer.push_byte(match qc.vote_type {
        VoteType::Propose => 0,
        VoteType::Prevote => 1,
        VoteType::Precommit => 2,
    });
    writer.push_bytes(&(qc.len() as u32).to_le_bytes());
    for (key, signature) in qc.public_keys.iter().zip(qc.signatures.iter()) {
        writer.push_bytes(&key.to_bytes());
        writer.push_bytes(&signature.to_bytes());
    }
    writer.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use aura_crypto::ed25519_public_key;
    use aura_tx::{TxParams, UnsignedTx, signer_address};

    const CHAIN: &str = "auralite-unit-1";
    const SEED: [u8; 32] = [0x31; 32];
    const RECIPIENT: Address = Address::from_bytes([0x52; 20]);
    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn sender() -> Address {
        signer_address(&ed25519_public_key(&SEED))
    }

    fn signed_transfer(
        chain: &str,
        nonce: u64,
        amount: u128,
        gas_limit: u64,
        max_fee: u128,
        tip: u128,
    ) -> Tx {
        UnsignedTx::new(TxParams {
            chain_id: ChainId::new(chain).unwrap(),
            signer: sender(),
            nonce: Nonce::new(nonce),
            kind: TxKind::Transfer,
            payload: encode_transfer_payload(RECIPIENT, Amount::from_units(amount)),
            gas_limit: Gas::new(gas_limit),
            max_fee: Amount::from_units(max_fee),
            tip: Amount::from_units(tip),
            expiry: Height::new(100),
        })
        .unwrap()
        .sign(&SEED)
        .unwrap()
    }

    fn temp_dir(label: &str) -> PathBuf {
        let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aura-d2-unit-{label}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn pipeline(label: &str) -> (Pipeline, PathBuf) {
        let path = temp_dir(label);
        let pipeline = Pipeline::open(PipelineConfig {
            chain_id: CHAIN.to_owned(),
            data_dir: path.clone(),
            validator_seed: [0xA7; 32],
            allocations: vec![(sender(), Amount::from_units(1_000_000))],
            genesis_time_ms: 1_700_000_000_000,
        })
        .unwrap();
        (pipeline, path)
    }

    #[test]
    fn rejects_replay_from_another_chain() {
        let (mut pipeline, path) = pipeline("chain");
        let result = pipeline.submit(signed_transfer(
            "auralite-other-1",
            0,
            1,
            TRANSFER_GAS,
            1,
            0,
        ));
        assert!(matches!(result, Err(PipelineError::InvalidTransaction(_))));
        drop(pipeline);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn rejects_nonce_gap() {
        let tx = signed_transfer(CHAIN, 1, 1, TRANSFER_GAS, 1, 0);
        let mut accounts = BTreeMap::from([(
            *sender().as_bytes(),
            Account::new(
                Nonce::new(0),
                Amount::from_units(100_000),
                Account::NO_CODE,
                Account::EMPTY_STORAGE_ROOT,
            ),
        )]);
        assert!(matches!(
            execute_transfer(&tx, Height::new(1), Address::ZERO, &mut accounts),
            Err(PipelineError::NonceMismatch {
                expected: 0,
                got: 1
            })
        ));
    }

    #[test]
    fn rejects_insufficient_balance_for_value_and_fee() {
        let tx = signed_transfer(CHAIN, 0, 1, TRANSFER_GAS, 1, 0);
        let mut accounts = BTreeMap::from([(
            *sender().as_bytes(),
            Account::new(
                Nonce::new(0),
                Amount::ONE,
                Account::NO_CODE,
                Account::EMPTY_STORAGE_ROOT,
            ),
        )]);
        assert_eq!(
            execute_transfer(&tx, Height::new(1), Address::ZERO, &mut accounts),
            Err(PipelineError::InsufficientBalance)
        );
    }

    #[test]
    fn rejects_transfer_below_gas_floor() {
        let tx = signed_transfer(CHAIN, 0, 1, TRANSFER_GAS - 1, 1, 0);
        let mut accounts = BTreeMap::new();
        assert!(matches!(
            execute_transfer(&tx, Height::new(1), Address::ZERO, &mut accounts),
            Err(PipelineError::GasLimitTooLow { .. })
        ));
    }

    #[test]
    fn rejects_fee_cap_below_base_fee_plus_tip() {
        let tx = signed_transfer(CHAIN, 0, 1, TRANSFER_GAS, 1, 1);
        let mut accounts = BTreeMap::new();
        assert_eq!(
            execute_transfer(&tx, Height::new(1), Address::ZERO, &mut accounts),
            Err(PipelineError::FeeCapTooLow)
        );
    }

    #[test]
    fn managed_subscriptions_are_pruned_after_disconnect() {
        let (mut pipeline, path) = pipeline("managed-subscription");
        let disconnected = pipeline.subscribe_managed();
        assert_eq!(pipeline.managed_subscribers.len(), 1);
        drop(disconnected);
        let active = pipeline.subscribe_managed();
        assert_eq!(pipeline.managed_subscribers.len(), 1);
        drop(active);
        drop(pipeline);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn receipt_is_durable_before_commit_event() {
        let (mut pipeline, path) = pipeline("receipt");
        let events = pipeline.subscribe();
        let txid = pipeline
            .submit(signed_transfer(CHAIN, 0, 1, TRANSFER_GAS, 1, 0))
            .unwrap();
        let _ = pipeline.produce_block().unwrap();
        let _ = pipeline.produce_block().unwrap();
        assert!(events.try_recv().is_err());
        let finalized = pipeline.produce_block().unwrap();
        assert_eq!(finalized.committed.len(), 1);
        assert!(pipeline.stored_receipt(&txid).unwrap().is_some());
        let event = events.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(event.committed.receipts[0].txid, txid);
        drop(pipeline);
        let _ = fs::remove_dir_all(path);
    }
}
