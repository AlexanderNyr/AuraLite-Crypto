//! W1 versioned encrypted keystore: Argon2id or scrypt + AES-256-GCM.
//!
//! The file stores only the encrypted 64-byte BIP-39 seed and public account
//! metadata. Mnemonics and plaintext seeds are never written to disk.

use std::collections::BTreeSet;
#[cfg(not(any(target_os = "redox", target_os = "espidf")))]
use std::fs;
#[cfg(not(unix))]
use std::fs::OpenOptions;
#[cfg(not(any(target_os = "redox", target_os = "espidf")))]
use std::io::{Read, Write};
use std::path::Path;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use rand_core::{OsRng, RngCore};
use serde_json::{Map, Value, json};
use zeroize::Zeroizing;

use crate::derivation::{self, AURA_PATH_TEMPLATE, Bip39NormalizationProfile, TESTNET_COIN_TYPE};
use crate::secret::{Password, SecretSeed};

/// Maximum keystore file size accepted by the loader.
pub const MAX_KEYSTORE_BYTES: u64 = 1_048_576;
/// Keystore format name.
pub const FORMAT: &str = "auralite-keystore";
/// Current keystore format version; version 2 authenticates the BIP-39 profile marker.
pub const VERSION: u64 = 2;
/// Prior W1 prototype version, which has no normalization-profile metadata.
const LEGACY_VERSION: u64 = 1;

/// Keystore errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeystoreError {
    /// Invalid JSON or missing/unknown fields.
    InvalidFormat,
    /// Unsupported format/version/cipher/KDF or unknown normalization-profile ID.
    Unsupported,
    /// Recovery marker or keystore does not contain a known normalization profile.
    RecoveryProfileMissing,
    /// Recovery-backup profile and authenticated keystore profile disagree.
    RecoveryProfileMismatch,
    /// KDF parameters are outside the accepted safety bounds.
    WeakKdfParameters,
    /// Password was wrong or ciphertext/header was tampered with.
    AuthenticationFailed,
    /// I/O failure.
    Io(String),
    /// File already exists and overwrite was not requested.
    AlreadyExists,
    /// Parent directory is not trusted, or a pathname changed during publication.
    UnsafeDirectory,
    /// Filesystem-backed keystores are unavailable on this platform.
    UnsupportedPlatform,
    /// Derivation failed.
    Derivation(String),
}

impl std::fmt::Display for KeystoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFormat => f.write_str("invalid keystore format"),
            Self::Unsupported => {
                f.write_str("unsupported keystore version, cipher, KDF or profile ID")
            }
            Self::RecoveryProfileMissing => {
                f.write_str("recovery normalization profile is missing")
            }
            Self::RecoveryProfileMismatch => {
                f.write_str("recovery and keystore normalization profiles do not match")
            }
            Self::WeakKdfParameters => {
                f.write_str("KDF parameters are weak or outside resource limits")
            }
            Self::AuthenticationFailed => f.write_str("keystore authentication failed"),
            Self::Io(e) => write!(f, "keystore I/O error: {e}"),
            Self::AlreadyExists => f.write_str("keystore file already exists"),
            Self::UnsafeDirectory => {
                f.write_str("keystore directory or publication path is unsafe")
            }
            Self::UnsupportedPlatform => {
                f.write_str("keystore filesystem operations are unsupported on this platform")
            }
            Self::Derivation(e) => write!(f, "derivation error: {e}"),
        }
    }
}

impl std::error::Error for KeystoreError {}

/// Explicit KDF parameters stored in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KdfParams {
    /// Argon2id with memory cost in KiB, iterations and parallelism.
    Argon2id {
        /// Memory cost in KiB.
        m_cost: u32,
        /// Time cost.
        t_cost: u32,
        /// Parallelism.
        p_cost: u32,
    },
    /// scrypt with log2(N), r and p.
    Scrypt {
        /// log2(N).
        log_n: u8,
        /// Block size.
        r: u32,
        /// Parallelism.
        p: u32,
    },
}

impl KdfParams {
    /// Production default: Argon2id, 64 MiB, t=3, p=1.
    pub fn default_argon2id() -> Self {
        Self::Argon2id {
            m_cost: 65_536,
            t_cost: 3,
            p_cost: 1,
        }
    }

    /// Test-only low-cost parameters. Never use for real funds.
    #[cfg(feature = "insecure-test-kdf")]
    pub fn testing() -> Self {
        Self::Argon2id {
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        }
    }

    // Structural/resource bounds are also used for explicit legacy migration.
    fn validate_limits(self) -> Result<(), KeystoreError> {
        match self {
            Self::Argon2id {
                m_cost,
                t_cost,
                p_cost,
            } => {
                let minimum_memory = if cfg!(feature = "insecure-test-kdf") {
                    8
                } else {
                    19_456
                };
                let minimum_time = if cfg!(feature = "insecure-test-kdf") {
                    1
                } else {
                    2
                };
                if !(minimum_memory..=262_144).contains(&m_cost)
                    || !(minimum_time..=10).contains(&t_cost)
                    || !(1..=4).contains(&p_cost)
                {
                    return Err(KeystoreError::WeakKdfParameters);
                }
            }
            Self::Scrypt { log_n, r, p } => {
                if !(14..=18).contains(&log_n)
                    || !(8..=16).contains(&r)
                    || !(1..=4).contains(&p)
                    || (1u64 << log_n) * u64::from(r) * 128 > 268_435_456
                {
                    return Err(KeystoreError::WeakKdfParameters);
                }
            }
        }
        Ok(())
    }

    // Ordinary APIs enforce minimum Argon2id and scrypt work factors even
    // when the explicitly insecure testing feature is enabled.
    fn validate(self) -> Result<(), KeystoreError> {
        self.validate_limits()?;
        // The feature must never relax the policy of the ordinary API.
        // Bounded weak parameters are accepted only by explicitly named test
        // methods, or by the separate scrypt migration-only method.
        if matches!(
            self,
            Self::Argon2id {
                m_cost: 0..=19_455,
                ..
            }
        ) || matches!(self, Self::Argon2id { t_cost: 0..=1, .. })
        {
            return Err(KeystoreError::WeakKdfParameters);
        }
        if matches!(self, Self::Scrypt { log_n: 14..=16, .. }) {
            return Err(KeystoreError::WeakKdfParameters);
        }
        Ok(())
    }

