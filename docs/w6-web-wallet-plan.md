# W6 web wallet — design of record (D-W6-1..D-W6-5)

**Phase:** W6 (WALLET_PLAN Stage 2). **Deliverable:** `patches/0101-W6-…`.
**Model:** `docs/w5-hardware-plan.md` (W5). The web wallet is a deterministic
model plus a real `wasm32-unknown-unknown` build: the browser extension and
deep links are thin glue, and every security decision lives in Rust that can
be built into the shipped WASM artifact `clients/aura-keys`.
PUBLIC TEST DATA ONLY — never use for funds.

## Problem

A page or deep link can show the human one recipient while the bytes it wants
signed carry another. Real incidents are dominated by exactly this
**display substitution** in web wallets, plus **phishing origins**
(homoglyph/lookalike domains) and **opaque sign requests** (blind signing).

## Decisions

- **D-W6-1 (real fields).** The signing confirmation shows only fields derived
  from the canonical transaction bytes (`ui: chain/kind/signer/nonce/to/
  amount/gas/max_fee/tip/expiry`). A host claim (`to`/`amount` from the page
  or deep link) is never rendered as truth: it is compared against the derived
  fields and a disagreement is refused (`recipient substitution` /
  `amount substitution`) — the real fields are shown even on the refusal path.
- **D-W6-2 (confirmation binding).** Signing happens only through a rendered
  view (`confirm_view -> human Confirm -> sign`); a request to sign without a
  view is always refused (`ui confirmation skipped`), and `Deny` never signs
  and never latches (retry works).
- **D-W6-3 (phishing protection).** Origins must be `https://`, without
  userinfo, with an ASCII host in the built-in allowlist. A host that is
  visually confusable with an allowlisted host (Cyrillic/Greek homoglyphs) is
  refused as `suspicious origin`; anything else unlisted is `untrusted origin`.
- **D-W6-4 (deep links).** `auralite://sign?…` has a strict schema: exactly
  the parameters `tx` (required) plus optional `to`, `amount`, `chain`, each
  at most once, `to` bech32, `amount` u64. Anything else is `malformed deep
  link` / `unknown deep-link parameter`. The `tx` parameter is a W3 unsigned
  envelope (checked bytes, BLAKE3 digest verified).
- **D-W6-5 (honest scope).** v1 signs `transfer` only (`recipient||amount`
  payloads per D-W5-2); other kinds are refused (`unsupported transaction
  kind`). The trusted computing base is Rust: `crates/aura-wallet/src/web.rs`
  is compiled into the workspace for tests and included verbatim into the
  standalone `clients/aura-keys` crate (own lockfile, `cdylib`), built for
  `wasm32-unknown-unknown` twice with remapped paths — the two artifacts must
  be byte-identical and their size/sha256/exports are pinned
  (`tools/build_aura_keys.py`). The extension statics under
  `clients/wallet-web/` are untrusted glue that call this artifact.

## Gate (WALLET_PLAN row W6)

`signing shows the human the **real** tx fields; NC: recipient substitution in UI -> red`:

| # | Scenario | Expect |
| --- | --- | --- |
| 1 | `sign_transfer_real_fields` | exit 0, `ui:` lines pinned + `sig` |
| 2 | `sign_transfer_matching_claim` | exit 0 (claim == derived) |
| 3 | `ui_recipient_substitution` | view shows the **real** `to`, then exit 65 `error: web: recipient substitution` — **named control** |
| 4 | `ui_amount_substitution` | view shows the real amount, exit 65 `amount substitution` |
| 5 | `deep_link_malformed` | exit 65 `error: web: malformed deep link` |
| 6 | `deep_link_unknown_param` | exit 65 `error: web: unknown deep-link parameter` |
| 7 | `origin_lookalike_refused` | exit 65 `error: web: suspicious origin` |
| 8 | `origin_trusted_signs` | exit 0 |
| 9 | `blind_ui_sign_refused` | exit 65 `error: web: ui confirmation skipped` |
| 10 | `unsupported_kind_refused` | exit 65 `error: web: unsupported transaction kind` |

Mutation controls (`tools/nc_w6.py`, 6): recipient comparison dropped, amount
comparison dropped, homoglyph check dropped, deep-link whitelist dropped,
blind signing accepted, view rendered from the claim — each must turn its
named scenario red.

Cumulative NC after W6: **662** (656 through W5 + 6).
