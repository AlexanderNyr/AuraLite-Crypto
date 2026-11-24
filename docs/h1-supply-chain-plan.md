# H1 — Supply chain (plan and design decisions)

Phase H1 of `docs/plans/SECURITY_PLAN.md`: "Supply chain — `cargo deny`,
`cargo audit`, pinning, SBOM (CycloneDX), gitleaks, release signatures;
CI fails on a known vulnerable version; NC: remove deny → red".

Status: in progress. Deviations from this plan are recorded here as
amendments (standard rule: deviations — only by editing the doc).

## Scope

1. **Policy** — `deny.toml`: one policy source consumed by the real
   `cargo-deny` (v0.20.2, pinned below) and by the deterministic gate
   `tools/check_supply_chain.py`.
2. **Pinning** — `rust-toolchain.toml` (channel 1.99.1 + rustfmt + clippy),
   committed `Cargo.lock`, no git dependencies without a pinned `rev`, and a
   pinned lockfile digest recorded by the oracle.
3. **Advisory gate (deny/audit)** — deterministic offline advisory snapshot
   `tools/fixtures/h1_advisories.json`; a lockfile that resolves a denied
   (crate, version) pair is refused with `error: supply-chain: denied
   RUSTSEC-… (name vX)`. The negative fixture
   `tools/fixtures/h1_vulnerable.lock` is the "CI fails on a known vulnerable
   version" demonstration: the gate exits non-zero on it.
4. **SBOM** — deterministic CycloneDX 1.5 JSON generated from `Cargo.lock`
   (`tools/gen_h1_sbom.py` → `sbom.cdx.json`), digest pinned by the oracle;
   SBOM drift fails the gate.
5. **Secrets (gitleaks class)** — deterministic scanner
   `tools/scan_secrets.py` with fixed rules (key blocks, tokens, password
   assignments, private key headers); planted-secret fixture
   `tools/fixtures/h1_secret_leak.txt` proves detection; the scanner also
   proves the clean repo contains no secrets.
6. **Release signatures** — Ed25519 (RFC 8032) detached signatures over
   release artifacts; `tools/sign_release.py` (sign/verify), pinned public key
   `docs/release-pubkey.txt`; tampered artifacts must fail verification.

## Design decisions

- **D-H1-1 — dual policy.** `deny.toml` is the normative policy; the real
  `cargo-deny`/`gitleaks` binaries are run as auxiliary evidence when
  available (pinned versions: cargo-deny 0.20.2, gitleaks 8.30.1, x86_64
  musl/linux builds from their upstream releases) but the phase gate is
  `tools/check_threat_model`-style: fully deterministic and offline. CI
  (=`make test`) must go red without any network.
- **D-H1-2 — advisory snapshot.** The offline deny verdicts come from
  `tools/fixtures/h1_advisories.json` (id, crate, patched ranges). It is a
  pinned snapshot, not a live database; regenerating requires a deliberate
  edit (`make gen-h1-vectors`). The RUSTSEC ids used in fixtures for tests
  are marked `TEST-` (RUSTSEC-TEST-0001 etc.) and never claimed to be real
  advisories; real-tool runs in evidence use the live rustsec advisory-db.
- **D-H1-3 — SBOM determinism.** `sbom.cdx.json` is generated purely from
  `Cargo.lock` (sorted, fixed serialNumber derived from content digest,
  CycloneDX 1.5 JSON). Any dependency change regenerates the SBOM; the gate
  compares the on-disk SBOM against the digest pinned by the oracle.
- **D-H1-4 — secrets rules.** Fixed regex rule set (PRIVATE KEY blocks,
  `AKIA…` keys, `gh[pousr]_…` tokens, `password = "…"` assignments,
  Bearer tokens). Findings are refused with `error: supply-chain: secret
  rule <id> matched <path>:<line>`. The rules themselves are pinned by the
  oracle vectors; a rules-mutation control proves the scan is load-bearing.
- **D-H1-5 — demo release key.** The signing key is derived deterministically
  from the pinned phrase `AuraLite-Crypto/h1-release-demo-key-v1` (SHA-256 →
  32-byte seed → Ed25519 RFC 8032). This is a DEMO/testnet release key chosen
  so the phase bundle is self-contained and reproducible across workspace
  restores; production release keys MUST be generated offline on air-gapped
  media (documented here as a hard requirement). The public key is pinned in
  `docs/release-pubkey.txt`; the gate verifies detached signatures
  (`*.sig`, base64) over `sha256(artifact)` and refuses tampered artifacts.
- **D-H1-6 — tool versions.** Auxiliary tool versions are recorded in
  `docs/evidence/h1/tool-versions.txt`; the deterministic gate never shells
  out to them (it re-implements the verdicts from the pinned fixtures).

## Gates and controls

- `make check-h1` runs `tools/check_supply_chain.py`: toolchain pin, lockfile
  pin + digest, advisory deny over `Cargo.lock`, SBOM digest, secrets scan of
  the tree, release-signature verification — plus the fixture self-test
  (vulnerable lock → denied, planted secret → detected, forged signature →
  refused). Failure output: `error: supply-chain: …` lines, exit 1.
- Negative controls (`tools/nc_h1.py`, 6): **NC-1 (named) remove deny → red**,
  NC-2 toolchain-pin check removed → red, NC-3 lockfile digest check removed
  → red, NC-4 SBOM comparison removed → red, NC-5 secret rule disabled → red,
  NC-6 signature verification skipped → red.
- Evidence `docs/evidence/h1/`: `report.md`, `gates.txt`, `check.log`,
  `nc.log`, `tool-versions.txt`, `inventory.txt` (cumulative TOTAL: 436),
  `SHA256SUMS`; regenerate with `make evidence-h1`.

## Post-closeout deviation (2026-11-14): patch-archive scan scope

- **Symptom.** `make test` on the closeout tree (the tree that includes
  `patches/0064-H1-*.patch`, created by `git format-patch` *after* the
  closeout test run) turned the supply-chain gate red: 7 findings
  (`private-key-block`, `github-token` x2, `aws-access-key` x2,
  `bearer-token` x2) at `patches/0064-*.patch`. The diff of the phase commit
  necessarily embeds the added lines of `tools/fixtures/h1_secret_leak.txt`
  and of the self-test PEM case.
- **Root cause.** Closeout sequencing: `make test` ran before `format-patch`,
  so the final tree state (with the patch archive) was never itself
  gate-green. The scanner's `SKIP_DIRS` had no `patches` entry.
- **Fix.** `tools/scan_secrets.py`: `SKIP_DIRS` now skips the immutable
  `patches/` review archive; the embedded fixture strings remain scanned at
  their canonical live paths (`tools/fixtures/h1_secret_leak.txt` is
  allow-listed as a known fixture, the rule-definition sources are
  allow-listed as such).
- **Controls.** New red/green guard `tools/test_patch_scope_guard.py`, wired
  into `make test`: patch-archive content -> no findings; the same content at
  a live path (`docs/`) -> `github-token` finding; dropping the scope rule
  (mutation) -> the guard turns red. `tools/check_supply_chain.py --selftest`
  stays 12/12 and `tools/nc_h1.py` stays 6/6.
- **Drift ledger.** Tool sources are pinned by historical evidence
  (`docs/evidence/h1/SHA256SUMS`); the touched pins are recorded in
  `docs/evidence/review-w1-source-drift.txt` per the historical checksum
  policy.
