# W4 evidence — M-of-N multisig contract (22 CLI vectors)

Gate (WALLET_PLAN row W4): "3-of-5: 3 collected -> success, 2 -> reject; NC:
threshold bypass -> red". All fixtures are PUBLIC TEST DATA ONLY — never use
for funds.

- `test-w4.log` — `cargo test --features insecure-test-kdf --test w4_multisig`:
  22 vectors plus `vectors_file_has_exactly_22_unique_cases`,
  byte-for-byte against the independent oracle.
- `nc.log` — `tools/nc_w4.py` mutation controls (6): each source mutation must
  turn its named test red; compilation failure is not a valid red.
- `inventory.txt` — the 22 cases with exit codes, the named control, the
  cumulative NC total (424) and SHA-256 pins of fixture, generator, runner.
- `report.md`, `gates.txt` — closeout report and gate summary.
- `SHA256SUMS` — digests of the deliverables and evidence files.

The vectors are produced by `tools/gen_w4_vectors.py` (pure Python: BLAKE3,
Ed25519 RFC 8032, domain_message, BIP-39/SLIP-0010, bech32). Regenerate with
`make gen-w4-vectors`; rebuild this bundle with `make evidence-w4`.
