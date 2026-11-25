//! W6 — web/browser wallet core: real-tx-field confirmation, deep links and
//! phishing protection.
//!
//! A page or deep link can show one recipient while the bytes it wants signed
//! carry another. This module is the deterministic model of the web wallet's
//! security core: every confirmation field is derived from the canonical
//! transaction bytes (D-W6-1), signing requires a rendered view (D-W6-2),
//! origins face a phishing policy (D-W6-3) and deep links follow a strict
//! schema (D-W6-4). Scope: `transfer` only (D-W6-5).
//!
//! Design of record: `docs/w6-web-wallet-plan.md` (D-W6-1..D-W6-5). This file
//! is compiled into the workspace for tests **and** included verbatim into the
//! standalone `clients/aura-keys` crate, whose `wasm32-unknown-unknown` build
//! is pinned by `tools/build_aura_keys.py`; it must stay self-contained
//! (only `aura-tx`, `aura-crypto`, `aura-types`, `std`).
//!
//! NC targets (`tools/nc_w6.py`):
//! 1. recipient comparison dropped -> `ui_recipient_substitution` red;
//! 2. amount comparison dropped -> `ui_amount_substitution` red;
//! 3. homoglyph (non-ASCII host) check dropped -> `origin_lookalike_refused`
//!    red (the lookalike then falls through to `untrusted origin`);
//! 4. deep-link whitelist dropped -> `deep_link_unknown_param` red;
//! 5. blind signing accepted -> `blind_ui_sign_refused` red;
//! 6. view rendered from the claim -> `ui_recipient_substitution` red.

use std::fmt;

use aura_crypto::blake3_hash;
use aura_tx::{Tx, TxKind, UnsignedTx};
use aura_types::Address;

/// Web wallet identity shown on every response.
pub const WEB_UI_VERSION: &str = "aura-keys-1";

/// Official hosts the wallet may run on (placeholders reserved under RFC 2606).
pub const TRUSTED_HOSTS: [&str; 2] = ["wallet.auralite.invalid", "app.auralite.invalid"];

/// The only deep-link action this protocol version accepts.
pub const DEEP_LINK_PREFIX: &str = "auralite://sign?";

/// Envelope magic of the W3 offline format (self-contained copy).
const ENVELOPE_MAGIC: &[u8] = b"AuraLite-OFFLINE-V1";

/// Envelope kind of an unsigned transaction body (W3).
const KIND_UNSIGNED: u8 = 0;

/// Refusals of the web wallet core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebError {
    /// The deep link does not match the strict schema of D-W6-4.
    MalformedDeepLink,
    /// The deep link carries a parameter outside the D-W6-4 whitelist.
    UnknownParameter,
    /// The origin is a lookalike of an official host or structurally hostile.
    SuspiciousOrigin,
    /// The origin is well-formed but not an official host.
    UntrustedOrigin,
    /// The claimed recipient disagrees with the bytes (D-W6-1).
    RecipientSubstitution,
    /// The claimed amount disagrees with the bytes (D-W6-1).
    AmountSubstitution,
    /// A signing request that skipped the rendered confirmation view (D-W6-2).
    UiConfirmationSkipped,
    /// The transaction targets a chain the wallet is not configured for.
    ForeignChainId {
        /// Chain id found in the transaction.
        found: String,
        /// Chain id the wallet is configured for.
        expected: String,
    },
    /// Only `transfer` is signable in v1 (D-W6-5).
    UnsupportedKind,
    /// The envelope or transaction bytes do not decode.
    UndecodableEnvelope,
    /// The transfer payload is not `recipient||amount` (D-W5-2 shape).
    UndecodablePayload,
    /// The human denied the request.
    DeniedByUser,
    /// The signing primitive failed.
    SigningFailed,
}

impl fmt::Display for WebError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedDeepLink => write!(f, "malformed deep link"),
            Self::UnknownParameter => write!(f, "unknown deep-link parameter"),
            Self::SuspiciousOrigin => write!(f, "suspicious origin"),
            Self::UntrustedOrigin => write!(f, "untrusted origin"),
            Self::RecipientSubstitution => write!(f, "recipient substitution"),
            Self::AmountSubstitution => write!(f, "amount substitution"),
            Self::UiConfirmationSkipped => write!(f, "ui confirmation skipped"),
            Self::ForeignChainId { .. } => write!(f, "foreign chain-id"),
            Self::UnsupportedKind => write!(f, "unsupported transaction kind"),
            Self::UndecodableEnvelope => write!(f, "undecodable envelope"),
            Self::UndecodablePayload => write!(f, "undecodable payload"),
            Self::DeniedByUser => write!(f, "denied by user"),
            Self::SigningFailed => write!(f, "signing failed"),
        }
    }
}

