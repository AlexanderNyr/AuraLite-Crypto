# O0 evidence — CI baseline (10 scenarios)

Gate (OPS_PLAN row O0): "disable a shard -> CI red (target registry check)".

- `test-o0.log` — `cargo test -p aura-testkit --test o0_ci`: 10
  scenarios plus `vectors_file_has_exactly_10_unique_cases`, byte-for-byte
  against the independent oracle.
- `nc.log` — `tools/nc_o0.py` mutation controls (6): each source mutation must
  turn its named test red; compilation failure is not a valid red.
- `registry-check.log`, `coverage.log` — `aura-ci check` / `aura-ci coverage`
  against the live `ci/registry.txt` and the discovered workspace targets.
- `inventory.txt` — the 10 cases with exit codes and NC refs, the named
  control, the cumulative NC total (674) and SHA-256 pins of fixture,
  generator, runner, registry.
- `report.md`, `gates.txt` — closeout report and gate summary.
- `SHA256SUMS` — digests of the deliverables and evidence files.

The vectors are produced by `tools/gen_o0_vectors.py` (pure Python). The
registry is refreshed with `make ci-sync`; rebuild this bundle with
`make evidence-o0`.