    fn to_json(self, salt: &[u8]) -> Value {
        match self {
            Self::Argon2id {
                m_cost,
                t_cost,
                p_cost,
            } => json!({
                "name": "argon2id", "salt": hex::encode(salt),
                "m_cost": m_cost, "t_cost": t_cost, "p_cost": p_cost,
            }),
            Self::Scrypt { log_n, r, p } => json!({
                "name": "scrypt", "salt": hex::encode(salt),
                "log_n": log_n, "r": r, "p": p,
            }),
        }
    }

    fn from_json(v: &Value) -> Result<(Self, Vec<u8>), KeystoreError> {
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .ok_or(KeystoreError::InvalidFormat)?;
        let salt = hex::decode(
            v.get("salt")
                .and_then(Value::as_str)
                .ok_or(KeystoreError::InvalidFormat)?,
        )
        .map_err(|_| KeystoreError::InvalidFormat)?;
        if salt.len() != 16 {
            return Err(KeystoreError::InvalidFormat);
        }
        let params = match name {
            "argon2id" => {
                exact_fields(v, &["name", "salt", "m_cost", "t_cost", "p_cost"])?;
                Self::Argon2id {
                    m_cost: u32::try_from(
                        v.get("m_cost")
                            .and_then(Value::as_u64)
                            .ok_or(KeystoreError::InvalidFormat)?,
                    )
                    .map_err(|_| KeystoreError::InvalidFormat)?,
                    t_cost: u32::try_from(
                        v.get("t_cost")
                            .and_then(Value::as_u64)
                            .ok_or(KeystoreError::InvalidFormat)?,
                    )
                    .map_err(|_| KeystoreError::InvalidFormat)?,
                    p_cost: u32::try_from(
                        v.get("p_cost")
                            .and_then(Value::as_u64)
                            .ok_or(KeystoreError::InvalidFormat)?,
                    )
                    .map_err(|_| KeystoreError::InvalidFormat)?,
                }
            }
            "scrypt" => {
                exact_fields(v, &["name", "salt", "log_n", "r", "p"])?;
                Self::Scrypt {
                    log_n: u8::try_from(
                        v.get("log_n")
                            .and_then(Value::as_u64)
                            .ok_or(KeystoreError::InvalidFormat)?,
                    )
                    .map_err(|_| KeystoreError::InvalidFormat)?,
                    r: u32::try_from(
                        v.get("r")
                            .and_then(Value::as_u64)
                            .ok_or(KeystoreError::InvalidFormat)?,
                    )
                    .map_err(|_| KeystoreError::InvalidFormat)?,
                    p: u32::try_from(
                        v.get("p")
                            .and_then(Value::as_u64)
                            .ok_or(KeystoreError::InvalidFormat)?,
                    )
                    .map_err(|_| KeystoreError::InvalidFormat)?,
                }
            }
            _ => return Err(KeystoreError::Unsupported),
        };
        params.validate_limits()?;
        Ok((params, salt))
    }

    fn derive_key(
        self,
        password: &Password,
        salt: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, KeystoreError> {
        // Callers select default or explicit legacy policy before reaching us.
        self.validate_limits()?;
        // This wrapper also wipes a partially derived key if the KDF returns an
        // error or unwinds before cipher construction completes.
        let mut key = Zeroizing::new([0u8; 32]);
        match self {
            Self::Argon2id {
                m_cost,
                t_cost,
                p_cost,
            } => {
                let params = Params::new(m_cost, t_cost, p_cost, Some(32))
                    .map_err(|_| KeystoreError::WeakKdfParameters)?;
                // The crate's allocating convenience method drops its large
                // password-derived memory without wiping it. Own that memory
                // explicitly so Zeroizing also wipes it on errors or unwind.
                let mut blocks =
                    Zeroizing::new(vec![argon2::Block::default(); params.block_count()]);
                let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
                argon
                    .hash_password_into_with_memory(
                        password.as_str().as_bytes(),
                        salt,
                        &mut key[..],
                        &mut blocks[..],
                    )
                    .map_err(|_| KeystoreError::WeakKdfParameters)?;
            }
            Self::Scrypt { log_n, r, p } => {
                let params = scrypt_w1_wipe::Params::new(log_n, r, p, 32)
                    .map_err(|_| KeystoreError::WeakKdfParameters)?;
                scrypt_w1_wipe::scrypt(password.as_str().as_bytes(), salt, &params, &mut key[..])
                    .map_err(|_| KeystoreError::WeakKdfParameters)?;
            }
        }
        Ok(key)
    }
}

/// Non-secret profile marker that travels with a mnemonic recovery backup.
/// The mnemonic itself is deliberately not part of this JSON metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryMetadata {
    normalization_profile: Bip39NormalizationProfile,
}

impl RecoveryMetadata {
    /// Create a recovery marker for an explicitly selected profile.
    pub const fn new(normalization_profile: Bip39NormalizationProfile) -> Self {
        Self {
            normalization_profile,
        }
    }

    /// Return the explicit profile carried by this marker.
    pub const fn normalization_profile(self) -> Bip39NormalizationProfile {
        self.normalization_profile
    }

    /// Serialize the non-secret recovery marker as compact JSON.
    pub fn to_json_string(self) -> Result<String, KeystoreError> {
        serde_json::to_string(&json!({
            "normalization_profile": self.normalization_profile.id(),
        }))
        .map_err(|_| KeystoreError::InvalidFormat)
    }

