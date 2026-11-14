# W3 closeout report — air-gapped offline signing

Date: 2026-11-11. Gate (WALLET_PLAN row W3): "air-gapped scenario end-to-end;
NC: signature with a foreign domain -> reject" — **satisfied**.

- **Air-gapped end-to-end**: `tx-prepare` (keyless, online) -> USB file or QR
  frames -> `tx-sign` (offline keystore) -> back -> `tx-verify` (online) and
  `broadcast` accepts the signed envelope directly. Vectors `e2e_usb`,
  `e2e_qr`, `e2e_broadcast` walk the full scenario byte-for-byte.
- **Named control**: `verify_foreign_domain` — a signature computed under a
  foreign (domain-less) digest is refused (`error: signature verification
  failed`, exit 65). Mutation NC-1 (`tools/nc_w3.py`) drops the domain-bound
  verification and requires this test to go red.
- **Vectors**: 24 deterministic cases from the independent oracle
  `tools/gen_w3_vectors.py` (AURALITE-OFFLINE-V1 envelope, QR framing,
  BLAKE3, Ed25519 RFC 8032-checked, C2/C4 encoders), asserted byte-for-byte
  by `tests/w3_offline.rs` (25/25 with the count guard).
- **Mutation controls**: 6/6 PASS (foreign domain accepted, envelope checksum
  skipped, signer binding dropped, QR gap accepted, QR duplicate accepted,
  `--against` comparison skipped).
- **Decisions** (docs/w3-offline-plan.md): D-W3-1 deterministic envelope with
  unkeyed BLAKE3 checksum (corruption detection only; the signature is the
  authentication), D-W3-2 QR frames `ALO1:ii/nn:chunk`, D-W3-3 fixed
  verification order with the W2 chain-binding message reused, D-W3-4 no new
  secrets (`tx-sign` reuses the W2 keystore flow).

Receipts: `gates.txt`, `test-w3.log`, `nc.log`, `inventory.txt`, `SHA256SUMS`.
Regenerate with `make evidence-w3`. Public test data only — never use for funds.
