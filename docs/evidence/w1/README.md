# W1 Evidence — Crypto Keystore Phase

## Artifacts
- `report.md` — W1 phase close-out report (receipt companion).
- `gates.txt` — `make test-w1`, `make test-w1-interop` and `make nc-w1` outputs.
- `test-w1.log`, `test-w1-interop.log` — individual gate logs.
- `nc.log` — `make nc-w1` output (`52/52` wallet plus `15/15` fuzz-oracle
  mutation controls; both RESULT lines).
- `inventory.txt` — cumulative negative-control inventory (`406` total through W1).
- `SHA256SUMS` — SHA-256 digests of the W1 deliverables and evidence files.

The interoperability format is frozen as W1-FORMAT-v1 in
`docs/spec/keystore-format.md` (owner decision). Scope is testnet-only; the
independent external security review is deferred to the Stage-2 audit by owner
decision (2026-11-08) and is not claimed here. Fuzz campaigns are bounded
research observations, not statistical proof.
