//! N4 block sync: header-first chain verification (`D-N4`), Quorum Certificate
//! (`SyncQc`) validation, and parallel block body download with `tx_root`
//! verification and automatic `BanList` escalation (`D-N3`).

use std::collections::{BTreeMap, BTreeSet};

use aura_crypto::{blake3_hash, verify_raw};

use crate::ban::{BanList, BanReason};
use crate::error::P2pError;
use crate::identity::{PeerId, PeerIdentity};

/// Domain tag for N4 block header hashing.
pub const BLOCK_SYNC_HEADER_DOMAIN: &[u8] = b"AuraLite-Crypto/p2p-block-sync-header-v1\0";

/// Domain tag for N4 Quorum Certificate (`SyncQc`) signatures.
pub const BLOCK_SYNC_QC_DOMAIN: &[u8] = b"AuraLite-Crypto/p2p-block-sync-qc-v1\0";

/// Domain tag for N4 block body `tx_root` hashing.
pub const BLOCK_SYNC_BODY_DOMAIN: &[u8] = b"AuraLite-Crypto/p2p-block-sync-body-v1\0";

/// Active validator set used to verify Quorum Certificates during header-first sync.
#[derive(Debug, Clone)]
pub struct SyncValidatorSet {
    validators: BTreeMap<PeerId, ([u8; 32], u64)>,
    total_stake: u64,
}

impl SyncValidatorSet {
    /// Construct a [`SyncValidatorSet`] from `(PeerId, ed25519_public_key, stake)` entries.
    ///
    /// Verifies `PeerId == BLAKE3(public_key)` for every validator.
    pub fn from_validators(entries: Vec<(PeerId, [u8; 32], u64)>) -> Result<Self, P2pError> {
        let mut validators = BTreeMap::new();
        let mut total_stake = 0u64;
        for (peer_id, pubkey, stake) in entries {
            peer_id.verify_public_key(&pubkey)?;
            if stake > 0 && validators.insert(peer_id, (pubkey, stake)).is_none() {
                total_stake = total_stake.saturating_add(stake);
            }
        }
        Ok(Self {
            validators,
            total_stake,
        })
    }

    /// Total active stake across all validators.
    pub const fn total_stake(&self) -> u64 {
        self.total_stake
    }

    /// BFT quorum threshold (`floor(2 * total_stake / 3) + 1`).
    pub const fn quorum_stake(&self) -> u64 {
        (self.total_stake * 2) / 3 + 1
    }

    /// Look up a validator's `(public_key, stake)` by `PeerId`.
    pub fn get(&self, peer_id: &PeerId) -> Option<&([u8; 32], u64)> {
        self.validators.get(peer_id)
    }
}

/// Block header downloaded during Phase 1 of N4 header-first sync (`D-N4`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncedHeader {
    /// Chain identifier.
    pub chain_id: String,
    /// Block height (`>= 1`).
    pub height: u64,
    /// Parent block hash (`[0u8; 32]` at height 0 genesis).
    pub parent_hash: [u8; 32],
    /// Root hash of the block body's transactions.
    pub tx_root: [u8; 32],
    /// Post-execution state root after applying the block's transactions.
    pub state_root: [u8; 32],
    /// Monotonic block timestamp in Unix milliseconds.
    pub timestamp_ms: u64,
    /// Proposer `PeerId`.
    pub proposer: PeerId,
    /// Canonical `BLAKE3` hash of this header.
    pub block_hash: [u8; 32],
}

impl SyncedHeader {
    /// Construct a [`SyncedHeader`] and compute its canonical `block_hash`.
    pub fn new(
        chain_id: &str,
        height: u64,
        parent_hash: [u8; 32],
        tx_root: [u8; 32],
        state_root: [u8; 32],
        timestamp_ms: u64,
        proposer: PeerId,
    ) -> Self {
        let mut header = Self {
            chain_id: chain_id.to_owned(),
            height,
            parent_hash,
            tx_root,
            state_root,
            timestamp_ms,
            proposer,
            block_hash: [0u8; 32],
        };
        header.block_hash = header.compute_block_hash();
        header
    }