    /// Parse a strict recovery marker. Missing/unknown profile IDs fail; this
    /// parser never substitutes the new-mnemonic default or probes both tables.
    pub fn from_json_str(text: &str) -> Result<Self, KeystoreError> {
        if text.len() > 4096 {
            return Err(KeystoreError::InvalidFormat);
        }
        let document: Value =
            serde_json::from_str(text).map_err(|_| KeystoreError::InvalidFormat)?;
        DuplicateKeyChecker::check(text)?;
        let object = document.as_object().ok_or(KeystoreError::InvalidFormat)?;
        if !object.contains_key("normalization_profile") {
            return Err(KeystoreError::RecoveryProfileMissing);
        }
        exact_fields(&document, &["normalization_profile"])?;
        let id = document["normalization_profile"]
            .as_str()
            .ok_or(KeystoreError::RecoveryProfileMissing)?;
        let normalization_profile =
            Bip39NormalizationProfile::from_id(id).ok_or(KeystoreError::Unsupported)?;
        Ok(Self {
            normalization_profile,
        })
    }
}

/// A parsed, encrypted keystore document.
#[derive(Debug, Clone)]
pub struct Keystore {
    document: Value,
}

impl Keystore {
    /// Encrypt a seed into a new keystore document.
    pub fn encrypt_seed(
        seed: &SecretSeed,
        password: &Password,
        kdf: KdfParams,
    ) -> Result<Self, KeystoreError> {
        Self::encrypt_seed_inner(seed, password, kdf, None, false)
    }

    /// Encrypt with intentionally weak, bounded KDF parameters for tests only.
    /// Never use this method for funds or application keystores, including in
    /// release builds. It is available only with `insecure-test-kdf` enabled.
    #[cfg(feature = "insecure-test-kdf")]
    pub fn encrypt_seed_insecure_for_testing(
        seed: &SecretSeed,
        password: &Password,
        kdf: KdfParams,
    ) -> Result<Self, KeystoreError> {
        Self::encrypt_seed_inner(seed, password, kdf, None, true)
    }

