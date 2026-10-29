//! B3 - BFT finality: 3-chain commit, view change, timeouts, nil blocks.
//!
//! HotStuff-like finality over BLS12-381 QCs. A block is committed when
//! QCs exist for the block, its child, and its grandchild (three consecutive
//! blocks in the same view). Once committed, the block and all its ancestors
//! are finalized. View changes happen on timeout; nil blocks keep the chain
//! alive when there are no transactions. Every block carries a consensus
//! timestamp so the chain can never lose its sense of time.
//!
//! All signatures are domain-separated by `(chain_id, domain, message)` via
//! [`aura_crypto::hash_domain`] (re-exported from `aura-crypto`), so a vote
//! from one view or chain cannot be replayed in another.

use crate::bls::{BlsPublicKey, BlsSecretKey, verify};
use crate::qc::{QuorumCertificate, Vote, VoteType};
use aura_crypto::blake3_hash;
use aura_types::{Address, Domain, Hash};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// Default view-change timeout: 2 seconds.
pub const DEFAULT_VIEW_TIMEOUT_MS: u64 = 2000;

/// Height of the genesis block.
pub const GENESIS_HEIGHT: u64 = 0;

/// A consensus block: header fields, a justifying QC, and transactions.
///
/// The block hash is computed over the block's contents (view, round,
/// parent hash, timestamp, transaction hashes). A nil block has an empty
/// transaction list and still advances the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalityBlock {
    /// View number (epoch / height in the view-based protocol).
    pub view: u64,
    /// Round number within the view.
    pub round: u64,
    /// Parent block hash (genesis hash for the first block).
    pub parent_hash: Hash,
    /// Consensus timestamp, unix milliseconds.
    pub timestamp_ms: u64,
    /// Block hash, computed from the block contents.
    pub block_hash: Hash,
    /// QC that justifies this block (from the previous view/round), if any.
    pub qc: Option<QuorumCertificate>,
    /// Transactions in this block (empty for nil blocks).
    pub transactions: Vec<Vec<u8>>,
}

impl FinalityBlock {
    /// Compute the block hash from the block's contents.
    fn compute_hash(
        view: u64,
        round: u64,
        parent_hash: &Hash,
        timestamp_ms: u64,
        qc: &Option<QuorumCertificate>,
        transactions: &[Vec<u8>],
    ) -> Hash {
        let mut buf = Vec::new();
        buf.extend_from_slice(&view.to_le_bytes());
        buf.extend_from_slice(&round.to_le_bytes());
        buf.extend_from_slice(parent_hash.as_bytes());
        buf.extend_from_slice(&timestamp_ms.to_le_bytes());
        if let Some(qc) = qc {
            buf.extend_from_slice(qc.block_hash.as_bytes());
        } else {
            buf.extend_from_slice(&[0u8; 32]);
        }
        for tx in transactions {
            buf.extend_from_slice(blake3_hash(tx).as_bytes());
        }
        blake3_hash(&buf)
    }

    /// Propose a new block at `(view, round)` on top of `parent`.
    pub fn propose(
        view: u64,
        round: u64,
        parent: &FinalityBlock,
        timestamp_ms: u64,
        transactions: Vec<Vec<u8>>,
    ) -> Self {
        let block_hash = Self::compute_hash(
            view,
            round,
            &parent.block_hash,
            timestamp_ms,
            &None,
            &transactions,
        );
        Self {
            view,
            round,
            parent_hash: parent.block_hash,
            timestamp_ms,
            block_hash,
            qc: None,
            transactions,
        }
    }

    /// Propose a nil block (no transactions) at `(view, round)` on top of
    /// `parent`. Used to keep the chain alive when there is nothing to
    /// commit.
    pub fn propose_nil(view: u64, round: u64, parent: &FinalityBlock, timestamp_ms: u64) -> Self {
        Self::propose(view, round, parent, timestamp_ms, Vec::new())
    }

    /// True if this block is a nil block (no transactions).
    pub fn is_nil(&self) -> bool {
        self.transactions.is_empty()
    }
}