    /// Compute the domain-separated `BLAKE3` digest of this header's fields.
    pub fn compute_block_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(192);
        buf.extend_from_slice(BLOCK_SYNC_HEADER_DOMAIN);
        buf.extend_from_slice(&(self.chain_id.len() as u32).to_le_bytes());
        buf.extend_from_slice(self.chain_id.as_bytes());
        buf.extend_from_slice(&self.height.to_le_bytes());
        buf.extend_from_slice(&self.parent_hash);
        buf.extend_from_slice(&self.tx_root);
        buf.extend_from_slice(&self.state_root);
        buf.extend_from_slice(&self.timestamp_ms.to_le_bytes());
        buf.extend_from_slice(self.proposer.as_bytes());
        *blake3_hash(&buf).as_bytes()
    }

    /// Verify that this header links cleanly to its verified parent header.
    pub fn verify_against_parent(
        &self,
        expected_chain_id: &str,
        parent_height: u64,
        parent_hash: &[u8; 32],
        parent_timestamp_ms: u64,
    ) -> Result<(), P2pError> {
        if self.chain_id != expected_chain_id {
            return Err(P2pError::ChainIdMismatch {
                expected: expected_chain_id.to_owned(),
                actual: self.chain_id.clone(),
            });
        }
        let expected_height = parent_height.saturating_add(1);
        if self.height != expected_height {
            return Err(P2pError::BlockSyncHeightMismatch {
                expected: expected_height,
                actual: self.height,
            });
        }
        if self.parent_hash != *parent_hash {
            return Err(P2pError::BlockSyncParentHashMismatch {
                height: self.height,
            });
        }
        if self.timestamp_ms < parent_timestamp_ms {
            return Err(P2pError::BlockSyncTimestampRegression {
                height: self.height,
            });
        }
        if self.block_hash != self.compute_block_hash() {
            return Err(P2pError::BlockSyncHeaderHashMismatch {
                height: self.height,
            });
        }
        Ok(())
    }
}

/// Quorum Certificate certifying a [`SyncedHeader`] during header-first sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncQc {
    /// Chain identifier.
    pub chain_id: String,
    /// Certified block height.
    pub height: u64,
    /// Consensus round number.
    pub round: u64,
    /// Certified header `block_hash`.
    pub block_hash: [u8; 32],
    /// Validator `(PeerId, Ed25519 signature)` pairs.
    pub signatures: Vec<(PeerId, [u8; 64])>,
}

impl SyncQc {
    /// Create a [`SyncQc`] signed by `signers`.
    pub fn sign(
        chain_id: &str,
        height: u64,
        round: u64,
        block_hash: [u8; 32],
        signers: &[&PeerIdentity],
    ) -> Self {
        let digest = qc_signing_digest(chain_id, height, round, &block_hash);
        let signatures = signers
            .iter()
            .map(|id| (id.peer_id(), id.sign_digest(&digest)))
            .collect();
        Self {
            chain_id: chain_id.to_owned(),
            height,
            round,
            block_hash,
            signatures,
        }
    }

    /// Verify all validator signatures and quorum stake (`>= 2/3 + 1`) for
    /// `(expected_chain_id, expected_height, expected_block_hash)`.
    pub fn verify(
        &self,
        expected_chain_id: &str,
        expected_height: u64,
        expected_block_hash: &[u8; 32],
        validators: &SyncValidatorSet,
    ) -> Result<u64, P2pError> {
        if self.chain_id != expected_chain_id {
            return Err(P2pError::ChainIdMismatch {
                expected: expected_chain_id.to_owned(),
                actual: self.chain_id.clone(),
            });
        }
        if self.height != expected_height {
            return Err(P2pError::BlockSyncHeightMismatch {
                expected: expected_height,
                actual: self.height,
            });
        }
        if self.block_hash != *expected_block_hash {
            return Err(P2pError::BlockSyncHeaderHashMismatch {
                height: self.height,
            });
        }

        let digest = qc_signing_digest(&self.chain_id, self.height, self.round, &self.block_hash);
        let mut seen = BTreeSet::new();
        let mut signed_stake = 0u64;

        for (peer_id, sig) in &self.signatures {
            if !seen.insert(*peer_id) {
                return Err(P2pError::BlockSyncQcDuplicateSigner(*peer_id));
            }
            let Some((pubkey, stake)) = validators.get(peer_id) else {
                return Err(P2pError::BlockSyncQcUnknownValidator(*peer_id));
            };
            verify_raw(pubkey, &digest, sig)
                .map_err(|_| P2pError::BlockSyncQcInvalidSignature(*peer_id))?;
            signed_stake = signed_stake.saturating_add(*stake);
        }

        let required_stake = validators.quorum_stake();
        if signed_stake < required_stake {
            return Err(P2pError::BlockSyncQcInsufficientQuorum {
                signed_stake,
                required_stake,
            });
        }
        Ok(signed_stake)
    }
}

