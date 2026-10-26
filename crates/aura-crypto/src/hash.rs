//! BLAKE3 hashing and domain separation.

use aura_types::{Domain, Hash};

/// Versioned context string for all domain-separated hashes.
///
/// NC target: `tools/nc_c3.py` changes this constant and requires the domain
/// golden vectors to go red. A change here is a protocol change: it requires
/// a new phase, a `CODEC_VERSION` bump and a migration test.
pub const HASH_CONTEXT: &[u8] = b"AuraLite-Crypto/crypto-v1\x00";

/// Raw BLAKE3 hash of a byte string, returned as our canonical [`Hash`].
pub fn blake3_hash(input: &[u8]) -> Hash {
    Hash::from_bytes(*blake3::hash(input).as_bytes())
}

/// Build the unambiguous message that is hashed/signed for a given domain.
///
/// Layout (little-endian lengths):
/// `HASH_CONTEXT || u64(prefix_len) || prefix || u64(payload_len) || payload`,
/// where `prefix = chain_id || 0x00 || domain_tag || 0x00` comes from C1's
/// [`Domain::prefix`]. Explicit lengths prevent ambiguous concatenation.
pub fn domain_message(chain_id: &str, domain: Domain, payload: &[u8]) -> Vec<u8> {
    let prefix = domain.prefix(chain_id);
    let mut out = Vec::with_capacity(HASH_CONTEXT.len() + 8 + prefix.len() + 8 + payload.len());
    out.extend_from_slice(HASH_CONTEXT);
    out.extend_from_slice(&(prefix.len() as u64).to_le_bytes());
    out.extend_from_slice(&prefix);
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// Domain-separated BLAKE3 hash. Used for block/tx hashes and as the digest
/// that Ed25519 signs.
pub fn hash_domain(chain_id: &str, domain: Domain, payload: &[u8]) -> Hash {
    blake3_hash(&domain_message(chain_id, domain, payload))
}
