//! B8 — Finality, fork choice, finalized checkpoints, consensus light-client
//! proofs, and deep-reorg defence (`D-B3`, `D-B4`): any attempt to reorganize
//! a finalized block is rejected and produces cryptographic [`SlashingEvidence`]
//! against the offending signer.
//!
//! Experimental only: fork scores contain an unauthenticated justified-QC view,
//! and this tree does not yet bind each validator address to its Ed25519 key.
//! It must not be used as a production fork-choice or slashing authority.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use aura_types::{Address, ChainId, Domain, Hash};

use crate::finality::FinalityBlock;
use crate::qc::{QuorumCertificate, Vote, VoteType, quorum_threshold};
use crate::signature::BlockSignature;
use crate::slashing::SlashingEvidence;

/// Default weak-subjectivity / unbonding window in blocks (`D-B3`).
pub const DEFAULT_WEAK_SUBJECTIVITY_WINDOW: u64 = 10_000;

/// Errors returned by the B8 fork-choice and deep-reorg defence engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkChoiceError {
    /// Block's parent hash is unknown in the fork-choice tree.
    UnknownParent(Hash),
    /// Checkpoint or proof QC failed BLS verification or lacks `2f+1` signatures.
    InvalidCheckpointQc(&'static str),
    /// Checkpoint height/view regresses below the current finalized checkpoint.
    CheckpointRegression {
        /// Current finalized height.
        current_height: u64,
        /// Proposed checkpoint height.
        proposed_height: u64,
    },
    /// Checkpoint age exceeds the weak-subjectivity window (`D-B3`).
    WeakSubjectivityExpired {
        /// Checkpoint height.
        checkpoint_height: u64,
        /// Current head height.
        head_height: u64,
        /// Configured weak-subjectivity window.
        window: u64,
    },
    /// Attempted to reorg a finalized block (`attempt to reorg a finalized block -> reject + evidence`).
    DeepReorgPastFinalized {
        /// Finalized checkpoint height.
        finalized_height: u64,
        /// Finalized checkpoint block hash.
        finalized_hash: Hash,
        /// Conflicting block height.
        conflicting_height: u64,
        /// Conflicting block hash.
        conflicting_hash: Hash,
    },
    /// Light-client finality proof failed 3-chain or QC checks.
    InvalidLightClientProof(&'static str),
    /// Candidate hash, signature, or height is inconsistent.
    InvalidCandidate(&'static str),
}

impl fmt::Display for ForkChoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParent(h) => write!(f, "unknown parent block {h}"),
            Self::InvalidCheckpointQc(msg) => write!(f, "invalid checkpoint QC: {msg}"),
            Self::CheckpointRegression {
                current_height,
                proposed_height,
            } => write!(
                f,
                "checkpoint height regression: proposed {proposed_height} < current {current_height}"
            ),
            Self::WeakSubjectivityExpired {
                checkpoint_height,
                head_height,
                window,
            } => write!(
                f,
                "weak subjectivity expired: checkpoint {checkpoint_height} is older than head {head_height} - window {window}"
            ),
            Self::DeepReorgPastFinalized {
                finalized_height,
                finalized_hash,
                conflicting_height,
                conflicting_hash,
            } => write!(
                f,
                "deep reorg rejected: conflicting block {conflicting_hash} at height {conflicting_height} conflicts with finalized checkpoint {finalized_hash} at height {finalized_height}"
            ),
            Self::InvalidLightClientProof(msg) => write!(f, "invalid light-client proof: {msg}"),
            Self::InvalidCandidate(msg) => write!(f, "invalid fork candidate: {msg}"),
        }
    }
}

impl std::error::Error for ForkChoiceError {}

/// A finalized consensus checkpoint anchored by a `2f+1` [`QuorumCertificate`] (`D-B3`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedCheckpoint {
    /// Finalized block height.
    pub height: u64,
    /// Consensus view at which the checkpoint was finalized.
    pub view: u64,
    /// Finalized block hash.
    pub block_hash: Hash,
    /// Executed state root at `height`.
    pub state_root: Hash,
    /// `2f+1` commit QC certifying `block_hash`.
    pub commit_qc: QuorumCertificate,
}

impl FinalizedCheckpoint {
    /// Verify the checkpoint QC against a trusted active committee. A single QC
    /// does not by itself prove finality, height, or the claimed state root.
    pub fn verify(
        &self,
        chain_id: &ChainId,
        active: &BTreeSet<Address>,
    ) -> Result<(), ForkChoiceError> {
        let required = quorum_threshold(active.len() as u64) as usize;
        if self.commit_qc.view != self.view || self.commit_qc.vote_type != VoteType::Precommit {
            return Err(ForkChoiceError::InvalidCheckpointQc(
                "checkpoint must have a matching-view precommit QC",
            ));
        }
        if self.commit_qc.block_hash != self.block_hash {
            return Err(ForkChoiceError::InvalidCheckpointQc(
                "commit_qc block_hash does not match checkpoint block_hash",
            ));
        }
        if self.commit_qc.len() < required
            || self.commit_qc.public_keys.len() != self.commit_qc.signatures.len()
        {
            return Err(ForkChoiceError::InvalidCheckpointQc(
                "commit_qc has fewer than 2f+1 signatures",
            ));
        }
        self.commit_qc
            .verify_with_members(chain_id.as_str(), Domain::Vote, active)
            .map_err(|_| {
                ForkChoiceError::InvalidCheckpointQc("commit_qc BLS verification failed")
            })?;
        Ok(())
    }
}

