# H7 — OpSec / reproducible builds plan (Stage-1 closeout)

Status: 🚧 IN PROGRESS — H7
Owner: core maintainer
Date: 2026-11-17
Gate: `NC: secret in git → CI red; two hosts build to the same sha256`

## 1. Scope and DoD

SECURITY_PLAN row H7 covers validator keys (HSM/offline), CI isolation,
permissions, access log and reproducible builds. The gate names the two
controls that must bite: a secret committed to the repository must turn
the CI red, and two hosts must build the same artifact to the same
sha256. H7 closes Stage 1 (H0–H7).

Deliverables:

- **D-H7-1 — two-host reproducible build.** `tools/h7_repro_build.py`
  snapshots the tree to two simulated hosts (different absolute paths,
  separate target directories), builds the pinned artifact
  (`aura-types --release --lib`) with `--remap-path-prefix=<host>=/auralite-build`
  and requires byte-identical sha256 (THREAT_MODEL H0-S18). Toolchain
  pinned via `rust-toolchain.toml` (1.99.1).
- **D-H7-2 — secret in git turns CI red.** `tools/check_opsec.py` runs
  `tools/scan_secrets.py` over the tree and fails closed on any finding;
  wired into `make test` (and therefore into every CI job).
- **D-H7-3 — CI isolation and least privilege.** Every workflow must
  declare a read-only `permissions:` block and pin the toolchain
  (`check_opsec` lint). The two shipped workflows carry the block.
- **D-H7-4 — validator keys owner-only.** Keystore files are created
  `0600` (dir `0700`, existing W1 behaviour) and — new in H7 — `Keystore::load`
  rejects a keystore with any group/other permission bits (a `0644`
  copy must not load).
- **D-H7-5 — access log without secrets.**
  `keystore::{access_log_line, append_access_log}`: deterministic records
  `keystore <op> file=<name> ok=<bool>` (never paths, passwords, seeds or
  key material), appended to an owner-only (`0600`) log file.
- **D-H7-6 — toolchain pin.** `rust-toolchain.toml` stays on the pinned
  channel; `check_opsec` verifies.

## 2. Tests

| test | property | NC |
| --- | --- | --- |
| `h7_keystore_owner_only` (aura-wallet) | save `0600`; loader rejects `0644`/`0666`, accepts `0600` | NC-4, NC-5 |
| `h7_access_log_without_secrets` (aura-wallet) | pinned record format, no secret fields, log `0600` | NC-7 |
| `tools/check_opsec.py` | secrets / CI lint / toolchain pin | NC-1, NC-3, NC-6 |
| `tools/h7_repro_build.py` | two hosts → same sha256 | NC-2 |

## 3. Modelled surfaces and deviation record

- **Deviation (2026-11-17, scope):** no HSM hardware exists in this
  environment; the HSM/offline half of the row is covered by the W3
  air-gapped signing flow and the W1 encrypted keystore, and H7 adds the
  filesystem half (owner-only files, permission-rejecting loader, access
  log). The "two hosts" are two independent source trees with disjoint
  paths and target directories — the same derivation two honest builders
  run; matching sha256 is the release-manager pin (H0-S18).
- The access log is a library facility (`append_access_log`); CLI wiring
  is deliberately out of scope here to keep the frozen W2 CLI vector
  surface unchanged.

## 4. Crash triage

A failing repro build or a red OpSec check is a regression: reproduce with
the recorded toolchain line and digests, fix with a regression control,
re-run.

## 5. Gates and controls

- `tools/nc_h7.py` injects bugs (restored after each control) and requires
  the named gates to go red:
  **NC-1 (named) injected bug -> secret in git must turn CI red** — a fake
  access key committed to the plan doc ⇒ `H7: secret in git must turn CI
  red`; **NC-2 (named)** the remap prefix made host-specific ⇒ `H7: two
  hosts must build to the same sha256`; NC-3 workflow `permissions:`
  renamed away ⇒ `H7: CI must pin permissions and toolchain`; NC-4 save
  mode widened to `0640` ⇒ `H7: validator keys must stay owner-only`;
  NC-5 loader permission check disabled ⇒ `H7: keystore loader must
  reject loose permissions`; NC-6 toolchain unpinned ⇒ `H7: toolchain
  must stay pinned`; NC-7 access-log record format leaked extra fields
  ⇒ `H7: key access must be logged without secrets`.
- Evidence `docs/evidence/h7/` via `make evidence-h7` (`gates.txt`,
  `nc.log` with `RESULT: 7/7 negative controls PASS`, `campaign/` bundle
  incl. `repro-result.json`, `report.md`, `inventory.txt` cumulative
  TOTAL: 474, `SHA256SUMS`).
<!-- H7 NC secret anchor -->
