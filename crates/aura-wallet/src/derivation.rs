//! W1 derivation: BIP-39 mnemonic -> seed, BIP-32 test-vector compatibility,
//! and SLIP-0010 Ed25519 hardened BIP-44-style account derivation.
//!
//! AuraLite transactions are Ed25519. SLIP-0010 Ed25519 only defines hardened
//! derivation, so the Stage-1 testnet path is `m/44'/1'/0'/0'/i'`.
//! `ALCRY` has no allocated SLIP-44 coin type; coin type `1` is the shared
//! testnet coin type and MUST NOT be used for mainnet value.

use aura_crypto::ed25519_public_key;
use aura_tx::signer_address;
use aura_types::Address;
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use sha2::Sha512;
use unicode_normalization::UnicodeNormalization as UnicodeNormalization18;
use unicode_normalization_15_1::UnicodeNormalization as UnicodeNormalization15_1;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::secret::{SecretKey32, SecretSeed};

/// Shared SLIP-44 testnet coin type used by Stage-1 AuraLite testnets.
pub const TESTNET_COIN_TYPE: u32 = 1;
/// Hardened Ed25519 account path template. `i` is hardened.
pub const AURA_PATH_TEMPLATE: &str = "m/44'/1'/0'/0'/i'";
/// SLIP-0010 Ed25519 master key HMAC key.
const ED25519_MASTER_KEY: &[u8] = b"ed25519 seed";
/// Hardened child index offset.
const HARDENED_OFFSET: u32 = 0x8000_0000;

/// Versioned Unicode NFKD table set used for BIP-39 mnemonic/passphrase input.
///
/// New mnemonic creation uses [`Bip39NormalizationProfile::Unicode18_0`].
/// `Unicode15_1` is retained only for explicit legacy recovery; never retry a
/// failed derivation under another profile automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bip39NormalizationProfile {
    /// Unicode 15.1 NFKD tables, only for an explicitly selected legacy recovery.
    Unicode15_1,
    /// Unicode 18.0 NFKD tables, the profile for newly created mnemonics.
    Unicode18_0,
}

impl Bip39NormalizationProfile {
    /// Stable recovery/keystore metadata ID for this profile.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Unicode15_1 => "bip39-nfkd-unicode-15.1",
            Self::Unicode18_0 => "bip39-nfkd-unicode-18.0",
        }
    }

    /// Parse a stable profile ID. Unknown IDs are never interpreted as a default.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "bip39-nfkd-unicode-15.1" => Some(Self::Unicode15_1),
            "bip39-nfkd-unicode-18.0" => Some(Self::Unicode18_0),
            _ => None,
        }
    }
}

/// Default profile for generating and restoring newly created mnemonics.
pub const DEFAULT_BIP39_NORMALIZATION_PROFILE: Bip39NormalizationProfile =
    Bip39NormalizationProfile::Unicode18_0;

/// Derivation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerivationError {
    /// Mnemonic is invalid or has a bad checksum.
    InvalidMnemonic,
    /// Path is not a hardened Ed25519 SLIP-0010 path.
    InvalidPath,
    /// Unsupported mnemonic word count.
    InvalidWordCount,
    /// SLIP-0010 seed must contain 128–512 bits.
    InvalidSeedLength,
}

impl std::fmt::Display for DerivationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMnemonic => f.write_str("invalid BIP-39 mnemonic"),
            Self::InvalidPath => f.write_str("invalid hardened Ed25519 derivation path"),
            Self::InvalidWordCount => f.write_str("mnemonic must be 12, 15, 18, 21 or 24 words"),
            Self::InvalidSeedLength => f.write_str("SLIP-0010 seed must be 16 to 64 bytes"),
        }
    }
}

impl std::error::Error for DerivationError {}

/// Generate a new English BIP-39 mnemonic. The caller must zeroize the string.
pub fn generate_mnemonic(words: usize) -> Result<String, DerivationError> {
    if !matches!(words, 12 | 15 | 18 | 21 | 24) {
        return Err(DerivationError::InvalidWordCount);
    }
    let entropy_len = match words {
        12 => 16,
        15 => 20,
        18 => 24,
        21 => 28,
        24 => 32,
        _ => return Err(DerivationError::InvalidWordCount),
    };
    let mut entropy = Zeroizing::new(vec![0u8; entropy_len]);
    OsRng.fill_bytes(&mut entropy);
    let mnemonic = bip39::Mnemonic::from_entropy_in(bip39::Language::English, &entropy)
        .map_err(|_| DerivationError::InvalidWordCount)?;
    Ok(mnemonic.to_string())
}

/// Validate an English mnemonic and derive its BIP-39 seed with an optional
/// passphrase using the caller-selected, versioned NFKD profile. Recovery code
/// must obtain this profile from trusted recovery metadata or explicit user
/// choice; it must not guess or retry another profile. After NFKD, words must
/// use exactly one ASCII space as separator, without leading/trailing spaces.
pub fn mnemonic_to_seed_with_profile(
    phrase: &str,
    passphrase: &str,
    profile: Bip39NormalizationProfile,
) -> Result<SecretSeed, DerivationError> {
    // Own both normalized copies so they are zeroized, including on invalid
    // mnemonic and unwind paths. The profile is a fixed table set, not the
    // current Unicode version of the host platform.
    let normalized_phrase = normalize_nfkd(phrase, profile);
    let normalized_passphrase = normalize_nfkd(passphrase, profile);
    let mnemonic =
        bip39::Mnemonic::parse_in_normalized(bip39::Language::English, &normalized_phrase)
            .map_err(|_| DerivationError::InvalidMnemonic)?;
    // The parser accepts arbitrary whitespace, but to_seed_normalized uses
    // the mnemonic's words joined with single spaces. Do not silently derive
    // from bytes different from the caller's profile-normalized input.
    let canonical_phrase = Zeroizing::new(mnemonic.to_string());
    if normalized_phrase.as_str() != canonical_phrase.as_str() {
        return Err(DerivationError::InvalidMnemonic);
    }
    drop(canonical_phrase);
    drop(normalized_phrase);
    let seed = mnemonic.to_seed_normalized(&normalized_passphrase);
    Ok(SecretSeed::new(seed))
}

