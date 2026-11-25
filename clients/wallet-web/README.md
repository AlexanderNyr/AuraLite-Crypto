# wallet-web — AuraLite browser extension (W6)

**This directory is untrusted glue.** The trusted computing base of the web
wallet is the `aura-keys` WASM artifact (`clients/aura-keys/`, built from
`crates/aura-wallet/src/web.rs`); every security decision — the real-tx-field
confirmation view (D-W6-1), the confirmation binding (D-W6-2), the phishing
policy (D-W6-3) and the deep-link schema (D-W6-4) — runs inside it. The
artifact has **zero imports**, so no JavaScript can be called from inside the
core.

## Layout

- `manifest.json` — Manifest V3 extension (popup, service worker, content
  script; the WASM artifact is shipped as a web-accessible resource).
- `content.js` — dApp bridge: forwards `auralite://sign?…` deep links and the
  true page origin. It never sees keys or transaction fields.
- `background.js` — relays requests through `aura_keys_deep_link_parse` and
  `aura_keys_origin_check`.
- `popup.html` / `popup.js` — the confirmation dialog. It renders **only** the
  `ui:` lines derived from the transaction bytes by `aura_keys_confirm_view`;
  a page claim is checked by `aura_keys_claim_check` and a disagreement blocks
  Confirm. `aura_keys_sign` receives the exact shown text and re-derives the
  view internally, refusing on any difference.

## Threat model (the three attacks this gates)

1. **display substitution** — the page claims one recipient while the bytes
   carry another: the dialog shows the bytes (never the claim), the claim is
   checked against the bytes, and the named control
   `ui_recipient_substitution` keeps the refusal red.
2. **blind signing** — any request that skips the rendered view is refused
   (`aura_keys_sign_without_view` is a tripwire that always refuses).
3. **phishing origins** — only `https://` official hosts pass; homoglyph
   lookalikes are refused as `suspicious origin`.

Vectors and mutation controls: `tests/fixtures/w6_web_wallet_vectors.json`
(`tools/gen_w6_vectors.py`), `tools/nc_w6.py`, gate run via `make test-w6`.
The WASM build is reproducible and pinned by `tools/build_aura_keys.py`.
PUBLIC TEST DATA ONLY — never use for funds.