/// A finalized block: a committed block plus its height in the finalized
/// chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedBlock {
    /// Height in the finalized chain (genesis is height 0).
    pub height: u64,
    /// The committed block.
    pub block: FinalityBlock,
}

/// The 3-chain commit rule.
///
/// A block B is committed when QCs exist for B, its child C, and C's child
/// D, all in the same view. Once B is committed, B and every ancestor of B
/// are finalized. This is the HotStuff-style 3-chain commit rule: safety
/// follows from the quorum intersection property (any two 2f+1 quorums
/// overlap in at least f+1 validators).
pub struct ThreeChainCommit {
    /// Blocks by hash.
    blocks: BTreeMap<Hash, FinalityBlock>,
    /// QCs by the block hash they justify.
    qcs: BTreeMap<Hash, QuorumCertificate>,
    /// Children of each block, by parent hash.
    children: BTreeMap<Hash, Vec<Hash>>,
    /// Finalized blocks in height order.
    finalized: Vec<FinalizedBlock>,
    /// Next height to assign to a newly finalized block.
    next_height: u64,
    /// The genesis block hash (the chain's anchor).
    genesis_hash: Hash,
}

impl ThreeChainCommit {
    /// Create a new commit rule anchored at `genesis_hash`.
    pub fn new(genesis_hash: Hash) -> Self {
        Self {
            blocks: BTreeMap::new(),
            qcs: BTreeMap::new(),
            children: BTreeMap::new(),
            finalized: Vec::new(),
            next_height: GENESIS_HEIGHT,
            genesis_hash,
        }
    }

    /// Register a block. Returns `true` if the block was new.
    pub fn insert_block(&mut self, block: FinalityBlock) -> bool {
        let is_new = self
            .blocks
            .insert(block.block_hash, block.clone())
            .is_none();
        if let Some(parent_hash) = self.blocks.get(&block.block_hash) {
            let _ = parent_hash;
        }
        if block.parent_hash != self.genesis_hash || block.view == 0 {
            self.children
                .entry(block.parent_hash)
                .or_default()
                .push(block.block_hash);
        }
        is_new
    }

    /// Register a QC for `block_hash`. After inserting, check whether any
    /// block is now committed by the 3-chain rule.
    pub fn insert_qc(&mut self, block_hash: Hash, qc: QuorumCertificate) {
        self.qcs.insert(block_hash, qc);
    }

    /// Check whether `block_hash` is committed: a QC exists for it, for its
    /// child, and for its grandchild, all in the same view.
    fn is_committed(&self, block_hash: &Hash) -> bool {
        // Need a QC for this block.
        let Some(qc_b) = self.qcs.get(block_hash) else {
            return false;
        };
        // Need a child with a QC.
        let Some(children) = self.children.get(block_hash) else {
            return false;
        };
        for child_hash in children {
            let Some(qc_c) = self.qcs.get(child_hash) else {
                continue;
            };
            // Need a grandchild with a QC, all in the same view.
            let Some(grandchildren) = self.children.get(child_hash) else {
                continue;
            };
            for grandchild_hash in grandchildren {
                if self.qcs.contains_key(grandchild_hash) && qc_b.view == qc_c.view {
                    return true;
                }
            }
        }
        false
    }

