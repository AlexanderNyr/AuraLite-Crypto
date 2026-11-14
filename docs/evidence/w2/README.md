# W2 evidence — `aura-wallet` CLI (40 CLI vectors)

Gate (WALLET_PLAN row W2): "40 CLI vectors, including rejection of a tx with a
foreign chain-id". All fixtures are PUBLIC TEST DATA ONLY — never use for funds.

- `test-w2.log` — `cargo test --features insecure-test-kdf --test w2_cli`:
  40 vectors plus `vectors_file_has_exactly_40_unique_cases` and
  `broadcast_replay_refused`, byte-for-byte against the independent oracle.
- `nc.log` — `tools/nc_w2.py` mutation controls (6): each source mutation must
  turn its named test red; compilation failure is not a valid red.
- `inventory.txt` — the 40 cases with exit codes, the named control, and the
  SHA-256 pin of the fixture, generator and runner.
- `gates.txt` — gate summary.
- `SHA256SUMS` — digests of the deliverables and evidence files.

The vectors are produced by `tools/gen_w2_vectors.py` (pure Python: BIP-39,
SLIP-0010, bech32, C4 wire encoder, BLAKE3, Ed25519; self-checked against
RFC 8032 and the W1 pinned identity). Regenerate with `make gen-w2-vectors`;
rebuild this bundle with `make evidence-w2`.
