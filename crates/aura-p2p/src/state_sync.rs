//! N5 state sync: network integration with S4 deterministic snapshots
//! (`SnapshotManifest`, `ChunkDescriptor`, `chunk_hash`, `chunk_root`,
//! `import_snapshot`, and S1 `state_root` verification).
//!
//! Enforces `D-N3` (no trusted peer) and `D-N4` (header-first):
//! 1. A syncing node first anchors on a QC-certified [`SyncedHeader`] (`SyncQc`
//!    verified against [`SyncValidatorSet`]), which pins `(chain_id, height, state_root)`.
//! 2. The S4 [`SnapshotManifest`] is decoded and verified (`ALS4` magic,
//!    SHA-256 checksum, `chunk_root`, `chain_id`, `height`, and `state_root`).
//! 3. Chunks are verified against their S4 descriptor (`offset`, `length`,
//!    `SHA256(SNAPSHOT_CHUNK_CONTEXT || payload)`, and canonical `decode_chunk`).
//! 4. On any corrupt chunk, the staged chunk is rolled back, the offending peer
//!    is banned (`BanList`), and the chunk is marked for re-request from an
//!    honest peer.
//! 5. Finalization runs S4 parallel chunk verification (`import_snapshot`) and
//!    recomputes the S1 sparse Merkle `state_root(&entries)` before committing.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use aura_state::{
    SnapshotManifest, StateEntries, chunk_hash, decode_chunk, export_snapshot, import_snapshot,
    manifest_bytes, state_root,
};
use aura_types::{ChainId, Hash, Height};

use crate::ban::{BanList, BanReason};
use crate::block_sync::{SyncQc, SyncValidatorSet, SyncedHeader};
use crate::error::P2pError;
use crate::identity::PeerId;

/// Summary receipt produced when a new node completes N5 state sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSyncReport {
    /// Synchronized snapshot height.
    pub height: u64,
    /// Verified S1 state root matching the QC-certified header.
    pub verified_state_root: [u8; 32],
    /// Total state `(key, value)` pairs imported.
    pub total_keys: usize,
    /// Number of S4 chunks verified and imported.
    pub chunks_verified: usize,
    /// Number of corrupt-chunk or root-mismatch rollbacks triggered during sync.
    pub rollbacks_triggered: usize,
    /// Chunk indices that were rolled back and re-requested from a fallback peer.
    pub re_requested_chunks: Vec<usize>,
    /// Peers banned for serving a corrupt manifest or chunk.
    pub banned_peers: Vec<PeerId>,
    /// Measured wall-clock sync duration in microseconds.
    pub elapsed_micros: u64,
}

/// Exported S4 snapshot bundle: `(manifest_bytes, wire_chunks, state_root)`.
pub type S4SnapshotBundle = (Vec<u8>, Vec<Vec<u8>>, [u8; 32]);

/// Export an S4 snapshot and split its concatenated data blob into individual
/// wire chunks ready for P2P state sync.
pub fn build_s4_snapshot_bundle(
    chain_id: &str,
    height: u64,
    entries: &[(Vec<u8>, Vec<u8>)],
    chunk_size: u32,
    workers: usize,
) -> Result<S4SnapshotBundle, P2pError> {
    let cid = ChainId::new(chain_id).map_err(|e| P2pError::Codec(e.to_string()))?;
    let root =
        state_root(entries).map_err(|e| P2pError::StateSyncInvalidManifest(e.to_string()))?;
    let snapshot = export_snapshot(entries, cid, Height::new(height), root, chunk_size, workers)
        .map_err(|e| P2pError::StateSyncInvalidManifest(e.to_string()))?;
    let raw_manifest = manifest_bytes(&snapshot);
    let chunks: Vec<Vec<u8>> = snapshot
        .manifest
        .chunks
        .iter()
        .map(|desc| {
            let start = desc.offset as usize;
            let end = start + desc.length as usize;
            snapshot.data[start..end].to_vec()
        })
        .collect();
    Ok((raw_manifest, chunks, *root.as_bytes()))
}

