//! N7 Light client: header+QC proof verification, finalized checkpoint
//! progression, simplified Data-Availability Sampling (DAS), and strict
//! proof byte-size and verification-gas bounds (`D-N3`, `D-N4`).

use std::collections::BTreeSet;

use aura_crypto::blake3_hash;

use crate::ban::{BanList, BanReason};
use crate::block_sync::{SyncQc, SyncValidatorSet, SyncedHeader};
use crate::error::P2pError;
use crate::identity::PeerId;

/// Domain tag for N7 DAS leaf hashes.
pub const LIGHT_CLIENT_LEAF_DOMAIN: &[u8] = b"AuraLite-Crypto/p2p-light-leaf-v1\0";

/// Domain tag for N7 DAS internal Merkle branch hashes.
pub const LIGHT_CLIENT_BRANCH_DOMAIN: &[u8] = b"AuraLite-Crypto/p2p-light-branch-v1\0";

/// Default maximum encoded light-client proof size in bytes (16 KiB).
pub const DEFAULT_MAX_PROOF_BYTES: usize = 16 * 1024;

/// Default maximum verification gas budget per light-client proof.
pub const DEFAULT_MAX_VERIFICATION_GAS: u64 = 150_000;

/// Default minimum number of distinct DAS chunk samples required per header.
pub const DEFAULT_MIN_DAS_SAMPLES: usize = 4;

/// Base verification gas charged for canonical header hashing and checkpoint checks.
pub const GAS_BASE_HEADER_HASH: u64 = 2_000;

/// Verification gas charged per QC validator signature verification.
pub const GAS_PER_QC_SIGNATURE: u64 = 8_000;

/// Verification gas charged per DAS leaf hash.
pub const GAS_PER_DAS_LEAF: u64 = 1_000;

/// Verification gas charged per Merkle branch step in a DAS sample proof.
pub const GAS_PER_MERKLE_STEP: u64 = 500;

/// Trusted finalized checkpoint anchored by a light client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedCheckpoint {
    /// Canonical chain identifier.
    pub chain_id: String,
    /// Finalized block height.
    pub height: u64,
    /// Finalized block header hash.
    pub block_hash: [u8; 32],
    /// Finalized S1 sparse Merkle `state_root`.
    pub state_root: [u8; 32],
    /// Finalized block timestamp in milliseconds.
    pub timestamp_ms: u64,
}

impl FinalizedCheckpoint {
    /// Construct a trusted finalized checkpoint from a verified header.
    pub fn from_header(header: &SyncedHeader) -> Self {
        Self {
            chain_id: header.chain_id.clone(),
            height: header.height,
            block_hash: header.block_hash,
            state_root: header.state_root,
            timestamp_ms: header.timestamp_ms,
        }
    }
}

/// Resource and sampling bounds enforced on every [`LightClientProof`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightClientBounds {
    /// Maximum encoded proof size in bytes (`<= DEFAULT_MAX_PROOF_BYTES`).
    pub max_proof_bytes: usize,
    /// Maximum verification gas budget (`<= DEFAULT_MAX_VERIFICATION_GAS`).
    pub max_verification_gas: u64,
    /// Minimum distinct DAS chunk samples required per block.
    pub min_das_samples: usize,
}

impl Default for LightClientBounds {
    fn default() -> Self {
        Self {
            max_proof_bytes: DEFAULT_MAX_PROOF_BYTES,
            max_verification_gas: DEFAULT_MAX_VERIFICATION_GAS,
            min_das_samples: DEFAULT_MIN_DAS_SAMPLES,
        }
    }
}

/// One Data-Availability Sampling (DAS) chunk sample and its Merkle inclusion
/// branch against `header.tx_root`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DasChunkSample {
    /// Zero-based index of this data chunk (`0..total_chunks`).
    pub chunk_index: usize,
    /// Total number of chunks in the block's DAS tree (power of two `>= 1`).
    pub total_chunks: usize,
    /// Raw chunk payload bytes.
    pub chunk_data: Vec<u8>,
    /// Sibling hashes from the leaf up to `header.tx_root`.
    pub merkle_branch: Vec<[u8; 32]>,
}

