# W6 closeout report — web wallet with real-tx-field confirmation

Date: 2026-11-25. Gate (WALLET_PLAN row W6): "signing shows the human the
**real** tx fields; NC: recipient substitution in UI -> red" — **satisfied**.

- **Real fields**: `ui_recipient_substitution` (the named control) — a deep
  link claiming a recipient that disagrees with the bytes is refused
  (`error: web: recipient substitution`, exit 65) after the dialog has shown
  the **real** `ui: to=…` line derived from the transaction; mutation NC-1
  (`tools/nc_w6.py`) drops the claim comparison and NC-6 renders the line
  from the wrong source — both must turn that test red.
- **Scenarios**: 10 deterministic cases from the independent oracle
  `tools/gen_w6_vectors.py` (W3 envelopes, `recipient||amount` payloads,
  Ed25519 RFC 8032-checked signatures under the fixed `tx` domain), asserted
  byte-for-byte by `tests/w6_web.rs` (11/11 with the count guard). Groups:
  confirm with real fields (2), claim substitutions (2), deep-link schema
  (2), phishing origins (2), blind signing (1), unsupported kind (1).
- **Mutation controls**: 6/6 PASS (recipient comparison dropped, amount
  comparison dropped, homoglyph check dropped, deep-link whitelist dropped,
  blind signing accepted, recipient line rendered from the wrong source).
- **WASM build**: `clients/aura-keys` (own workspace and lockfile) compiles
  `crates/aura-wallet/src/web.rs` verbatim and exposes it through a
  deterministic C ABI with **zero imports**; `tools/build_aura_keys.py` builds
  it twice into independent target directories and refuses to pin unless the
  two artifacts are byte-identical — size/sha256/exports are pinned in
  `tests/fixtures/w6_aura_keys_wasm.txt` and the shipped
  `clients/aura-keys/wasm/aura_keys.wasm`.
- **Device model** (docs/w6-web-wallet-plan.md): D-W6-1 confirmation fields
  come only from the signed bytes, D-W6-2 signing requires the rendered view
  (`aura_keys_sign` re-derives and refuses a doctored `shown` view;
  `aura_keys_sign_without_view` is an always-refusing tripwire), D-W6-3 the
  phishing policy (https-only, no userinfo, ASCII host, official allowlist,
  homoglyphs -> `suspicious origin`), D-W6-4 the strict
  `auralite://sign?tx=…` schema, D-W6-5 honest scope (transfer only; the
  extension under `clients/wallet-web/` is untrusted glue).

Receipts: `gates.txt`, `test-w6.log`, `nc.log`, `wasm-build.log`,
`inventory.txt`, `SHA256SUMS`. Regenerate with `make evidence-w6`. Public
test data only — never use for funds.