    fn encrypt_seed_inner(
        seed: &SecretSeed,
        password: &Password,
        kdf: KdfParams,
        normalization_profile: Option<Bip39NormalizationProfile>,
        allow_insecure_test: bool,
    ) -> Result<Self, KeystoreError> {
        if allow_insecure_test {
            kdf.validate_limits()?;
        } else {
            kdf.validate()?;
        }
        let mut id = [0u8; 16];
        let mut salt = [0u8; 16];
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut id);
        OsRng.fill_bytes(&mut salt);
        OsRng.fill_bytes(&mut nonce_bytes);
        let key = kdf.derive_key(password, &salt)?;
        let cipher = Aes256Gcm::new_from_slice(&key[..]).map_err(|_| KeystoreError::Unsupported)?;
        drop(key);
        let account = derivation::derive_account(seed, 0)
            .map_err(|e| KeystoreError::Derivation(e.to_string()))?;
        let mut document = json!({
            "format": FORMAT,
            "version": VERSION,
            "id": hex::encode(id),
            "kdf": kdf.to_json(&salt),
            "cipher": { "name": "aes-256-gcm", "nonce": hex::encode(nonce_bytes) },
            "seed_derivation": {
                "normalization_profile": normalization_profile.map(Bip39NormalizationProfile::id),
            },
            "derivation": {
                "scheme": "slip10-ed25519",
                "coin_type": TESTNET_COIN_TYPE,
                "path_template": AURA_PATH_TEMPLATE,
            },
            "accounts": [{
                "index": 0,
                "path": account.path,
                "public_key": hex::encode(account.public_key),
                "address": account.address.to_string(),
            }],
        });
        let aad = header_aad(&document)?;
        let ciphertext = cipher
            .encrypt(
                &Nonce::from(nonce_bytes),
                Payload {
                    msg: seed.as_bytes(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| KeystoreError::AuthenticationFailed)?;
        document["ciphertext"] = Value::String(hex::encode(ciphertext));
        Ok(Self { document })
    }

    /// Create a keystore from a BIP-39 mnemonic using an explicit NFKD profile.
    /// The phrase is never stored; the selected profile ID is authenticated in
    /// the keystore header. New mnemonic flows should pass
    /// `DEFAULT_BIP39_NORMALIZATION_PROFILE`; recovery flows must use the ID
    /// carried by recovery metadata or a deliberate user selection.
    pub fn create_from_mnemonic_with_profile(
        phrase: &str,
        mnemonic_passphrase: &str,
        normalization_profile: Bip39NormalizationProfile,
        password: &Password,
        kdf: KdfParams,
    ) -> Result<Self, KeystoreError> {
        let seed = derivation::mnemonic_to_seed_with_profile(
            phrase,
            mnemonic_passphrase,
            normalization_profile,
        )
        .map_err(|e| KeystoreError::Derivation(e.to_string()))?;
        Self::encrypt_seed_inner(&seed, password, kdf, Some(normalization_profile), false)
    }

    /// Decrypt the seed. Wrong passwords and tampered files fail closed.
    pub fn decrypt_seed(&self, password: &Password) -> Result<SecretSeed, KeystoreError> {
        self.decrypt_seed_inner(password, false, false)
    }

    /// Decrypt with intentionally weak, bounded KDF parameters for tests only.
    /// Never use this method for funds or application keystores, including in
    /// release builds. It is available only with `insecure-test-kdf` enabled.
    #[cfg(feature = "insecure-test-kdf")]
    pub fn decrypt_seed_insecure_for_testing(
        &self,
        password: &Password,
    ) -> Result<SecretSeed, KeystoreError> {
        self.decrypt_seed_inner(password, false, true)
    }

    /// Decrypt a legacy weak-scrypt document solely to migrate it to a stronger
    /// KDF. This is an explicit opt-in: log2(N) 14–16 remains weak, and callers
    /// must re-encrypt the returned seed rather than continue using that file.
    /// Other KDFs and documents outside the resource bounds are rejected.
    pub fn decrypt_legacy_scrypt_for_migration(
        &self,
        password: &Password,
    ) -> Result<SecretSeed, KeystoreError> {
        self.decrypt_seed_inner(password, true, false)
    }

    /// Return the profile marker from the document, if present. Treat this as
    /// unauthenticated input until `decrypt_seed` succeeds.
    pub fn normalization_profile(&self) -> Option<Bip39NormalizationProfile> {
        if self.document["version"].as_u64() != Some(VERSION) {
            return None;
        }
        self.document["seed_derivation"]["normalization_profile"]
            .as_str()
            .and_then(Bip39NormalizationProfile::from_id)
    }

    /// Return a recovery marker suitable for a separate mnemonic backup
    /// manifest. This does not include or write the mnemonic.
    pub fn recovery_metadata(&self) -> Option<RecoveryMetadata> {
        self.normalization_profile().map(RecoveryMetadata::new)
    }

    /// Verify a recovered mnemonic against both the backup marker and this
    /// encrypted keystore. The password is required so the keystore's marker
    /// is authenticated before any profile match is trusted.
    pub fn verify_mnemonic_recovery(
        &self,
        phrase: &str,
        mnemonic_passphrase: &str,
        recovery_metadata: RecoveryMetadata,
        password: &Password,
    ) -> Result<(), KeystoreError> {
        let keystore_seed = self.decrypt_seed(password)?;
        let stored_profile = self
            .normalization_profile()
            .ok_or(KeystoreError::RecoveryProfileMissing)?;
        if stored_profile != recovery_metadata.normalization_profile() {
            return Err(KeystoreError::RecoveryProfileMismatch);
        }
        let recovered_seed = derivation::mnemonic_to_seed_with_profile(
            phrase,
            mnemonic_passphrase,
            recovery_metadata.normalization_profile(),
        )
        .map_err(|e| KeystoreError::Derivation(e.to_string()))?;
        if !aura_crypto::constant_time_eq(recovered_seed.as_bytes(), keystore_seed.as_bytes()) {
            return Err(KeystoreError::AuthenticationFailed);
        }
        Ok(())
    }

    /// Re-encrypt a seed with a stronger/new password or KDF while preserving
    /// the authenticated normalization-profile marker. A markerless legacy
    /// document remains markerless; this method never guesses a profile.
    pub fn reencrypt(
        &self,
        current_password: &Password,
        new_password: &Password,
        new_kdf: KdfParams,
    ) -> Result<Self, KeystoreError> {
        let seed = self.decrypt_seed(current_password)?;
        let profile = self.normalization_profile();
        Self::encrypt_seed_inner(&seed, new_password, new_kdf, profile, false)
    }

    /// Explicitly migrate a weak-scrypt keystore to a stronger KDF while
    /// preserving its authenticated profile marker (or preserving absence).
    pub fn reencrypt_legacy_scrypt_for_migration(
        &self,
        current_password: &Password,
        new_password: &Password,
        new_kdf: KdfParams,
    ) -> Result<Self, KeystoreError> {
        let seed = self.decrypt_legacy_scrypt_for_migration(current_password)?;
        let profile = self.normalization_profile();
        Self::encrypt_seed_inner(&seed, new_password, new_kdf, profile, false)
    }

    fn decrypt_seed_inner(
        &self,
        password: &Password,
        allow_legacy_scrypt: bool,
        allow_insecure_test: bool,
    ) -> Result<SecretSeed, KeystoreError> {
        self.validate_header()?;
        let (kdf, salt) = KdfParams::from_json(&self.document["kdf"])?;
        if allow_legacy_scrypt {
            if !matches!(kdf, KdfParams::Scrypt { log_n: 14..=16, .. }) {
                return Err(KeystoreError::Unsupported);
            }
        } else if allow_insecure_test {
            kdf.validate_limits()?;
        } else {
            kdf.validate()?;
        }
        let nonce_hex = self.document["cipher"]["nonce"]
            .as_str()
            .ok_or(KeystoreError::InvalidFormat)?;
        let nonce_vec = hex::decode(nonce_hex).map_err(|_| KeystoreError::InvalidFormat)?;
        let nonce_bytes: [u8; 12] = nonce_vec
            .try_into()
            .map_err(|_| KeystoreError::InvalidFormat)?;
        let ciphertext = hex::decode(
            self.document["ciphertext"]
                .as_str()
                .ok_or(KeystoreError::InvalidFormat)?,
        )
        .map_err(|_| KeystoreError::InvalidFormat)?;
        let key = kdf.derive_key(password, &salt)?;
        let cipher = Aes256Gcm::new_from_slice(&key[..]).map_err(|_| KeystoreError::Unsupported)?;
        drop(key);
        let aad = header_aad(&self.document)?;
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    &Nonce::from(nonce_bytes),
                    Payload {
                        msg: &ciphertext,
                        aad: aad.as_bytes(),
                    },
                )
                .map_err(|_| KeystoreError::AuthenticationFailed)?,
        );
        if plaintext.len() != 64 {
            return Err(KeystoreError::InvalidFormat);
        }
        let mut seed = [0u8; 64];
        seed.copy_from_slice(&plaintext);
        drop(plaintext); // Wipe the temporary as soon as the seed has been copied.
        let seed = SecretSeed::new(seed);
        let account = derivation::derive_account(&seed, 0)
            .map_err(|e| KeystoreError::Derivation(e.to_string()))?;
        let metadata = &self.document["accounts"][0];
        if metadata["public_key"].as_str() != Some(hex::encode(account.public_key).as_str())
            || metadata["address"].as_str() != Some(account.address.to_string().as_str())
        {
            return Err(KeystoreError::AuthenticationFailed);
        }
        Ok(seed)
    }

    /// JSON document, normally pretty-printed. Near the loader's size limit,
    /// compact encoding is used instead; an oversized result is rejected. The
    /// authenticated header uses separate canonical bytes, not this output.
    pub fn to_json_string(&self) -> Result<String, KeystoreError> {
        let pretty = serde_json::to_string_pretty(&self.document)
            .map_err(|_| KeystoreError::InvalidFormat)?;
        if pretty.len() as u64 <= MAX_KEYSTORE_BYTES {
            return Ok(pretty);
        }
        // A compact valid input near the limit may grow when pretty-printed.
        // Never emit a file that our own bounded parser/loader would reject.
        let compact =
            serde_json::to_string(&self.document).map_err(|_| KeystoreError::InvalidFormat)?;
        if compact.len() as u64 > MAX_KEYSTORE_BYTES {
            return Err(KeystoreError::InvalidFormat);
        }
        Ok(compact)
    }

    /// Parse a keystore document without decrypting it.
    pub fn from_json_str(text: &str) -> Result<Self, KeystoreError> {
        if text.len() as u64 > MAX_KEYSTORE_BYTES {
            return Err(KeystoreError::InvalidFormat);
        }
        let document: Value =
            serde_json::from_str(text).map_err(|_| KeystoreError::InvalidFormat)?;
        DuplicateKeyChecker::check(text)?;
        let ks = Self { document };
        ks.validate_header()?;
        Ok(ks)
    }

    /// Publish without overwrite with owner-only Unix permissions. On Unix the
    /// final parent must be owned by the effective user, not writable by group
    /// or others, and not a symlink. File and directory operations are anchored
    /// to an open directory descriptor. A post-publication error may leave the
    /// destination present; inspect it rather than assuming no file was created.
    /// Newly created ancestor directories are not synchronized. Non-Unix ACL,
    /// concurrent directory attackers and durability policy are not established.
    pub fn save_new(&self, path: &Path) -> Result<(), KeystoreError> {
        #[cfg(any(target_os = "redox", target_os = "espidf"))]
        {
            let _ = path;
            Err(KeystoreError::UnsupportedPlatform)
        }
        #[cfg(not(any(target_os = "redox", target_os = "espidf")))]
        {
            self.save_new_with_parent_sync(path, sync_parent_directory)
        }
    }

    #[cfg(not(any(target_os = "redox", target_os = "espidf")))]
    fn save_new_with_parent_sync(
        &self,
        path: &Path,
        sync_parent: impl FnOnce(&Path, Option<&fs::File>) -> Result<(), KeystoreError>,
    ) -> Result<(), KeystoreError> {
        self.save_new_with_hooks(path, sync_parent, |_, _| Ok(()))
    }

    #[cfg(not(any(target_os = "redox", target_os = "espidf")))]
    fn save_new_with_hooks(
        &self,
        path: &Path,
        sync_parent: impl FnOnce(&Path, Option<&fs::File>) -> Result<(), KeystoreError>,
        before_publish: impl FnOnce(&Path, &Path) -> Result<(), KeystoreError>,
    ) -> Result<(), KeystoreError> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            // Path::file_name normalizes these suffixes, but Unix would treat
            // them as directory paths rather than a file named by the caller.
            let raw = path.as_os_str().as_bytes();
            if raw.ends_with(b"/") || raw.ends_with(b"/.") {
                return Err(KeystoreError::InvalidFormat);
            }
        }
        let text = self.to_json_string()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let name = path.file_name().ok_or(KeystoreError::InvalidFormat)?;

        #[cfg(unix)]
        {
            use rustix::fs::{AtFlags, Mode, OFlags, linkat, openat, unlinkat};
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true).mode(0o700);
            builder
                .create(parent)
                .map_err(|e| KeystoreError::Io(e.to_string()))?;
            let directory = open_trusted_parent(parent)?;
            let initial_directory = directory
                .metadata()
                .map_err(|e| KeystoreError::Io(e.to_string()))?;
            let mut suffix = [0u8; 16];
            OsRng.fill_bytes(&mut suffix);
            let tmp_name = format!(".auralite-keystore-{}.tmp", hex::encode(suffix));
            let tmp_path = parent.join(&tmp_name);
            let fd = openat(
                &directory,
                tmp_name.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )
            .map_err(|e| KeystoreError::Io(e.to_string()))?;
            let mut file = fs::File::from(fd);
            let original = file
                .metadata()
                .map_err(|e| KeystoreError::Io(e.to_string()))?;
            let result = (|| {
                file.write_all(text.as_bytes())
                    .map_err(|e| KeystoreError::Io(e.to_string()))?;
                file.sync_all()
                    .map_err(|e| KeystoreError::Io(e.to_string()))?;
                before_publish(parent, &tmp_path)?;
                if !directory_entry_matches(&directory, tmp_name.as_ref(), &original) {
                    return Err(KeystoreError::UnsafeDirectory);
                }
                // Both names resolve relative to the same already-open directory.
                linkat(
                    &directory,
                    tmp_name.as_str(),
                    &directory,
                    name,
                    AtFlags::empty(),
                )
                .map_err(|e| {
                    if e == rustix::io::Errno::EXIST {
                        KeystoreError::AlreadyExists
                    } else {
                        KeystoreError::Io(e.to_string())
                    }
                })?;
                // Detect source substitution between precheck and link. This is
                // not a defense against a malicious process with the same UID.
                if !directory_entry_matches(&directory, name, &original) {
                    return Err(KeystoreError::UnsafeDirectory);
                }
                Ok(())
            })();
            // Do not remove a name that no longer refers to our own temporary.
            let cleanup = if directory_entry_matches(&directory, tmp_name.as_ref(), &original) {
                unlinkat(&directory, tmp_name.as_str(), AtFlags::empty())
                    .map_err(|e| KeystoreError::Io(e.to_string()))
            } else {
                Err(KeystoreError::UnsafeDirectory)
            };
            result?;
            cleanup?;
            sync_parent(parent, Some(&directory))?;
            // A renamed/replaced pathname cannot be reported as success even
            // though publication to the original open directory may have occurred.
            let current = open_trusted_parent(parent)?;
            let current_metadata = current
                .metadata()
                .map_err(|e| KeystoreError::Io(e.to_string()))?;
            if !same_file_identity(&initial_directory, &current_metadata) {
                return Err(KeystoreError::UnsafeDirectory);
            }
            Ok(())
        }

        #[cfg(not(unix))]
        {
            let _ = name;
            fs::create_dir_all(parent).map_err(|e| KeystoreError::Io(e.to_string()))?;
            let mut suffix = [0u8; 16];
            OsRng.fill_bytes(&mut suffix);
            let tmp = parent.join(format!(".auralite-keystore-{}.tmp", hex::encode(suffix)));
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .map_err(|e| KeystoreError::Io(e.to_string()))?;
            let result = (|| {
                file.write_all(text.as_bytes())
                    .map_err(|e| KeystoreError::Io(e.to_string()))?;
                file.sync_all()
                    .map_err(|e| KeystoreError::Io(e.to_string()))?;
                before_publish(parent, &tmp)?;
                fs::hard_link(&tmp, path).map_err(|e| {
                    if e.kind() == std::io::ErrorKind::AlreadyExists {
                        KeystoreError::AlreadyExists
                    } else {
                        KeystoreError::Io(e.to_string())
                    }
                })?;
                Ok(())
            })();
            let cleanup = fs::remove_file(&tmp);
            result?;
            cleanup.map_err(|e| KeystoreError::Io(e.to_string()))?;
            sync_parent(parent, None)
        }
    }

    /// Load a regular keystore file with a strict size bound. On supported Unix
    /// platforms the final component must not be a symlink; nonregular files are rejected after a nonblocking open.
    #[cfg(not(any(target_os = "redox", target_os = "espidf")))]
    pub fn load(path: &Path) -> Result<Self, KeystoreError> {
        #[cfg(unix)]
        let file = {
            use rustix::fs::{Mode, OFlags, open};
            let fd = open(
                path,
                OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|e| KeystoreError::Io(e.to_string()))?;
            fs::File::from(fd)
        };
        #[cfg(not(unix))]
        let file = fs::File::open(path).map_err(|e| KeystoreError::Io(e.to_string()))?;
        if !file
            .metadata()
            .map_err(|e| KeystoreError::Io(e.to_string()))?
            .is_file()
        {
            return Err(KeystoreError::InvalidFormat);
        }
        let mut bytes = Vec::new();
        file.take(MAX_KEYSTORE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| KeystoreError::Io(e.to_string()))?;
        if bytes.len() as u64 > MAX_KEYSTORE_BYTES {
            return Err(KeystoreError::InvalidFormat);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| KeystoreError::InvalidFormat)?;
        Self::from_json_str(text)
    }

    /// Filesystem-backed keystores are not implemented on Redox or ESP-IDF.
    #[cfg(any(target_os = "redox", target_os = "espidf"))]
    pub fn load(_path: &Path) -> Result<Self, KeystoreError> {
        Err(KeystoreError::UnsupportedPlatform)
    }

    fn validate_header(&self) -> Result<(), KeystoreError> {
        let d = &self.document;
        let version = d["version"].as_u64().ok_or(KeystoreError::InvalidFormat)?;
        match version {
            LEGACY_VERSION => exact_fields(
                d,
                &[
                    "format",
                    "version",
                    "id",
                    "kdf",
                    "cipher",
                    "ciphertext",
                    "derivation",
                    "accounts",
                ],
            )?,
            VERSION => exact_fields(
                d,
                &[
                    "format",
                    "version",
                    "id",
                    "kdf",
                    "cipher",
                    "ciphertext",
                    "seed_derivation",
                    "derivation",
                    "accounts",
                ],
            )?,
            _ => return Err(KeystoreError::Unsupported),
        }
        if d["format"].as_str() != Some(FORMAT) {
            return Err(KeystoreError::Unsupported);
        }
        if version == VERSION {
            exact_fields(&d["seed_derivation"], &["normalization_profile"])?;
            match d["seed_derivation"]["normalization_profile"].as_str() {
                Some(id) if Bip39NormalizationProfile::from_id(id).is_some() => {}
                Some(_) => return Err(KeystoreError::Unsupported),
                None if d["seed_derivation"]["normalization_profile"].is_null() => {}
                None => return Err(KeystoreError::InvalidFormat),
            }
        }
        exact_fields(&d["cipher"], &["name", "nonce"])?;
        if d["cipher"]["name"].as_str() != Some("aes-256-gcm") {
            return Err(KeystoreError::Unsupported);
        }
        exact_fields(&d["derivation"], &["scheme", "coin_type", "path_template"])?;
        if d["derivation"]["scheme"].as_str() != Some("slip10-ed25519")
            || d["derivation"]["coin_type"].as_u64() != Some(u64::from(TESTNET_COIN_TYPE))
            || d["derivation"]["path_template"].as_str() != Some(AURA_PATH_TEMPLATE)
        {
            return Err(KeystoreError::Unsupported);
        }
        validate_hex(&d["id"], 16)?;
        validate_hex(&d["cipher"]["nonce"], 12)?;
        validate_hex(&d["ciphertext"], 80)?;
        let accounts = d["accounts"]
            .as_array()
            .ok_or(KeystoreError::InvalidFormat)?;
        if accounts.len() != 1 {
            return Err(KeystoreError::InvalidFormat);
        }
        let account = &accounts[0];
        exact_fields(account, &["index", "path", "public_key", "address"])?;
        if account["index"].as_u64() != Some(0)
            || account["path"].as_str() != Some(derivation::account_path(0).as_str())
            || account["address"].as_str().is_none_or(str::is_empty)
        {
            return Err(KeystoreError::InvalidFormat);
        }
        validate_hex(&account["public_key"], 32)?;
        KdfParams::from_json(&d["kdf"])?;
        Ok(())
    }
}