/// Header-first S4 state sync session with per-chunk verification, rollback,
/// peer banning, and automatic chunk re-request (`D-N3`, `D-N4`).
#[derive(Debug, Clone)]
pub struct StateSyncSession {
    chain_id: String,
    target_height: u64,
    expected_state_root: [u8; 32],
    ban_list: BanList,
    manifest: Option<SnapshotManifest>,
    manifest_raw: Vec<u8>,
    staged_chunks: BTreeMap<usize, Vec<u8>>,
    needs_rerequest: BTreeSet<usize>,
    rollbacks_triggered: usize,
    re_requested_history: Vec<usize>,
    banned_peers: Vec<PeerId>,
    committed_entries: Option<StateEntries>,
    committed_state_root: Option<[u8; 32]>,
}

impl StateSyncSession {
    /// Create a [`StateSyncSession`] anchored on a QC-certified header (`D-N4`).
    pub fn from_certified_header(
        header: &SyncedHeader,
        qc: &SyncQc,
        validators: &SyncValidatorSet,
    ) -> Result<Self, P2pError> {
        if header.block_hash != header.compute_block_hash() {
            return Err(P2pError::BlockSyncHeaderHashMismatch {
                height: header.height,
            });
        }
        qc.verify(
            &header.chain_id,
            header.height,
            &header.block_hash,
            validators,
        )?;
        Ok(Self {
            chain_id: header.chain_id.clone(),
            target_height: header.height,
            expected_state_root: header.state_root,
            ban_list: BanList::new(),
            manifest: None,
            manifest_raw: Vec::new(),
            staged_chunks: BTreeMap::new(),
            needs_rerequest: BTreeSet::new(),
            rollbacks_triggered: 0,
            re_requested_history: Vec::new(),
            banned_peers: Vec::new(),
            committed_entries: None,
            committed_state_root: None,
        })
    }

    /// Borrow the local [`BanList`].
    pub const fn ban_list(&self) -> &BanList {
        &self.ban_list
    }

    /// Number of corrupt-chunk or root-mismatch rollbacks triggered so far.
    pub const fn rollbacks_triggered(&self) -> usize {
        self.rollbacks_triggered
    }

    /// Chunk indices currently marked for re-request after a rollback.
    pub fn pending_rerequests(&self) -> Vec<usize> {
        self.needs_rerequest.iter().copied().collect()
    }

    /// Committed state entries, available only after [`Self::finalize_and_verify_root`] succeeds.
    pub fn committed_entries(&self) -> Option<&StateEntries> {
        self.committed_entries.as_ref()
    }

    /// Committed state root, available only after [`Self::finalize_and_verify_root`] succeeds.
    pub const fn committed_state_root(&self) -> Option<[u8; 32]> {
        self.committed_state_root
    }

    /// Verify and accept an S4 [`SnapshotManifest`] against the QC-certified header.
    pub fn accept_manifest(
        &mut self,
        from_peer: PeerId,
        raw_manifest: &[u8],
        now_ms: u64,
    ) -> Result<&SnapshotManifest, P2pError> {
        self.ban_list.check_peer(&from_peer, now_ms)?;

        let parsed = match SnapshotManifest::from_canonical_bytes(raw_manifest) {
            Ok(manifest) => manifest,
            Err(err) => {
                self.ban_peer(from_peer, format!("invalid_s4_manifest:{err}"));
                return Err(P2pError::StateSyncInvalidManifest(err.to_string()));
            }
        };

        if parsed.chain_id.as_str() != self.chain_id {
            self.ban_peer(from_peer, "manifest_chain_id_mismatch".to_owned());
            return Err(P2pError::ChainIdMismatch {
                expected: self.chain_id.clone(),
                actual: parsed.chain_id.as_str().to_owned(),
            });
        }

        if parsed.height.value() != self.target_height {
            self.ban_peer(from_peer, "manifest_height_mismatch".to_owned());
            return Err(P2pError::BlockSyncHeightMismatch {
                expected: self.target_height,
                actual: parsed.height.value(),
            });
        }

        if *parsed.state_root.as_bytes() != self.expected_state_root {
            self.ban_peer(from_peer, "manifest_state_root_mismatch".to_owned());
            return Err(P2pError::StateSyncRootMismatch {
                expected: self.expected_state_root,
                actual: *parsed.state_root.as_bytes(),
            });
        }

        self.staged_chunks.clear();
        self.needs_rerequest.clear();
        self.manifest_raw = raw_manifest.to_vec();
        self.manifest = Some(parsed);
        Ok(self.manifest.as_ref().unwrap())
    }

