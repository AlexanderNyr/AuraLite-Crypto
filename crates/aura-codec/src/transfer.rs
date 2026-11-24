//! AuraLite-Crypto P1 — zero-copy transfer wire block (phase P1
//! codec/crypto, `docs/plans/PERF_PLAN.md`).
//!
//! One wire format, two decoders — the P1 "zero-copy decode"
//! optimization (D-P2: every optimization ships with its before/after
//! pair):
//!
//! * [`decode_zero_copy`] — the *after* path: validates and **borrows**
//!   every field straight from the input buffer, zero allocations on the
//!   happy path (the standard high-performance parsing technique: the
//!   deserialized value is a borrowed view over the input, not fresh
//!   copies — the serde/rkyv/zerocopy class of codecs).
//! * [`decode_copy`] — the *before* baseline: same shared validation,
//!   independent field extraction into owned storage (allocates the
//!   memo). The two decoders are separate code paths and the P1 gate
//!   requires them to agree byte for byte
//!   (`P1: zero-copy decode mismatch`) and to reject the same malformed
//!   inputs (`P1: malformed input accepted ({case})`).
//!
//! The wire block is canonical: fixed [`TRANSFER_LEN`] (the D-P1
//! workload: a 350-byte transfer), exact version, zero-padded memo —
//! anything else is rejected.

use std::fmt;

/// Wire version of the fixed transfer block.
pub const TRANSFER_WIRE_VERSION: u16 = 1;

/// Fixed wire size of one transfer block in bytes (the D-P1 workload:
/// a 350-byte transfer).
pub const TRANSFER_LEN: usize = 350;

/// Offset of the sender key inside the wire block.
pub const OFF_FROM: usize = 2;
/// Offset of the recipient key inside the wire block.
pub const OFF_TO: usize = 34;
/// Offset of the amount inside the wire block.
pub const OFF_AMOUNT: usize = 66;
/// Offset of the nonce inside the wire block.
pub const OFF_NONCE: usize = 74;
/// Offset of the memo length byte inside the wire block.
pub const OFF_MEMO_LEN: usize = 82;
/// Offset of the memo bytes inside the wire block.
pub const OFF_MEMO: usize = 83;
/// Maximum memo length that fits the wire block.
pub const MAX_MEMO: usize = TRANSFER_LEN - OFF_MEMO;

/// Owned transfer (the `decode_copy` baseline: allocates the memo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    /// Wire version ([`TRANSFER_WIRE_VERSION`]).
    pub version: u16,
    /// Sender public key.
    pub from: [u8; 32],
    /// Recipient public key.
    pub to: [u8; 32],
    /// Amount in the smallest unit.
    pub amount: u64,
    /// Account nonce.
    pub nonce: u64,
    /// Memo bytes (zero padding stripped in the canonical form).
    pub memo: Vec<u8>,
}

/// Borrowed zero-copy view over a wire block (the `decode_zero_copy`
/// optimization: every field references the input buffer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferView<'a> {
    /// Wire version ([`TRANSFER_WIRE_VERSION`]).
    pub version: u16,
    /// Sender public key (borrowed from the input).
    pub from: &'a [u8; 32],
    /// Recipient public key (borrowed from the input).
    pub to: &'a [u8; 32],
    /// Amount in the smallest unit.
    pub amount: u64,
    /// Account nonce.
    pub nonce: u64,
    /// Memo bytes (borrowed, zero padding stripped).
    pub memo: &'a [u8],
}

/// Transfer wire-block errors. Every case name maps to the P1
/// malformed-input gate (`P1: malformed input accepted ({case})`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferError {
    /// Input shorter than [`TRANSFER_LEN`].
    TooShort {
        /// Observed input length in bytes.
        len: usize,
    },
    /// Input longer than [`TRANSFER_LEN`] (non-canonical).
    Overlong {
        /// Observed input length in bytes.
        len: usize,
    },
    /// Unknown wire version.
    BadVersion {
        /// Observed version field.
        got: u16,
    },
    /// Memo length out of range, or non-zero bytes after the memo
    /// (non-canonical).
    BadMemoPadding,
}

impl TransferError {
    /// Case name for the P1 gate: `short-buffer`, `overlong`,
    /// `bad-version`, `bad-memo-padding`.
    pub fn case(&self) -> &'static str {
        match self {
            TransferError::TooShort { .. } => "short-buffer",
            TransferError::Overlong { .. } => "overlong",
            TransferError::BadVersion { .. } => "bad-version",
            TransferError::BadMemoPadding => "bad-memo-padding",
        }
    }
}

impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransferError::TooShort { len } => {
                write!(f, "transfer: buffer too short ({len}/350)")
            }
            TransferError::Overlong { len } => {
                write!(f, "transfer: buffer overlong ({len}/350)")
            }
            TransferError::BadVersion { got } => {
                write!(f, "transfer: bad version ({got})")
            }
            TransferError::BadMemoPadding => write!(f, "transfer: non-zero memo padding"),
        }
    }
}

impl std::error::Error for TransferError {}