/// Three-chain proof; the state root requires an independently authenticated anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsensusLightClientProof {
    /// Committed target block `B`.
    pub target_block: FinalityBlock,
    /// Claimed state root; not committed by `FinalityBlock` and not independently authenticated here.
    pub state_root: Hash,
    /// Child block `B'` whose `parent_hash == target_block.block_hash`.
    pub child_block: FinalityBlock,
    /// Grandchild block `B''` whose `parent_hash == child_block.block_hash`.
    pub grandchild_block: FinalityBlock,
    /// QC for `target_block` (`B`).
    pub target_qc: QuorumCertificate,
    /// QC for `child_block` (`B'`).
    pub child_qc: QuorumCertificate,
    /// QC for `grandchild_block` (`B''`).
    pub grandchild_qc: QuorumCertificate,
}

impl ConsensusLightClientProof {
    /// Check the 3-chain against a trusted active committee and an independently
    /// authenticated state root. The current block format does not commit to
    /// `state_root`; this method cannot itself authenticate that root.
    pub fn verify(
        &self,
        chain_id: &ChainId,
        active: &BTreeSet<Address>,
        trusted_state_root: Hash,
    ) -> Result<(), ForkChoiceError> {
        if self.state_root != trusted_state_root {
            return Err(ForkChoiceError::InvalidLightClientProof(
                "state root differs from trusted root",
            ));
        }
        if self.child_block.parent_hash != self.target_block.block_hash
            || self.grandchild_block.parent_hash != self.child_block.block_hash
        {
            return Err(ForkChoiceError::InvalidLightClientProof(
                "broken parent_hash chain across target -> child -> grandchild",
            ));
        }
        if self.target_qc.block_hash != self.target_block.block_hash
            || self.child_qc.block_hash != self.child_block.block_hash
            || self.grandchild_qc.block_hash != self.grandchild_block.block_hash
        {
            return Err(ForkChoiceError::InvalidLightClientProof(
                "QC block_hash mismatch in 3-chain proof",
            ));
        }
        if self.target_qc.view != self.child_qc.view
            || self.child_qc.view != self.grandchild_qc.view
        {
            return Err(ForkChoiceError::InvalidLightClientProof(
                "3-chain QCs must share the same view",
            ));
        }
        for (block, qc) in [
            (&self.target_block, &self.target_qc),
            (&self.child_block, &self.child_qc),
            (&self.grandchild_block, &self.grandchild_qc),
        ] {
            if !block.has_valid_hash()
                || qc.view != block.view
                || qc.round != block.round
                || qc.vote_type != VoteType::Precommit
            {
                return Err(ForkChoiceError::InvalidLightClientProof(
                    "block hash or QC context mismatch",
                ));
            }
        }
        if self.target_block.round.checked_add(1) != Some(self.child_block.round)
            || self.child_block.round.checked_add(1) != Some(self.grandchild_block.round)
        {
            return Err(ForkChoiceError::InvalidLightClientProof(
                "3-chain rounds must be adjacent",
            ));
        }
        let required = quorum_threshold(active.len() as u64) as usize;
        for qc in [&self.target_qc, &self.child_qc, &self.grandchild_qc] {
            if qc.len() < required || qc.public_keys.len() != qc.signatures.len() {
                return Err(ForkChoiceError::InvalidLightClientProof(
                    "QC in 3-chain proof has fewer than 2f+1 signatures",
                ));
            }
            qc.verify_with_members(chain_id.as_str(), Domain::Vote, active)
                .map_err(|_| {
                    ForkChoiceError::InvalidLightClientProof(
                        "QC BLS signature verification failed in 3-chain proof",
                    )
                })?;
        }
        Ok(())
    }
}

/// Signed block candidate submitted to [`ForkChoiceTree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedForkBlock {
    /// Block height (`0` for genesis).
    pub height: u64,
    /// Consensus block.
    pub block: FinalityBlock,
    /// Highest QC view justified by this block.
    pub justified_qc_view: u64,
    /// Proposer's Ed25519 block signature.
    pub proposer_sig: BlockSignature,
    /// Optional validator precommit vote accompanying this block.
    pub attester_vote: Option<Vote>,
}

/// Outcome of submitting a candidate block to [`ForkChoiceTree`] when a deep reorg
/// against a finalized block is attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReorgRejection {
    /// Specific deep-reorg error.
    pub error: ForkChoiceError,
    /// Cryptographic [`SlashingEvidence`] generated from the conflicting finalized
    /// and reorg signatures, ready for [`crate::slashing::SlashingEngine::submit_evidence`].
    pub evidence: SlashingEvidence,
}