    /// Verify and stage one S4 chunk (`chunk_index`) from `from_peer`.
    ///
    /// If the chunk fails SHA-256 digest verification (`chunk_hash`) or
    /// canonical S4 chunk decoding (`decode_chunk`), any staged data at
    /// `chunk_index` is rolled back, `from_peer` is banned, and `chunk_index`
    /// is queued in `needs_rerequest`.
    pub fn ingest_chunk(
        &mut self,
        chunk_index: usize,
        from_peer: PeerId,
        payload: Vec<u8>,
        now_ms: u64,
    ) -> Result<(), P2pError> {
        self.ban_list.check_peer(&from_peer, now_ms)?;

        let Some(ref manifest) = self.manifest else {
            return Err(P2pError::StateSyncManifestMissing);
        };

        let total_chunks = manifest.chunks.len();
        let Some(desc) = manifest.chunks.get(chunk_index).copied() else {
            return Err(P2pError::StateSyncChunkOutOfRange {
                chunk_index,
                total_chunks,
            });
        };

        let length_ok = payload.len() as u64 == desc.length;
        let hash_ok = chunk_hash(&payload) == desc.sha256;
        let canonical_ok = length_ok && hash_ok && decode_chunk(&payload).is_ok();

        if !canonical_ok {
            // Roll back any staged state for this chunk, ban the offending peer,
            // and mark the chunk for re-request from an honest peer.
            self.staged_chunks.remove(&chunk_index);
            self.needs_rerequest.insert(chunk_index);
            self.rollbacks_triggered += 1;
            self.ban_peer(from_peer, format!("corrupt_state_sync_chunk:{chunk_index}"));
            return Err(P2pError::StateSyncCorruptChunk {
                chunk_index,
                peer_id: from_peer,
            });
        }

        if self.needs_rerequest.remove(&chunk_index) {
            self.re_requested_history.push(chunk_index);
        }
        self.staged_chunks.insert(chunk_index, payload);
        Ok(())
    }

    /// Assemble all verified chunks, execute S4 parallel `import_snapshot`, and
    /// verify the recomputed S1 sparse Merkle `state_root(&entries)` against
    /// the QC-certified header's `expected_state_root`.
    ///
    /// If the recomputed S1 state root does not match, all staged chunks are
    /// rolled back and `committed_entries` remains `None`.
    pub fn finalize_and_verify_root(&mut self, workers: usize) -> Result<StateEntries, P2pError> {
        let Some(ref manifest) = self.manifest else {
            return Err(P2pError::StateSyncManifestMissing);
        };
        let need = manifest.chunks.len();
        let have = self.staged_chunks.len();
        if have != need {
            return Err(P2pError::StateSyncIncompleteChunks { have, need });
        }

        let mut data = Vec::new();
        for idx in 0..need {
            let Some(chunk_bytes) = self.staged_chunks.get(&idx) else {
                return Err(P2pError::StateSyncIncompleteChunks { have, need });
            };
            data.extend_from_slice(chunk_bytes);
        }

        let entries = import_snapshot(&self.manifest_raw, &data, workers).map_err(|err| {
            self.staged_chunks.clear();
            self.rollbacks_triggered += 1;
            P2pError::StateSyncInvalidManifest(err.to_string())
        })?;

        let recomputed: Hash = state_root(&entries).map_err(|err| {
            self.staged_chunks.clear();
            self.rollbacks_triggered += 1;
            P2pError::StateSyncInvalidManifest(err.to_string())
        })?;

        if *recomputed.as_bytes() != self.expected_state_root {
            self.staged_chunks.clear();
            self.rollbacks_triggered += 1;
            return Err(P2pError::StateSyncRootMismatch {
                expected: self.expected_state_root,
                actual: *recomputed.as_bytes(),
            });
        }

        self.committed_entries = Some(entries.clone());
        self.committed_state_root = Some(self.expected_state_root);
        Ok(entries)
    }

