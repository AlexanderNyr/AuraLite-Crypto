//! `aura-keys` — the WASM key layer of the AuraLite web wallet (W6).
//!
//! This artifact is the trusted computing base of `clients/wallet-web/`: the
//! browser extension is untrusted glue, and every security decision — the
//! real-tx-field confirmation view (D-W6-1), the confirmation binding
//! (D-W6-2), the phishing policy (D-W6-3) and the deep-link schema
//! (D-W6-4) — runs inside this module. The logic is `crates/aura-wallet/src/web.rs`
//! included verbatim; the `wasm32-unknown-unknown` build is pinned by
//! `tools/build_aura_keys.py` (size, sha256, imports, exports).
//!
//! All exports are deterministic and buffer-based: text inputs arrive as
//! `(ptr, len)` UTF-8 pairs, outputs are written into a caller-provided
//! buffer and the return value is either the number of bytes written or a
//! negative [`codes`] error. Nothing here allocates on behalf of the caller.

#![deny(missing_docs)]

#[allow(dead_code)]
#[path = "../../../crates/aura-wallet/src/web.rs"]
mod web;

/// Deterministic return codes shared by every export.
pub mod codes {
    /// Success; for buffer exports, the number of bytes written is returned
    /// as a non-negative length instead.
    pub const OK: i32 = 0;
    /// The deep link is malformed (D-W6-4).
    pub const ERR_MALFORMED_DEEP_LINK: i32 = -1;
    /// The deep link carries a parameter outside the whitelist (D-W6-4).
    pub const ERR_UNKNOWN_PARAMETER: i32 = -2;
    /// The origin is a lookalike or structurally hostile (D-W6-3).
    pub const ERR_SUSPICIOUS_ORIGIN: i32 = -3;
    /// The origin is well-formed but not an official host (D-W6-3).
    pub const ERR_UNTRUSTED_ORIGIN: i32 = -4;
    /// The request skipped the rendered confirmation view (D-W6-2).
    pub const ERR_UI_CONFIRMATION_SKIPPED: i32 = -5;
    /// The transaction targets another chain.
    pub const ERR_FOREIGN_CHAIN_ID: i32 = -6;
    /// The transaction kind is out of scope for v1 (D-W6-5).
    pub const ERR_UNSUPPORTED_KIND: i32 = -7;
    /// The envelope or transaction bytes do not decode.
    pub const ERR_UNDECODABLE_ENVELOPE: i32 = -8;
    /// The transfer payload is not `recipient||amount`.
    pub const ERR_UNDECODABLE_PAYLOAD: i32 = -9;
    /// The shown view disagrees with the view derived from the bytes (D-W6-2).
    pub const ERR_SHOWN_VIEW_MISMATCH: i32 = -10;
    /// The signing primitive failed.
    pub const ERR_SIGNING_FAILED: i32 = -11;
    /// The out buffer is too small for the result.
    pub const ERR_BUFFER_TOO_SMALL: i32 = -12;
    /// A pointer/length/UTF-8 input is invalid.
    pub const ERR_BAD_INPUT: i32 = -13;
    /// The claimed recipient disagrees with the bytes (D-W6-1).
    pub const ERR_RECIPIENT_SUBSTITUTION: i32 = -14;
    /// The claimed amount disagrees with the bytes (D-W6-1).
    pub const ERR_AMOUNT_SUBSTITUTION: i32 = -15;
}

/// API version of this export surface (bumped on breaking changes).
pub const API_VERSION: u32 = 1;

/// WASM randomness shim: the artifact never generates randomness — keys come
/// from the caller and every flow is deterministic (W6). If some future path
/// calls `getrandom`, it fails loudly instead of returning zeros.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod wasm_rng {
    fn unsupported(_dest: &mut [u8]) -> Result<(), getrandom::Error> {
        Err(getrandom::Error::UNSUPPORTED)
    }
    getrandom::register_custom_getrandom!(unsupported);
}