#[cfg(all(unix, not(any(target_os = "redox", target_os = "espidf"))))]
fn open_trusted_parent(parent: &Path) -> Result<fs::File, KeystoreError> {
    use rustix::fs::{Mode, OFlags, open};
    use std::os::unix::fs::MetadataExt;
    let fd = open(
        parent,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| KeystoreError::Io(e.to_string()))?;
    let directory = fs::File::from(fd);
    let meta = directory
        .metadata()
        .map_err(|e| KeystoreError::Io(e.to_string()))?;
    if meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o022 != 0 {
        return Err(KeystoreError::UnsafeDirectory);
    }
    Ok(directory)
}

#[cfg(all(unix, not(any(target_os = "redox", target_os = "espidf"))))]
fn same_file_identity(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(all(unix, not(any(target_os = "redox", target_os = "espidf"))))]
#[allow(clippy::unnecessary_cast)] // stat integer widths vary among Unix targets.
fn directory_entry_matches(
    directory: &fs::File,
    name: &std::ffi::OsStr,
    expected: &fs::Metadata,
) -> bool {
    use rustix::fs::{AtFlags, statat};
    use std::os::unix::fs::MetadataExt;
    statat(directory, name, AtFlags::SYMLINK_NOFOLLOW).is_ok_and(|actual| {
        actual.st_dev as u64 == expected.dev() && actual.st_ino as u64 == expected.ino()
    })
}