fn normalize_nfkd(input: &str, profile: Bip39NormalizationProfile) -> Zeroizing<String> {
    match profile {
        Bip39NormalizationProfile::Unicode15_1 => {
            Zeroizing::new(UnicodeNormalization15_1::nfkd(input).collect::<String>())
        }
        Bip39NormalizationProfile::Unicode18_0 => {
            Zeroizing::new(UnicodeNormalization18::nfkd(input).collect::<String>())
        }
    }
}

/// A SLIP-0010 Ed25519 extended private key.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct ExtendedEdKey {
    secret: [u8; 32],
    chain_code: [u8; 32],
}

impl ExtendedEdKey {
    /// Derive the SLIP-0010 Ed25519 master key from a BIP-39 seed.
    pub fn master(seed: &SecretSeed) -> Self {
        Self::master_from_bytes(seed.as_bytes()).expect("BIP-39 seed is 64 bytes")
    }

    /// Derive the SLIP-0010 master key from 16–64 bytes of seed material.
    pub fn master_from_bytes(seed: &[u8]) -> Result<Self, DerivationError> {
        if !(16..=64).contains(&seed.len()) {
            return Err(DerivationError::InvalidSeedLength);
        }
        let mut mac = Hmac::<Sha512>::new_from_slice(ED25519_MASTER_KEY).expect("HMAC key");
        mac.update(seed);
        let mut out = mac.finalize().into_bytes();
        let mut secret = [0u8; 32];
        let mut chain_code = [0u8; 32];
        secret.copy_from_slice(&out[..32]);
        chain_code.copy_from_slice(&out[32..]);
        out[..].zeroize();
        Ok(Self { secret, chain_code })
    }

    /// Derive a hardened child. Non-hardened Ed25519 derivation is refused.
    pub fn derive_hardened(&self, index: u32) -> Result<Self, DerivationError> {
        if index >= HARDENED_OFFSET {
            return Err(DerivationError::InvalidPath);
        }
        let child = index | HARDENED_OFFSET;
        let mut data = Zeroizing::new(Vec::with_capacity(37));
        data.push(0);
        data.extend_from_slice(&self.secret);
        data.extend_from_slice(&child.to_be_bytes());
        let mut mac = Hmac::<Sha512>::new_from_slice(&self.chain_code).expect("HMAC key");
        mac.update(&data);
        drop(data); // Wipe the private-key-containing child-derivation buffer promptly.
        let mut out = mac.finalize().into_bytes();
        let mut secret = [0u8; 32];
        let mut chain_code = [0u8; 32];
        secret.copy_from_slice(&out[..32]);
        chain_code.copy_from_slice(&out[32..]);
        out[..].zeroize();
        Ok(Self { secret, chain_code })
    }

    /// Derive a hardened path such as `m/44'/1'/0'/0'/0'`.
    pub fn derive_path(seed: &SecretSeed, path: &str) -> Result<Self, DerivationError> {
        let mut parts = path.split('/');
        if parts.next() != Some("m") {
            return Err(DerivationError::InvalidPath);
        }
        let mut key = Self::master(seed);
        for part in parts {
            let index = part
                .strip_suffix('\'')
                .ok_or(DerivationError::InvalidPath)?
                .parse::<u32>()
                .map_err(|_| DerivationError::InvalidPath)?;
            key = key.derive_hardened(index)?;
        }
        Ok(key)
    }

    /// The 32-byte Ed25519 signing key.
    pub fn signing_key(&self) -> SecretKey32 {
        SecretKey32(self.secret)
    }

    /// The Ed25519 public key.
    pub fn public_key(&self) -> [u8; 32] {
        ed25519_public_key(&self.secret)
    }
}

/// A derived AuraLite account.
#[derive(Debug, Clone)]
pub struct DerivedAccount {
    /// Account index.
    pub index: u32,
    /// Hardened derivation path.
    pub path: String,
    /// Ed25519 signing key, zeroized on drop.
    pub secret: SecretKey32,
    /// Ed25519 public key.
    pub public_key: [u8; 32],
    /// AuraLite address.
    pub address: Address,
}

/// Canonical account path for index `i`.
pub fn account_path(index: u32) -> String {
    format!("m/44'/{TESTNET_COIN_TYPE}'/0'/0'/{index}'")
}

/// Derive account `index` from a BIP-39 seed.
pub fn derive_account(seed: &SecretSeed, index: u32) -> Result<DerivedAccount, DerivationError> {
    let path = account_path(index);
    let key = ExtendedEdKey::derive_path(seed, &path)?;
    let public_key = key.public_key();
    let address = signer_address(&public_key);
    Ok(DerivedAccount {
        index,
        path,
        secret: key.signing_key(),
        public_key,
        address,
    })
}
