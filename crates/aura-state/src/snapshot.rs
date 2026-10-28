//! S4: deterministic state snapshots (chunks, SHA-256 manifest, parallel I/O).
//!
//! A snapshot is `manifest_bytes || data_blob`. The data blob is a concatenation
//! of independently SHA-256-hashed chunks; the manifest holds a descriptor for
//! every chunk (offset, length, digest), the S1 `state_root`, a BLAKE3
//! `chunk_root` over the ordered digests, and a SHA-256 checksum of the whole
//! manifest payload. Export and import hash chunks in parallel via
//! `std::thread::scope`; output ordering is fixed by the descriptor list.
//!
//! Golden vectors: `tools/gen_s4_vectors.py` (independent Python: hashlib
//! SHA-256 + pure-Python BLAKE3 + S1 sparse Merkle tree).
//! No `serde`, no `HashMap`, no wall clock. All integers are fixed-width LE or
//! minimal LEB128 varints from `aura-codec`. `sha2` was already in the C3 lock
//! via `k256`; S4 adds no new external crate.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use aura_codec::{Decode, Encode, Reader, Writer, read_varint, write_varint};
use aura_crypto::blake3_hash;
use aura_types::{ChainId, Hash, Height};

use crate::error::SnapshotError;
use crate::key::StateKey;
use crate::tree::{MerkleTree, state_path, versioned_root};

/// Magic prefix of every snapshot payload. (NC target: `tools/nc_s4.py`.)
pub const SNAPSHOT_MAGIC: &[u8] = b"ALS4";
/// Wire version of the snapshot format.
pub const SNAPSHOT_VERSION: u32 = 1;
/// Domain tag for the manifest SHA-256 checksum. (NC target: `tools/nc_s4.py`.)
pub const SNAPSHOT_MANIFEST_CONTEXT: &[u8] = b"AuraLite-Crypto/s4-manifest-v1\x00";
/// Domain tag for per-chunk SHA-256 digests. (NC target: `tools/nc_s4.py`.)
pub const SNAPSHOT_CHUNK_CONTEXT: &[u8] = b"AuraLite-Crypto/s4-chunk-v1\x00";
/// Domain tag for the BLAKE3 `chunk_root`.
pub const SNAPSHOT_ROOT_CONTEXT: &[u8] = b"AuraLite-Crypto/s4-root-v1\x00";
/// Maximum number of chunks in one snapshot.
pub const MAX_CHUNKS: u32 = 1_000_000;
/// Maximum byte length of one chunk.
pub const MAX_CHUNK_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum byte length of one key.
pub const MAX_KEY_BYTES: u64 = 1024;
/// Maximum byte length of one value.
pub const MAX_VALUE_BYTES: u64 = 1024 * 1024;

/// One chunk descriptor in the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkDescriptor {
    /// Byte offset of this chunk inside the data blob.
    pub offset: u64,
    /// Byte length of the chunk payload.
    pub length: u64,
    /// `SHA256(SNAPSHOT_CHUNK_CONTEXT || payload)`.
    pub sha256: Hash,
}

/// The snapshot manifest (everything except the data blob).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotManifest {
    /// Wire version.
    pub version: u32,
    /// Chain this snapshot belongs to.
    pub chain_id: ChainId,
    /// Height the snapshot was taken at.
    pub height: Height,
    /// Versioned S1 state root.
    pub state_root: Hash,
    /// BLAKE3 root over the ordered chunk digests.
    pub chunk_root: Hash,
    /// Number of entries per chunk (last chunk may be shorter).
    pub chunk_size: u32,
    /// Total number of state entries in this snapshot.
    pub total_keys: u64,
    /// Ordered chunk descriptors.
    pub chunks: Vec<ChunkDescriptor>,
}

/// A complete snapshot: manifest bytes plus the concatenated chunk payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Parsed manifest.
    pub manifest: SnapshotManifest,
    /// Concatenated chunk payload bytes.
    pub data: Vec<u8>,
}

/// Ordered `(key, value)` state entries.
pub type StateEntries = Vec<(Vec<u8>, Vec<u8>)>;

pub(crate) fn sha256(data: &[u8]) -> Hash {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&digest);
    Hash::from_bytes(bytes)
}