/// What the page or deep link *claims* the human approved (D-W6-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UiClaim {
    /// Recipient the host claims the user is paying.
    pub recipient: Option<Address>,
    /// Amount the host claims the user is paying.
    pub amount: Option<u64>,
}

/// A parsed `auralite://sign?…` deep link (D-W6-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepLink {
    /// The W3 unsigned envelope carried in `tx`.
    pub envelope: String,
    /// Host-claimed fields carried in `to`/`amount`.
    pub claim: UiClaim,
    /// Optional chain claim carried in `chain`.
    pub chain: Option<String>,
}

/// The rendered confirmation view derived from the transaction bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmView {
    /// The canonical unsigned transaction the view was derived from.
    pub unsigned: UnsignedTx,
    /// Real recipient decoded from the payload.
    pub recipient: Address,
    /// Real amount decoded from the payload.
    pub amount: u64,
    /// Deterministic `ui: key=value` render of the real fields.
    pub lines: Vec<String>,
}

impl ConfirmView {
    /// Bind a host claim against the derived fields (D-W6-1).
    ///
    /// # Errors
    ///
    /// [`WebError::RecipientSubstitution`] / [`WebError::AmountSubstitution`]
    /// when the claim disagrees with the bytes the user would sign.
    pub fn bind(&self, claim: &UiClaim) -> Result<(), WebError> {
        // A claimed recipient that disagrees with the bytes is the classic UI
        // substitution attack (NC-1 target).
        if claim.recipient.is_some_and(|r| r != self.recipient) {
            return Err(WebError::RecipientSubstitution);
        }
        if claim.amount.is_some_and(|a| a != self.amount) {
            return Err(WebError::AmountSubstitution);
        }
        Ok(())
    }
}

/// Unpadded base64url decode (self-contained W3 alphabet).
fn b64url_decode(text: &str) -> Option<Vec<u8>> {
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

/// Decode a W3 unsigned envelope into its canonical transaction body.
fn decode_unsigned_envelope(text: &str) -> Result<Vec<u8>, WebError> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let bin = b64url_decode(&cleaned).ok_or(WebError::UndecodableEnvelope)?;
    let header_len = ENVELOPE_MAGIC.len() + 1 + 8;
    if bin.len() < header_len + 32 || &bin[..ENVELOPE_MAGIC.len()] != ENVELOPE_MAGIC {
        return Err(WebError::UndecodableEnvelope);
    }
    if bin[ENVELOPE_MAGIC.len()] != KIND_UNSIGNED {
        return Err(WebError::UndecodableEnvelope);
    }
    let len_bytes: [u8; 8] = bin[ENVELOPE_MAGIC.len() + 1..header_len]
        .try_into()
        .expect("8 bytes");
    let body_len = usize::try_from(u64::from_le_bytes(len_bytes))
        .map_err(|_| WebError::UndecodableEnvelope)?;
    if bin.len() != header_len + body_len + 32 {
        return Err(WebError::UndecodableEnvelope);
    }
    let digest = blake3_hash(&bin[..header_len + body_len]);
    if digest.as_bytes() != &bin[header_len + body_len..] {
        return Err(WebError::UndecodableEnvelope);
    }
    Ok(bin[header_len..header_len + body_len].to_vec())
}

/// Parse a `auralite://sign?…` deep link under the strict D-W6-4 schema.
///
/// # Errors
///
/// [`WebError::MalformedDeepLink`] for structural violations,
/// [`WebError::UnknownParameter`] for parameters outside the whitelist.
pub fn parse_deep_link(uri: &str) -> Result<DeepLink, WebError> {
    let query = uri
        .strip_prefix(DEEP_LINK_PREFIX)
        .ok_or(WebError::MalformedDeepLink)?;
    if query.is_empty() {
        return Err(WebError::MalformedDeepLink);
    }
    let mut envelope: Option<String> = None;
    let mut recipient: Option<Address> = None;
    let mut amount: Option<u64> = None;
    let mut chain: Option<String> = None;
    for item in query.split('&') {
        let (key, value) = item.split_once('=').ok_or(WebError::MalformedDeepLink)?;
        if value.is_empty() {
            return Err(WebError::MalformedDeepLink);
        }
        // The parameter set is a strict whitelist (D-W6-4); anything else is
        // a host trying to smuggle semantics past the wallet (NC-4 target is
        // the catch-all arm below).
        match key {
            "tx" => {
                if envelope.replace(value.to_owned()).is_some() {
                    return Err(WebError::MalformedDeepLink);
                }
            }
            "to" => {
                let parsed =
                    Address::from_bech32(value).map_err(|_| WebError::MalformedDeepLink)?;
                if recipient.replace(parsed).is_some() {
                    return Err(WebError::MalformedDeepLink);
                }
            }
            "amount" => {
                let parsed = value
                    .parse::<u64>()
                    .map_err(|_| WebError::MalformedDeepLink)?;
                if amount.replace(parsed).is_some() {
                    return Err(WebError::MalformedDeepLink);
                }
            }
            "chain" => {
                if chain.replace(value.to_owned()).is_some() {
                    return Err(WebError::MalformedDeepLink);
                }
            }
            _ => return Err(WebError::UnknownParameter),
        }
    }
    let envelope = envelope.ok_or(WebError::MalformedDeepLink)?;
    Ok(DeepLink {
        envelope,
        claim: UiClaim { recipient, amount },
        chain,
    })
}