fn map_web(e: web::WebError) -> i32 {
    match e {
        web::WebError::MalformedDeepLink => codes::ERR_MALFORMED_DEEP_LINK,
        web::WebError::UnknownParameter => codes::ERR_UNKNOWN_PARAMETER,
        web::WebError::SuspiciousOrigin => codes::ERR_SUSPICIOUS_ORIGIN,
        web::WebError::UntrustedOrigin => codes::ERR_UNTRUSTED_ORIGIN,
        web::WebError::RecipientSubstitution => codes::ERR_RECIPIENT_SUBSTITUTION,
        web::WebError::AmountSubstitution => codes::ERR_AMOUNT_SUBSTITUTION,
        web::WebError::UiConfirmationSkipped => codes::ERR_UI_CONFIRMATION_SKIPPED,
        web::WebError::ForeignChainId { .. } => codes::ERR_FOREIGN_CHAIN_ID,
        web::WebError::UnsupportedKind => codes::ERR_UNSUPPORTED_KIND,
        web::WebError::UndecodableEnvelope => codes::ERR_UNDECODABLE_ENVELOPE,
        web::WebError::UndecodablePayload => codes::ERR_UNDECODABLE_PAYLOAD,
        web::WebError::DeniedByUser => codes::ERR_UI_CONFIRMATION_SKIPPED,
        web::WebError::SigningFailed => codes::ERR_SIGNING_FAILED,
    }
}

unsafe fn text_from_raw<'a>(ptr: *const u8, len: usize) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    std::str::from_utf8(slice).ok()
}

unsafe fn write_out(out: *mut u8, out_cap: usize, bytes: &[u8]) -> i32 {
    if out.is_null() || bytes.len() > out_cap {
        return codes::ERR_BUFFER_TOO_SMALL;
    }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()) };
    i32::try_from(bytes.len()).unwrap_or(codes::ERR_BUFFER_TOO_SMALL)
}

/// Return the API version of this artifact.
#[unsafe(no_mangle)]
pub extern "C" fn aura_keys_api_version() -> u32 {
    API_VERSION
}

/// Check a request origin against the D-W6-3 phishing policy.
///
/// Returns [`codes::OK`] when the origin is an official `https://` host,
/// [`codes::ERR_SUSPICIOUS_ORIGIN`] / [`codes::ERR_UNTRUSTED_ORIGIN`] on
/// refusal, [`codes::ERR_BAD_INPUT`] on invalid pointer/UTF-8 input.
///
/// # Safety
///
/// `origin` must reference `origin_len` readable bytes of UTF-8 text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aura_keys_origin_check(origin: *const u8, origin_len: usize) -> i32 {
    let Some(text) = (unsafe { text_from_raw(origin, origin_len) }) else {
        return codes::ERR_BAD_INPUT;
    };
    match web::check_origin(text) {
        Ok(()) => codes::OK,
        Err(e) => map_web(e),
    }
}

/// Parse a `auralite://sign?…` deep link (D-W6-4) and write the W3 unsigned
/// envelope carried in `tx` into `out`.
///
/// Returns the written length on success, or a negative [`codes`] error.
///
/// # Safety
///
/// `uri` must reference `uri_len` readable bytes of UTF-8 text; `out` must
/// reference `out_cap` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aura_keys_deep_link_parse(
    uri: *const u8,
    uri_len: usize,
    out: *mut u8,
    out_cap: usize,
) -> i32 {
    let Some(text) = (unsafe { text_from_raw(uri, uri_len) }) else {
        return codes::ERR_BAD_INPUT;
    };
    match web::parse_deep_link(text) {
        Ok(link) => unsafe { write_out(out, out_cap, link.envelope.as_bytes()) },
        Err(e) => map_web(e),
    }
}

/// Derive the confirmation view from the unsigned envelope and write the
/// rendered `ui:` lines (separated by `\n`) into `out` (D-W6-1).
///
/// Returns the written length on success, or a negative [`codes`] error.
///
/// # Safety
///
/// `envelope`/`chain` must reference readable UTF-8 bytes; `out` must
/// reference `out_cap` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aura_keys_confirm_view(
    envelope: *const u8,
    envelope_len: usize,
    chain: *const u8,
    chain_len: usize,
    out: *mut u8,
    out_cap: usize,
) -> i32 {
    let Some(envelope) = (unsafe { text_from_raw(envelope, envelope_len) }) else {
        return codes::ERR_BAD_INPUT;
    };
    let Some(chain) = (unsafe { text_from_raw(chain, chain_len) }) else {
        return codes::ERR_BAD_INPUT;
    };
    match web::derive_view(envelope, chain) {
        Ok(view) => unsafe { write_out(out, out_cap, view.lines.join("\n").as_bytes()) },
        Err(e) => map_web(e),
    }
}

