# W4 closeout report — wallet-level M-of-N multisig contract

Date: 2026-11-11. Gate (WALLET_PLAN row W4): "3-of-5: 3 collected -> success,
2 -> reject; NC: threshold bypass -> red" — **satisfied**.

- **Threshold gate**: `collect_3_of_5_passed` (3 approvals of 5 -> exit 0,
  `signatures=3/threshold=3`) and the named control `collect_2_of_5_rejected`
  (2 approvals -> `error: multisig: threshold not met (2/3)`, exit 65).
  Mutation NC-1 (`tools/nc_w4.py`) bypasses the threshold check and requires
  that test to go red.
- **Vectors**: 22 deterministic cases from the independent oracle
  `tools/gen_w4_vectors.py` (canonical descriptor/proposal/approval bytes,
  BLAKE3, Ed25519 RFC 8032-checked, `contract` domain), asserted
  byte-for-byte by `tests/w4_multisig.rs` (23/23 with the count guard).
  Groups: init (4), propose (5), approve (4), collect (7), rotation/e2e (2).
- **Mutation controls**: 6/6 PASS (threshold bypassed, signature verification
  skipped, duplicate approvals accepted, membership dropped, expiry dropped,
  proposal binding dropped).
- **Contract model** (docs/w4-multisig-plan.md): D-W4-1 canonical documents
  (contract id = BLAKE3 of the descriptor bytes), D-W4-2 the fixed AuraLite
  `contract` signature domain, D-W4-3 member rotation as a `rotate` proposal
  under the old membership, D-W4-4 deterministic `--height` timeouts with the
  fixed refusal set.
- **Cross-check pin**: the 3-of-5 contract over the W1 smoke keys
  (accounts 0..4, timeout 1000) has id
  `a3ed446e112c368e49e44553e7fbc288a6e8fc3c75bc35b508d08ab1e2ff9eb6` in both
  the Rust implementation and the Python oracle.

Receipts: `gates.txt`, `test-w4.log`, `nc.log`, `inventory.txt`, `SHA256SUMS`.
Regenerate with `make evidence-w4`. Public test data only — never use for funds.
