//! State key and account errors.

use std::fmt;

use aura_codec::CodecError;

use crate::key::KeySpace;

/// Every reason a state key or account operation can be rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// The key prefix is not one of the five protocol namespaces.
    ReservedKeySpace(u8),
    /// The key has no bytes at all.
    EmptyKey,
    /// The key is longer than [`crate::MAX_STATE_KEY_LEN`].
    KeyTooLong(usize),
    /// Known prefix but wrong total length for that namespace.
    InvalidKeyLength {
        /// Namespace decoded from the prefix.
        space: KeySpace,
        /// Required total length, including the prefix byte.
        expected: usize,
        /// Actual length.
        got: usize,
    },
    /// A contract tried to write outside its own storage namespace.
    ForbiddenContractWrite,
    /// A contract tried to write into another contract's storage.
    WrongContractStorage,
    /// Re-encoding a decoded key did not reproduce the input bytes.
    NonCanonicalKeyEncoding,
    /// Canonical decoding failed (structural).
    Decode(CodecError),
    /// Balance addition overflowed `u128`.
    BalanceOverflow,
    /// Balance subtraction would go below zero.
    InsufficientBalance,
    /// Nonce increment overflowed `u64`.
    NonceOverflow,
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReservedKeySpace(prefix) => {
                write!(f, "reserved key space prefix 0x{prefix:02x}")
            }
            Self::EmptyKey => f.write_str("empty state key"),
            Self::KeyTooLong(n) => write!(f, "state key of {n} bytes exceeds the limit"),
            Self::InvalidKeyLength {
                space,
                expected,
                got,
            } => write!(f, "{space:?} key must be {expected} bytes, got {got}"),
            Self::ForbiddenContractWrite => {
                f.write_str("contract may only write to its own storage namespace")
            }
            Self::WrongContractStorage => {
                f.write_str("contract tried to write another contract's storage")
            }
            Self::NonCanonicalKeyEncoding => {
                f.write_str("re-encoding did not reproduce the key bytes")
            }
            Self::Decode(error) => write!(f, "canonical decode failed: {error}"),
            Self::BalanceOverflow => f.write_str("balance overflow"),
            Self::InsufficientBalance => f.write_str("insufficient balance"),
            Self::NonceOverflow => f.write_str("nonce overflow"),
        }
    }
}

impl std::error::Error for StateError {}

impl From<CodecError> for StateError {
    fn from(error: CodecError) -> Self {
        StateError::Decode(error)
    }
}

/// Errors produced by the S3 execution overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayError {
    /// Gas limit exceeded. All state changes in this view are discarded.
    OutOfGas {
        /// The configured gas limit for this execution.
        limit: u64,
        /// Gas consumed before the failing charge.
        used: u64,
        /// Gas requested by the failing charge.
        needed: u64,
    },
    /// Explicit revert (VM `REVERT`); the reason is opaque bytes-safe text.
    Revert(String),
    /// A canonical diff failed its BLAKE3 checksum.
    BadChecksum,
    /// A canonical diff has the wrong magic prefix.
    BadMagic,
    /// A canonical diff has bytes after the checksum.
    TrailingBytes(usize),
    /// A canonical diff was truncated.
    UnexpectedEof,
    /// A length prefix was not minimally encoded.
    NonCanonicalVarint,
    /// Diff changes are not in strictly ascending path order (or duplicate).
    UnsortedChanges,
    /// Diff contains more than `MAX_WRITES` changes.
    TooManyWrites(usize),
    /// Diff contains more than `MAX_EVENTS` events.
    TooManyEvents(usize),
    /// An option tag was neither `0` nor `1`.
    InvalidOptionTag(u8),
    /// A value or event exceeds the configured size cap.
    ValueTooLarge(usize),
    /// A diff value could not be applied to the base tree.
    RootMismatch,
    /// The base root in the diff does not match the base tree.
    BaseRootMismatch,
    /// A write exceeded the `usize` range / checked arithmetic overflow.
    Overflow,
    /// A canonical diff could not be decoded.
    Decode(String),
}

