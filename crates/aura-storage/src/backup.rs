//! S7: backup, restore and checksum verification.
//!
//! A node that cannot be restored is not a node — it is a single point of
//! failure with an RPC port. S7 adds a **self-verifying backup bundle** built
//! from the S2 durable store:
//!
//! ```text
//! bundle         = u32le(manifest_len) || manifest_bytes || blob
//! manifest bytes = payload || BLAKE3(S7_MANIFEST_CONTEXT || payload)
//! payload        = "ALM7" || u32 format_version || v-len chain_id ||
//!                  u64 head_version || 32 head_root || u64 record_count ||
//!                  u64 blob_len || 32 blob_sha256
//! blob           = the S2 framed, BLAKE3-checksummed WAL records 1..=head
//! ```
//!
//! Verification is layered and every layer must pass:
//!
//! 1. `u32` manifest length is in range and the manifest checksum matches;
//! 2. magic and format version are known;
//! 3. `blob_len` equals the actual blob length and `SHA-256(blob)` matches;
//! 4. every S2 record inside the blob passes its own BLAKE3 checksum, the
//!    magic and the truncation/trailing checks;
//! 5. versions are consecutive and every `parent_root` chains to the previous
//!    `state_root`;
//! 6. replay through the real commit path recomputes each declared
//!    `state_root` (a backup that lies about a root is rejected);
//! 7. the final head and root equal the manifest, and
//!    `Storage::verify_state_root()` re-derives the root from the leaves.
//!
//! Restore refuses to write into a directory that already holds a store: a
//! restore that silently merges into live state is how people lose data.
//!
//! Golden vectors: `tools/gen_s7_vectors.py` (independent Python).
//! Report: `docs/evidence/s7/report.md`.

use std::path::Path;

use aura_codec::{Decode, Encode, Reader, Writer, read_varint, write_varint};
use aura_crypto::blake3_hash;
use aura_state::sha256_bytes;
use aura_types::Hash;

use crate::error::{BackupError, StorageError};
use crate::store::Storage;
use crate::wal::{WalRecord, decode_record};

/// Magic prefix of an S7 backup manifest.
pub const MANIFEST_MAGIC: &[u8] = b"ALM7";
/// Domain tag for the manifest checksum.
///
/// NC target: `tools/nc_s7.py` changes this constant and requires the S7
/// golden-bytes test to go red.
pub const S7_MANIFEST_CONTEXT: &[u8] = b"AuraLite-Crypto/s7-manifest-v1\x00";
/// Backup format version.
pub const BACKUP_FORMAT_VERSION: u32 = 1;
/// Largest accepted chain-id in a manifest.
pub const MAX_CHAIN_ID_LEN: usize = 64;
/// Largest accepted manifest (a backup from a hostile peer must not allocate
/// unbounded memory before verification).
pub const MAX_MANIFEST_BYTES: usize = 4 * 1024;
/// Largest accepted blob.
pub const MAX_BLOB_BYTES: u64 = 512 * 1024 * 1024;

/// The header of a backup bundle: everything needed to verify the blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupManifest {
    /// Backup format version.
    pub format_version: u32,
    /// Chain this backup belongs to (`auralite-test-1` in Stage 1).
    pub chain_id: String,
    /// Head version the blob restores to.
    pub head_version: u64,
    /// Head state root the blob restores to.
    pub head_root: Hash,
    /// Number of WAL records in the blob.
    pub record_count: u64,
    /// Exact blob length in bytes.
    pub blob_len: u64,
    /// SHA-256 of the blob.
    pub blob_sha256: Hash,
}

impl BackupManifest {
    /// Canonical payload (without the checksum).
    fn payload(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.push_bytes(MANIFEST_MAGIC);
        self.format_version.encode(&mut writer);
        write_varint(&mut writer, self.chain_id.len() as u64);
        writer.push_bytes(self.chain_id.as_bytes());
        self.head_version.encode(&mut writer);
        writer.push_bytes(self.head_root.as_bytes());
        self.record_count.encode(&mut writer);
        self.blob_len.encode(&mut writer);
        writer.push_bytes(self.blob_sha256.as_bytes());
        writer.into_bytes()
    }

    /// Canonical manifest bytes: payload plus its domain-separated checksum.
    pub fn to_bytes(&self) -> Vec<u8> {
        let payload = self.payload();
        let mut bytes = Vec::with_capacity(payload.len() + 32);
        bytes.extend_from_slice(&payload);
        bytes.extend_from_slice(manifest_checksum(&payload).as_bytes());
        bytes
    }

