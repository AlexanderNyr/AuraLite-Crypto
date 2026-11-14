//! W3 — air-gapped offline signing: envelopes, QR transport, verification.
//!
//! Design of record: `docs/w3-offline-plan.md` (D-W3-1..D-W3-4).
//!
//! The envelope (`AURALITE-OFFLINE-V1`) is the deterministic container that
//! carries an unsigned tx to the offline machine and a signed tx back. Its
//! BLAKE3 checksum detects corruption only — it is **not** authentication;
//! the Ed25519 signature under the fixed AuraLite `tx` domain is.

use aura_crypto::blake3_hash;
use aura_tx::Tx;

/// Envelope magic (D-W3-1).
pub const ENVELOPE_MAGIC: &[u8] = b"AuraLite-OFFLINE-V1";
/// Envelope kind: unsigned transaction.
pub const KIND_UNSIGNED: u8 = 0;
/// Envelope kind: signed transaction.
pub const KIND_SIGNED: u8 = 1;
/// QR frame prefix (D-W3-2).
pub const FRAME_PREFIX: &[u8] = b"ALO1:";
/// Default QR chunk size in characters of the text envelope.
pub const DEFAULT_QR_SIZE: usize = 180;

/// Fixed refusal messages (byte-for-byte asserted by the W3 vectors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfflineError {
    /// Base64/structure failure.
    BadEncoding,
    /// Envelope checksum mismatch.
    ChecksumMismatch,
    /// A signed envelope was required.
    ExpectedSigned,
    /// An unsigned envelope was required.
    ExpectedUnsigned,
    /// Ed25519 signature does not verify under the fixed AuraLite `tx` domain.
    SignatureInvalid,
    /// `--against` unsigned envelope does not match the signed transaction.
    UnsignedMismatch,
    /// Transaction bound to a foreign chain (W2 named-control semantics).
    ForeignChainId {
        /// Chain-id found in the transaction.
        found: String,
        /// Chain-id the wallet is configured for.
        expected: String,
    },
    /// QR frame cannot be parsed.
    QrBadFrame,
    /// QR frame set has a gap.
    QrMissingChunk,
    /// QR frame set contains an index twice.
    QrDuplicateChunk,
}

impl std::fmt::Display for OfflineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadEncoding => write!(f, "envelope: bad encoding"),
            Self::ChecksumMismatch => write!(f, "envelope: checksum mismatch"),
            Self::ExpectedSigned => write!(f, "envelope: expected signed"),
            Self::ExpectedUnsigned => write!(f, "envelope: expected unsigned"),
            Self::SignatureInvalid => write!(f, "signature verification failed"),
            Self::UnsignedMismatch => write!(f, "unsigned mismatch"),
            Self::ForeignChainId { found, expected } => {
                write!(f, "foreign chain-id: tx={found}, wallet={expected}")
            }
            Self::QrBadFrame => write!(f, "qr: bad frame"),
            Self::QrMissingChunk => write!(f, "qr: missing chunk"),
            Self::QrDuplicateChunk => write!(f, "qr: duplicate chunk"),
        }
    }
}

/// Unpadded base64url encode.
#[must_use]
pub fn b64url_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        }
    }
    out
}

/// Unpadded base64url decode; `None` on any invalid input.
#[must_use]
pub fn b64url_decode(text: &str) -> Option<Vec<u8>> {
    let mut values = Vec::with_capacity(text.len());
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        values.push(v);
    }
    if values.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(values.len() * 3 / 4);
    for chunk in values.chunks(4) {
        let n = (u32::from(chunk[0]) << 18)
            | (u32::from(chunk[1]) << 12)
            | chunk.get(2).map_or(0, |&v| u32::from(v) << 6)
            | chunk.get(3).map_or(0, |&v| u32::from(v));
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

fn envelope_binary(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut bin = Vec::with_capacity(ENVELOPE_MAGIC.len() + 1 + 8 + body.len() + 32);
    bin.extend_from_slice(ENVELOPE_MAGIC);
    bin.push(kind);
    bin.extend_from_slice(&body.len().to_le_bytes());
    bin.extend_from_slice(body);
    let digest = blake3_hash(&bin);
    bin.extend_from_slice(digest.as_bytes());
    bin
}

/// Wrap `body` in a kind-`kind` envelope; returns the deterministic text form.
#[must_use]
pub fn encode_envelope(kind: u8, body: &[u8]) -> String {
    b64url_encode(&envelope_binary(kind, body))
}

/// Parse a text envelope; returns `(kind, body)` after checksum verification.
pub fn parse_envelope(text: &str) -> Result<(u8, Vec<u8>), OfflineError> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let bin = b64url_decode(&cleaned).ok_or(OfflineError::BadEncoding)?;
    let header_len = ENVELOPE_MAGIC.len() + 1 + 8;
    let total = header_len + 32;
    if bin.len() < total || &bin[..ENVELOPE_MAGIC.len()] != ENVELOPE_MAGIC {
        return Err(OfflineError::BadEncoding);
    }
    let kind = bin[ENVELOPE_MAGIC.len()];
    if kind != KIND_UNSIGNED && kind != KIND_SIGNED {
        return Err(OfflineError::BadEncoding);
    }
    let len_bytes: [u8; 8] = bin[ENVELOPE_MAGIC.len() + 1..header_len]
        .try_into()
        .expect("8 bytes");
    let body_len =
        usize::try_from(u64::from_le_bytes(len_bytes)).map_err(|_| OfflineError::BadEncoding)?;
    if bin.len() != header_len + body_len + 32 {
        return Err(OfflineError::BadEncoding);
    }
    let body = bin[header_len..header_len + body_len].to_vec();
    let digest = blake3_hash(&bin[..header_len + body_len]);
    if digest.as_bytes() != &bin[header_len + body_len..] {
        return Err(OfflineError::ChecksumMismatch);
    }
    Ok((kind, body))
}