#[cfg(all(unix, not(any(target_os = "redox", target_os = "espidf"))))]
fn sync_parent_directory(
    _parent: &Path,
    directory: Option<&fs::File>,
) -> Result<(), KeystoreError> {
    directory
        .ok_or(KeystoreError::UnsafeDirectory)?
        .sync_all()
        .map_err(|e| KeystoreError::Io(e.to_string()))
}

#[cfg(not(unix))]
fn sync_parent_directory(
    _parent: &Path,
    _directory: Option<&fs::File>,
) -> Result<(), KeystoreError> {
    // Windows and other targets need a separately designed durability/ACL policy.
    Ok(())
}

// serde_json::Value accepts duplicate keys by retaining the last one. Reject
// duplicates (including equivalent escaped keys) before using a keystore.
// Syntax is first checked by serde_json, then this scanner walks the valid JSON.
struct DuplicateKeyChecker<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> DuplicateKeyChecker<'a> {
    fn check(text: &'a str) -> Result<(), KeystoreError> {
        let mut scanner = Self { text, pos: 0 };
        scanner.value(0)?;
        scanner.whitespace();
        if scanner.pos != text.len() {
            return Err(KeystoreError::InvalidFormat);
        }
        Ok(())
    }
    fn whitespace(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.pos)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.pos += 1;
        }
    }
    fn consume(&mut self, b: u8) -> Result<(), KeystoreError> {
        self.whitespace();
        if self.text.as_bytes().get(self.pos) != Some(&b) {
            return Err(KeystoreError::InvalidFormat);
        }
        self.pos += 1;
        Ok(())
    }
    fn string(&mut self) -> Result<String, KeystoreError> {
        self.whitespace();
        let start = self.pos;
        self.consume(b'"')?;
        while let Some(&byte) = self.text.as_bytes().get(self.pos) {
            self.pos += 1;
            if byte == b'\\' {
                if self.pos >= self.text.len() {
                    return Err(KeystoreError::InvalidFormat);
                }
                self.pos += 1;
            } else if byte == b'"' {
                return serde_json::from_str(&self.text[start..self.pos])
                    .map_err(|_| KeystoreError::InvalidFormat);
            }
        }
        Err(KeystoreError::InvalidFormat)
    }
    fn value(&mut self, depth: usize) -> Result<(), KeystoreError> {
        if depth > 128 {
            return Err(KeystoreError::InvalidFormat);
        }
        self.whitespace();
        match self.text.as_bytes().get(self.pos).copied() {
            Some(b'{') => {
                self.pos += 1;
                let mut keys = BTreeSet::new();
                self.whitespace();
                if self.text.as_bytes().get(self.pos) == Some(&b'}') {
                    self.pos += 1;
                    return Ok(());
                }
                loop {
                    let key = self.string()?;
                    if !keys.insert(key) {
                        return Err(KeystoreError::InvalidFormat);
                    }
                    self.consume(b':')?;
                    self.value(depth + 1)?;
                    self.whitespace();
                    match self.text.as_bytes().get(self.pos) {
                        Some(b',') => self.pos += 1,
                        Some(b'}') => {
                            self.pos += 1;
                            return Ok(());
                        }
                        _ => return Err(KeystoreError::InvalidFormat),
                    }
                }
            }
            Some(b'[') => {
                self.pos += 1;
                self.whitespace();
                if self.text.as_bytes().get(self.pos) == Some(&b']') {
                    self.pos += 1;
                    return Ok(());
                }
                loop {
                    self.value(depth + 1)?;
                    self.whitespace();
                    match self.text.as_bytes().get(self.pos) {
                        Some(b',') => self.pos += 1,
                        Some(b']') => {
                            self.pos += 1;
                            return Ok(());
                        }
                        _ => return Err(KeystoreError::InvalidFormat),
                    }
                }
            }
            Some(b'"') => {
                self.string()?;
                Ok(())
            }
            Some(_) => {
                let start = self.pos;
                while let Some(&byte) = self.text.as_bytes().get(self.pos) {
                    if byte.is_ascii_whitespace() || matches!(byte, b',' | b'}' | b']') {
                        break;
                    }
                    self.pos += 1;
                }
                if self.pos == start {
                    return Err(KeystoreError::InvalidFormat);
                }
                Ok(())
            }
            None => Err(KeystoreError::InvalidFormat),
        }
    }
}