/// B8 Fork-Choice & Deep-Reorg Defence engine (`D-B3`, `D-B4`).
#[derive(Debug, Clone)]
pub struct ForkChoiceTree {
    chain_id: ChainId,
    active: BTreeSet<Address>,
    weak_subjectivity_window: u64,
    blocks: BTreeMap<Hash, SignedForkBlock>,
    canonical_by_height: BTreeMap<u64, Hash>,
    finalized_checkpoint: FinalizedCheckpoint,
    emitted_evidence: Vec<SlashingEvidence>,
}

impl ForkChoiceTree {
    /// Initialize a [`ForkChoiceTree`] anchored at `genesis` and `genesis_checkpoint`.
    pub fn new(
        chain_id: ChainId,
        active: BTreeSet<Address>,
        genesis: SignedForkBlock,
        genesis_checkpoint: FinalizedCheckpoint,
    ) -> Self {
        let genesis_hash = genesis.block.block_hash;
        let mut blocks = BTreeMap::new();
        blocks.insert(genesis_hash, genesis);
        let mut canonical_by_height = BTreeMap::new();
        canonical_by_height.insert(0, genesis_hash);
        Self {
            chain_id,
            active,
            weak_subjectivity_window: DEFAULT_WEAK_SUBJECTIVITY_WINDOW,
            blocks,
            canonical_by_height,
            finalized_checkpoint: genesis_checkpoint,
            emitted_evidence: Vec::new(),
        }
    }

    /// Override the weak-subjectivity window in blocks (`D-B3`).
    pub fn with_weak_subjectivity_window(mut self, window: u64) -> Self {
        self.weak_subjectivity_window = window;
        self
    }

    /// Borrow the latest [`FinalizedCheckpoint`].
    pub const fn finalized_checkpoint(&self) -> &FinalizedCheckpoint {
        &self.finalized_checkpoint
    }

    /// Borrow all [`SlashingEvidence`] items emitted by rejected deep-reorg attempts.
    pub fn emitted_evidence(&self) -> &[SlashingEvidence] {
        &self.emitted_evidence
    }

    /// Check whether `block_hash` descends from (or equals) `ancestor_hash`.
    pub fn descends_from(&self, mut block_hash: Hash, ancestor_hash: Hash) -> bool {
        let mut seen = BTreeSet::new();
        while let Some(node) = self.blocks.get(&block_hash) {
            if !seen.insert(block_hash) {
                return false;
            }
            if block_hash == ancestor_hash {
                return true;
            }
            if node.height == 0 || node.block.parent_hash == block_hash {
                break;
            }
            block_hash = node.block.parent_hash;
        }
        false
    }

    /// Select the canonical head among all known blocks that **descend from
    /// `finalized_checkpoint.block_hash`**, ordered by:
    /// `(justified_qc_view, height)` descending, tie-broken by lowest `block_hash`.
    pub fn select_canonical_head(&self) -> Hash {
        let finalized_hash = self.finalized_checkpoint.block_hash;
        let mut best_hash = finalized_hash;
        let mut best_score = self
            .blocks
            .get(&finalized_hash)
            .map(|b| (b.justified_qc_view, b.height))
            .unwrap_or((0, self.finalized_checkpoint.height));

        for (&hash, candidate) in &self.blocks {
            if !self.descends_from(hash, finalized_hash) {
                continue;
            }
            let score = (candidate.justified_qc_view, candidate.height);
            if score > best_score || (score == best_score && hash < best_hash) {
                best_score = score;
                best_hash = hash;
            }
        }
        best_hash
    }

    /// Advance only with a verified three-chain proof for the same block and an
    /// independently trusted state root. The caller must authenticate the root
    /// separately: `FinalityBlock` does not currently commit it.
    pub fn update_finalized_checkpoint(
        &mut self,
        checkpoint: FinalizedCheckpoint,
        proof: &ConsensusLightClientProof,
        trusted_state_root: Hash,
        current_head_height: u64,
    ) -> Result<(), ForkChoiceError> {
        if checkpoint.height < self.finalized_checkpoint.height {
            return Err(ForkChoiceError::CheckpointRegression {
                current_height: self.finalized_checkpoint.height,
                proposed_height: checkpoint.height,
            });
        }
        if current_head_height.saturating_sub(checkpoint.height) > self.weak_subjectivity_window {
            return Err(ForkChoiceError::WeakSubjectivityExpired {
                checkpoint_height: checkpoint.height,
                head_height: current_head_height,
                window: self.weak_subjectivity_window,
            });
        }
        checkpoint.verify(&self.chain_id, &self.active)?;
        proof.verify(&self.chain_id, &self.active, trusted_state_root)?;
        if checkpoint.block_hash != proof.target_block.block_hash
            || checkpoint.state_root != trusted_state_root
        {
            return Err(ForkChoiceError::InvalidCheckpointQc(
                "checkpoint does not match verified proof and trusted root",
            ));
        }
        let entry =
            self.blocks
                .get(&checkpoint.block_hash)
                .ok_or(ForkChoiceError::InvalidCheckpointQc(
                    "checkpoint block is unknown",
                ))?;
        if entry.height != checkpoint.height
            || entry.block.view != checkpoint.view
            || !self.descends_from(checkpoint.block_hash, self.finalized_checkpoint.block_hash)
        {
            return Err(ForkChoiceError::InvalidCheckpointQc(
                "checkpoint height, view, or ancestry mismatch",
            ));
        }

        // Walk ancestors from checkpoint back to previous finalized checkpoint to index canonical heights.
        let mut cursor = checkpoint.block_hash;
        while let Some(entry) = self.blocks.get(&cursor) {
            self.canonical_by_height.insert(entry.height, cursor);
            if entry.height == 0 || cursor == self.finalized_checkpoint.block_hash {
                break;
            }
            cursor = entry.block.parent_hash;
        }
        self.finalized_checkpoint = checkpoint;
        Ok(())
    }

