# O1 — Releases: reproducible build, signed artifacts, SBOM (design)

**Gate (OPS_PLAN):** two independent hosts → identical binary sha256.
**DoD:** reproducible build, Docker multi-stage, signatures, SBOM,
CHANGELOG, semver. **Named scenario: `repro_mismatch_refused`.**

The release process is a checked pipeline, not folklore: `aura-release`
validates a release directory (manifest, digests, signatures, SBOM, the
two-host reproducibility record, Dockerfile policy) against the workspace,
and every claim has a test and a negative control. The crypto verify of
signatures is the existing H1 tool (`tools/sign_release.py`, demo key);
the structural checks below are the fixture surface.

## Decisions

- **D-O1-1 (reproducible release build).** `make repro-check`
  (`tools/repro_check.py`) builds the release binary (`aurad`) twice into
  two independent `CARGO_TARGET_DIR`s — the "two hosts" of the gate — with
  `SOURCE_DATE_EPOCH` pinned to the commit date and
  `RUSTFLAGS=--remap-path-prefix`, and refuses unless the two sha256
  digests are identical. The digests are recorded in `repro.txt` (the
  release record: `repro v1` / `binary <name>` / `toolchain` /
  `source_date_epoch` / `host <id> sha256 <hex>` ×2); a release whose
  record shows differing host digests is `error: release: repro mismatch:
  <binary>` (exit 65) — the named control `repro_mismatch_refused`.
- **D-O1-2 (the release manifest is a checked registry).**
  `release/manifest.txt` (strict text, stdlib-parsable): `rule release-v1`,
  `version <semver>`, `keyid <hex-64>`, `artifact <name>`, `sig
  <artifact> <sigfile>`. `aura-release check` validates, in order: strict
  parse (`unknown manifest line`), semver + workspace version match
  (`version drift`), CHANGELOG entry `## [<version>` (`changelog entry
  missing`), artifact presence (`artifact missing`), SHA256SUMS digests
  (`checksum mismatch`/`checksum stale`), signature structure
  (`signature missing`/`signature malformed` — exactly 64 decoded bytes —
  / `key mismatch`), SBOM regeneration (`sbom drift`), the repro record
  (`repro mismatch`), and the Dockerfile policy (`docker single stage` /
  `docker base unpinned`). `aura-release sync` regenerates
  `manifest.txt`/`SHA256SUMS`/`SBOM.txt` from the release directory.
- **D-O1-3 (signatures: structural check + wired RFC 8032 verify).**
  The fixture surface checks the binding layer: every artifact carries a
  detached `.sig` (base64, exactly 64 bytes = ed25519) and the manifest
  `keyid` equals the pinned `docs/release-pubkey.txt`. The real
  verification (`tools/sign_release.py verify`, H1 demo key — production
  keys are generated offline on air-gapped media, D-H1-5) runs as a wired
  make step inside `make release-check`. Never claim structural checks are
  cryptographic verification.
- **D-O1-4 (SBOM + Docker + CHANGELOG + semver).** `make sbom` writes a
  deterministic `release/SBOM.txt` (`sbom v1` + `pkg name=… version=…
  license=… deps=…`) from the workspace manifests (name, workspace
  version/license, dependency keys). `docker/Dockerfile` is multi-stage
  with digest-pinned bases (refused: `docker base unpinned`). The workspace
  version comes from `[workspace.package]`; every crate must stay on
  `version.workspace = true` (semver single-source). CHANGELOG must carry
  the release entry `## [<version>`.
- **D-O1-5 (two implementations).** `tools/gen_o1_vectors.py` computes the
  expected verdicts and the SHA-256 digests independently (Python
  `hashlib`); `crates/aura-testkit/src/release.rs` embeds a std-only
  SHA-256 and must match the fixture byte-for-byte
  (`tests/o1_release.rs` + `tests/fixtures/o1_release_vectors.json`).

## Test scenarios (10, fixture `o1_release_vectors.json`)

| # | name | scenario | exit |
|---|---|---|---|
| 1 | release_valid_passes | the whole valid release checks out (check + regenerated SHA256SUMS) | 0 `release=ok version=0.1.0 artifacts=2` |
| 2 | sbom_regeneration_deterministic | `sbom` prints the pinned SBOM (byte-for-byte) | 0 |
| 3 | **repro_mismatch_refused (NAMED)** | two host digests differ -> `repro mismatch: aura-bin` | 65 |
| 4 | version_drift_refused | manifest version != workspace version -> `version drift: …` | 65 |
| 5 | changelog_missing_refused | no `## [<version>` entry -> `changelog entry missing: …` | 65 |
| 6 | sbom_drift_refused | SBOM edited -> `sbom drift` | 65 |
| 7 | checksum_mismatch_refused | SHA256SUMS hex wrong -> `checksum mismatch: …` | 65 |
| 8 | signature_malformed_refused | `.sig` not 64 bytes -> `signature malformed: …` | 65 |
| 9 | docker_base_unpinned_refused | `FROM node:latest` -> `docker base unpinned: …` | 65 |
| 10 | foreign_manifest_line_refused | `exec curl …` line -> `unknown manifest line: …` | 65 |

## Negative controls (`tools/nc_o1.py`, 6)

| NC | mutation in `release.rs` | must turn red |
|---|---|---|
| 1 | repro-equality check dropped | `repro_mismatch_refused` (**named**) |
| 2 | version check dropped | `version_drift_refused` |
| 3 | changelog check dropped | `changelog_missing_refused` |
| 4 | checksum match check dropped | `checksum_mismatch_refused` |
| 5 | signature length check dropped | `signature_malformed_refused` |
| 6 | unknown-line strictness dropped | `foreign_manifest_line_refused` |

## Make targets / CI

- `make sbom` — regenerate `release/SBOM.txt` (deterministic).
- `make repro-check` — the named gate: two independent release builds of
  `aurad`, identical sha256 or red (`tools/repro_check.py`).
- `make release-sync` — regenerate `release/manifest.txt` + `SHA256SUMS`.
- `make release-check` — `aura-release check` + `tools/sign_release.py
  verify` over the release artifacts (the wired RFC 8032 step).
- `make {gen-o1-vectors,test-o1,nc-o1,evidence-o1}` — the phase loop.
- Workflow: the O0 `registry` job keeps running `make test` gates; the O1
  release checks run inside `make test` (manifest/sbom/docker policy) and
  `make repro-check` is the explicit release gate.

## Evidence (`docs/evidence/o1/`)

- `test.log`, `nc.log`, `inventory.log` (`TOTAL: 680`),
  `repro-check.log` (two digests + `repro=ok sha256=…`),
  `release-check.log` (`release=ok …` + signature verify),
  `report.md`, `gates.txt`, `SHA256SUMS`, `README.md`.