fn qc_signing_digest(chain_id: &str, height: u64, round: u64, block_hash: &[u8; 32]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(128);
    buf.extend_from_slice(BLOCK_SYNC_QC_DOMAIN);
    buf.extend_from_slice(&(chain_id.len() as u32).to_le_bytes());
    buf.extend_from_slice(chain_id.as_bytes());
    buf.extend_from_slice(&height.to_le_bytes());
    buf.extend_from_slice(&round.to_le_bytes());
    buf.extend_from_slice(block_hash);
    *blake3_hash(&buf).as_bytes()
}

/// Block body downloaded in parallel after its [`SyncedHeader`] + [`SyncQc`]
/// have been verified (`D-N4`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncedBlockBody {
    /// Block height matching the verified header.
    pub height: u64,
    /// Block hash matching the verified header.
    pub block_hash: [u8; 32],
    /// Ordered raw transactions in this block.
    pub transactions: Vec<Vec<u8>>,
}

impl SyncedBlockBody {
    /// Construct a [`SyncedBlockBody`].
    pub const fn new(height: u64, block_hash: [u8; 32], transactions: Vec<Vec<u8>>) -> Self {
        Self {
            height,
            block_hash,
            transactions,
        }
    }

    /// Compute the canonical `tx_root` for `transactions`.
    pub fn compute_tx_root(transactions: &[Vec<u8>]) -> [u8; 32] {
        let mut buf = Vec::with_capacity(64 + transactions.len() * 36);
        buf.extend_from_slice(BLOCK_SYNC_BODY_DOMAIN);
        buf.extend_from_slice(&(transactions.len() as u32).to_le_bytes());
        for tx in transactions {
            buf.extend_from_slice(&(tx.len() as u32).to_le_bytes());
            buf.extend_from_slice(blake3_hash(tx).as_bytes());
        }
        *blake3_hash(&buf).as_bytes()
    }

    /// Verify that this block body matches `header` (`height`, `block_hash`, and `tx_root`).
    pub fn verify_against_header(&self, header: &SyncedHeader) -> Result<(), P2pError> {
        if self.height != header.height {
            return Err(P2pError::BlockSyncHeightMismatch {
                expected: header.height,
                actual: self.height,
            });
        }
        if self.block_hash != header.block_hash {
            return Err(P2pError::BlockSyncHeaderHashMismatch {
                height: self.height,
            });
        }
        let actual_root = Self::compute_tx_root(&self.transactions);
        if actual_root != header.tx_root {
            return Err(P2pError::BlockSyncTxRootMismatch {
                height: self.height,
            });
        }
        Ok(())
    }
}

/// Header-first + parallel-body block sync engine (`D-N3`, `D-N4`).
#[derive(Debug, Clone)]
pub struct BlockSyncEngine {
    chain_id: String,
    validators: SyncValidatorSet,
    ban_list: BanList,
    header_tip_height: u64,
    header_tip_hash: [u8; 32],
    header_tip_timestamp_ms: u64,
    synced_head_height: u64,
    synced_head_hash: [u8; 32],
    synced_state_root: [u8; 32],
    verified_headers: BTreeMap<u64, (SyncedHeader, SyncQc)>,
    verified_bodies: BTreeMap<u64, SyncedBlockBody>,
}

impl BlockSyncEngine {
    /// Construct a new [`BlockSyncEngine`] anchored at genesis (`height = 0`).
    pub fn new(
        chain_id: &str,
        validators: SyncValidatorSet,
        genesis_hash: [u8; 32],
        genesis_state_root: [u8; 32],
        genesis_timestamp_ms: u64,
    ) -> Self {
        Self {
            chain_id: chain_id.to_owned(),
            validators,
            ban_list: BanList::new(),
            header_tip_height: 0,
            header_tip_hash: genesis_hash,
            header_tip_timestamp_ms: genesis_timestamp_ms,
            synced_head_height: 0,
            synced_head_hash: genesis_hash,
            synced_state_root: genesis_state_root,
            verified_headers: BTreeMap::new(),
            verified_bodies: BTreeMap::new(),
        }
    }

    /// Borrow the local `BanList`.
    pub const fn ban_list(&self) -> &BanList {
        &self.ban_list
    }

    /// Highest contiguous header height verified with a valid [`SyncQc`].
    pub const fn header_tip_height(&self) -> u64 {
        self.header_tip_height
    }