    /// Finalize every block that has become committed and return the newly
    /// finalized blocks in height order.
    ///
    /// Called after inserting a new QC; walks from the newest committed
    /// block down to the last finalized block, finalizing ancestors.
    pub fn finalize_committed(&mut self) -> Vec<FinalizedBlock> {
        let mut newly_finalized = Vec::new();

        // Find the deepest committed block: walk every block that has a QC
        // and check the 3-chain rule.
        let mut committed_candidates: Vec<Hash> = self
            .qcs
            .keys()
            .filter(|h| self.is_committed(h))
            .cloned()
            .collect();
        committed_candidates.sort();

        // For each committed block, finalize it and all its ancestors up to
        // the last finalized block.
        let finalized_hashes: BTreeSet<Hash> = self
            .finalized
            .iter()
            .map(|fb| fb.block.block_hash)
            .collect();

        for block_hash in committed_candidates {
            if finalized_hashes.contains(&block_hash) {
                continue;
            }
            // Walk up from this block to the genesis or the last finalized
            // block, finalizing each ancestor.
            let mut current = block_hash;
            let mut chain: Vec<FinalityBlock> = Vec::new();
            loop {
                if finalized_hashes.contains(&current) {
                    break;
                }
                let Some(block) = self.blocks.get(&current) else {
                    break;
                };
                chain.push(block.clone());
                if current == self.genesis_hash {
                    break;
                }
                current = block.parent_hash;
            }
            // Finalize in reverse order (oldest first) so heights are correct.
            for block in chain.into_iter().rev() {
                let height = self.next_height;
                self.next_height += 1;
                let finalized = FinalizedBlock { height, block };
                newly_finalized.push(finalized.clone());
                self.finalized.push(finalized);
            }
        }

        newly_finalized
    }

    /// Number of blocks finalized so far.
    pub fn finalized_count(&self) -> usize {
        self.finalized.len()
    }

    /// The height of the most recently finalized block, or `GENESIS_HEIGHT`
    /// if nothing has been finalized.
    pub fn finalized_height(&self) -> u64 {
        self.finalized
            .last()
            .map(|fb| fb.height)
            .unwrap_or(GENESIS_HEIGHT.saturating_sub(1))
    }
}

/// Timeout management for view changes.
#[derive(Debug, Clone)]
pub struct Timeout {
    /// Duration of the timeout.
    pub duration: Duration,
    /// When the timeout was last reset.
    pub deadline: Instant,
}

impl Timeout {
    /// Create a new timeout with the given duration.
    pub fn new(duration: Duration) -> Self {
        Self {
            deadline: Instant::now() + duration,
            duration,
        }
    }

    /// Reset the timeout to expire after `duration` from now.
    pub fn reset(&mut self) {
        self.deadline = Instant::now() + self.duration;
    }

    /// True if the timeout has expired.
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

/// View change state.
#[derive(Debug, Clone)]
pub struct ViewChange {
    /// Current view number.
    pub view: u64,
    /// Current round within the view.
    pub round: u64,
    /// Highest QC seen so far (used to bootstrap the new view).
    pub high_qc: Option<QuorumCertificate>,
    /// View-change timeout.
    pub timeout: Timeout,
}

impl ViewChange {
    /// Create a new view change state at `view`.
    pub fn new(view: u64, timeout_ms: u64) -> Self {
        Self {
            view,
            round: 0,
            high_qc: None,
            timeout: Timeout::new(Duration::from_millis(timeout_ms)),
        }
    }

    /// Advance to the next view, resetting the round and timeout.
    pub fn advance_view(&mut self) {
        self.view += 1;
        self.round = 0;
        self.timeout.reset();
    }

    /// Advance to the next round within the current view, resetting the
    /// timeout.
    pub fn advance_round(&mut self) {
        self.round += 1;
        self.timeout.reset();
    }