    /// Insert a candidate block into the fork-choice tree.
    ///
    /// If `candidate` attempts to reorg a finalized block (`candidate.height <= finalized_height`
    /// with `block_hash != canonical_hash`, or does not descend from `finalized_checkpoint`
    /// despite having `height > finalized_height`), the reorg is **rejected** and
    /// cryptographic [`SlashingEvidence`] is emitted and returned (`Err(Box<ReorgRejection>)`).
    pub fn insert_block(
        &mut self,
        candidate: SignedForkBlock,
    ) -> Result<Hash, Result<ForkChoiceError, Box<ReorgRejection>>> {
        let hash = candidate.block.block_hash;
        if self.blocks.contains_key(&hash)
            || !candidate.block.has_valid_hash()
            || candidate.proposer_sig.block_hash != hash
            || !self.active.contains(&candidate.proposer_sig.validator)
            || candidate
                .proposer_sig
                .verify(self.chain_id.as_str())
                .is_err()
        {
            return Err(Ok(ForkChoiceError::InvalidCandidate(
                "duplicate, invalid hash, signature, or validator",
            )));
        }
        if candidate.height > 0 && !self.blocks.contains_key(&candidate.block.parent_hash) {
            return Err(Ok(ForkChoiceError::UnknownParent(
                candidate.block.parent_hash,
            )));
        }

        if candidate.height == 0
            || self
                .blocks
                .get(&candidate.block.parent_hash)
                .and_then(|parent| parent.height.checked_add(1))
                != Some(candidate.height)
        {
            return Err(Ok(ForkChoiceError::InvalidCandidate(
                "height does not follow parent",
            )));
        }

        let fin_height = self.finalized_checkpoint.height;
        let fin_hash = self.finalized_checkpoint.block_hash;

        // Check if this block conflicts with the finalized chain!
        let conflicts_with_finalized = if candidate.height <= fin_height {
            self.canonical_by_height
                .get(&candidate.height)
                .map(|&canon| canon != hash)
                .unwrap_or(true)
        } else {
            !self.descends_from(candidate.block.parent_hash, fin_hash)
        };

        if conflicts_with_finalized {
            let err = ForkChoiceError::DeepReorgPastFinalized {
                finalized_height: fin_height,
                finalized_hash: fin_hash,
                conflicting_height: candidate.height,
                conflicting_hash: hash,
            };
            if let Some(evidence) = self.build_reorg_slashing_evidence(&candidate) {
                self.emitted_evidence.push(evidence.clone());
                return Err(Err(Box::new(ReorgRejection {
                    error: err,
                    evidence,
                })));
            }
            return Err(Ok(err));
        }

        self.blocks.insert(hash, candidate);
        Ok(self.select_canonical_head())
    }

    fn build_reorg_slashing_evidence(
        &self,
        conflicting: &SignedForkBlock,
    ) -> Option<SlashingEvidence> {
        // 1. If a finalized block at the same height (or same view/round) was signed by the same proposer,
        // emit DoubleSignProposal evidence.
        if let Some(&canon_hash) = self.canonical_by_height.get(&conflicting.height)
            && let Some(canon_block) = self.blocks.get(&canon_hash)
        {
            if canon_block.proposer_sig.validator == conflicting.proposer_sig.validator
                && canon_block.proposer_sig.block_hash != conflicting.proposer_sig.block_hash
            {
                return Some(SlashingEvidence::DoubleSignProposal {
                    view: conflicting.block.view,
                    round: conflicting.block.round,
                    first: canon_block.proposer_sig.clone(),
                    second: conflicting.proposer_sig.clone(),
                });
            }
            if let (Some(v_canon), Some(v_fork)) =
                (&canon_block.attester_vote, &conflicting.attester_vote)
                && v_canon.validator == v_fork.validator
                && v_canon.view == v_fork.view
                && v_canon.round == v_fork.round
                && v_canon.block_hash != v_fork.block_hash
            {
                return Some(SlashingEvidence::DoubleSignVote {
                    first: v_canon.clone(),
                    second: v_fork.clone(),
                });
            }
        }

        // 2. Otherwise scan finalized blocks for any conflicting proposal or vote from the same validator.
        for &canon_hash in self.canonical_by_height.values() {
            if let Some(canon_block) = self.blocks.get(&canon_hash)
                && canon_block.proposer_sig.validator == conflicting.proposer_sig.validator
                && canon_block.proposer_sig.block_hash != conflicting.proposer_sig.block_hash
            {
                return Some(SlashingEvidence::DoubleSignProposal {
                    view: conflicting.block.view,
                    round: conflicting.block.round,
                    first: canon_block.proposer_sig.clone(),
                    second: conflicting.proposer_sig.clone(),
                });
            }
        }
        None
    }

