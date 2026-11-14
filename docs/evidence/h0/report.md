# H0 closeout report — threat model with owned scenarios

Date: 2026-11-11. Gate (SECURITY_PLAN row H0): "scenario without an owner ->
red" over `docs/THREAT_MODEL.md` (assets, adversaries, trust boundaries, 30+
scenarios) — **satisfied**.

- **Review gate**: `tools/check_threat_model.py` parses the threat model and
  enforces: at least 30 scenarios, unique ids, the required fields (class,
  boundary, assets, entry, impact, owner, mitigations), known
  class/boundary/asset references, owners drawn from the seven roles, and
  non-empty mitigations. Result: `THREAT MODEL: OK — 32
  scenarios, all owned`.
- **Named control** (scenario_without_an_owner): `tools/nc_h0.py` NC-1 deletes one Owner field
  (H0-S07) and requires the gate to fail with
  `error: scenario H0-S07: missing owner`.
- **Vectors**: the deterministic oracle `tools/gen_h0_vectors.py` extracts the
  scenario inventory into `tools/fixtures/h0_threat_model_vectors.json`
  (AURALITE-THREAT-VECTORS-V1); the gate re-derives the records with its own
  parser and compares field-by-field (byte-for-byte property).
- **Mutation controls**: 6/6 PASS (owner dropped, duplicate id, unknown class,
  unknown boundary, empty mitigations, fewer than 30 scenarios).
- **Content**: 32 scenarios H0-S01..H0-S32 across the five
  adversary classes (external, validator, insider, supply chain, physical)
  and six trust boundaries, each mapped to shipped mitigations (W0–W4, B/N/D
  phases) and residual risk; unowned scenarios cannot exist by construction.

Receipts: `gates.txt`, `check.log`, `nc.log`, `inventory.txt`, `SHA256SUMS`.
Regenerate with `make evidence-h0`. Public project material only.