impl DasChunkSample {
    /// Deterministic wire size of this sample in bytes.
    pub fn encoded_size_bytes(&self) -> usize {
        8 + 8 + 4 + self.chunk_data.len() + 4 + self.merkle_branch.len() * 32
    }

    /// Verify that this chunk sample's Merkle inclusion branch resolves to `expected_tx_root`.
    pub fn verify(&self, expected_tx_root: &[u8; 32]) -> bool {
        if self.chunk_data.is_empty()
            || self.total_chunks == 0
            || !self.total_chunks.is_power_of_two()
            || self.chunk_index >= self.total_chunks
        {
            return false;
        }
        let expected_depth = self.total_chunks.trailing_zeros() as usize;
        if self.merkle_branch.len() != expected_depth {
            return false;
        }
        let mut current = das_leaf_hash(self.chunk_index, self.total_chunks, &self.chunk_data);
        let mut idx = self.chunk_index;
        for sibling in &self.merkle_branch {
            current = if idx & 1 == 0 {
                das_branch_hash(&current, sibling)
            } else {
                das_branch_hash(sibling, &current)
            };
            idx >>= 1;
        }
        &current == expected_tx_root
    }
}

fn das_leaf_hash(chunk_index: usize, total_chunks: usize, chunk_data: &[u8]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(LIGHT_CLIENT_LEAF_DOMAIN.len() + 16 + 4 + chunk_data.len());
    buf.extend_from_slice(LIGHT_CLIENT_LEAF_DOMAIN);
    buf.extend_from_slice(&(chunk_index as u64).to_le_bytes());
    buf.extend_from_slice(&(total_chunks as u64).to_le_bytes());
    buf.extend_from_slice(&(chunk_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(chunk_data);
    *blake3_hash(&buf).as_bytes()
}

fn das_branch_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut buf = [0u8; LIGHT_CLIENT_BRANCH_DOMAIN.len() + 64];
    let dlen = LIGHT_CLIENT_BRANCH_DOMAIN.len();
    buf[..dlen].copy_from_slice(LIGHT_CLIENT_BRANCH_DOMAIN);
    buf[dlen..dlen + 32].copy_from_slice(left);
    buf[dlen + 32..dlen + 64].copy_from_slice(right);
    *blake3_hash(&buf).as_bytes()
}

/// Build a power-of-two DAS Merkle tree over `chunks`, returning `(tx_root, all_samples)`.
pub fn build_das_tree(chunks: &[Vec<u8>]) -> Result<([u8; 32], Vec<DasChunkSample>), P2pError> {
    if chunks.is_empty() || !chunks.len().is_power_of_two() {
        return Err(P2pError::Codec(
            "DAS chunk count must be a non-zero power of two".to_owned(),
        ));
    }
    let total_chunks = chunks.len();
    let mut levels: Vec<Vec<[u8; 32]>> = Vec::new();
    let leaves: Vec<[u8; 32]> = chunks
        .iter()
        .enumerate()
        .map(|(i, data)| das_leaf_hash(i, total_chunks, data))
        .collect();
    levels.push(leaves);

    while levels.last().unwrap().len() > 1 {
        let prev = levels.last().unwrap();
        let mut next = Vec::with_capacity(prev.len() / 2);
        for [left, right] in prev.as_chunks::<2>().0 {
            next.push(das_branch_hash(left, right));
        }
        levels.push(next);
    }

    let root = levels.last().unwrap()[0];
    let depth = total_chunks.trailing_zeros() as usize;
    let mut samples = Vec::with_capacity(total_chunks);
    for (idx, data) in chunks.iter().enumerate() {
        let mut branch = Vec::with_capacity(depth);
        let mut cur = idx;
        for level in &levels[..depth] {
            let sibling_idx = cur ^ 1;
            branch.push(level[sibling_idx]);
            cur >>= 1;
        }
        samples.push(DasChunkSample {
            chunk_index: idx,
            total_chunks,
            chunk_data: data.clone(),
            merkle_branch: branch,
        });
    }
    Ok((root, samples))
}

/// Deterministically select `sample_count` distinct DAS chunk indices for `(block_hash, light_peer)`.
pub fn sample_das_chunks(
    all_samples: &[DasChunkSample],
    block_hash: &[u8; 32],
    light_peer: &PeerId,
    sample_count: usize,
) -> Vec<DasChunkSample> {
    let total = all_samples.len();
    if total == 0 {
        return Vec::new();
    }
    let target = sample_count.min(total);
    let mut chosen = BTreeSet::new();
    let mut out = Vec::with_capacity(target);
    let mut nonce = 0u64;
    while out.len() < target {
        let mut seed_buf = [0u8; 72];
        seed_buf[..32].copy_from_slice(block_hash);
        seed_buf[32..64].copy_from_slice(light_peer.as_bytes());
        seed_buf[64..72].copy_from_slice(&nonce.to_le_bytes());
        let h = blake3_hash(&seed_buf);
        let idx = (u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap()) as usize) % total;
        if chosen.insert(idx) {
            out.push(all_samples[idx].clone());
        }
        nonce += 1;
    }
    out
}

