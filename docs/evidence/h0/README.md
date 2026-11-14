# H0 evidence — threat model review gate (32 owned scenarios)

Gate (SECURITY_PLAN row H0): "scenario without an owner -> red".

- `check.log` — `tools/check_threat_model.py`: parses `docs/THREAT_MODEL.md`,
  enforces the schema (30+ scenarios, owners, references, mitigations) and
  compares against the pinned oracle output.
- `nc.log` — `tools/nc_h0.py` mutation controls (6): each document mutation
  must turn the gate red with its specific `error:` line.
- `inventory.txt` — the 32 scenarios with owner/class, the named
  control, the cumulative NC total (430) and SHA-256 pins of document,
  fixture, generator and gate tool.
- `report.md`, `gates.txt` — closeout report and gate summary.
- `SHA256SUMS` — digests of the deliverables and evidence files.

The vectors are produced by `tools/gen_h0_vectors.py` (regenerate with
`make gen-h0-vectors` after editing the threat model); rebuild this bundle
with `make evidence-h0`.