    /// Helper to look up a block's proposer address if present.
    pub fn block_proposer(&self, block_hash: &Hash) -> Option<Address> {
        self.blocks
            .get(block_hash)
            .map(|b| b.proposer_sig.validator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bls::{self, BlsSecretKey};
    use crate::qc::{QcAggregator, VoteType};
    use crate::slashing::{SlashingEngine, SlashingParams, ValidatorSlashingRecord};
    use aura_crypto::ed25519_public_key;

    struct TestVal {
        ed_seed: [u8; 32],
        bls_sk: BlsSecretKey,
        record: ValidatorSlashingRecord,
    }

    fn make_validators(count: usize) -> Vec<TestVal> {
        (0..count)
            .map(|i| {
                let seed = [(i as u8) + 1; 32];
                let bls_sk = BlsSecretKey::from_seed(&seed);
                let bls_pk = bls_sk.public_key();
                let address = bls::address_from_public_key(&bls_pk);
                let ed_pk = ed25519_public_key(&seed);
                TestVal {
                    ed_seed: seed,
                    bls_sk,
                    record: ValidatorSlashingRecord {
                        address,
                        ed25519_pubkey: ed_pk,
                        bls_pubkey: bls_pk,
                        current_stake: 10_000_000,
                        jailed: false,
                        tombstoned: false,
                        slash_count: 0,
                    },
                }
            })
            .collect()
    }

    fn build_qc(
        vals: &[TestVal],
        chain_id: &ChainId,
        view: u64,
        round: u64,
        block_hash: Hash,
        signers: usize,
    ) -> QuorumCertificate {
        let mut agg = QcAggregator::with_signers(
            chain_id.as_str(),
            &vals[0..signers]
                .iter()
                .map(|v| v.record.bls_pubkey.clone())
                .collect::<Vec<_>>(),
        );
        let mut out = None;
        for v in &vals[0..signers] {
            let mut msg = Vec::with_capacity(49);
            msg.extend_from_slice(&view.to_le_bytes());
            msg.extend_from_slice(&round.to_le_bytes());
            msg.extend_from_slice(block_hash.as_bytes());
            msg.push(VoteType::Precommit as u8);
            let sig = v.bls_sk.sign(chain_id.as_str(), Domain::Vote, &msg);
            let vote = Vote {
                view,
                round,
                block_hash,
                validator: v.record.address,
                vote_type: VoteType::Precommit,
                signature: sig,
            };
            if let Some(qc) = agg.add_vote(v.record.bls_pubkey.clone(), vote) {
                out = Some(qc);
            }
        }
        out.unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn make_signed_block(
        val: &TestVal,
        chain_id: &ChainId,
        height: u64,
        view: u64,
        round: u64,
        parent: &FinalityBlock,
        justified_qc_view: u64,
        tx_tag: &[u8],
    ) -> SignedForkBlock {
        let block = if height == 0 {
            FinalityBlock {
                view: 0,
                round: 0,
                parent_hash: Hash::ZERO,
                timestamp_ms: 1_000,
                block_hash: Hash::from_bytes([0x01; 32]),
                qc: None,
                transactions: Vec::new(),
            }
        } else {
            FinalityBlock::propose(
                view,
                round,
                parent,
                1_000 + height * 1_000,
                vec![tx_tag.to_vec()],
            )
        };
        let proposer_sig = BlockSignature::sign_block(
            &val.ed_seed,
            val.record.address,
            block.block_hash,
            chain_id.as_str(),
        );
        SignedForkBlock {
            height,
            block,
            justified_qc_view,
            proposer_sig,
            attester_vote: None,
        }
    }

    fn setup_tree() -> (ForkChoiceTree, Vec<TestVal>, ChainId, SignedForkBlock) {
        let chain_id = ChainId::new("auralite-b8-unit").unwrap();
        let vals = make_validators(4);
        let dummy = FinalityBlock {
            view: 0,
            round: 0,
            parent_hash: Hash::ZERO,
            timestamp_ms: 0,
            block_hash: Hash::ZERO,
            qc: None,
            transactions: Vec::new(),
        };
        let genesis = make_signed_block(&vals[0], &chain_id, 0, 0, 0, &dummy, 0, b"genesis");
        let genesis_qc = build_qc(&vals, &chain_id, 0, 0, genesis.block.block_hash, 3);
        let cp = FinalizedCheckpoint {
            height: 0,
            view: 0,
            block_hash: genesis.block.block_hash,
            state_root: Hash::from_bytes([0xAA; 32]),
            commit_qc: genesis_qc,
        };
        let active = vals.iter().map(|v| v.record.address).collect();
        let tree = ForkChoiceTree::new(chain_id.clone(), active, genesis.clone(), cp);
        (tree, vals, chain_id, genesis)
    }

    fn proof_for_target(
        vals: &[TestVal],
        chain_id: &ChainId,
        target: &FinalityBlock,
        state_root: Hash,
    ) -> ConsensusLightClientProof {
        let child = FinalityBlock::propose(
            target.view,
            target.round + 1,
            target,
            target.timestamp_ms + 1,
            vec![],
        );
        let grandchild = FinalityBlock::propose(
            target.view,
            target.round + 2,
            &child,
            target.timestamp_ms + 2,
            vec![],
        );
        ConsensusLightClientProof {
            state_root,
            target_qc: build_qc(
                vals,
                chain_id,
                target.view,
                target.round,
                target.block_hash,
                3,
            ),
            child_qc: build_qc(vals, chain_id, child.view, child.round, child.block_hash, 3),
            grandchild_qc: build_qc(
                vals,
                chain_id,
                grandchild.view,
                grandchild.round,
                grandchild.block_hash,
                3,
            ),
            target_block: target.clone(),
            child_block: child,
            grandchild_block: grandchild,
        }
    }

    #[test]
    fn attempt_to_reorg_finalized_block_is_rejected_and_emits_slashing_evidence() {
        let (mut tree, vals, chain_id, genesis) = setup_tree();

        // Build and finalize heights 1 and 2.
        let b1 = make_signed_block(&vals[1], &chain_id, 1, 1, 0, &genesis.block, 0, b"canon-1");
        let b2 = make_signed_block(&vals[2], &chain_id, 2, 2, 0, &b1.block, 1, b"canon-2");
        tree.insert_block(b1.clone()).unwrap();
        tree.insert_block(b2.clone()).unwrap();

        let qc2 = build_qc(&vals, &chain_id, 2, 0, b2.block.block_hash, 3);
        let trusted_root = Hash::from_bytes([0xBB; 32]);
        let proof = proof_for_target(&vals, &chain_id, &b2.block, trusted_root);
        tree.update_finalized_checkpoint(
            FinalizedCheckpoint {
                height: 2,
                view: 2,
                block_hash: b2.block.block_hash,
                state_root: Hash::from_bytes([0xBB; 32]),
                commit_qc: qc2,
            },
            &proof,
            trusted_root,
            2,
        )
        .unwrap();

        // Now validator 1 attempts to reorg finalized block 1 by signing a conflicting fork block at height 1!
        let reorg_b1 =
            make_signed_block(&vals[1], &chain_id, 1, 1, 0, &genesis.block, 10, b"reorg-1");
        let res = tree.insert_block(reorg_b1);
        let Err(Err(rejection)) = res else {
            panic!("expected DeepReorgPastFinalized with SlashingEvidence, got {res:?}");
        };
        assert!(matches!(
            rejection.error,
            ForkChoiceError::DeepReorgPastFinalized {
                finalized_height: 2,
                conflicting_height: 1,
                ..
            }
        ));
        assert_eq!(tree.emitted_evidence().len(), 1);

        // Verify the emitted evidence slashes and tombstones validator 1 in SlashingEngine!
        let records = vals.iter().map(|v| v.record.clone()).collect();
        let mut slashing = SlashingEngine::new(chain_id, SlashingParams::default(), records);
        let receipt = slashing
            .submit_evidence(&rejection.evidence, vals[0].record.address, None)
            .unwrap();
        assert_eq!(receipt.offender, vals[1].record.address);
        assert!(receipt.tombstoned);
    }

    #[test]
    fn fork_choice_prefers_branch_with_highest_qc_view_descending_from_finalized() {
        let (mut tree, vals, chain_id, genesis) = setup_tree();

        // Branch A: height 1, justified_qc_view = 2
        let branch_a =
            make_signed_block(&vals[1], &chain_id, 1, 1, 0, &genesis.block, 2, b"branch-a");
        // Branch B: height 1, justified_qc_view = 5 (higher QC view wins!)
        let branch_b =
            make_signed_block(&vals[2], &chain_id, 1, 2, 0, &genesis.block, 5, b"branch-b");

        tree.insert_block(branch_a).unwrap();
        let head = tree.insert_block(branch_b.clone()).unwrap();
        assert_eq!(head, branch_b.block.block_hash);
    }

    #[test]
    fn fork_not_descending_from_finalized_checkpoint_is_never_selected() {
        let (mut tree, vals, chain_id, genesis) = setup_tree();

        // Create two competing blocks at height 1 BEFORE finalizing either.
        let b1_canon =
            make_signed_block(&vals[1], &chain_id, 1, 1, 0, &genesis.block, 1, b"canon-1");
        let b1_stale =
            make_signed_block(&vals[2], &chain_id, 1, 2, 0, &genesis.block, 99, b"stale-1");
        tree.insert_block(b1_canon.clone()).unwrap();
        tree.insert_block(b1_stale.clone()).unwrap();

        // Finalize b1_canon! Even though b1_stale had justified_qc_view = 99, canonical head must now be b1_canon.
        let qc1 = build_qc(&vals, &chain_id, 1, 0, b1_canon.block.block_hash, 3);
        let trusted_root = Hash::from_bytes([0xCC; 32]);
        let proof = proof_for_target(&vals, &chain_id, &b1_canon.block, trusted_root);
        tree.update_finalized_checkpoint(
            FinalizedCheckpoint {
                height: 1,
                view: 1,
                block_hash: b1_canon.block.block_hash,
                state_root: Hash::from_bytes([0xCC; 32]),
                commit_qc: qc1,
            },
            &proof,
            trusted_root,
            1,
        )
        .unwrap();

        assert_eq!(tree.select_canonical_head(), b1_canon.block.block_hash);
    }

    #[test]
    fn finalized_checkpoint_rejects_invalid_or_sub_quorum_qc() {
        let (mut tree, vals, chain_id, genesis) = setup_tree();
        let b1 = make_signed_block(&vals[1], &chain_id, 1, 1, 0, &genesis.block, 1, b"b1");
        tree.insert_block(b1.clone()).unwrap();

        // Only 2 signers (< 3 required for 4 validators) -> rejected!
        let sub_quorum_qc = build_qc(&vals, &chain_id, 1, 0, b1.block.block_hash, 2);
        let bad_cp = FinalizedCheckpoint {
            height: 1,
            view: 1,
            block_hash: b1.block.block_hash,
            state_root: Hash::from_bytes([0xDD; 32]),
            commit_qc: sub_quorum_qc,
        };
        let trusted_root = bad_cp.state_root;
        let proof = proof_for_target(&vals, &chain_id, &b1.block, trusted_root);
        assert!(matches!(
            tree.update_finalized_checkpoint(bad_cp, &proof, trusted_root, 1),
            Err(ForkChoiceError::InvalidCheckpointQc(_))
        ));
    }

    #[test]
    fn light_client_proof_verifies_3_chain_finality_and_rejects_forged_qc() {
        let (_tree, vals, chain_id, genesis) = setup_tree();
        let b1 = FinalityBlock::propose(1, 0, &genesis.block, 2_000, vec![b"tx1".to_vec()]);
        let b2 = FinalityBlock::propose(1, 1, &b1, 3_000, vec![b"tx2".to_vec()]);
        let b3 = FinalityBlock::propose(1, 2, &b2, 4_000, vec![b"tx3".to_vec()]);

        let qc1 = build_qc(&vals, &chain_id, 1, 0, b1.block_hash, 3);
        let qc2 = build_qc(&vals, &chain_id, 1, 1, b2.block_hash, 3);
        let qc3 = build_qc(&vals, &chain_id, 1, 2, b3.block_hash, 3);

        let proof = ConsensusLightClientProof {
            target_block: b1,
            state_root: Hash::from_bytes([0xEE; 32]),
            child_block: b2,
            grandchild_block: b3,
            target_qc: qc1,
            child_qc: qc2,
            grandchild_qc: qc3,
        };
        proof
            .verify(
                &chain_id,
                &vals.iter().map(|v| v.record.address).collect(),
                proof.state_root,
            )
            .unwrap();

        // Sign a genuine cross-view certificate. Merely changing its view after
        // signing also breaks the signature and would not isolate this rule.
        let mut bad_proof = proof.clone();
        bad_proof.grandchild_block =
            FinalityBlock::propose(2, 2, &bad_proof.child_block, 4_000, vec![b"tx3".to_vec()]);
        bad_proof.grandchild_qc = build_qc(
            &vals,
            &chain_id,
            2,
            2,
            bad_proof.grandchild_block.block_hash,
            3,
        );
        assert!(matches!(
            bad_proof.verify(
                &chain_id,
                &vals.iter().map(|v| v.record.address).collect(),
                proof.state_root
            ),
            Err(ForkChoiceError::InvalidLightClientProof(_))
        ));
    }

    // Regression checks for trusted signer membership and an externally
    // authenticated root; the block format itself still does not commit that root.
    fn b8_sample_proof(
        vals: &[TestVal],
        chain_id: &ChainId,
        genesis: &SignedForkBlock,
    ) -> ConsensusLightClientProof {
        let b1 = FinalityBlock::propose(1, 0, &genesis.block, 2_000, vec![]);
        let b2 = FinalityBlock::propose(1, 1, &b1, 3_000, vec![]);
        let b3 = FinalityBlock::propose(1, 2, &b2, 4_000, vec![]);
        ConsensusLightClientProof {
            state_root: Hash::from_bytes([0xEE; 32]),
            target_qc: build_qc(vals, chain_id, 1, 0, b1.block_hash, 3),
            child_qc: build_qc(vals, chain_id, 1, 1, b2.block_hash, 3),
            grandchild_qc: build_qc(vals, chain_id, 1, 2, b3.block_hash, 3),
            target_block: b1,
            child_block: b2,
            grandchild_block: b3,
        }
    }

    #[test]
    fn b8_rejects_untrusted_signers() {
        let (_tree, trusted, chain_id, genesis) = setup_tree();
        let attacker_vals = make_validators(7);
        let proof = b8_sample_proof(&attacker_vals[4..], &chain_id, &genesis);
        assert!(
            proof
                .verify(
                    &chain_id,
                    &trusted.iter().map(|v| v.record.address).collect(),
                    proof.state_root
                )
                .is_err(),
            "untrusted signers were accepted"
        );
    }

    #[test]
    fn b8_rejects_root_different_from_trusted_anchor() {
        let (_tree, vals, chain_id, genesis) = setup_tree();
        let mut proof = b8_sample_proof(&vals, &chain_id, &genesis);
        let trusted_root = proof.state_root;
        proof.state_root = Hash::from_bytes([0x99; 32]);
        assert!(
            proof
                .verify(
                    &chain_id,
                    &vals.iter().map(|v| v.record.address).collect(),
                    trusted_root
                )
                .is_err(),
            "unbound state root was accepted"
        );
    }

    #[test]
    fn proof_rejects_unbound_block_data_and_mismatched_qc_context() {
        let (_tree, vals, chain_id, genesis) = setup_tree();
        let proof = b8_sample_proof(&vals, &chain_id, &genesis);
        let active = vals.iter().map(|v| v.record.address).collect();
        let mut altered = proof.clone();
        altered.child_block.transactions.push(b"changed".to_vec());
        assert!(
            altered
                .verify(&chain_id, &active, proof.state_root)
                .is_err()
        );
        let mut altered = proof.clone();
        altered.child_qc = build_qc(&vals, &chain_id, 1, 12, proof.child_block.block_hash, 3);
        assert!(
            altered
                .verify(&chain_id, &active, proof.state_root)
                .is_err()
        );
    }

    #[test]
    fn checkpoint_requires_a_valid_three_chain_and_trusted_root() {
        let (mut tree, vals, chain_id, genesis) = setup_tree();
        let b1 = make_signed_block(&vals[1], &chain_id, 1, 1, 0, &genesis.block, 1, b"b1");
        tree.insert_block(b1.clone()).unwrap();
        let root = Hash::from_bytes([0x90; 32]);
        let mut proof = proof_for_target(&vals, &chain_id, &b1.block, root);
        let cp = FinalizedCheckpoint {
            height: 1,
            view: 1,
            block_hash: b1.block.block_hash,
            state_root: root,
            commit_qc: proof.target_qc.clone(),
        };
        // A valid single target QC cannot substitute for the child/grandchild QCs.
        proof.child_qc = build_qc(&vals, &chain_id, 1, 1, proof.child_block.block_hash, 2);
        assert!(
            tree.update_finalized_checkpoint(cp.clone(), &proof, root, 1)
                .is_err()
        );
        assert_eq!(tree.finalized_checkpoint().height, 0);
        let proof = proof_for_target(&vals, &chain_id, &b1.block, root);
        assert!(
            tree.update_finalized_checkpoint(cp, &proof, Hash::ZERO, 1)
                .is_err()
        );
        assert_eq!(tree.finalized_checkpoint().height, 0);
    }

    #[test]
    fn candidate_duplicate_tampering_and_height_mismatch_are_rejected() {
        let (mut tree, vals, chain_id, genesis) = setup_tree();
        let b1 = make_signed_block(&vals[1], &chain_id, 1, 1, 0, &genesis.block, 1, b"b1");
        tree.insert_block(b1.clone()).unwrap();
        assert!(tree.insert_block(b1.clone()).is_err());
        let mut b2 = make_signed_block(&vals[2], &chain_id, 2, 1, 1, &b1.block, 1, b"b2");
        b2.block.transactions.push(b"altered".to_vec());
        assert!(tree.insert_block(b2).is_err());
        let mut b2 = make_signed_block(&vals[2], &chain_id, 2, 1, 1, &b1.block, 1, b"b2");
        b2.height = 99;
        assert!(tree.insert_block(b2).is_err());
    }

    #[test]
    fn weak_subjectivity_rejects_stale_checkpoint_beyond_unbonding_window() {
        let (tree, vals, chain_id, genesis) = setup_tree();
        let mut tree = tree.with_weak_subjectivity_window(100);
        let b1 = make_signed_block(&vals[1], &chain_id, 1, 1, 0, &genesis.block, 1, b"b1");
        tree.insert_block(b1.clone()).unwrap();
        let qc1 = build_qc(&vals, &chain_id, 1, 0, b1.block.block_hash, 3);
        let cp = FinalizedCheckpoint {
            height: 1,
            view: 1,
            block_hash: b1.block.block_hash,
            state_root: Hash::from_bytes([0xFF; 32]),
            commit_qc: qc1,
        };
        // Head height = 500, checkpoint height = 1 -> age 499 > window 100 -> rejected!
        let trusted_root = cp.state_root;
        let proof = proof_for_target(&vals, &chain_id, &b1.block, trusted_root);
        assert!(matches!(
            tree.update_finalized_checkpoint(cp, &proof, trusted_root, 500),
            Err(ForkChoiceError::WeakSubjectivityExpired { .. })
        ));
    }
}