/// Raw SHA-256 of arbitrary bytes (NC target for S4: `tools/nc_s4.py`).
pub fn sha256_bytes(data: &[u8]) -> Hash {
    sha256(data)
}

/// SHA-256 of one chunk payload, with the S4 domain tag.
pub fn chunk_hash(payload: &[u8]) -> Hash {
    let mut input = Vec::with_capacity(SNAPSHOT_CHUNK_CONTEXT.len() + payload.len());
    input.extend_from_slice(SNAPSHOT_CHUNK_CONTEXT);
    input.extend_from_slice(payload);
    sha256(&input)
}

/// BLAKE3 root over the ordered chunk digests.
pub fn chunk_root(digests: &[Hash]) -> Hash {
    let mut input = Vec::with_capacity(SNAPSHOT_ROOT_CONTEXT.len() + 4 + digests.len() * 32);
    input.extend_from_slice(SNAPSHOT_ROOT_CONTEXT);
    input.extend_from_slice(&(digests.len() as u32).to_le_bytes());
    for digest in digests {
        input.extend_from_slice(digest.as_bytes());
    }
    blake3_hash(&input)
}

/// Versioned S1 state root over `(C7 key, value)` entries.
pub fn state_root(entries: &[(Vec<u8>, Vec<u8>)]) -> Result<Hash, SnapshotError> {
    let mut leaves: Vec<(Hash, Vec<u8>)> = Vec::with_capacity(entries.len());
    for (raw_key, value) in entries {
        let key = StateKey::from_raw(raw_key).map_err(SnapshotError::Key)?;
        leaves.push((state_path(&key), value.clone()));
    }
    let tree = MerkleTree::from_entries(1, &leaves);
    Ok(versioned_root(1, &tree.tree_root()))
}

/// Canonical encoding of one chunk:
/// `varint(count) || (varint(len) || key || varint(len) || value)*`.
pub fn encode_chunk(entries: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut writer = Writer::new();
    write_varint(&mut writer, entries.len() as u64);
    for (key, value) in entries {
        write_varint(&mut writer, key.len() as u64);
        writer.push_bytes(key);
        write_varint(&mut writer, value.len() as u64);
        writer.push_bytes(value);
    }
    writer.into_bytes()
}

/// Decode a chunk with strict canonicality (minimal varints, sorted keys,
/// no duplicates, no trailing bytes, byte-identical re-encoding).
pub fn decode_chunk(bytes: &[u8]) -> Result<StateEntries, SnapshotError> {
    let mut reader = Reader::new(bytes);
    let count = read_varint(&mut reader).map_err(SnapshotError::Decode)?;
    if count > MAX_CHUNKS as u64 {
        return Err(SnapshotError::TooManyEntries(count as usize));
    }
    let mut entries: StateEntries = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let key_len = read_varint(&mut reader).map_err(SnapshotError::Decode)?;
        if key_len > MAX_KEY_BYTES {
            return Err(SnapshotError::KeyTooLarge(key_len as usize));
        }
        let key = reader
            .read_exact(key_len as usize)
            .map_err(SnapshotError::Decode)?
            .to_vec();
        let value_len = read_varint(&mut reader).map_err(SnapshotError::Decode)?;
        if value_len > MAX_VALUE_BYTES {
            return Err(SnapshotError::ValueTooLarge(value_len as usize));
        }
        let value = reader
            .read_exact(value_len as usize)
            .map_err(SnapshotError::Decode)?
            .to_vec();
        if entries.last().is_some_and(|(previous, _)| previous >= &key) {
            return Err(SnapshotError::UnsortedKeys);
        }
        entries.push((key, value));
    }
    reader.finish().map_err(SnapshotError::Decode)?;
    if encode_chunk(&entries) != bytes {
        return Err(SnapshotError::NonCanonicalChunk);
    }
    Ok(entries)
}

