# O1 closeout report — Releases (the two-host reproducibility gate)

Date: 2026-11-25. Gate (OPS_PLAN row O1): "two independent hosts ->
identical binary sha256" — **satisfied** (`tools/repro_check.py` builds
`aurad` twice into independent CARGO_TARGET_DIRs with SOURCE_DATE_EPOCH and
`--remap-path-prefix`; the recorded digests are identical, see
`repro-check.log`).

- **The gate**: `repro_mismatch_refused` (the named control) feeds the checker a
  `repro.txt` whose two host digests differ and expects `error: release:
  repro mismatch: aura-bin` (exit 65); mutation NC-1 (`tools/nc_o1.py`)
  drops the two-host equality check and the case must turn red.
- **Scenarios**: 10 deterministic cases from the independent oracle
  `tools/gen_o1_vectors.py` (Python hashlib sha256), asserted byte-for-byte
  by `tests/o1_release.rs` (11/11 with the count guard) against
  `aura_testkit::release` (std-only SHA-256).
- **Mutation controls**: 6/6 PASS (repro-equality check [named], version
  check, changelog check, checksum match check, signature length check,
  unknown-line strictness).
- **Device model** (docs/o1-releases-plan.md): D-O1-1 the reproducible
  release build and its two-host record; D-O1-2 the checked release
  manifest `release/manifest.txt` (strict parse, semver/CHANGELOG binding,
  SHA256SUMS digests, signature structure, SBOM regeneration, repro
  record, Dockerfile policy); D-O1-3 signature binding is structural —
  the RFC 8032 verification is the wired `tools/sign_release.py verify`
  step (H1 demo key; production keys are offline/air-gapped); D-O1-4 the
  deterministic SBOM, digest-pinned multi-stage `docker/Dockerfile`;
  D-O1-5 two independent implementations (Python hashlib vs the std-only
  SHA-256).
- **Correction note**: `docs/evidence/o0/` was regenerated with this
  bundle after fixing the named-control string check in
  `tools/write_o0_evidence.py` (it still matched the pre-redesign `shard
  07` text and reported a false `FAIL`); the O0 gate verdicts are
  unchanged (11/11 tests, 6/6 NC).

Receipts: `gates.txt`, `test.log`, `nc.log`, `sbom.log`,
`repro-check.log`, `release-demo.log`, `release-check.log`,
`inventory.txt`, `SHA256SUMS`. Regenerate with `make evidence-o1`.