    /// Highest contiguous block height for which both header+QC and body are verified.
    pub const fn synced_head_height(&self) -> u64 {
        self.synced_head_height
    }

    /// Block hash of the highest fully synced block (`synced_head_height`).
    pub const fn synced_head_hash(&self) -> [u8; 32] {
        self.synced_head_hash
    }

    /// State root of the highest fully synced block (`synced_head_height`).
    pub const fn synced_state_root(&self) -> [u8; 32] {
        self.synced_state_root
    }

    /// Phase 1 (`D-N4` header-first): verify and ingest a single header and its QC.
    ///
    /// If `from_peer` serves an invalid header or QC, `from_peer` is placed on
    /// the [`BanList`] (`D-N3`).
    pub fn ingest_header_with_qc(
        &mut self,
        from_peer: PeerId,
        header: SyncedHeader,
        qc: SyncQc,
        now_ms: u64,
    ) -> Result<(), P2pError> {
        self.ban_list.check_peer(&from_peer, now_ms)?;

        if let Err(err) = header.verify_against_parent(
            &self.chain_id,
            self.header_tip_height,
            &self.header_tip_hash,
            self.header_tip_timestamp_ms,
        ) {
            self.ban_list.ban_peer(
                from_peer,
                BanReason::ProtocolViolation(format!("invalid_sync_header:{err}")),
                None,
            );
            return Err(err);
        }

        if let Err(err) = qc.verify(
            &self.chain_id,
            header.height,
            &header.block_hash,
            &self.validators,
        ) {
            self.ban_list.ban_peer(
                from_peer,
                BanReason::ProtocolViolation(format!("invalid_sync_qc:{err}")),
                None,
            );
            return Err(err);
        }

        self.header_tip_height = header.height;
        self.header_tip_hash = header.block_hash;
        self.header_tip_timestamp_ms = header.timestamp_ms;
        self.verified_headers.insert(header.height, (header, qc));
        self.advance_synced_head();
        Ok(())
    }

    /// Ingest a contiguous batch of `(SyncedHeader, SyncQc)` pairs from `from_peer`.
    ///
    /// Sequential parent-hash linkage is verified in height order, and Quorum
    /// Certificate Ed25519 signatures across the batch are verified in parallel
    /// worker threads (`std::thread::scope`).
    pub fn ingest_header_batch(
        &mut self,
        from_peer: PeerId,
        batch: Vec<(SyncedHeader, SyncQc)>,
        now_ms: u64,
    ) -> Result<usize, P2pError> {
        self.ban_list.check_peer(&from_peer, now_ms)?;
        if batch.is_empty() {
            return Ok(0);
        }

        let mut parent_height = self.header_tip_height;
        let mut parent_hash = self.header_tip_hash;
        let mut parent_ts = self.header_tip_timestamp_ms;
        for (header, _) in &batch {
            if let Err(err) =
                header.verify_against_parent(&self.chain_id, parent_height, &parent_hash, parent_ts)
            {
                self.ban_list.ban_peer(
                    from_peer,
                    BanReason::ProtocolViolation(format!("invalid_sync_header:{err}")),
                    None,
                );
                return Err(err);
            }
            parent_height = header.height;
            parent_hash = header.block_hash;
            parent_ts = header.timestamp_ms;
        }

        let chain_id = self.chain_id.as_str();
        let validators = &self.validators;
        let chunk_size = batch.len().div_ceil(4).max(1);
        let qc_err: Option<P2pError> = std::thread::scope(|s| {
            let handles: Vec<_> = batch
                .chunks(chunk_size)
                .map(|slice| {
                    s.spawn(move || {
                        for (header, qc) in slice {
                            qc.verify(chain_id, header.height, &header.block_hash, validators)?;
                        }
                        Ok::<(), P2pError>(())
                    })
                })
                .collect();
            for h in handles {
                if let Err(err) = h.join().unwrap() {
                    return Some(err);
                }
            }
            None
        });

        if let Some(err) = qc_err {
            self.ban_list.ban_peer(
                from_peer,
                BanReason::ProtocolViolation(format!("invalid_sync_qc:{err}")),
                None,
            );
            return Err(err);
        }

        let count = batch.len();
        for (header, qc) in batch {
            self.header_tip_height = header.height;
            self.header_tip_hash = header.block_hash;
            self.header_tip_timestamp_ms = header.timestamp_ms;
            self.verified_headers.insert(header.height, (header, qc));
        }
        self.advance_synced_head();
        Ok(count)
    }