    /// Download all manifest chunks across `peer_sources`, automatically rolling
    /// back and re-requesting any corrupt chunk from the next non-banned peer.
    pub fn sync_from_peers(
        &mut self,
        peer_sources: &[(PeerId, Vec<Vec<u8>>)],
        workers: usize,
        now_ms: u64,
    ) -> Result<StateSyncReport, P2pError> {
        let start = Instant::now();
        let Some(ref manifest) = self.manifest else {
            return Err(P2pError::StateSyncManifestMissing);
        };
        let total_chunks = manifest.chunks.len();

        for chunk_idx in 0..total_chunks {
            let mut synced_this_chunk = false;
            for offset in 0..peer_sources.len() {
                let source_idx = (chunk_idx + offset) % peer_sources.len();
                let (peer_id, chunks) = &peer_sources[source_idx];
                if self.ban_list.check_peer(peer_id, now_ms).is_err() {
                    continue;
                }
                let Some(payload) = chunks.get(chunk_idx) else {
                    continue;
                };
                if self
                    .ingest_chunk(chunk_idx, *peer_id, payload.clone(), now_ms)
                    .is_ok()
                {
                    synced_this_chunk = true;
                    break;
                }
            }
            if !synced_this_chunk {
                return Err(P2pError::StateSyncIncompleteChunks {
                    have: self.staged_chunks.len(),
                    need: total_chunks,
                });
            }
        }

        let entries = self.finalize_and_verify_root(workers)?;
        let elapsed_micros = start.elapsed().as_micros() as u64;
        Ok(StateSyncReport {
            height: self.target_height,
            verified_state_root: self.expected_state_root,
            total_keys: entries.len(),
            chunks_verified: total_chunks,
            rollbacks_triggered: self.rollbacks_triggered,
            re_requested_chunks: self.re_requested_history.clone(),
            banned_peers: self.banned_peers.clone(),
            elapsed_micros,
        })
    }