impl SnapshotManifest {
    /// Canonical wire bytes: `payload || sha256(SNAPSHOT_MANIFEST_CONTEXT || payload)`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let payload = self.payload();
        let mut out = payload.clone();
        let checksum = sha256(&[SNAPSHOT_MANIFEST_CONTEXT, &payload].concat());
        out.extend_from_slice(checksum.as_bytes());
        out
    }

    /// Strict canonical decode: checksum, magic, version, minimal varints,
    /// contiguous descriptors, caps, no trailing bytes, byte-identical
    /// re-encoding, and a recomputed `chunk_root`.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, SnapshotError> {
        if bytes.len() < 32 {
            return Err(SnapshotError::TruncatedManifest);
        }
        let split = bytes.len() - 32;
        let (payload, checksum) = bytes.split_at(split);
        let expected = sha256(&[SNAPSHOT_MANIFEST_CONTEXT, payload].concat());
        if expected.as_bytes() != checksum {
            return Err(SnapshotError::BadManifestChecksum);
        }
        let mut reader = Reader::new(payload);
        let magic = reader.read_exact(4).map_err(SnapshotError::Decode)?;
        if magic != SNAPSHOT_MAGIC {
            return Err(SnapshotError::BadMagic);
        }
        let version = u32::decode(&mut reader).map_err(SnapshotError::Decode)?;
        if version != SNAPSHOT_VERSION {
            return Err(SnapshotError::UnsupportedVersion(SNAPSHOT_VERSION, version));
        }
        let chain_id = ChainId::decode(&mut reader).map_err(SnapshotError::Decode)?;
        let height = Height::decode(&mut reader).map_err(SnapshotError::Decode)?;
        let state_root = Hash::decode(&mut reader).map_err(SnapshotError::Decode)?;
        let root_digest = Hash::decode(&mut reader).map_err(SnapshotError::Decode)?;
        let chunk_size = u32::decode(&mut reader).map_err(SnapshotError::Decode)?;
        let total_keys = u64::decode(&mut reader).map_err(SnapshotError::Decode)?;
        let chunk_count = u32::decode(&mut reader).map_err(SnapshotError::Decode)?;
        if chunk_size == 0 {
            return Err(SnapshotError::ZeroChunkSize);
        }
        if chunk_count > MAX_CHUNKS {
            return Err(SnapshotError::TooManyChunks(chunk_count as usize));
        }
        let mut chunks = Vec::with_capacity(chunk_count as usize);
        let mut running_offset = 0u64;
        for _ in 0..chunk_count {
            let offset = u64::decode(&mut reader).map_err(SnapshotError::Decode)?;
            let length = u64::decode(&mut reader).map_err(SnapshotError::Decode)?;
            let mut digest = [0u8; 32];
            digest.copy_from_slice(reader.read_exact(32).map_err(SnapshotError::Decode)?);
            if offset != running_offset {
                return Err(SnapshotError::BadChunkOffset);
            }
            if length == 0 || length > MAX_CHUNK_BYTES {
                return Err(SnapshotError::BadChunkLength(length));
            }
            running_offset = offset + length;
            chunks.push(ChunkDescriptor {
                offset,
                length,
                sha256: Hash::from_bytes(digest),
            });
        }
        reader.finish().map_err(SnapshotError::Decode)?;
        let manifest = SnapshotManifest {
            version,
            chain_id,
            height,
            state_root,
            chunk_root: root_digest,
            chunk_size,
            total_keys,
            chunks,
        };
        if manifest.to_bytes() != bytes {
            return Err(SnapshotError::NonCanonicalManifest);
        }
        let digests: Vec<Hash> = manifest.chunks.iter().map(|d| d.sha256).collect();
        if chunk_root(&digests) != manifest.chunk_root {
            return Err(SnapshotError::ChunkRootMismatch);
        }
        Ok(manifest)
    }

    fn payload(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.push_bytes(SNAPSHOT_MAGIC);
        self.version.encode(&mut writer);
        self.chain_id.encode(&mut writer);
        self.height.encode(&mut writer);
        self.state_root.encode(&mut writer);
        self.chunk_root.encode(&mut writer);
        self.chunk_size.encode(&mut writer);
        self.total_keys.encode(&mut writer);
        (self.chunks.len() as u32).encode(&mut writer);
        for chunk in &self.chunks {
            chunk.offset.encode(&mut writer);
            chunk.length.encode(&mut writer);
            chunk.sha256.encode(&mut writer);
        }
        writer.into_bytes()
    }
}

/// Canonical manifest bytes of a snapshot, ready to persist.
pub fn manifest_bytes(snapshot: &Snapshot) -> Vec<u8> {
    snapshot.manifest.to_bytes()
}

