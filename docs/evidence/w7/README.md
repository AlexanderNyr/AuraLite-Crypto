# W7 evidence — offline genesis ceremony (10 scenarios)

Gate (WALLET_PLAN row W7): "genesis ceremony never writes one private key
outside shredded media; NC: no_private_key_in_outputs". All fixtures are PUBLIC TEST DATA ONLY
— never use for real validator keys.

- `test-w7.log` — `cargo test -p aura-genesis --test w7_ceremony`:
  10 scenarios plus `vectors_file_has_exactly_10_unique_cases`,
  byte-for-byte against the independent oracle.
- `nc.log` — `tools/nc_w7.py` mutation controls (6): each source mutation must
  turn its named test red; compilation failure is not a valid red.
- `inventory.txt` — the 10 cases with exit codes and NC refs, the named
  control, the cumulative NC total (668) and SHA-256 pins of fixture,
  generator, runner.
- `report.md`, `gates.txt` — closeout report and gate summary.
- `SHA256SUMS` — digests of the deliverables and evidence files.

The vectors are produced by `tools/gen_w7_vectors.py` (pure Python: BLAKE3,
Ed25519 RFC 8032, Shamir GF(2^8), gen_c8 genesis primitives). Regenerate with
`make gen-w7-vectors`; rebuild this bundle with `make evidence-w7`.