/// Sign an envelope only when the view the human saw (`shown`) is exactly the
/// view derived from the bytes (D-W6-2), and write the 64-byte Ed25519
/// signature into `out`.
///
/// Returns the written length (64) on success, or a negative [`codes`]
/// error; a doctored `shown` view is [`codes::ERR_SHOWN_VIEW_MISMATCH`].
///
/// # Safety
///
/// `key` must reference 32 readable bytes; `envelope`/`chain`/`shown` must
/// reference readable UTF-8 bytes; `out` must reference `out_cap` writable
/// bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aura_keys_sign(
    key: *const u8,
    key_len: usize,
    envelope: *const u8,
    envelope_len: usize,
    chain: *const u8,
    chain_len: usize,
    shown: *const u8,
    shown_len: usize,
    out: *mut u8,
    out_cap: usize,
) -> i32 {
    if key.is_null() || key_len != 32 {
        return codes::ERR_BAD_INPUT;
    }
    let key = unsafe { std::slice::from_raw_parts(key, key_len) };
    let key: [u8; 32] = key.try_into().expect("32 bytes");
    let Some(envelope) = (unsafe { text_from_raw(envelope, envelope_len) }) else {
        return codes::ERR_BAD_INPUT;
    };
    let Some(chain) = (unsafe { text_from_raw(chain, chain_len) }) else {
        return codes::ERR_BAD_INPUT;
    };
    let Some(shown) = (unsafe { text_from_raw(shown, shown_len) }) else {
        return codes::ERR_BAD_INPUT;
    };
    let view = match web::derive_view(envelope, chain) {
        Ok(view) => view,
        Err(e) => return map_web(e),
    };
    // The signature is produced only for bytes whose real view equals what
    // the human was shown (D-W6-2).
    if view.lines.join("\n") != shown {
        return codes::ERR_SHOWN_VIEW_MISMATCH;
    }
    match web::sign_confirmed(&view, &key) {
        Ok(tx) => unsafe { write_out(out, out_cap, &tx.sig.bytes) },
        Err(e) => map_web(e),
    }
}

/// The blind-signing tripwire (D-W6-2): always refuses.
///
/// Runs the shared `web::sign_without_view` refusal path and returns
/// [`codes::ERR_UI_CONFIRMATION_SKIPPED`] regardless of input.
#[unsafe(no_mangle)]
pub extern "C" fn aura_keys_sign_without_view() -> i32 {
    match web::sign_without_view("", &[0u8; 32]) {
        Ok(_) => codes::OK,
        Err(e) => map_web(e),
    }
}

/// Return the web wallet UI identity string into `out`.
///
/// Returns the written length on success or [`codes::ERR_BUFFER_TOO_SMALL`].
///
/// # Safety
///
/// `out` must reference `out_cap` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aura_keys_ui_version(out: *mut u8, out_cap: usize) -> i32 {
    unsafe { write_out(out, out_cap, web::WEB_UI_VERSION.as_bytes()) }
}

/// Bind a host claim (`to` bech32, `amount` decimal; empty string = absent)
/// against the view derived from the envelope (D-W6-1).
///
/// Returns [`codes::OK`] when the claim agrees with the bytes,
/// [`codes::ERR_RECIPIENT_SUBSTITUTION`] / [`codes::ERR_AMOUNT_SUBSTITUTION`]
/// when it disagrees, or a negative [`codes`] error.
///
/// # Safety
///
/// All text parameters must reference readable UTF-8 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aura_keys_claim_check(
    envelope: *const u8,
    envelope_len: usize,
    chain: *const u8,
    chain_len: usize,
    to: *const u8,
    to_len: usize,
    amount: *const u8,
    amount_len: usize,
) -> i32 {
    let Some(envelope) = (unsafe { text_from_raw(envelope, envelope_len) }) else {
        return codes::ERR_BAD_INPUT;
    };
    let Some(chain) = (unsafe { text_from_raw(chain, chain_len) }) else {
        return codes::ERR_BAD_INPUT;
    };
    let Some(to) = (unsafe { text_from_raw(to, to_len) }) else {
        return codes::ERR_BAD_INPUT;
    };
    let Some(amount) = (unsafe { text_from_raw(amount, amount_len) }) else {
        return codes::ERR_BAD_INPUT;
    };
    let view = match web::derive_view(envelope, chain) {
        Ok(view) => view,
        Err(e) => return map_web(e),
    };
    let claim = web::UiClaim {
        recipient: if to.is_empty() {
            None
        } else {
            match aura_types::Address::from_bech32(to) {
                Ok(address) => Some(address),
                Err(_) => return codes::ERR_BAD_INPUT,
            }
        },
        amount: if amount.is_empty() {
            None
        } else {
            match amount.parse::<u64>() {
                Ok(value) => Some(value),
                Err(_) => return codes::ERR_BAD_INPUT,
            }
        },
    };
    match view.bind(&claim) {
        Ok(()) => codes::OK,
        Err(e) => map_web(e),
    }
}