fn exact_fields(value: &Value, fields: &[&str]) -> Result<(), KeystoreError> {
    let object = value.as_object().ok_or(KeystoreError::InvalidFormat)?;
    if object.len() != fields.len() || !fields.iter().all(|key| object.contains_key(*key)) {
        return Err(KeystoreError::InvalidFormat);
    }
    Ok(())
}

fn validate_hex(value: &Value, byte_len: usize) -> Result<(), KeystoreError> {
    let text = value.as_str().ok_or(KeystoreError::InvalidFormat)?;
    if text.len() != byte_len * 2 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(KeystoreError::InvalidFormat);
    }
    Ok(())
}

fn header_aad(document: &Value) -> Result<String, KeystoreError> {
    let mut header = document.clone();
    if let Value::Object(ref mut map) = header {
        map.remove("ciphertext");
    }
    canonical_json(&header)
}

fn canonical_json(value: &Value) -> Result<String, KeystoreError> {
    match value {
        Value::Object(map) => {
            let mut sorted = Map::new();
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            for key in keys {
                sorted.insert(key.clone(), map[&key].clone());
            }
            let mut out = String::from("{");
            for (i, (k, v)) in sorted.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).map_err(|_| KeystoreError::InvalidFormat)?);
                out.push(':');
                out.push_str(&canonical_json(v)?);
            }
            out.push('}');
            Ok(out)
        }
        Value::Array(items) => {
            let mut out = String::from("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&canonical_json(item)?);
            }
            out.push(']');
            Ok(out)
        }
        _ => serde_json::to_string(value).map_err(|_| KeystoreError::InvalidFormat),
    }
}