/// Compact proof package verified by a light client without a full node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightClientProof {
    /// Trusted finalized checkpoint height referenced by this proof.
    pub checkpoint_height: u64,
    /// Trusted finalized checkpoint block hash referenced by this proof.
    pub checkpoint_hash: [u8; 32],
    /// Candidate header to verify.
    pub header: SyncedHeader,
    /// 2f+1 Quorum Certificate over `header`.
    pub qc: SyncQc,
    /// Sampled DAS chunks with Merkle branches to `header.tx_root`.
    pub das_samples: Vec<DasChunkSample>,
}

impl LightClientProof {
    /// Total encoded wire size of this proof in bytes.
    pub fn encoded_size_bytes(&self) -> usize {
        let header_bytes = 200 + self.header.chain_id.len();
        let qc_bytes = 56 + self.qc.chain_id.len() + self.qc.signatures.len() * 96;
        let das_bytes: usize = self
            .das_samples
            .iter()
            .map(DasChunkSample::encoded_size_bytes)
            .sum();
        8 + 32 + header_bytes + qc_bytes + 4 + das_bytes
    }

    /// Deterministic verification gas cost of this proof.
    pub fn verification_gas(&self) -> u64 {
        let qc_gas = (self.qc.signatures.len() as u64) * GAS_PER_QC_SIGNATURE;
        let das_gas: u64 = self
            .das_samples
            .iter()
            .map(|s| GAS_PER_DAS_LEAF + (s.merkle_branch.len() as u64) * GAS_PER_MERKLE_STEP)
            .sum();
        GAS_BASE_HEADER_HASH + qc_gas + das_gas
    }
}

/// Receipt emitted when a [`LightClientProof`] is verified and advances the checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightClientVerificationReceipt {
    /// Verified block height.
    pub height: u64,
    /// Verified `block_hash`.
    pub block_hash: [u8; 32],
    /// Verified `state_root` (retrievable without running a full node).
    pub state_root: [u8; 32],
    /// Encoded proof byte size (`<= max_proof_bytes`).
    pub proof_bytes: usize,
    /// Verification gas spent (`<= max_verification_gas`).
    pub verification_gas: u64,
    /// Number of distinct DAS chunk samples verified against `header.tx_root`.
    pub das_samples_verified: usize,
}

/// N7 Light client verifier enforcing bounded proof size/gas, finalized
/// checkpoint invariants, header+QC verification, and simplified DAS (`D-N3`, `D-N4`).
#[derive(Debug, Clone)]
pub struct LightClientVerifier {
    checkpoint: FinalizedCheckpoint,
    validator_set: SyncValidatorSet,
    bounds: LightClientBounds,
    ban_list: BanList,
}

impl LightClientVerifier {
    /// Initialize a light client from a trusted finalized checkpoint and validator set.
    pub fn new(
        checkpoint: FinalizedCheckpoint,
        validator_set: SyncValidatorSet,
        bounds: LightClientBounds,
    ) -> Self {
        Self {
            checkpoint,
            validator_set,
            bounds,
            ban_list: BanList::new(),
        }
    }

    /// Current finalized checkpoint held by the light client.
    pub const fn checkpoint(&self) -> &FinalizedCheckpoint {
        &self.checkpoint
    }

    /// Borrow the light client's [`BanList`].
    pub const fn ban_list(&self) -> &BanList {
        &self.ban_list
    }