    /// Phase 2 (`D-N4` body download): verify and ingest a [`SyncedBlockBody`]
    /// whose header+QC is already in `verified_headers`.
    pub fn ingest_body(
        &mut self,
        from_peer: PeerId,
        body: SyncedBlockBody,
        now_ms: u64,
    ) -> Result<(), P2pError> {
        self.ban_list.check_peer(&from_peer, now_ms)?;

        let Some((header, _)) = self.verified_headers.get(&body.height) else {
            return Err(P2pError::BlockSyncHeaderMissing(body.height));
        };

        if let Err(err) = body.verify_against_header(header) {
            self.ban_list.ban_peer(
                from_peer,
                BanReason::ProtocolViolation(format!("invalid_sync_body:{err}")),
                None,
            );
            return Err(err);
        }

        self.verified_bodies.insert(body.height, body);
        self.advance_synced_head();
        Ok(())
    }

    /// Verify and ingest multiple body chunks downloaded in parallel across
    /// worker peers (`std::thread::scope`).
    ///
    /// Chunks may arrive in any order (e.g. worker 3 finishes heights
    /// `7501..=10000` before worker 0 finishes `1..=2500`). Any peer serving a
    /// corrupted body is banned and its error is returned.
    pub fn download_bodies_parallel(
        &mut self,
        chunks: Vec<(PeerId, Vec<SyncedBlockBody>)>,
        now_ms: u64,
    ) -> Result<usize, P2pError> {
        for (peer_id, _) in &chunks {
            self.ban_list.check_peer(peer_id, now_ms)?;
        }

        let headers_ref = &self.verified_headers;
        let results: Vec<(PeerId, Result<Vec<SyncedBlockBody>, P2pError>)> =
            std::thread::scope(|s| {
                let handles: Vec<_> = chunks
                    .into_iter()
                    .map(|(peer_id, bodies)| {
                        s.spawn(move || {
                            for body in &bodies {
                                let Some((header, _)) = headers_ref.get(&body.height) else {
                                    return (
                                        peer_id,
                                        Err(P2pError::BlockSyncHeaderMissing(body.height)),
                                    );
                                };
                                if let Err(err) = body.verify_against_header(header) {
                                    return (peer_id, Err(err));
                                }
                            }
                            (peer_id, Ok(bodies))
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });

        let mut total_ingested = 0usize;
        for (peer_id, outcome) in results {
            match outcome {
                Ok(bodies) => {
                    for body in bodies {
                        self.verified_bodies.insert(body.height, body);
                        total_ingested += 1;
                    }
                }
                Err(err) => {
                    if !matches!(err, P2pError::BlockSyncHeaderMissing(_)) {
                        self.ban_list.ban_peer(
                            peer_id,
                            BanReason::ProtocolViolation(format!("invalid_sync_body:{err}")),
                            None,
                        );
                    }
                    return Err(err);
                }
            }
        }
        self.advance_synced_head();
        Ok(total_ingested)
    }

    fn advance_synced_head(&mut self) {
        while let Some(next_height) = self.synced_head_height.checked_add(1) {
            if self.verified_bodies.contains_key(&next_height)
                && let Some((header, _)) = self.verified_headers.get(&next_height)
            {
                self.synced_head_height = next_height;
                self.synced_head_hash = header.block_hash;
                self.synced_state_root = header.state_root;
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAIN: &str = "auralite-n4-unit";
    const GENESIS_HASH: [u8; 32] = [0xAA; 32];
    const GENESIS_STATE: [u8; 32] = [0xBB; 32];

    fn make_validators() -> (Vec<PeerIdentity>, SyncValidatorSet) {
        let ids: Vec<PeerIdentity> = (1..=4u8)
            .map(|i| PeerIdentity::from_seed([i; 32]))
            .collect();
        let entries = ids
            .iter()
            .map(|id| (id.peer_id(), *id.public_key(), 100u64))
            .collect();
        let set = SyncValidatorSet::from_validators(entries).unwrap();
        (ids, set)
    }

    fn sample_block(
        ids: &[PeerIdentity],
        height: u64,
        parent_hash: [u8; 32],
        txs: Vec<Vec<u8>>,
    ) -> (SyncedHeader, SyncQc, SyncedBlockBody) {
        let tx_root = SyncedBlockBody::compute_tx_root(&txs);
        let state_root = *blake3_hash(&height.to_le_bytes()).as_bytes();
        let header = SyncedHeader::new(
            CHAIN,
            height,
            parent_hash,
            tx_root,
            state_root,
            1_700_000_000_000 + height * 1_000,
            ids[0].peer_id(),
        );
        let signers = [&ids[0], &ids[1], &ids[2]];
        let qc = SyncQc::sign(CHAIN, height, 1, header.block_hash, &signers);
        let body = SyncedBlockBody::new(height, header.block_hash, txs);
        (header, qc, body)
    }

    #[test]
    fn header_chain_rejects_parent_hash_mismatch() {
        let (ids, _) = make_validators();
        let (mut h1, _, _) = sample_block(&ids, 1, GENESIS_HASH, vec![b"tx1".to_vec()]);
        h1.parent_hash = [0xFF; 32];
        h1.block_hash = h1.compute_block_hash();
        assert_eq!(
            h1.verify_against_parent(CHAIN, 0, &GENESIS_HASH, 1_700_000_000_000),
            Err(P2pError::BlockSyncParentHashMismatch { height: 1 })
        );
    }

    #[test]
    fn header_rejects_tampered_block_hash() {
        let (ids, _) = make_validators();
        let (mut h1, _, _) = sample_block(&ids, 1, GENESIS_HASH, vec![b"tx1".to_vec()]);
        h1.block_hash[0] ^= 0x01;
        assert_eq!(
            h1.verify_against_parent(CHAIN, 0, &GENESIS_HASH, 1_700_000_000_000),
            Err(P2pError::BlockSyncHeaderHashMismatch { height: 1 })
        );
    }

    #[test]
    fn qc_rejects_below_quorum_stake() {
        let (ids, set) = make_validators();
        let (h1, _, _) = sample_block(&ids, 1, GENESIS_HASH, vec![b"tx1".to_vec()]);
        // 2-of-4 validators = 200 stake < 267 required quorum stake.
        let signers = [&ids[0], &ids[1]];
        let under_qc = SyncQc::sign(CHAIN, 1, 1, h1.block_hash, &signers);
        assert_eq!(
            under_qc.verify(CHAIN, 1, &h1.block_hash, &set),
            Err(P2pError::BlockSyncQcInsufficientQuorum {
                signed_stake: 200,
                required_stake: 267,
            })
        );
    }

    #[test]
    fn qc_rejects_forged_validator_signature() {
        let (ids, set) = make_validators();
        let (h1, mut qc, _) = sample_block(&ids, 1, GENESIS_HASH, vec![b"tx1".to_vec()]);
        qc.signatures[1].1[0] ^= 0xFF;
        assert_eq!(
            qc.verify(CHAIN, 1, &h1.block_hash, &set),
            Err(P2pError::BlockSyncQcInvalidSignature(ids[1].peer_id()))
        );
    }

    #[test]
    fn body_ingestion_requires_verified_header_first() {
        let (ids, set) = make_validators();
        let mut engine =
            BlockSyncEngine::new(CHAIN, set, GENESIS_HASH, GENESIS_STATE, 1_700_000_000_000);
        let (_, _, body1) = sample_block(&ids, 1, GENESIS_HASH, vec![b"tx1".to_vec()]);
        let peer = PeerIdentity::from_seed([99u8; 32]).peer_id();

        assert_eq!(
            engine.ingest_body(peer, body1, 1_700_000_001_000),
            Err(P2pError::BlockSyncHeaderMissing(1))
        );
    }

    #[test]
    fn body_rejects_tx_root_mismatch_and_bans_peer() {
        let (ids, set) = make_validators();
        let mut engine =
            BlockSyncEngine::new(CHAIN, set, GENESIS_HASH, GENESIS_STATE, 1_700_000_000_000);
        let (h1, qc1, mut body1) = sample_block(&ids, 1, GENESIS_HASH, vec![b"tx1".to_vec()]);
        let honest_peer = PeerIdentity::from_seed([90u8; 32]).peer_id();
        let bad_peer = PeerIdentity::from_seed([91u8; 32]).peer_id();

        engine
            .ingest_header_with_qc(honest_peer, h1, qc1, 1_700_000_001_000)
            .unwrap();

        body1.transactions = vec![b"corrupted-tx".to_vec()];
        assert_eq!(
            engine.ingest_body(bad_peer, body1, 1_700_000_001_000),
            Err(P2pError::BlockSyncTxRootMismatch { height: 1 })
        );
        assert!(
            engine
                .ban_list()
                .check_peer(&bad_peer, 1_700_000_001_000)
                .is_err()
        );
    }
}
