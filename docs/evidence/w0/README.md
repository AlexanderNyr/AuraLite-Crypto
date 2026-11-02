# W0 Evidence — Wallet Baseline & Key-Exposure Audit

## Artifacts
- `threat-notes.md` — 8 key-exposure surfaces ("where keys could end up") and 6 threat model notes (`T-W0-1`..`T-W0-6`).
- `gates.txt` — combined vector generation, unit + integration test output, and 6/6 negative controls.
- `report.md` — W0 phase close-out report.
- `inventory.txt` — cumulative negative-control inventory (`339` total through W0).
- `gate.log` — `cargo test -p aura-wallet` output.
- `nc.log` — `python3 tools/nc_w0.py` output (`6/6 PASS`).
- `vectors.log` — `python3 tools/gen_w0_vectors.py` output (`w0_baseline.txt` and `w0_key_audit.txt`).
- `SHA256SUMS` — SHA-256 digests of all W0 deliverables and evidence files.
