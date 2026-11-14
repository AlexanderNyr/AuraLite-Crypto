# W3 evidence — air-gapped offline signing (24 CLI vectors)

Gate (WALLET_PLAN row W3): "air-gapped scenario end-to-end; NC: signature with
a foreign domain -> reject". All fixtures are PUBLIC TEST DATA ONLY — never
use for funds.

- `test-w3.log` — `cargo test --features insecure-test-kdf --test w3_offline`:
  24 vectors plus `vectors_file_has_exactly_24_unique_cases`,
  byte-for-byte against the independent oracle.
- `nc.log` — `tools/nc_w3.py` mutation controls (6): each source mutation must
  turn its named test red; compilation failure is not a valid red.
- `inventory.txt` — the 24 cases with exit codes, the named control, the
  cumulative NC total (418) and SHA-256 pins of fixture, generator, runner.
- `report.md`, `gates.txt` — closeout report and gate summary.
- `SHA256SUMS` — digests of the deliverables and evidence files.

The vectors are produced by `tools/gen_w3_vectors.py` (pure Python: BLAKE3,
Ed25519 RFC 8032, C2/C4 encoders, bech32, BIP-39/SLIP-0010). Regenerate with
`make gen-w3-vectors`; rebuild this bundle with `make evidence-w3`.