impl fmt::Display for OverlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfGas {
                limit,
                used,
                needed,
            } => write!(f, "out of gas: used {used} + {needed} > limit {limit}"),
            Self::Revert(reason) => write!(f, "revert: {reason}"),
            Self::BadChecksum => f.write_str("diff checksum mismatch"),
            Self::BadMagic => f.write_str("diff magic prefix mismatch"),
            Self::TrailingBytes(n) => write!(f, "{n} trailing bytes after diff"),
            Self::UnexpectedEof => f.write_str("diff truncated"),
            Self::NonCanonicalVarint => f.write_str("non-minimal varint in diff"),
            Self::UnsortedChanges => f.write_str("diff changes are not strictly ascending"),
            Self::TooManyWrites(n) => write!(f, "diff has {n} writes, over the cap"),
            Self::TooManyEvents(n) => write!(f, "diff has {n} events, over the cap"),
            Self::InvalidOptionTag(tag) => write!(f, "invalid option tag {tag}"),
            Self::ValueTooLarge(n) => write!(f, "value of {n} bytes exceeds the cap"),
            Self::RootMismatch => f.write_str("diff result root does not match materialised tree"),
            Self::BaseRootMismatch => f.write_str("diff base root does not match base tree"),
            Self::Overflow => f.write_str("checked arithmetic overflow in overlay"),
            Self::Decode(message) => write!(f, "diff is not canonical: {message}"),
        }
    }
}

impl std::error::Error for OverlayError {}

impl From<aura_codec::CodecError> for OverlayError {
    fn from(error: aura_codec::CodecError) -> Self {
        use aura_codec::CodecError as E;
        match error {
            E::UnexpectedEof(_, _) | E::VarintOverflow => OverlayError::UnexpectedEof,
            E::NonCanonicalVarint => OverlayError::NonCanonicalVarint,
            E::TrailingBytes(n) => OverlayError::TrailingBytes(n),
            other => OverlayError::Decode(other.to_string()),
        }
    }
}