    /// Strict canonical decode: magic, version, caps, checksum, no trailing.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BackupError> {
        if bytes.len() < MANIFEST_MAGIC.len() + 32 {
            return Err(BackupError::Truncated);
        }
        let (payload, checksum) = bytes.split_at(bytes.len() - 32);
        if &payload[..MANIFEST_MAGIC.len()] != MANIFEST_MAGIC {
            return Err(BackupError::BadMagic);
        }
        if manifest_checksum(payload).as_bytes() != checksum {
            return Err(BackupError::ChecksumMismatch);
        }
        let mut reader = Reader::new(payload);
        reader
            .read_exact(MANIFEST_MAGIC.len())
            .map_err(|_| BackupError::Truncated)?;
        let format_version = u32::decode(&mut reader).map_err(|_| BackupError::Truncated)?;
        if format_version != BACKUP_FORMAT_VERSION {
            return Err(BackupError::UnsupportedVersion(format_version));
        }
        let chain_len = read_varint(&mut reader).map_err(|_| BackupError::Truncated)? as usize;
        if chain_len == 0 || chain_len > MAX_CHAIN_ID_LEN {
            return Err(BackupError::BadChainId);
        }
        let chain_bytes = reader
            .read_exact(chain_len)
            .map_err(|_| BackupError::Truncated)?;
        let chain_id = std::str::from_utf8(chain_bytes)
            .map_err(|_| BackupError::BadChainId)?
            .to_string();
        let head_version = u64::decode(&mut reader).map_err(|_| BackupError::Truncated)?;
        let head_root = read_hash(&mut reader)?;
        let record_count = u64::decode(&mut reader).map_err(|_| BackupError::Truncated)?;
        let blob_len = u64::decode(&mut reader).map_err(|_| BackupError::Truncated)?;
        let blob_sha256 = read_hash(&mut reader)?;
        reader.finish().map_err(|_| BackupError::NonCanonical)?;
        let manifest = BackupManifest {
            format_version,
            chain_id,
            head_version,
            head_root,
            record_count,
            blob_len,
            blob_sha256,
        };
        if manifest.to_bytes() != bytes {
            return Err(BackupError::NonCanonical);
        }
        Ok(manifest)
    }
}

fn read_hash(reader: &mut Reader<'_>) -> Result<Hash, BackupError> {
    let bytes = reader.read_exact(32).map_err(|_| BackupError::Truncated)?;
    Hash::from_slice(bytes).map_err(|_| BackupError::Truncated)
}

/// Checksum of a manifest payload.
pub fn manifest_checksum(payload: &[u8]) -> Hash {
    let mut input = Vec::with_capacity(S7_MANIFEST_CONTEXT.len() + payload.len());
    input.extend_from_slice(S7_MANIFEST_CONTEXT);
    input.extend_from_slice(payload);
    blake3_hash(&input)
}

/// A verified backup bundle split into its parts.
#[derive(Debug, Clone)]
pub struct BackupBundle {
    /// Verified manifest.
    pub manifest: BackupManifest,
    /// Raw blob (framed S2 records).
    pub blob: Vec<u8>,
}

impl BackupBundle {
    /// Parse and fully verify a bundle (all layers except record replay).
    pub fn parse(bytes: &[u8]) -> Result<Self, BackupError> {
        if bytes.len() < 4 {
            return Err(BackupError::Truncated);
        }
        let manifest_len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if manifest_len == 0 || manifest_len > MAX_MANIFEST_BYTES {
            return Err(BackupError::BadManifestLength(manifest_len));
        }
        let rest = &bytes[4..];
        if rest.len() < manifest_len {
            return Err(BackupError::Truncated);
        }
        let (manifest_bytes, blob) = rest.split_at(manifest_len);
        let manifest = BackupManifest::from_bytes(manifest_bytes)?;
        if manifest.blob_len != blob.len() as u64 {
            return Err(BackupError::LengthMismatch {
                expected: manifest.blob_len,
                got: blob.len() as u64,
            });
        }
        if manifest.blob_len > MAX_BLOB_BYTES {
            return Err(BackupError::BlobTooLarge(manifest.blob_len));
        }
        if sha256_bytes(blob) != manifest.blob_sha256 {
            return Err(BackupError::BlobHashMismatch);
        }
        Ok(BackupBundle {
            manifest,
            blob: blob.to_vec(),
        })
    }

    /// Canonical bundle bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let manifest = self.manifest.to_bytes();
        let mut bytes = Vec::with_capacity(4 + manifest.len() + self.blob.len());
        bytes.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&manifest);
        bytes.extend_from_slice(&self.blob);
        bytes
    }

    /// Decode every record in the blob, verifying each S2 frame.
    pub fn records(&self) -> Result<Vec<WalRecord>, BackupError> {
        let mut reader = Reader::new(&self.blob);
        let mut records = Vec::new();
        while reader.remaining_len() > 0 {
            let record = decode_record(&mut reader).map_err(|error| match error {
                StorageError::ChecksumMismatch => BackupError::RecordChecksum,
                _ => BackupError::RecordCorrupt,
            })?;
            records.push(record);
        }
        if records.len() as u64 != self.manifest.record_count {
            return Err(BackupError::LengthMismatch {
                expected: self.manifest.record_count,
                got: records.len() as u64,
            });
        }
        Ok(records)
    }
}

