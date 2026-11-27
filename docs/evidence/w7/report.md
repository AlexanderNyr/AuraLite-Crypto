# W7 closeout report — offline genesis ceremony (the O5 gate)

Date: 2026-11-25. Gate (WALLET_PLAN row W7): "genesis ceremony never writes
one private key outside shredded media; NC: no_private_key_in_outputs" — **satisfied**.

- **The gate**: `no_private_key_in_outputs` (the named control) runs the full ceremony
  (open, generate, split, shred) and then `verify --root .`: the scanner
  flags any `media/` file or `secret=` line and must report `scan=clean`;
  mutation NC-1 (`tools/nc_w7.py`) skips shredding and the case must turn
  red. "Not one private key in the repository/CI" — the O5 DoD.
- **Scenarios**: 10 deterministic cases from the independent oracle
  `tools/gen_w7_vectors.py` (BLAKE3 via gen_c8, Ed25519 RFC 8032 via gen_c4,
  gen_c8 genesis canonical bytes/hash/validator_set_hash), asserted
  byte-for-byte by `tests/w7_ceremony.rs` (11/11 with the count guard).
  Groups: sealing trajectory (1), pubkey-set binding (1), private-material
  scan (1), Shamir sharing (3), witness chain (2), ceremony ordering (2).
- **Mutation controls**: 6/6 PASS (shredding skipped, threshold check
  dropped, witness chain check dropped, pubkey-set binding dropped,
  reconstruction corrupted, media-present check dropped).
- **Device model** (docs/w7-genesis-ceremony-plan.md): D-W7-1 deterministic
  offline key derivation (BLAKE3(KEY_CONTEXT || entropy || i), Ed25519),
  D-W7-2 Shamir GF(2^8) sharing with deterministic coefficients and
  Lagrange-at-0 reconstruction, D-W7-3 the hash-chained witness log
  (`witness chain broken` / `witness log incomplete`), D-W7-4 destruction of
  temporary media (double overwrite + remove; seal refuses while media
  exists), D-W7-5 the sealed genesis commits to exactly the witnessed pubkey
  set (`validator set mismatch`).
- **Reproducibility**: the whole ceremony runs from two inputs only
  (`params`, `entropy`); the fixture pins the full trajectory, the genesis
  hash `15d321f9…` and validator-set hash `70f63791…` are BLAKE3
  commitments computed independently in Python and Rust.

Receipts: `gates.txt`, `test-w7.log`, `nc.log`, `inventory.txt`,
`SHA256SUMS`. Regenerate with `make evidence-w7`. Public test data only —
never use for real validator keys.
