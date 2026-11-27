# O0 closeout report — CI baseline (the shard registry)

Date: 2026-11-25. Gate (OPS_PLAN row O0): "disable a shard -> CI red (target
registry check)" — **satisfied**.

- **The gate**: `disabled_shard_refused` (the named control) feeds the checker a registry
  with `shard security disabled` while the lane still holds targets and
  expects `error: ci: shard security disabled` (exit 65); mutation NC-1
  (`tools/nc_o0.py`) drops the enabled-check and the case must turn red.
- **Scenarios**: 10 deterministic cases from the independent oracle
  `tools/gen_o0_vectors.py`, asserted byte-for-byte by `tests/o0_ci.rs`
  (11/11 with the count guard) against `aura_testkit::ci`. Groups: valid
  registry (1), D-O1 command surface (1), refusals (7), coverage report (1).
- **Mutation controls**: 6/6 PASS (disabled-shard check, missing-target
  check, duplicate-target check, shard-set check, empty-shard check,
  unknown-line strictness).
- **Device model** (docs/o0-ci-plan.md): D-O0-1 the plain-text shard
  registry `ci/registry.txt` (the ten lanes of CONTRIBUTING.md over `--lib`
  + `tests/*` targets, each lane pinning its main test; `make ci-sync`
  re-registers); D-O0-2 the target registry check (`aura-ci check`, wired
  into `make test`, incl. the workflow matrix); D-O0-3 no secret shards —
  the format cannot carry commands and `aura-ci run --shard <lane>` emits
  plain `cargo test` forms only; D-O0-4 the deterministic target-coverage
  report (`aura-ci coverage`); D-O0-5 two independent implementations
  (Python oracle vs the shipped Rust checker).
- **CI layout** (`.github/workflows/ci.yml`): `registry` job (`make
  ci-check`), `test` matrix over the ten lanes (`make ci-shard SHARD=<lane>`), a
  `coverage` job (`make coverage`), cargo cache keyed on `Cargo.lock`. Every
  step is a make target — `make test` remains the one local command (D-O1).

Receipts: `gates.txt`, `test-o0.log`, `nc.log`, `registry-check.log`,
`coverage.log`, `inventory.txt`, `SHA256SUMS`. Regenerate with
`make evidence-o0`.