/// Shared validation for both decoders: exact length, exact version,
/// `memo_len` in range, zero padding after the memo (the canonical wire
/// form — one value has exactly one encoding).
fn validate(bytes: &[u8]) -> Result<(), TransferError> {
    if bytes.len() < TRANSFER_LEN {
        return Err(TransferError::TooShort { len: bytes.len() });
    }
    if bytes.len() > TRANSFER_LEN {
        return Err(TransferError::Overlong { len: bytes.len() });
    }
    let version = u16::from_le_bytes([bytes[0], bytes[1]]);
    if version != TRANSFER_WIRE_VERSION {
        return Err(TransferError::BadVersion { got: version });
    }
    let memo_len = bytes[OFF_MEMO_LEN] as usize;
    if memo_len > MAX_MEMO {
        return Err(TransferError::BadMemoPadding);
    }
    if bytes[OFF_MEMO + memo_len..].iter().any(|b| *b != 0) {
        return Err(TransferError::BadMemoPadding);
    }
    Ok(())
}

/// Zero-copy decode (the P1 optimization): validates the canonical wire
/// block and **borrows** every field from `bytes` — no allocation.
pub fn decode_zero_copy(bytes: &[u8]) -> Result<TransferView<'_>, TransferError> {
    validate(bytes)?;
    let from: &[u8; 32] = bytes[OFF_FROM..OFF_TO]
        .try_into()
        .expect("fixed offsets inside the 350-byte block");
    let to: &[u8; 32] = bytes[OFF_TO..OFF_AMOUNT]
        .try_into()
        .expect("fixed offsets inside the 350-byte block");
    Ok(TransferView {
        version: TRANSFER_WIRE_VERSION,
        from,
        to,
        amount: u64::from_le_bytes(
            bytes[OFF_AMOUNT..OFF_NONCE]
                .try_into()
                .expect("fixed offsets inside the 350-byte block"),
        ),
        nonce: u64::from_le_bytes(
            bytes[OFF_NONCE..OFF_MEMO_LEN]
                .try_into()
                .expect("fixed offsets inside the 350-byte block"),
        ),
        memo: &bytes[OFF_MEMO..OFF_MEMO + bytes[OFF_MEMO_LEN] as usize],
    })
}

/// Copy decode (the pre-P1 baseline): same shared validation as
/// [`decode_zero_copy`], but its own field extraction into owned storage
/// (the two decoders are independent code paths — the equivalence gate
/// compares their outputs).
pub fn decode_copy(bytes: &[u8]) -> Result<Transfer, TransferError> {
    validate(bytes)?;
    let mut from = [0u8; 32];
    from.copy_from_slice(&bytes[OFF_FROM..OFF_TO]);
    let mut to = [0u8; 32];
    to.copy_from_slice(&bytes[OFF_TO..OFF_AMOUNT]);
    Ok(Transfer {
        version: TRANSFER_WIRE_VERSION,
        from,
        to,
        amount: u64::from_le_bytes(
            bytes[OFF_AMOUNT..OFF_NONCE]
                .try_into()
                .expect("fixed offsets inside the 350-byte block"),
        ),
        nonce: u64::from_le_bytes(
            bytes[OFF_NONCE..OFF_MEMO_LEN]
                .try_into()
                .expect("fixed offsets inside the 350-byte block"),
        ),
        memo: bytes[OFF_MEMO..OFF_MEMO + bytes[OFF_MEMO_LEN] as usize].to_vec(),
    })
}

/// Encode a transfer into the canonical 350-byte wire block
/// (zero-copy write: straight into the caller's buffer).
pub fn encode(view: &TransferView<'_>, out: &mut [u8]) -> Result<(), TransferError> {
    if out.len() != TRANSFER_LEN {
        return Err(TransferError::TooShort { len: out.len() });
    }
    out.fill(0);
    out[0..2].copy_from_slice(&TRANSFER_WIRE_VERSION.to_le_bytes());
    out[OFF_FROM..OFF_TO].copy_from_slice(view.from);
    out[OFF_TO..OFF_AMOUNT].copy_from_slice(view.to);
    out[OFF_AMOUNT..OFF_NONCE].copy_from_slice(&view.amount.to_le_bytes());
    out[OFF_NONCE..OFF_MEMO_LEN].copy_from_slice(&view.nonce.to_le_bytes());
    if view.memo.len() > MAX_MEMO {
        return Err(TransferError::Overlong {
            len: view.memo.len(),
        });
    }
    out[OFF_MEMO_LEN] = view.memo.len() as u8;
    out[OFF_MEMO..OFF_MEMO + view.memo.len()].copy_from_slice(view.memo);
    Ok(())
}

/// A deterministic sample transfer (fixed keys/amount/nonce/memo) — the
/// P1 wire fixture used by the gates and the bench example.
pub fn sample_wire() -> [u8; TRANSFER_LEN] {
    let mut from = [0u8; 32];
    let mut to = [0u8; 32];
    for (i, b) in from.iter_mut().enumerate() {
        *b = 0x11 ^ (i as u8);
    }
    for (i, b) in to.iter_mut().enumerate() {
        *b = 0x5A ^ (i as u8);
    }
    let view = TransferView {
        version: TRANSFER_WIRE_VERSION,
        from: &from,
        to: &to,
        amount: 10_000_000_000_000_000_000u64,
        nonce: 7,
        memo: b"p1-sample",
    };
    let mut out = [0u8; TRANSFER_LEN];
    encode(&view, &mut out).expect("sample fits the 350-byte block");
    out
}