    fn ban_peer(&mut self, peer_id: PeerId, reason: String) {
        self.ban_list
            .ban_peer(peer_id, BanReason::ProtocolViolation(reason), None);
        if !self.banned_peers.contains(&peer_id) {
            self.banned_peers.push(peer_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::PeerIdentity;
    use aura_state::{Account, StateKey, chunk_root};
    use aura_types::{Address, Amount, Nonce};

    const CHAIN: &str = "auralite-n5-unit";
    const HEIGHT: u64 = 100;
    const NOW_MS: u64 = 1_700_000_000_000;

    fn sample_entries(count: usize) -> StateEntries {
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let mut addr_bytes = [0u8; 20];
            addr_bytes[0] = 0x11;
            addr_bytes[12..20].copy_from_slice(&(i as u64 + 1).to_be_bytes());
            let key = StateKey::account(&Address::from_bytes(addr_bytes));
            let account = Account::new(
                Nonce::new(i as u64),
                Amount::from_units((i as u128 + 1) * 1_000),
                Account::NO_CODE,
                Account::EMPTY_STORAGE_ROOT,
            );
            entries.push((key.as_bytes().to_vec(), account.to_bytes()));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    fn sample_certified_session(
        state_root_bytes: [u8; 32],
    ) -> (Vec<PeerIdentity>, StateSyncSession) {
        let ids: Vec<PeerIdentity> = (1..=4u8)
            .map(|i| PeerIdentity::from_seed([i; 32]))
            .collect();
        let val_entries = ids
            .iter()
            .map(|id| (id.peer_id(), *id.public_key(), 100u64))
            .collect();
        let val_set = SyncValidatorSet::from_validators(val_entries).unwrap();
        let header = SyncedHeader::new(
            CHAIN,
            HEIGHT,
            [0x10; 32],
            [0x20; 32],
            state_root_bytes,
            NOW_MS,
            ids[0].peer_id(),
        );
        let signers = [&ids[0], &ids[1], &ids[2]];
        let qc = SyncQc::sign(CHAIN, HEIGHT, 1, header.block_hash, &signers);
        let session = StateSyncSession::from_certified_header(&header, &qc, &val_set).unwrap();
        (ids, session)
    }

    #[test]
    fn manifest_rejects_state_root_mismatch_against_certified_header() {
        let entries = sample_entries(12);
        let (raw_manifest, _, actual_root) =
            build_s4_snapshot_bundle(CHAIN, HEIGHT, &entries, 4, 2).unwrap();
        let wrong_header_root = [0xEE; 32];
        let (_, mut session) = sample_certified_session(wrong_header_root);
        let peer = PeerIdentity::from_seed([50u8; 32]).peer_id();

        assert_eq!(
            session.accept_manifest(peer, &raw_manifest, NOW_MS),
            Err(P2pError::StateSyncRootMismatch {
                expected: wrong_header_root,
                actual: actual_root,
            })
        );
        assert!(session.ban_list().check_peer(&peer, NOW_MS).is_err());
    }

    #[test]
    fn manifest_rejects_tampered_s4_checksum_or_chunk_root() {
        let entries = sample_entries(12);
        let (mut raw_manifest, _, root) =
            build_s4_snapshot_bundle(CHAIN, HEIGHT, &entries, 4, 2).unwrap();
        let (_, mut session) = sample_certified_session(root);
        let peer = PeerIdentity::from_seed([51u8; 32]).peer_id();

        let last = raw_manifest.len() - 1;
        raw_manifest[last] ^= 0xFF;
        assert!(matches!(
            session.accept_manifest(peer, &raw_manifest, NOW_MS),
            Err(P2pError::StateSyncInvalidManifest(_))
        ));
        assert!(session.ban_list().check_peer(&peer, NOW_MS).is_err());
    }

    #[test]
    fn chunk_ingestion_requires_accepted_manifest_first() {
        let entries = sample_entries(8);
        let (_, chunks, root) = build_s4_snapshot_bundle(CHAIN, HEIGHT, &entries, 4, 2).unwrap();
        let (_, mut session) = sample_certified_session(root);
        let peer = PeerIdentity::from_seed([52u8; 32]).peer_id();

        assert_eq!(
            session.ingest_chunk(0, peer, chunks[0].clone(), NOW_MS),
            Err(P2pError::StateSyncManifestMissing)
        );
    }

    #[test]
    fn corrupt_chunk_triggers_rollback_bans_peer_and_marks_rerequest() {
        let entries = sample_entries(12);
        let (raw_manifest, chunks, root) =
            build_s4_snapshot_bundle(CHAIN, HEIGHT, &entries, 4, 2).unwrap();
        let (_, mut session) = sample_certified_session(root);
        let honest = PeerIdentity::from_seed([60u8; 32]).peer_id();
        let corrupt_peer = PeerIdentity::from_seed([61u8; 32]).peer_id();

        session
            .accept_manifest(honest, &raw_manifest, NOW_MS)
            .unwrap();

        let mut bad_chunk = chunks[1].clone();
        bad_chunk[0] ^= 0xFF;
        assert_eq!(
            session.ingest_chunk(1, corrupt_peer, bad_chunk, NOW_MS),
            Err(P2pError::StateSyncCorruptChunk {
                chunk_index: 1,
                peer_id: corrupt_peer,
            })
        );
        assert_eq!(session.rollbacks_triggered(), 1);
        assert_eq!(session.pending_rerequests(), vec![1]);
        assert!(
            session
                .ban_list()
                .check_peer(&corrupt_peer, NOW_MS)
                .is_err()
        );
    }

    #[test]
    fn finalize_verifies_recomputed_s1_state_root() {
        let entries = sample_entries(8);
        let other_entries = sample_entries(9);
        let forged_root = *state_root(&other_entries).unwrap().as_bytes();

        // Build a valid S4 manifest whose chunks contain `entries`, but whose
        // manifest claims `forged_root` (matching a forged header).
        let cid = ChainId::new(CHAIN).unwrap();
        let snap = export_snapshot(
            &entries,
            cid,
            Height::new(HEIGHT),
            Hash::from_bytes(forged_root),
            4,
            2,
        )
        .unwrap();
        let raw_manifest = manifest_bytes(&snap);
        let chunks: Vec<Vec<u8>> = snap
            .manifest
            .chunks
            .iter()
            .map(|d| snap.data[d.offset as usize..(d.offset + d.length) as usize].to_vec())
            .collect();
        let digests: Vec<Hash> = snap.manifest.chunks.iter().map(|d| d.sha256).collect();
        assert_eq!(chunk_root(&digests), snap.manifest.chunk_root);

        let (_, mut session) = sample_certified_session(forged_root);
        let peer = PeerIdentity::from_seed([70u8; 32]).peer_id();
        session
            .accept_manifest(peer, &raw_manifest, NOW_MS)
            .unwrap();
        for (i, ch) in chunks.into_iter().enumerate() {
            session.ingest_chunk(i, peer, ch, NOW_MS).unwrap();
        }

        // Finalization must recompute S1 `state_root(&entries)` and reject the mismatch!
        let actual_root = *state_root(&entries).unwrap().as_bytes();
        assert_eq!(
            session.finalize_and_verify_root(2),
            Err(P2pError::StateSyncRootMismatch {
                expected: forged_root,
                actual: actual_root,
            })
        );
        assert!(session.committed_entries().is_none());
        assert_eq!(session.rollbacks_triggered(), 1);
    }

    #[test]
    fn failover_rerequests_corrupt_chunk_from_honest_peer() {
        let entries = sample_entries(16);
        let (raw_manifest, honest_chunks, root) =
            build_s4_snapshot_bundle(CHAIN, HEIGHT, &entries, 4, 2).unwrap();
        let (_, mut session) = sample_certified_session(root);

        let bad_peer = PeerIdentity::from_seed([80u8; 32]).peer_id();
        let honest_peer = PeerIdentity::from_seed([81u8; 32]).peer_id();
        session
            .accept_manifest(honest_peer, &raw_manifest, NOW_MS)
            .unwrap();

        let mut bad_chunks = honest_chunks.clone();
        bad_chunks[0][0] ^= 0x55; // corrupt chunk 0 on bad_peer

        let report = session
            .sync_from_peers(
                &[(bad_peer, bad_chunks), (honest_peer, honest_chunks)],
                2,
                NOW_MS,
            )
            .unwrap();

        assert_eq!(report.total_keys, 16);
        assert_eq!(report.chunks_verified, 4);
        assert_eq!(report.rollbacks_triggered, 1);
        assert_eq!(report.re_requested_chunks, vec![0]);
        assert_eq!(report.banned_peers, vec![bad_peer]);
        assert_eq!(report.verified_state_root, root);
        assert_eq!(session.committed_entries().unwrap(), &entries);
    }
}
