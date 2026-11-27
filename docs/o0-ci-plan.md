# O0 — CI baseline (the shard registry and the target registry check)

Date: 2026-11-25. Phase O0 of `docs/plans/OPS_PLAN.md`: "CI baseline — GitHub
Actions, 10 shards, cache, `make test`, coverage report"; gate: **NC: disable
a shard -> CI red (target registry check)**. Patch `0103-O0-ci-baseline`.
D-O1 governs: *everything CI runs can be run locally with one command
(`make test`); no "secret" shards.*

## Problem

"CI" that only exists as a YAML file nobody can run locally is theater: the
shards drift from the suite, a disabled shard silently drops tests, and the
coverage number is whatever the badge generator felt like printing. O0 makes
the CI layout a **checked registry**: ten shards over the workspace's test
targets, validated by a tool that `make test` runs, with the GitHub Actions
workflow reduced to calling the same make targets anyone can run here.

## Decisions

- **D-O0-1 (the shard registry).** `ci/registry.txt` is plain text
  (stdlib-parsable, no serde): `rule <name>`, `shard <lane>
  <enabled|disabled>`, `main <lane> <pkg>/<name>`, `target <pkg>/<name>
  shard <lane>`. A target is either `<pkg>/--lib` (unit tests) or
  `<pkg>/<test-file-stem>` (integration test). The ten lanes are the shards
  of `CONTRIBUTING.md` — core, state, exec, consensus, net, node, wallet,
  ecosystem, security, perf — and every lane pins its **main test**: a shard
  that does not run its main test is a dead gate (the MATURITY_AUDIT
  lesson: 27 unregistered cases). The registry is the single source of
  truth for the CI layout; `make ci-sync` (the `aura-ci sync` command)
  recomputes assignments by the documented lane table (phase prefix of the
  test stem: `c`/`o`→core, `s`/`t`→state, `x`→exec, `b`→consensus,
  `n`→net, `d`→node, `w`→wallet, `e`→ecosystem, `h`→security, `p`→perf;
  unit tests by their package's primary lane), preserving disabled flags
  and the canonical main pins.
- **D-O0-2 (the target registry check).** `aura-ci check` validates the
  registry against the **discovered** test targets of the workspace
  (every `crates/*/src/lib.rs` and `crates/*/tests/*.rs`) and against the
  workflow matrix: the canonical ten-lane set, no empty shard, every
  lane's main test registered to that lane (dead gate → `shard <lane>
  main dead: …`), no unknown/stale targets, no duplicates, no missing
  targets, no disabled shard holding targets, no foreign lines, and every
  enabled lane present in the `shard:` matrix of the workflow. Any
  violation is `error: ci: …`, exit 65. This is the O0 gate: **disable a
  shard -> CI red** (named control `disabled_shard_refused`).
- **D-O0-3 (no secret shards, D-O1).** The registry format cannot carry
  commands at all — a `command …` line is an `unknown registry line`
  refusal. `aura-ci run --shard <lane>` emits plain `cargo test -p <pkg>
  [--lib|--test <name>]` commands only, and `--exec` runs exactly those;
  the ten shards union to `cargo test --workspace`, which `make test`
  already runs. Every workflow step is a make target.
- **D-O0-4 (coverage report).** `aura-ci coverage` prints a deterministic
  **target-coverage** report (targets per shard, covered/total, percent) and
  refuses on an invalid registry. Line coverage (`cargo llvm-cov`) is wired
  as an optional CI job only where the toolchain provides it; the pinned,
  gated artifact is the target-coverage report — honest about what it
  measures.
- **D-O0-5 (two implementations).** `tools/gen_o0_vectors.py` computes the
  expected verdicts for ten synthetic registries independently (pure
  Python); `crates/aura-testkit/src/ci.rs` implements the shipped checker
  and must match the fixture byte-for-byte
  (`tests/o0_ci.rs` + `tests/fixtures/o0_ci_vectors.json`).

## Test scenarios (10, fixture `o0_ci_vectors.json`)

| # | name | scenario | exit |
|---|---|---|---|
| 1 | registry_valid_passes | 20 synthetic targets, the ten lanes, all covered (check + coverage report) | 0 `registry=ok shards=10 targets=20` |
| 2 | run_emits_make_test_commands | `run --shard core` prints plain `cargo test` lines (D-O1) | 0 |
| 3 | **disabled_shard_refused (NAMED)** | one lane `disabled` with targets -> `shard security disabled` | 65 |
| 4 | missing_target_refused | a discovered target not registered -> `unregistered target: …` | 65 |
| 5 | duplicate_target_refused | one target in two shards -> `target in two shards: …` | 65 |
| 6 | shard_set_mismatch_refused | a lane renamed (`ops`) -> `missing shard: core` | 65 |
| 7 | empty_shard_refused | lane with no targets -> `shard net empty` | 65 |
| 8 | shard_main_dead_refused | a lane's main test registered elsewhere -> `shard core main dead: …` | 65 |
| 9 | foreign_registry_line_refused | `command curl …` line -> `unknown registry line: …` | 65 |
| 10 | workflow_matrix_missing_shard_refused | workflow matrix omits a lane -> `workflow matrix missing shard: wallet` | 65 |

## Negative controls (`tools/nc_o0.py`, 6)

| NC | mutation in `ci.rs` | must turn red |
|---|---|---|
| 1 | disabled-shard check dropped (`if *disabled && shard_load(...) > 0` -> `if false`) | `disabled_shard_refused` (**named**) |
| 2 | missing-target check dropped | `missing_target_refused` |
| 3 | duplicate-target check dropped | `duplicate_target_refused` |
| 4 | shard-set check dropped | `shard_set_mismatch_refused` |
| 5 | empty-shard check dropped | `empty_shard_refused` |
| 6 | unknown-line strictness dropped | `foreign_registry_line_refused` |

## CI layout (`.github/workflows/ci.yml`)

- `registry` job: `make ci-check` (the target registry check, also
  verifying the workflow matrix).
- `test` job: matrix `shard: [core, state, exec, consensus, net, node,
  wallet, ecosystem, security, perf]` (the lanes of CONTRIBUTING.md),
  `make ci-shard SHARD=<lane>`; cargo cache keyed on `Cargo.lock`
  (`actions/cache` over `~/.cargo` + `target`).
- `coverage` job: `make coverage` (target-coverage report, archived as an
  artifact) plus `cargo llvm-cov` when the toolchain provides it.

Every step is a make target: `make test` remains the one local command
(D-O1). The historical jobs (`core`, `s2-native`, `exec-wasmtime`) keep
their H7 least-privilege baseline untouched.