/// Create a backup bundle of `storage` for `chain_id`.
///
/// The output is deterministic: the same store always produces the same bytes.
pub fn create_backup(storage: &Storage, chain_id: &str) -> Result<Vec<u8>, BackupError> {
    if chain_id.is_empty() || chain_id.len() > MAX_CHAIN_ID_LEN {
        return Err(BackupError::BadChainId);
    }
    let records = storage.wal_records();
    let mut blob = Vec::new();
    for record in records {
        blob.extend_from_slice(&crate::wal::encode_record(record));
    }
    let head = storage.head();
    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        chain_id: chain_id.to_string(),
        head_version: head.version,
        head_root: head.root,
        record_count: records.len() as u64,
        blob_len: blob.len() as u64,
        blob_sha256: sha256_bytes(&blob),
    };
    Ok(BackupBundle { manifest, blob }.to_bytes())
}

/// What a restore did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    /// Chain the backup belongs to.
    pub chain_id: String,
    /// Records replayed.
    pub records: u64,
    /// Head version after restore.
    pub head_version: u64,
    /// Head root after restore.
    pub head_root: Hash,
    /// Backend the restored store uses.
    pub backend: String,
}

/// Restore `bundle` into `dir`, which must not already contain a store.
///
/// Every record is replayed through the real commit path, so a backup whose
/// declared roots do not match recomputed ones is rejected
/// ([`BackupError::RootMismatch`]) instead of being trusted.
pub fn restore_backup(
    bundle: &[u8],
    dir: &Path,
    expected_chain_id: &str,
) -> Result<RestoreReport, BackupError> {
    let bundle = BackupBundle::parse(bundle)?;
    if bundle.manifest.chain_id != expected_chain_id {
        return Err(BackupError::WrongChainId {
            expected: expected_chain_id.to_string(),
            got: bundle.manifest.chain_id.clone(),
        });
    }
    if dir.join("wal.log").exists() || dir.join("head.bin").exists() {
        return Err(BackupError::TargetNotEmpty);
    }
    let records = bundle.records()?;
    let mut storage = Storage::open(dir).map_err(BackupError::Storage)?;
    let mut expected_parent = Hash::ZERO;
    for (index, record) in records.iter().enumerate() {
        let expected_version = index as u64 + 1;
        if record.version != expected_version {
            return Err(BackupError::NonSequential {
                expected: expected_version,
                got: record.version,
            });
        }
        if record.parent_root != expected_parent {
            return Err(BackupError::ParentRootMismatch);
        }
        let receipt = storage
            .commit(record.changes.clone())
            .map_err(BackupError::Storage)?;
        if receipt.root != record.state_root {
            return Err(BackupError::RootMismatch {
                version: record.version,
            });
        }
        expected_parent = record.state_root;
    }
    let head = storage.head();
    if head.version != bundle.manifest.head_version || head.root != bundle.manifest.head_root {
        return Err(BackupError::HeadMismatch);
    }
    storage.verify_state_root().map_err(BackupError::Storage)?;
    Ok(RestoreReport {
        chain_id: bundle.manifest.chain_id,
        records: records.len() as u64,
        head_version: head.version,
        head_root: head.root,
        backend: storage.backend_name().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_and_rejects_a_flipped_byte() {
        let manifest = BackupManifest {
            format_version: BACKUP_FORMAT_VERSION,
            chain_id: "auralite-test-1".to_string(),
            head_version: 7,
            head_root: Hash::from_bytes([3u8; 32]),
            record_count: 7,
            blob_len: 1024,
            blob_sha256: Hash::from_bytes([4u8; 32]),
        };
        let bytes = manifest.to_bytes();
        assert_eq!(&bytes[..4], MANIFEST_MAGIC);
        assert_eq!(BackupManifest::from_bytes(&bytes).unwrap(), manifest);
        let mut bad = bytes.clone();
        let last = bad.len() - 33;
        bad[last] ^= 1;
        assert!(matches!(
            BackupManifest::from_bytes(&bad),
            Err(BackupError::ChecksumMismatch)
        ));
        let mut bad_magic = bytes.clone();
        bad_magic[0] ^= 1;
        assert!(matches!(
            BackupManifest::from_bytes(&bad_magic),
            Err(BackupError::BadMagic)
        ));
    }
}
