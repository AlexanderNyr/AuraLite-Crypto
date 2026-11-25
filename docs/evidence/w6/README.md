# W6 evidence — web wallet with real-tx-field confirmation (10 scenarios)

Gate (WALLET_PLAN row W6): "signing shows the human the **real** tx fields;
NC: recipient substitution in UI -> red". All fixtures are PUBLIC TEST DATA
ONLY — never use for funds.

- `test-w6.log` — `cargo test --features insecure-test-kdf --test w6_web`:
  10 scenarios plus `vectors_file_has_exactly_10_unique_cases`,
  byte-for-byte against the independent oracle.
- `nc.log` — `tools/nc_w6.py` mutation controls (6): each source mutation must
  turn its named test red; compilation failure is not a valid red.
- `wasm-build.log` — `tools/build_aura_keys.py --check`: the `aura-keys` WASM
  artifact rebuilt twice into independent target directories, byte-identical,
  matching the pinned hash in `tests/fixtures/w6_aura_keys_wasm.txt`.
- `inventory.txt` — the 10 cases with exit codes, the named control, the WASM
  pins, the cumulative NC total (662) and SHA-256 pins of fixture,
  generator, runner.
- `report.md`, `gates.txt` — closeout report and gate summary.
- `SHA256SUMS` — digests of the deliverables and evidence files.

The vectors are produced by `tools/gen_w6_vectors.py` (pure Python: BLAKE3,
Ed25519 RFC 8032, domain_message, BIP-39/SLIP-0010, bech32). Regenerate with
`make gen-w6-vectors`; rebuild this bundle with `make evidence-w6`.