    /// Verify a [`LightClientProof`] from `from_peer` and advance the finalized checkpoint.
    pub fn verify_and_advance(
        &mut self,
        from_peer: PeerId,
        proof: &LightClientProof,
        now_ms: u64,
    ) -> Result<LightClientVerificationReceipt, P2pError> {
        self.ban_list.check_peer(&from_peer, now_ms)?;

        // 1. Pre-crypto resource bounds: proof byte size and verification gas budget.
        let proof_bytes = proof.encoded_size_bytes();
        if proof_bytes > self.bounds.max_proof_bytes {
            self.ban_peer(from_peer, "light_client_proof_too_large");
            return Err(P2pError::LightClientProofTooLarge {
                size: proof_bytes,
                max: self.bounds.max_proof_bytes,
            });
        }

        let verification_gas = proof.verification_gas();
        if verification_gas > self.bounds.max_verification_gas {
            self.ban_peer(from_peer, "light_client_gas_limit_exceeded");
            return Err(P2pError::LightClientGasLimitExceeded {
                gas: verification_gas,
                max: self.bounds.max_verification_gas,
            });
        }

        // 2. Finalized checkpoint anchor and monotonicity check.
        if proof.checkpoint_height != self.checkpoint.height
            || proof.checkpoint_hash != self.checkpoint.block_hash
        {
            self.ban_peer(from_peer, "light_client_checkpoint_anchor_mismatch");
            return Err(P2pError::LightClientCheckpointViolation(
                "proof checkpoint anchor does not match trusted finalized checkpoint".to_owned(),
            ));
        }
        if proof.header.height <= self.checkpoint.height {
            self.ban_peer(from_peer, "light_client_height_not_advancing");
            return Err(P2pError::LightClientCheckpointViolation(format!(
                "header height {} must exceed checkpoint height {}",
                proof.header.height, self.checkpoint.height
            )));
        }
        if proof.header.timestamp_ms < self.checkpoint.timestamp_ms {
            self.ban_peer(from_peer, "light_client_timestamp_regression");
            return Err(P2pError::LightClientCheckpointViolation(
                "header timestamp regressed relative to finalized checkpoint".to_owned(),
            ));
        }
        if proof.header.height == self.checkpoint.height + 1
            && proof.header.parent_hash != self.checkpoint.block_hash
        {
            self.ban_peer(from_peer, "light_client_parent_hash_mismatch");
            return Err(P2pError::LightClientCheckpointViolation(
                "adjacent header parent_hash does not match checkpoint block_hash".to_owned(),
            ));
        }

        // 3. Header self-hash & chain_id check + 2f+1 Quorum Certificate verification.
        if proof.header.chain_id != self.checkpoint.chain_id {
            self.ban_peer(from_peer, "light_client_chain_id_mismatch");
            return Err(P2pError::ChainIdMismatch {
                expected: self.checkpoint.chain_id.clone(),
                actual: proof.header.chain_id.clone(),
            });
        }
        if proof.header.block_hash != proof.header.compute_block_hash() {
            self.ban_peer(from_peer, "light_client_forged_header");
            return Err(P2pError::BlockSyncHeaderHashMismatch {
                height: proof.header.height,
            });
        }
        if let Err(err) = proof.qc.verify(
            &self.checkpoint.chain_id,
            proof.header.height,
            &proof.header.block_hash,
            &self.validator_set,
        ) {
            self.ban_peer(from_peer, "light_client_invalid_qc");
            return Err(err);
        }

        // 4. Simplified Data-Availability Sampling (DAS) verification against `header.tx_root`.
        let mut distinct_indices = BTreeSet::new();
        let mut expected_total_chunks: Option<usize> = None;
        for sample in &proof.das_samples {
            if let Some(total) = expected_total_chunks {
                if sample.total_chunks != total {
                    self.ban_peer(from_peer, "light_client_inconsistent_das_total");
                    return Err(P2pError::LightClientDasSampleInvalid {
                        chunk_index: sample.chunk_index,
                    });
                }
            } else {
                expected_total_chunks = Some(sample.total_chunks);
            }
            if !sample.verify(&proof.header.tx_root) {
                self.ban_peer(from_peer, "light_client_invalid_das_sample");
                return Err(P2pError::LightClientDasSampleInvalid {
                    chunk_index: sample.chunk_index,
                });
            }
            distinct_indices.insert(sample.chunk_index);
        }

        let required_samples = expected_total_chunks
            .map(|total| self.bounds.min_das_samples.min(total))
            .unwrap_or(self.bounds.min_das_samples);
        if distinct_indices.len() < required_samples {
            self.ban_peer(from_peer, "light_client_insufficient_das_samples");
            return Err(P2pError::LightClientDasInsufficientSamples {
                have: distinct_indices.len(),
                need: required_samples,
            });
        }

        // 5. Advance the finalized checkpoint.
        self.checkpoint = FinalizedCheckpoint::from_header(&proof.header);
        Ok(LightClientVerificationReceipt {
            height: proof.header.height,
            block_hash: proof.header.block_hash,
            state_root: proof.header.state_root,
            proof_bytes,
            verification_gas,
            das_samples_verified: distinct_indices.len(),
        })
    }