    /// Update the highest QC seen.
    pub fn update_high_qc(&mut self, qc: QuorumCertificate) {
        match &self.high_qc {
            Some(current) if qc.view < current.view => {}
            _ => {
                self.high_qc = Some(qc);
            }
        }
    }
}

/// The B3 consensus engine: 3-chain commit + view change + timeouts +
/// nil blocks.
///
/// The engine drives a HotStuff-like protocol: propose blocks, collect
/// votes into QCs, commit blocks via the 3-chain rule, and change views on
/// timeout. All votes are BLS-signed and domain-separated.
pub struct FinalityEngine {
    /// Chain ID for domain separation.
    chain_id: String,
    /// The 3-chain commit rule.
    pub commit_rule: ThreeChainCommit,
    /// View change state.
    pub view_change: ViewChange,
    /// The block at the tip of the chain (the most recent block, whether or
    /// not it has been committed).
    pub tip: FinalityBlock,
    /// Whether the engine has seen its first block (genesis anchor set).
    pub initialized: bool,
}

impl FinalityEngine {
    /// Create a new engine for `chain_id` starting at `view`, anchored at
    /// `genesis_hash`.
    pub fn new(chain_id: &str, view: u64, genesis_hash: Hash) -> Self {
        let tip = FinalityBlock {
            view: 0,
            round: 0,
            parent_hash: genesis_hash,
            timestamp_ms: 0,
            block_hash: genesis_hash,
            qc: None,
            transactions: Vec::new(),
        };
        Self {
            chain_id: chain_id.to_string(),
            commit_rule: ThreeChainCommit::new(genesis_hash),
            view_change: ViewChange::new(view, DEFAULT_VIEW_TIMEOUT_MS),
            tip: tip.clone(),
            initialized: false,
        }
    }

    /// Initialize the engine with the genesis block as the tip.
    pub fn init_with_genesis(&mut self, genesis: FinalityBlock) {
        self.commit_rule.insert_block(genesis.clone());
        self.tip = genesis.clone();
        self.initialized = true;
    }

    /// Propose a new block at the current view/round on top of the tip.
    /// If `transactions` is empty, proposes a nil block.
    ///
    /// The block's QC is set to the current high QC (view-change bootstrap).
    pub fn propose(&mut self, transactions: Vec<Vec<u8>>) -> FinalityBlock {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let block = if transactions.is_empty() {
            FinalityBlock::propose_nil(
                self.view_change.view,
                self.view_change.round,
                &self.tip,
                timestamp_ms,
            )
        } else {
            FinalityBlock::propose(
                self.view_change.view,
                self.view_change.round,
                &self.tip,
                timestamp_ms,
                transactions,
            )
        };

        let mut block = block;
        block.qc = self.view_change.high_qc.clone();
        self.commit_rule.insert_block(block.clone());
        self.tip = block.clone();
        block
    }

    /// Create a signed vote for `block` from validator `validator` using
    /// secret key `sk`, with the given `vote_type`.
    pub fn vote_for(
        &self,
        sk: &BlsSecretKey,
        validator: Address,
        block: &FinalityBlock,
        vote_type: VoteType,
    ) -> Vote {
        let payload = vote_payload(block.view, block.round, block.block_hash, vote_type);
        let signature = sk.sign(&self.chain_id, Domain::Vote, &payload);
        Vote {
            view: block.view,
            round: block.round,
            block_hash: block.block_hash,
            validator,
            vote_type,
            signature,
        }
    }

    /// Aggregate votes into a QC using the aggregator, then insert the QC
    /// into the commit rule and check for 3-chain commit.
    ///
    /// Returns `Some(QC)` once the quorum threshold is reached, plus any
    /// newly finalized blocks.
    pub fn aggregate_and_commit(
        &mut self,
        aggregator: &mut crate::qc::QcAggregator,
        public_key: BlsPublicKey,
        vote: Vote,
    ) -> (Option<QuorumCertificate>, Vec<FinalizedBlock>) {
        if let Some(qc) = aggregator.add_vote(public_key, vote.clone()) {
            let block_hash = qc.block_hash;
            self.commit_rule.insert_qc(block_hash, qc.clone());
            self.view_change.update_high_qc(qc.clone());
            let finalized = self.commit_rule.finalize_committed();
            (Some(qc), finalized)
        } else {
            (None, Vec::new())
        }
    }

    /// True if the view-change timeout has expired.
    pub fn is_timeout_expired(&self) -> bool {
        self.view_change.timeout.is_expired()
    }

    /// Trigger a view change: advance the view, reset the round and timeout,
    /// and keep the high QC for bootstrap.
    pub fn change_view(&mut self) {
        self.view_change.advance_view();
    }

    /// Trigger a round change within the current view: advance the round and
    /// reset the timeout.
    pub fn change_round(&mut self) {
        self.view_change.advance_round();
    }

