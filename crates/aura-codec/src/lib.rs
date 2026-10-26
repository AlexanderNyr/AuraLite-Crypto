//! AuraLite-Crypto canonical deterministic serialization.
//!
//! `aura-codec` is the **only** serializer allowed for tx/block/state data
//! (ROADMAP rule D4). Its contract: one value has exactly one byte encoding.
//!
//! Properties enforced here:
//! * fixed field order (defined once per type, in `Encode`/`Decode`);
//! * fixed-width little-endian integers — no `usize`/`isize` implementations
//!   exist, because their width is platform-dependent;
//! * length-prefixed byte strings, vectors and maps with minimal unsigned
//!   LEB128 varints (non-minimal encodings are rejected);
//! * tagged `Option`, `bool`, `Domain` and `Denom` with strict tag validation;
//! * `BTreeMap` entries must be strictly ascending (no `HashMap` anywhere);
//! * `decode_canonical` re-encodes the decoded value and requires byte
//!   equality with the input — a second, structural canonicality check;
//! * versioned wire format via [`CODEC_VERSION`].
//!
//! `serde` is deliberately not a dependency. It is allowed only outside
//! consensus (RPC, config files).
//!
//! Golden vectors come from `tools/gen_c2_vectors.py`, an independent Python
//! implementation of this format — not from this crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod codec;
mod error;
mod impls;
mod record;

pub use codec::{
    Decode, Encode, Reader, Writer, decode_canonical, decode_from_slice, encode_to_bytes,
    read_varint, write_varint,
};
pub use error::CodecError;
pub use record::CanonicalRecord;

/// Wire format version. A change requires a new phase and a migration test.
pub const CODEC_VERSION: u8 = 1;

/// Maximum byte length of any length-prefixed value (1 MiB).
pub const MAX_LENGTH: usize = 1 << 20;

/// Maximum element count of an encoded vector or map.
pub const MAX_ITEMS: usize = 1 << 16;