/// Errors produced by S4 snapshot export/import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    /// Canonical decoding failed.
    Decode(aura_codec::CodecError),
    /// An invalid C7 state key was found.
    Key(StateError),
    /// A chunk declares too many entries.
    TooManyEntries(usize),
    /// A key is longer than the cap.
    KeyTooLarge(usize),
    /// A value is longer than the cap.
    ValueTooLarge(usize),
    /// Keys are not strictly ascending or contain a duplicate.
    UnsortedKeys,
    /// Re-encoding a chunk did not reproduce its bytes.
    NonCanonicalChunk,
    /// The manifest is shorter than its own checksum.
    TruncatedManifest,
    /// The manifest SHA-256 checksum does not match.
    BadManifestChecksum,
    /// The manifest magic prefix is wrong.
    BadMagic,
    /// Unsupported snapshot wire version. Tuple is `(expected, got)`.
    UnsupportedVersion(u32, u32),
    /// The chain-id in the manifest is invalid.
    BadChainId,
    /// `chunk_size` must be non-zero.
    ZeroChunkSize,
    /// Too many chunks in one snapshot.
    TooManyChunks(usize),
    /// Descriptor offsets are not contiguous.
    BadChunkOffset,
    /// A chunk length is zero or over the cap.
    BadChunkLength(u64),
    /// Re-encoding the manifest did not reproduce its bytes.
    NonCanonicalManifest,
    /// The BLAKE3 chunk root does not match the descriptors.
    ChunkRootMismatch,
    /// Data blob length does not match the descriptors. Tuple is
    /// `(expected, got)`.
    DataLengthMismatch(u64, u64),
    /// A chunk's SHA-256 does not match its descriptor. Value is the chunk index.
    ChunkHashMismatch(usize),
    /// Decoded entry count does not match `total_keys`. Tuple is `(expected, got)`.
    EntryCountMismatch(u64, u64),
    /// A parallel worker failed.
    Parallel(String),
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "canonical decode failed: {e}"),
            Self::Key(e) => write!(f, "invalid state key: {e}"),
            Self::TooManyEntries(n) => write!(f, "chunk has {n} entries, over the cap"),
            Self::KeyTooLarge(n) => write!(f, "key of {n} bytes exceeds the cap"),
            Self::ValueTooLarge(n) => write!(f, "value of {n} bytes exceeds the cap"),
            Self::UnsortedKeys => f.write_str("keys are not strictly ascending"),
            Self::NonCanonicalChunk => f.write_str("chunk is not canonically encoded"),
            Self::TruncatedManifest => f.write_str("manifest is truncated"),
            Self::BadManifestChecksum => f.write_str("manifest checksum mismatch"),
            Self::BadMagic => f.write_str("snapshot magic prefix mismatch"),
            Self::UnsupportedVersion(expected, got) => {
                write!(f, "unsupported snapshot version {got}, expected {expected}")
            }
            Self::BadChainId => f.write_str("invalid chain-id in manifest"),
            Self::ZeroChunkSize => f.write_str("chunk_size must be non-zero"),
            Self::TooManyChunks(n) => write!(f, "snapshot has {n} chunks, over the cap"),
            Self::BadChunkOffset => f.write_str("chunk offsets are not contiguous"),
            Self::BadChunkLength(n) => write!(f, "bad chunk length {n}"),
            Self::NonCanonicalManifest => f.write_str("manifest is not canonically encoded"),
            Self::ChunkRootMismatch => f.write_str("chunk root does not match descriptors"),
            Self::DataLengthMismatch(expected, got) => {
                write!(f, "data length {got}, expected {expected}")
            }
            Self::ChunkHashMismatch(index) => {
                write!(f, "chunk {index} SHA-256 mismatch (substituted chunk)")
            }
            Self::EntryCountMismatch(expected, got) => {
                write!(f, "entry count {got}, expected {expected}")
            }
            Self::Parallel(message) => write!(f, "parallel worker failed: {message}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

impl From<aura_codec::CodecError> for SnapshotError {
    fn from(error: aura_codec::CodecError) -> Self {
        SnapshotError::Decode(error)
    }
}

/// Errors produced by S5 state sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    /// Canonical decoding failed.
    Decode(aura_codec::CodecError),
    /// The S4 snapshot import failed.
    Snapshot(SnapshotError),
    /// A head, delta or package SHA-256 checksum does not match.
    ChecksumMismatch,
    /// A magic prefix is wrong.
    BadMagic,
    /// Unsupported wire version. Tuple is `(expected, got)`.
    UnsupportedVersion(u32, u32),
    /// A sync head has the wrong chain-id.
    WrongChainId,
    /// The quorum of heads is not unanimous — a node may be lying.
    QuorumMismatch,
    /// Wrong number of heads.
    HeadCount(usize),
    /// Delta heights are inconsistent with the heads / manifest.
    HeightMismatch,
    /// Delta changes are not strictly ascending or contain a duplicate.
    UnsortedChanges,
    /// Too many delta changes.
    TooManyChanges(usize),
    /// A delta change key is longer than the cap.
    KeyTooLarge(usize),
    /// A delta change value is longer than the cap.
    ValueTooLarge(usize),
    /// The delta declares the wrong number of changes.
    ChangeCountMismatch(u32, usize),
    /// Recomputed state root after snapshot+delta does not match the quorum.
    RootMismatch,
    /// Snapshot `chunk_root` does not match the head's `snapshot_root`.
    SnapshotRootMismatch,
    /// The package is not canonically encoded.
    NonCanonical,
    /// A blob is longer than the cap.
    BlobTooLarge(usize),
    /// Sync took longer than the allowed budget. Value is elapsed milliseconds.
    TooSlow(u64),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "canonical decode failed: {e}"),
            Self::Snapshot(e) => write!(f, "snapshot import failed: {e}"),
            Self::ChecksumMismatch => f.write_str("sync checksum mismatch"),
            Self::BadMagic => f.write_str("sync magic prefix mismatch"),
            Self::UnsupportedVersion(expected, got) => {
                write!(f, "unsupported sync version {got}, expected {expected}")
            }
            Self::WrongChainId => f.write_str("sync head has the wrong chain-id"),
            Self::QuorumMismatch => f.write_str("head quorum is not unanimous"),
            Self::HeadCount(n) => write!(f, "package has {n} heads, expected the quorum size"),
            Self::HeightMismatch => f.write_str("delta heights do not match the heads/manifest"),
            Self::UnsortedChanges => f.write_str("delta changes are not strictly ascending"),
            Self::TooManyChanges(n) => write!(f, "delta has {n} changes, over the cap"),
            Self::KeyTooLarge(n) => write!(f, "delta key of {n} bytes exceeds the cap"),
            Self::ValueTooLarge(n) => write!(f, "delta value of {n} bytes exceeds the cap"),
            Self::ChangeCountMismatch(expected, got) => {
                write!(f, "delta decoded {got} changes, expected {expected}")
            }
            Self::RootMismatch => f.write_str("recomputed state root does not match the quorum"),
            Self::SnapshotRootMismatch => {
                f.write_str("snapshot chunk_root does not match the head")
            }
            Self::NonCanonical => f.write_str("sync object is not canonically encoded"),
            Self::BlobTooLarge(n) => write!(f, "sync blob of {n} bytes exceeds the cap"),
            Self::TooSlow(ms) => write!(f, "sync took {ms} ms, over the budget"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<aura_codec::CodecError> for SyncError {
    fn from(error: aura_codec::CodecError) -> Self {
        SyncError::Decode(error)
    }
}

impl From<SnapshotError> for SyncError {
    fn from(error: SnapshotError) -> Self {
        SyncError::Snapshot(error)
    }
}
