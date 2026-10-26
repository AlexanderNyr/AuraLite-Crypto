//! AuraLite-Crypto hashing, signatures and secret handling.
//!
//! C3 sits on top of C1 (`aura-types`) and implements the cryptographic
//! primitives named in `docs/ARCHITECTURE.md`:
//!
//! * **BLAKE3** (`pure` feature, no C/assembly) for all hashes;
//! * **Ed25519** (RFC 8032) for transaction and consensus signatures;
//! * **secp256k1 / ECDSA** (RFC 6979 deterministic, low-S) for compatibility;
//! * **domain separation** — `chain-id || 0 || domain || 0` framing plus a
//!   versioned context and explicit length prefixes, so a vote can never be
//!   replayed as a transaction and a testnet signature dies on mainnet;
//! * **constant-time comparison** and **zeroizing** secret storage.
//!
//! All public APIs use fixed-size byte arrays. No external crate types leak
//! out of this crate, and no randomness is used anywhere: signing is fully
//! deterministic given a seed (rule D3).
//!
//! Golden vectors come from `tools/gen_c3_vectors.py`: official BLAKE3 vectors,
//! RFC 8032 §7.1, and a pure-Python secp256k1/RFC6979 implementation verified
//! with OpenSSL. Full `cargo deny` (licences + advisories) arrives with H1;
//! C3 adds the exact-pin dependency gate `tools/check_deps.py` to `make test`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod ed25519;
mod error;
mod hash;
mod secp;
mod secret;

pub use ed25519::{ed25519_public_key, sign, sign_raw, verify, verify_raw};
pub use error::CryptoError;
pub use hash::{HASH_CONTEXT, blake3_hash, domain_message, hash_domain};
pub use secp::{secp_public_key, sign_prehash_raw, verify_prehash_raw};
pub use secret::{SecretBytes, constant_time_eq};