/// Apply the D-W6-3 phishing policy to a request origin.
///
/// Policy: `https://` only, no userinfo, ASCII host (homoglyph gate), and an
/// exact match against the official hosts.
///
/// # Errors
///
/// [`WebError::SuspiciousOrigin`] for hostile or lookalike origins,
/// [`WebError::UntrustedOrigin`] for anything outside the official hosts.
pub fn check_origin(origin: &str) -> Result<(), WebError> {
    let rest = origin
        .strip_prefix("https://")
        .ok_or(WebError::SuspiciousOrigin)?;
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return Err(WebError::SuspiciousOrigin);
    }
    let host = authority.split(':').next().unwrap_or("");
    if host.is_empty() {
        return Err(WebError::SuspiciousOrigin);
    }
    // A non-ASCII host is a homoglyph until proven otherwise: browsers show
    // punycode for exactly these, and the wallet refuses outright (NC-3).
    if !host.is_ascii() {
        return Err(WebError::SuspiciousOrigin);
    }
    let lower = host.to_ascii_lowercase();
    if TRUSTED_HOSTS.contains(&lower.as_str()) {
        Ok(())
    } else {
        Err(WebError::UntrustedOrigin)
    }
}

/// Derive the confirmation view from the bytes the user would sign (D-W6-1).
///
/// # Errors
///
/// Envelope/chain/kind/payload refusals as documented on [`WebError`].
pub fn derive_view(envelope: &str, wallet_chain: &str) -> Result<ConfirmView, WebError> {
    let body = decode_unsigned_envelope(envelope)?;
    let unsigned =
        UnsignedTx::from_canonical_bytes(&body).map_err(|_| WebError::UndecodableEnvelope)?;
    if unsigned.chain_id.as_str() != wallet_chain {
        return Err(WebError::ForeignChainId {
            found: unsigned.chain_id.as_str().to_owned(),
            expected: wallet_chain.to_owned(),
        });
    }
    if unsigned.kind != TxKind::Transfer {
        return Err(WebError::UnsupportedKind);
    }
    if unsigned.payload.len() != 28 {
        return Err(WebError::UndecodablePayload);
    }
    let recipient = Address::from_bytes(unsigned.payload[0..20].try_into().expect("20 bytes"));
    let amount = u64::from_le_bytes(unsigned.payload[20..28].try_into().expect("8 bytes"));
    let lines = vec![
        format!("ui: chain={}", unsigned.chain_id.as_str()),
        String::from("ui: kind=transfer"),
        format!("ui: signer={}", unsigned.signer.to_bech32()),
        format!("ui: nonce={}", unsigned.nonce.value()),
        // The recipient line is rendered from the *bytes*, never from a claim
        // (NC-6 target).
        format!("ui: to={}", recipient.to_bech32()),
        format!("ui: amount={amount}"),
        format!("ui: gas={}", unsigned.gas_limit.value()),
        format!("ui: max_fee={}", unsigned.max_fee.units()),
        format!("ui: tip={}", unsigned.tip.units()),
        format!("ui: expiry={}", unsigned.expiry.value()),
    ];
    Ok(ConfirmView {
        unsigned,
        recipient,
        amount,
        lines,
    })
}

/// Sign a confirmed transaction under the fixed AuraLite `tx` domain (D-W6-2).
///
/// # Errors
///
/// [`WebError::SigningFailed`] when the signing primitive fails.
pub fn sign_confirmed(view: &ConfirmView, secret: &[u8; 32]) -> Result<Tx, WebError> {
    view.unsigned
        .sign(secret)
        .map_err(|_| WebError::SigningFailed)
}

/// The blind-signing attack surface: always refused (D-W6-2).
///
/// # Errors
///
/// Always [`WebError::UiConfirmationSkipped`].
pub fn sign_without_view(_envelope: &str, _secret: &[u8; 32]) -> Result<Tx, WebError> {
    // Never sign unseen bytes — refused before any key use (NC-5 target).
    Err(WebError::UiConfirmationSkipped)
}