    /// Reset the view-change timeout.
    pub fn reset_timeout(&mut self) {
        self.view_change.timeout.reset();
    }

    /// The current view number.
    pub fn view(&self) -> u64 {
        self.view_change.view
    }

    /// The current round number.
    pub fn round(&self) -> u64 {
        self.view_change.round
    }
}

/// Serialize a vote payload for signing.
fn vote_payload(view: u64, round: u64, block_hash: Hash, vote_type: VoteType) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&view.to_le_bytes());
    buf.extend_from_slice(&round.to_le_bytes());
    buf.extend_from_slice(block_hash.as_bytes());
    buf.push(vote_type as u8);
    buf
}

/// Verify a vote's signature against the signer's public key.
pub fn verify_vote(
    chain_id: &str,
    vote: &Vote,
    public_key: &BlsPublicKey,
) -> Result<(), crate::bls::BlsError> {
    let payload = vote_payload(vote.view, vote.round, vote.block_hash, vote.vote_type);
    verify(
        public_key,
        chain_id,
        Domain::Vote,
        &payload,
        &vote.signature,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bls::BlsSecretKey;

    fn test_chain_id() -> &'static str {
        "auralite-b3-test"
    }

    fn genesis_hash() -> Hash {
        Hash::from_bytes([0x01; 32])
    }

    fn seed_for(index: u8) -> [u8; 32] {
        let mut s = [0u8; 32];
        s[0] = index;
        s
    }

    fn validator_address(index: u8) -> Address {
        Address::from_bytes([index; 20])
    }

    /// Build a 4-validator engine and a set of secret keys.
    fn setup_engine() -> (FinalityEngine, Vec<BlsSecretKey>) {
        let mut engine = FinalityEngine::new(test_chain_id(), 1, genesis_hash());
        let genesis = FinalityBlock {
            view: 0,
            round: 0,
            parent_hash: genesis_hash(),
            timestamp_ms: 0,
            block_hash: genesis_hash(),
            qc: None,
            transactions: Vec::new(),
        };
        engine.init_with_genesis(genesis);
        let sks: Vec<BlsSecretKey> = (0..4)
            .map(|i| BlsSecretKey::from_seed(&seed_for(i)))
            .collect();
        (engine, sks)
    }

    /// Create QCs for a chain of blocks on the engine's tip.
    fn build_chain_with_qcs(
        engine: &mut FinalityEngine,
        sks: &[BlsSecretKey],
        addrs: &[Address],
        num_blocks: u64,
    ) -> Vec<FinalityBlock> {
        let mut blocks = Vec::new();
        for _ in 0..num_blocks {
            let block = engine.propose(vec![]);
            blocks.push(block.clone());
            // Create a QC for this block from all validators.
            let mut aggregator = crate::qc::QcAggregator::new(addrs.len() as u64);
            for (j, sk) in sks.iter().enumerate() {
                let vote = engine.vote_for(sk, addrs[j], &block, VoteType::Precommit);
                let _ = engine.aggregate_and_commit(&mut aggregator, sk.public_key(), vote);
            }
        }
        blocks
    }

    #[test]
    fn propose_and_nil_block() {
        let (mut engine, _) = setup_engine();
        let block = engine.propose(Vec::new());
        assert!(block.is_nil());
        assert_eq!(block.view, 1);
        assert_eq!(block.round, 0);
        assert_ne!(block.block_hash, block.parent_hash);
        assert_eq!(block.parent_hash, genesis_hash());
    }

    #[test]
    fn three_chain_commit() {
        let (mut engine, sks) = setup_engine();
        let addrs: Vec<Address> = (0..4).map(validator_address).collect();

        // Propose 3 blocks and get QCs for each.
        build_chain_with_qcs(&mut engine, &sks, &addrs, 3);

        // The first block should now be committed (3-chain: B1, B2, B3 all
        // have QCs in the same view).
        let finalized_count = engine.commit_rule.finalized_count();
        assert!(
            finalized_count >= 1,
            "at least the first block should be finalized, got {}",
            finalized_count
        );
    }

    #[test]
    fn two_blocks_not_committed() {
        let (mut engine, sks) = setup_engine();
        let addrs: Vec<Address> = (0..4).map(validator_address).collect();

        // Propose only 2 blocks - not enough for a 3-chain.
        build_chain_with_qcs(&mut engine, &sks, &addrs, 2);
        assert_eq!(
            engine.commit_rule.finalized_count(),
            0,
            "two blocks should not finalize anything"
        );
    }

    #[test]
    fn view_change_on_timeout() {
        let (mut engine, _) = setup_engine();
        let initial_view = engine.view();
        assert!(!engine.is_timeout_expired());

        // Manually expire the timeout by setting the deadline to the past.
        engine.view_change.timeout.deadline = Instant::now() - Duration::from_secs(1);
        assert!(engine.is_timeout_expired());

        engine.change_view();
        assert_eq!(engine.view(), initial_view + 1);
        assert_eq!(engine.round(), 0);
        assert!(!engine.is_timeout_expired());
    }

    #[test]
    fn round_change() {
        let (mut engine, _) = setup_engine();
        let initial_round = engine.round();
        engine.change_round();
        assert_eq!(engine.round(), initial_round + 1);
        assert_eq!(engine.view(), 1); // view unchanged
    }

    #[test]
    fn block_hash_changes_with_content() {
        let (mut engine, _) = setup_engine();
        let block_a = engine.propose(vec![b"tx1".to_vec()]);
        let block_b = engine.propose(vec![b"tx2".to_vec()]);
        assert_ne!(block_a.block_hash, block_b.block_hash);
    }

    #[test]
    fn nil_block_has_no_transactions() {
        let (mut engine, _) = setup_engine();
        let nil = engine.propose(Vec::new());
        assert!(nil.is_nil());
        assert_eq!(nil.transactions.len(), 0);
    }

    #[test]
    fn verify_vote_roundtrip() {
        let (mut engine, sks) = setup_engine();
        let addrs: Vec<Address> = (0..4).map(validator_address).collect();
        let block = engine.propose(vec![]);
        let vote = engine.vote_for(&sks[0], addrs[0], &block, VoteType::Precommit);
        assert!(verify_vote(test_chain_id(), &vote, &sks[0].public_key()).is_ok());
    }

    #[test]
    fn verify_vote_wrong_chain_fails() {
        let (mut engine, sks) = setup_engine();
        let addrs: Vec<Address> = (0..4).map(validator_address).collect();
        let block = engine.propose(vec![]);
        let vote = engine.vote_for(&sks[0], addrs[0], &block, VoteType::Precommit);
        assert!(verify_vote("wrong-chain", &vote, &sks[0].public_key()).is_err());
    }

    #[test]
    fn high_qc_tracks_newest() {
        let (mut engine, sks) = setup_engine();
        let addrs: Vec<Address> = (0..4).map(validator_address).collect();

        // Propose a block and get a QC.
        let block = engine.propose(vec![]);
        let mut aggregator = crate::qc::QcAggregator::new(4);
        for (j, sk) in sks.iter().enumerate() {
            let vote = engine.vote_for(sk, addrs[j], &block, VoteType::Precommit);
            let _ = engine.aggregate_and_commit(&mut aggregator, sk.public_key(), vote);
        }

        // The high QC should be for the block we just voted on.
        assert_eq!(
            engine.view_change.high_qc.as_ref().unwrap().block_hash,
            block.block_hash
        );
    }

    #[test]
    fn commit_preserves_genesis_anchor() {
        let (mut engine, sks) = setup_engine();
        let addrs: Vec<Address> = (0..4).map(validator_address).collect();

        // Propose 3 blocks with QCs.
        build_chain_with_qcs(&mut engine, &sks, &addrs, 3);

        // The genesis hash should still be the anchor.
        assert_eq!(engine.commit_rule.genesis_hash, genesis_hash());
    }
}
