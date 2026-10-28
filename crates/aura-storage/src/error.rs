//! Storage errors.

use std::fmt;

/// Every way durable state can fail to open, commit or recover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// RocksDB reported an error (message kept opaque).
    Backend(String),
    /// A WAL record failed its BLAKE3 checksum.
    ChecksumMismatch,
    /// A WAL record has a bad magic prefix or cannot be decoded.
    CorruptWal,
    /// WAL versions are not consecutive.
    NonSequentialWal,
    /// A recovered record's parent root does not match the current root.
    ParentRootMismatch,
    /// Recomputed state root does not match the published root.
    RootMismatch,
    /// The WAL contains no record for a published head.
    MissingWalRecord,
    /// Filesystem I/O failure.
    Io(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(message) => write!(f, "rocksdb: {message}"),
            Self::ChecksumMismatch => f.write_str("WAL checksum mismatch"),
            Self::CorruptWal => f.write_str("corrupt WAL record"),
            Self::NonSequentialWal => f.write_str("WAL versions are not consecutive"),
            Self::ParentRootMismatch => f.write_str("WAL parent root mismatch"),
            Self::RootMismatch => f.write_str("state root does not match published root"),
            Self::MissingWalRecord => f.write_str("head published without a WAL record"),
            Self::Io(message) => write!(f, "I/O error: {message}"),
        }
    }
}

impl std::error::Error for StorageError {}

/// Errors produced by S6 version retention (prune / archive / compaction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PruneError {
    /// `keep_versions` must be in `1..=MAX_KEEP_VERSIONS`.
    InvalidKeepVersions(u64),
    /// A version is older than what this store still retains.
    Pruned {
        /// Requested version.
        version: u64,
        /// Oldest version this store can still serve.
        oldest: u64,
    },
    /// A version was never committed or is beyond the head.
    VersionNotFound(u64),
    /// Archive nodes may not compact: that would destroy served history.
    ArchiveCompactionForbidden,
    /// A record payload failed its BLAKE3 checksum.
    ChecksumMismatch,
    /// A record magic prefix is wrong.
    BadMagic,
    /// A record is structurally invalid (truncated, over-cap, bad tag).
    CorruptRecord,
    /// A record is not canonically encoded.
    NonCanonical,
    /// A record carries too many changes.
    TooManyChanges(usize),
    /// A recomputed state root does not match the stored root.
    RootMismatch,
    /// The S1 tree rejected the operation.
    Tree(aura_state::TreeError),
}

impl fmt::Display for PruneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKeepVersions(keep) => {
                write!(f, "keep_versions {keep} is outside 1..=MAX_KEEP_VERSIONS")
            }
            Self::Pruned { version, oldest } => {
                write!(
                    f,
                    "version {version} was pruned (oldest served is {oldest})"
                )
            }
            Self::VersionNotFound(version) => write!(f, "version {version} was never committed"),
            Self::ArchiveCompactionForbidden => {
                f.write_str("archive mode must not compact: it would drop served history")
            }
            Self::ChecksumMismatch => f.write_str("version-record checksum mismatch"),
            Self::BadMagic => f.write_str("version-record magic prefix mismatch"),
            Self::CorruptRecord => f.write_str("corrupt version record"),
            Self::NonCanonical => f.write_str("version record is not canonically encoded"),
            Self::TooManyChanges(count) => {
                write!(f, "version record has {count} changes, over the cap")
            }
            Self::RootMismatch => {
                f.write_str("recomputed state root does not match the stored root")
            }
            Self::Tree(error) => write!(f, "S1 tree error: {error}"),
        }
    }
}

impl std::error::Error for PruneError {}

impl From<std::io::Error> for StorageError {
    fn from(error: std::io::Error) -> Self {
        StorageError::Io(error.to_string())
    }
}

/// Errors produced by S7 backup and restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupError {
    /// The bundle or manifest is cut short.
    Truncated,
    /// A magic prefix is wrong.
    BadMagic,
    /// Unknown backup format version.
    UnsupportedVersion(u32),
    /// Manifest checksum mismatch.
    ChecksumMismatch,
    /// Manifest length prefix is out of range.
    BadManifestLength(usize),
    /// Chain id is empty, too long or not UTF-8.
    BadChainId,
    /// The manifest chain id is not the one the caller expects.
    WrongChainId {
        /// Chain id the caller asked for.
        expected: String,
        /// Chain id found in the manifest.
        got: String,
    },
    /// Declared length does not match the actual one.
    LengthMismatch {
        /// Length declared in the manifest.
        expected: u64,
        /// Length actually present.
        got: u64,
    },
    /// Blob is larger than the cap.
    BlobTooLarge(u64),
    /// SHA-256 of the blob does not match the manifest.
    BlobHashMismatch,
    /// A WAL record inside the blob failed its BLAKE3 checksum.
    RecordChecksum,
    /// A WAL record inside the blob cannot be decoded.
    RecordCorrupt,
    /// Manifest is not canonically encoded.
    NonCanonical,
    /// Records are not consecutive.
    NonSequential {
        /// Expected version.
        expected: u64,
        /// Version found.
        got: u64,
    },
    /// A record's parent root does not chain to the previous record.
    ParentRootMismatch,
    /// Replaying a record produced a different root than the backup declares.
    RootMismatch {
        /// Version whose root disagreed.
        version: u64,
    },
    /// Restored head does not match the manifest.
    HeadMismatch,
    /// The target directory already holds a store.
    TargetNotEmpty,
    /// The durable store refused an operation.
    Storage(StorageError),
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("backup bundle is truncated"),
            Self::BadMagic => f.write_str("backup manifest magic mismatch"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported backup format version {version}")
            }
            Self::ChecksumMismatch => f.write_str("backup manifest checksum mismatch"),
            Self::BadManifestLength(len) => write!(f, "manifest length {len} is out of range"),
            Self::BadChainId => f.write_str("backup chain-id is empty, too long or not UTF-8"),
            Self::WrongChainId { expected, got } => {
                write!(f, "backup is for chain {got}, expected {expected}")
            }
            Self::LengthMismatch { expected, got } => {
                write!(f, "declared length {expected} != actual {got}")
            }
            Self::BlobTooLarge(len) => write!(f, "backup blob of {len} bytes is over the cap"),
            Self::BlobHashMismatch => f.write_str("backup blob SHA-256 mismatch"),
            Self::RecordChecksum => f.write_str("backup record checksum mismatch"),
            Self::RecordCorrupt => f.write_str("corrupt backup record"),
            Self::NonCanonical => f.write_str("backup manifest is not canonically encoded"),
            Self::NonSequential { expected, got } => {
                write!(f, "backup records jump from {expected} to {got}")
            }
            Self::ParentRootMismatch => f.write_str("backup record parent root mismatch"),
            Self::RootMismatch { version } => {
                write!(
                    f,
                    "replayed root of version {version} does not match the backup"
                )
            }
            Self::HeadMismatch => f.write_str("restored head does not match the manifest"),
            Self::TargetNotEmpty => f.write_str("restore target already contains a store"),
            Self::Storage(error) => write!(f, "storage error during restore: {error}"),
        }
    }
}

impl std::error::Error for BackupError {}

impl From<StorageError> for BackupError {
    fn from(error: StorageError) -> Self {
        BackupError::Storage(error)
    }
}