/// Export a deterministic snapshot with **parallel** chunk hashing.
pub fn export_snapshot(
    entries: &[(Vec<u8>, Vec<u8>)],
    chain_id: ChainId,
    height: Height,
    root: Hash,
    chunk_size: u32,
    workers: usize,
) -> Result<Snapshot, SnapshotError> {
    if chunk_size == 0 {
        return Err(SnapshotError::ZeroChunkSize);
    }
    let mut payloads: Vec<Vec<u8>> = Vec::new();
    for chunk in entries.chunks(chunk_size as usize) {
        payloads.push(encode_chunk(chunk));
    }
    let digests = parallel_chunk_hashes(&payloads, workers)?;
    let mut descriptors = Vec::with_capacity(payloads.len());
    let mut data = Vec::new();
    for (payload, digest) in payloads.iter().zip(&digests) {
        descriptors.push(ChunkDescriptor {
            offset: data.len() as u64,
            length: payload.len() as u64,
            sha256: *digest,
        });
        data.extend_from_slice(payload);
    }
    let manifest = SnapshotManifest {
        version: SNAPSHOT_VERSION,
        chain_id,
        height,
        state_root: root,
        chunk_root: chunk_root(&digests),
        chunk_size,
        total_keys: entries.len() as u64,
        chunks: descriptors,
    };
    Ok(Snapshot { manifest, data })
}

/// Import a snapshot: verify every chunk digest in parallel, then decode.
///
/// NC target: `tools/nc_s4.py` disables the digest comparison here and requires
/// the substituted-chunk gate to go red.
pub fn import_snapshot(
    manifest_bytes: &[u8],
    data: &[u8],
    workers: usize,
) -> Result<StateEntries, SnapshotError> {
    let manifest = SnapshotManifest::from_canonical_bytes(manifest_bytes)?;
    let expected_len: u64 = manifest.chunks.iter().map(|c| c.length).sum();
    if data.len() as u64 != expected_len {
        return Err(SnapshotError::DataLengthMismatch(
            expected_len,
            data.len() as u64,
        ));
    }
    let payloads: Vec<&[u8]> = manifest
        .chunks
        .iter()
        .map(|c| {
            let start = c.offset as usize;
            &data[start..start + c.length as usize]
        })
        .collect();
    let computed = parallel_chunk_hashes(&payloads, workers)?;
    let digests_match = computed == manifest.chunks.iter().map(|c| c.sha256).collect::<Vec<_>>();
    if !digests_match {
        let index = computed
            .iter()
            .zip(&manifest.chunks)
            .position(|(hash, chunk)| *hash != chunk.sha256)
            .unwrap_or(0);
        return Err(SnapshotError::ChunkHashMismatch(index));
    }
    let mut entries: StateEntries = Vec::with_capacity(manifest.total_keys as usize);
    for payload in &payloads {
        entries.extend(decode_chunk(payload)?);
    }
    if entries.len() as u64 != manifest.total_keys {
        return Err(SnapshotError::EntryCountMismatch(
            manifest.total_keys,
            entries.len() as u64,
        ));
    }
    Ok(entries)
}

/// Hash chunk payloads in parallel with a bounded worker pool. Deterministic
/// output ordering (one digest per input chunk, in input order).
pub fn parallel_chunk_hashes<T: AsRef<[u8]> + Sync>(
    payloads: &[T],
    workers: usize,
) -> Result<Vec<Hash>, SnapshotError> {
    let count = payloads.len();
    if count > MAX_CHUNKS as usize {
        return Err(SnapshotError::TooManyChunks(count));
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let worker_count = workers.clamp(1, count.min(8));
    let mut out = vec![Hash::ZERO; count];
    let digests = std::sync::Mutex::new(&mut out);
    let next = AtomicUsize::new(0);

    thread::scope(|scope| -> Result<(), SnapshotError> {
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let digests = &digests;
            let next = &next;
            let payloads = &payloads;
            handles.push(scope.spawn(move || {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= payloads.len() {
                        break;
                    }
                    let hash = chunk_hash(payloads[index].as_ref());
                    let mut guard = digests.lock().expect("mutex poisoned");
                    guard[index] = hash;
                }
            }));
        }
        for handle in handles {
            handle.join().expect("worker panicked");
        }
        Ok(())
    })?;
    Ok(out)
}