/// Split the text envelope into QR frames (D-W3-2).
#[must_use]
pub fn split_frames(text: &str, size: usize) -> Vec<String> {
    let size = size.max(1);
    let chunks: Vec<&str> = text
        .as_bytes()
        .chunks(size)
        .map(|c| std::str::from_utf8(c).expect("ascii envelope"))
        .collect();
    let n = chunks.len();
    chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| format!("ALO1:{i:02}/{n:02}:{chunk}"))
        .collect()
}

/// Reassemble QR frames into the text envelope (D-W3-2).
pub fn join_frames(lines: &[String]) -> Result<String, OfflineError> {
    let mut slots: Vec<Option<String>> = Vec::new();
    let mut expected_n: Option<usize> = None;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let rest = line.strip_prefix("ALO1:").ok_or(OfflineError::QrBadFrame)?;
        let (head, chunk) = rest.split_once(':').ok_or(OfflineError::QrBadFrame)?;
        let (i_str, n_str) = head.split_once('/').ok_or(OfflineError::QrBadFrame)?;
        let i: usize = i_str.parse().map_err(|_| OfflineError::QrBadFrame)?;
        let n: usize = n_str.parse().map_err(|_| OfflineError::QrBadFrame)?;
        if n == 0 || i >= n || chunk.is_empty() {
            return Err(OfflineError::QrBadFrame);
        }
        if expected_n.is_some_and(|e| e != n) {
            return Err(OfflineError::QrBadFrame);
        }
        expected_n = Some(n);
        if slots.len() < n {
            slots.resize_with(n, || None);
        }
        if slots[i].is_some() {
            return Err(OfflineError::QrDuplicateChunk);
        }
        slots[i] = Some(chunk.to_string());
    }
    let n = expected_n.ok_or(OfflineError::QrBadFrame)?;
    let mut text = String::new();
    for slot in slots.iter().take(n) {
        text.push_str(slot.as_ref().ok_or(OfflineError::QrMissingChunk)?);
    }
    Ok(text)
}

/// Decode a signed transaction from a kind-1 envelope and run the D-W3-3
/// verification contract. `against` (if given) must be the text envelope of
/// the unsigned transaction the human approved.
pub fn verify_signed_envelope(
    text: &str,
    expected_chain: &str,
    against: Option<&str>,
) -> Result<Tx, OfflineError> {
    let (kind, body) = parse_envelope(text)?;
    if kind != KIND_SIGNED {
        return Err(OfflineError::ExpectedSigned);
    }
    let tx = Tx::from_canonical_bytes(&body).map_err(|_| OfflineError::BadEncoding)?;
    if tx.unsigned.chain_id.as_str() != expected_chain {
        return Err(OfflineError::ForeignChainId {
            found: tx.unsigned.chain_id.as_str().to_owned(),
            expected: expected_chain.to_owned(),
        });
    }
    // The W3 named control: only the fixed AuraLite `tx` domain verifies.
    tx.verify_signature()
        .map_err(|_| OfflineError::SignatureInvalid)?;
    if let Some(against_text) = against {
        let (a_kind, a_body) = parse_envelope(against_text)?;
        if a_kind != KIND_UNSIGNED {
            return Err(OfflineError::ExpectedUnsigned);
        }
        if a_body != tx.unsigned.to_bytes() {
            return Err(OfflineError::UnsignedMismatch);
        }
    }
    Ok(tx)
}
