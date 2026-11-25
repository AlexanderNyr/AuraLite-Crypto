# W5 closeout report — hardware keys with on-screen confirmation

Date: 2026-11-24. Gate (WALLET_PLAN row W5): "10 "human confirmed/denied"
scenarios; NC: blind confirmation -> red" — **satisfied**.

- **Confirmation binding**: `blind_confirmation_rejected` (the named control)
  — a `BlindSign` request is refused (`error: hardware: blind confirmation
  rejected`, exit 65) before any key use; mutation NC-1 (`tools/nc_w5.py`)
  makes the device sign blind requests and requires that test to go red.
- **Scenarios**: 10 deterministic cases from the independent oracle
  `tools/gen_w5_vectors.py` (display derivation, transfer/delegate payloads,
  Ed25519 RFC 8032-checked signatures under the fixed `tx` domain), asserted
  byte-for-byte by `tests/w5_hardware.rs` (11/11 with the count guard).
  Groups: confirm (3), deny (2), blind (1), display substitution (1), retry
  after denial (1), undecodable payload (1), foreign chain (1).
- **Mutation controls**: 6/6 PASS (blind acceptance, denial ignored, display
  binding dropped, chain check dropped, amount render altered, payload decode
  check dropped).
- **Device model** (docs/w5-hardware-plan.md): D-W5-1 the screen is derived
  exclusively from the transaction bytes, D-W5-2 only payloads that can be
  fully displayed are signed (`recipient||amount` for transfer, canonical
  stake payload for delegate/undelegate) and host previews are bound to the
  derived display, D-W5-3 nothing signs without `Armed -> Confirm` and a
  denial never latches, D-W5-4 signatures are ordinary AuraLite `tx` domain
  signatures (verifiable by the W3 offline path).

Receipts: `gates.txt`, `test-w5.log`, `nc.log`, `inventory.txt`, `SHA256SUMS`.
Regenerate with `make evidence-w5`. Public test data only — never use for funds.