    fn ban_peer(&mut self, peer_id: PeerId, reason: &str) {
        self.ban_list.ban_peer(
            peer_id,
            BanReason::ProtocolViolation(reason.to_owned()),
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::PeerIdentity;

    const CHAIN: &str = "auralite-n7-unit";
    const NOW_MS: u64 = 1_700_007_000_000;

    fn test_fixture() -> (
        Vec<PeerIdentity>,
        SyncValidatorSet,
        FinalizedCheckpoint,
        LightClientProof,
    ) {
        let validators: Vec<PeerIdentity> = (1..=4u8)
            .map(|i| PeerIdentity::from_seed([i; 32]))
            .collect();
        let val_entries = validators
            .iter()
            .map(|id| (id.peer_id(), *id.public_key(), 100u64))
            .collect();
        let val_set = SyncValidatorSet::from_validators(val_entries).unwrap();

        let genesis_header = SyncedHeader::new(
            CHAIN,
            100,
            [0x11; 32],
            [0x22; 32],
            [0x33; 32],
            NOW_MS,
            validators[0].peer_id(),
        );
        let checkpoint = FinalizedCheckpoint::from_header(&genesis_header);

        let chunks: Vec<Vec<u8>> = (0..8u8).map(|i| vec![0xA0 + i; 128]).collect();
        let (tx_root, all_samples) = build_das_tree(&chunks).unwrap();
        let header = SyncedHeader::new(
            CHAIN,
            101,
            checkpoint.block_hash,
            tx_root,
            [0x99; 32],
            NOW_MS + 1_000,
            validators[1].peer_id(),
        );
        let signers = [&validators[0], &validators[1], &validators[2]];
        let qc = SyncQc::sign(CHAIN, 101, 1, header.block_hash, &signers);
        let light_peer = PeerIdentity::from_seed([0x77; 32]).peer_id();
        let das_samples = sample_das_chunks(&all_samples, &header.block_hash, &light_peer, 4);

        let proof = LightClientProof {
            checkpoint_height: checkpoint.height,
            checkpoint_hash: checkpoint.block_hash,
            header,
            qc,
            das_samples,
        };
        (validators, val_set, checkpoint, proof)
    }

    #[test]
    fn forged_header_or_tampered_state_root_is_rejected_and_peer_banned() {
        let (_validators, val_set, checkpoint, mut proof) = test_fixture();
        let mut verifier =
            LightClientVerifier::new(checkpoint, val_set, LightClientBounds::default());
        let bad_peer = PeerIdentity::from_seed([0xE1; 32]).peer_id();

        // Tamper with `state_root` without recomputing `block_hash` + QC.
        proof.header.state_root[0] ^= 0xFF;
        assert_eq!(
            verifier.verify_and_advance(bad_peer, &proof, NOW_MS),
            Err(P2pError::BlockSyncHeaderHashMismatch { height: 101 })
        );
        assert!(verifier.ban_list().check_peer(&bad_peer, NOW_MS).is_err());
    }

    #[test]
    fn qc_below_quorum_or_forged_signature_is_rejected() {
        let (_validators, val_set, checkpoint, mut proof) = test_fixture();
        let mut verifier =
            LightClientVerifier::new(checkpoint, val_set, LightClientBounds::default());
        let bad_peer = PeerIdentity::from_seed([0xE2; 32]).peer_id();

        // Corrupt a validator signature in the QC.
        proof.qc.signatures[0].1[0] ^= 0xAA;
        assert!(matches!(
            verifier.verify_and_advance(bad_peer, &proof, NOW_MS),
            Err(P2pError::BlockSyncQcInvalidSignature(_))
        ));
        assert!(verifier.ban_list().check_peer(&bad_peer, NOW_MS).is_err());
    }

    #[test]
    fn checkpoint_mismatch_or_height_regression_is_rejected() {
        let (_validators, val_set, checkpoint, mut proof) = test_fixture();
        let mut verifier =
            LightClientVerifier::new(checkpoint, val_set, LightClientBounds::default());
        let bad_peer = PeerIdentity::from_seed([0xE3; 32]).peer_id();

        proof.checkpoint_hash[0] ^= 0x55;
        assert!(matches!(
            verifier.verify_and_advance(bad_peer, &proof, NOW_MS),
            Err(P2pError::LightClientCheckpointViolation(_))
        ));
        assert!(verifier.ban_list().check_peer(&bad_peer, NOW_MS).is_err());
    }

    #[test]
    fn proof_exceeding_size_or_gas_bound_is_rejected_before_crypto() {
        let (_validators, val_set, checkpoint, proof) = test_fixture();
        let tight_bounds = LightClientBounds {
            max_proof_bytes: 512,
            max_verification_gas: 10_000,
            min_das_samples: 4,
        };
        let mut verifier =
            LightClientVerifier::new(checkpoint.clone(), val_set.clone(), tight_bounds);
        let peer_a = PeerIdentity::from_seed([0xE4; 32]).peer_id();
        assert!(matches!(
            verifier.verify_and_advance(peer_a, &proof, NOW_MS),
            Err(P2pError::LightClientProofTooLarge { .. })
        ));

        let gas_bounds = LightClientBounds {
            max_proof_bytes: DEFAULT_MAX_PROOF_BYTES,
            max_verification_gas: 10_000,
            min_das_samples: 4,
        };
        let mut verifier_gas = LightClientVerifier::new(checkpoint, val_set, gas_bounds);
        let peer_b = PeerIdentity::from_seed([0xE5; 32]).peer_id();
        assert!(matches!(
            verifier_gas.verify_and_advance(peer_b, &proof, NOW_MS),
            Err(P2pError::LightClientGasLimitExceeded { .. })
        ));
    }

    #[test]
    fn das_sampling_rejects_insufficient_or_duplicate_chunk_samples() {
        let (_validators, val_set, checkpoint, mut proof) = test_fixture();
        let mut verifier =
            LightClientVerifier::new(checkpoint, val_set, LightClientBounds::default());
        let bad_peer = PeerIdentity::from_seed([0xE6; 32]).peer_id();

        // Provide 4 samples, but all 4 are duplicates of sample[0] (only 1 distinct index).
        let first = proof.das_samples[0].clone();
        proof.das_samples = vec![first.clone(), first.clone(), first.clone(), first];
        assert_eq!(
            verifier.verify_and_advance(bad_peer, &proof, NOW_MS),
            Err(P2pError::LightClientDasInsufficientSamples { have: 1, need: 4 })
        );
    }

    #[test]
    fn das_sampling_rejects_corrupted_chunk_merkle_proof_and_bans_peer() {
        let (_validators, val_set, checkpoint, mut proof) = test_fixture();
        let mut verifier =
            LightClientVerifier::new(checkpoint, val_set, LightClientBounds::default());
        let bad_peer = PeerIdentity::from_seed([0xE7; 32]).peer_id();

        proof.das_samples[1].chunk_data[0] ^= 0xFF;
        assert_eq!(
            verifier.verify_and_advance(bad_peer, &proof, NOW_MS),
            Err(P2pError::LightClientDasSampleInvalid {
                chunk_index: proof.das_samples[1].chunk_index,
            })
        );
        assert!(verifier.ban_list().check_peer(&bad_peer, NOW_MS).is_err());
    }
}
