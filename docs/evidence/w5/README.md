# W5 evidence — hardware keys with on-screen confirmation (10 scenarios)

Gate (WALLET_PLAN row W5): "10 \"human confirmed/denied\" scenarios; NC: blind
confirmation -> red". All fixtures are PUBLIC TEST DATA ONLY — never use for
funds.

- `test-w5.log` — `cargo test --features insecure-test-kdf --test w5_hardware`:
  10 scenarios plus `vectors_file_has_exactly_10_unique_cases`,
  byte-for-byte against the independent oracle.
- `nc.log` — `tools/nc_w5.py` mutation controls (6): each source mutation must
  turn its named test red; compilation failure is not a valid red.
- `inventory.txt` — the 10 cases with exit codes, the named control, the
  cumulative NC total (656) and SHA-256 pins of fixture, generator, runner.
- `report.md`, `gates.txt` — closeout report and gate summary.
- `SHA256SUMS` — digests of the deliverables and evidence files.

The vectors are produced by `tools/gen_w5_vectors.py` (pure Python: BLAKE3,
Ed25519 RFC 8032, domain_message, BIP-39/SLIP-0010, bech32). Regenerate with
`make gen-w5-vectors`; rebuild this bundle with `make evidence-w5`.