#[cfg(all(test, not(any(target_os = "redox", target_os = "espidf"))))]
mod durability_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn public_test_keystore() -> Keystore {
        Keystore::from_json_str(include_str!("../tests/fixtures/w1_interop_argon2id.json")).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn temporary_entry_substitution_is_detected_before_publication() {
        let directory = std::env::temp_dir().join(format!(
            "auralite-w1-tmp-substitution-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let target = directory.join("keystore.json");
        let result = public_test_keystore().save_new_with_hooks(
            &target,
            sync_parent_directory,
            |_parent, tmp| {
                fs::remove_file(tmp).unwrap();
                fs::write(tmp, b"attacker-controlled replacement").unwrap();
                Ok(())
            },
        );
        assert_eq!(result, Err(KeystoreError::UnsafeDirectory));
        assert!(
            !target.exists(),
            "substituted content must not be published"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn parent_directory_replacement_does_not_redirect_publication() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!(
            "auralite-w1-parent-replacement-{}",
            std::process::id()
        ));
        let parent = root.join("parent");
        let moved = root.join("moved");
        let attacker = root.join("attacker");
        fs::create_dir_all(&parent).unwrap();
        fs::create_dir_all(&attacker).unwrap();
        let target = parent.join("keystore.json");
        let result =
            public_test_keystore().save_new_with_hooks(&target, sync_parent_directory, |_, _| {
                fs::rename(&parent, &moved).unwrap();
                symlink(&attacker, &parent).unwrap();
                Ok(())
            });
        assert!(
            result.is_err(),
            "replaced parent cannot produce a success result"
        );
        assert!(!attacker.join("keystore.json").exists());
        assert!(Keystore::load(&moved.join("keystore.json")).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sync_error_after_publication_is_reported_without_overwrite() {
        let directory =
            std::env::temp_dir().join(format!("auralite-w1-sync-error-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("keystore.json");
        let store = public_test_keystore();
        let called = AtomicBool::new(false);
        let result = store.save_new_with_parent_sync(&path, |parent, _directory| {
            assert_eq!(parent, directory);
            called.store(true, Ordering::SeqCst);
            Err(KeystoreError::Io(
                "injected directory sync failure".to_owned(),
            ))
        });
        assert!(called.load(Ordering::SeqCst));
        assert!(matches!(result, Err(KeystoreError::Io(_))));
        assert!(
            Keystore::load(&path).is_ok(),
            "publication can precede the error"
        );
        assert_eq!(store.save_new(&path), Err(KeystoreError::AlreadyExists));
        assert_eq!(
            fs::read_dir(&directory).unwrap().count(),
            1,
            "temporary file was removed"
        );
        fs::remove_dir_all(directory).unwrap();
    }
}

#[cfg(test)]
mod serialization_size_tests {
    use super::*;

    #[test]
    fn oversized_compact_output_fails_closed() {
        // Construct an internal adversarial value; public parsers already bound
        // input size. This tests the serializer's independent final cap.
        let store = Keystore {
            document: json!({ "x": "x".repeat(MAX_KEYSTORE_BYTES as usize) }),
        };
        assert_eq!(
            store.to_json_string().err(),
            Some(KeystoreError::InvalidFormat)
        );
        #[cfg(not(any(target_os = "redox", target_os = "espidf")))]
        {
            let root =
                std::env::temp_dir().join(format!("auralite-w1-oversized-{}", std::process::id()));
            let target = root.join("store.json");
            assert_eq!(store.save_new(&target), Err(KeystoreError::InvalidFormat));
            assert!(
                !root.exists(),
                "serialization must fail before creating directories"
            );
        }
    }
}

#[cfg(test)]
mod aad_reference_tests {
    use super::*;

    #[test]
    fn canonical_aad_matches_independent_python_bytes() {
        // AAD-only public fixtures: the escaping address is invalid; altered
        // profile cases retain an invalid GCM tag and must not be decrypted.
        let cases: Value =
            serde_json::from_str(include_str!("../tests/fixtures/w1_aad_reference.json")).unwrap();
        let cases = cases.as_array().unwrap();
        assert_eq!(cases.len(), 6);
        for case in cases {
            let document = serde_json::to_string(&case["document"]).unwrap();
            let keystore = Keystore::from_json_str(&document).unwrap();
            let aad = header_aad(&keystore.document).unwrap();
            assert_eq!(
                hex::encode(aad.as_bytes()),
                case["aad_hex"].as_str().unwrap(),
                "AAD mismatch: {}",
                case["name"]
            );
        }
    }
}

#[cfg(test)]
mod kdf_memory_tests {
    use super::*;
    use zeroize::Zeroize;

    #[test]
    fn argon2_memory_blocks_can_be_wiped_and_match_reference() {
        // This tests the upstream zeroize feature on the exact working type;
        // proving complete erasure of every compiler/library copy is impossible.
        let mut blocks = vec![argon2::Block::default(); 2];
        blocks[0].as_mut()[0] = 0x1234;
        blocks[1].as_mut()[127] = 0x5678;
        blocks.iter_mut().zeroize();
        assert_eq!(blocks.len(), 2);
        assert!(
            blocks
                .iter()
                .all(|block| block.as_ref().iter().all(|v| *v == 0))
        );

        let password = Password::new("public-memory-test");
        let salt = [7u8; 16];
        let params = Params::new(19_456, 2, 1, Some(32)).unwrap();
        let mut expected = Zeroizing::new([0u8; 32]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password.as_str().as_bytes(), &salt, &mut expected[..])
            .unwrap();
        let actual = KdfParams::Argon2id {
            m_cost: 19_456,
            t_cost: 2,
            p_cost: 1,
        }
        .derive_key(&password, &salt)
        .unwrap();
        assert_eq!(&actual[..], &expected[..]);
    }
}
