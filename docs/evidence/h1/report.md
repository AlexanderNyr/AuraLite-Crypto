# H1 closeout report — supply chain

Date: 2026-11-14. Gate (SECURITY_PLAN row H1): "CI fails on a known
vulnerable version; NC: remove deny -> red" — **satisfied**.

- **Vulnerable-version gate**: `tools/check_supply_chain.py` refuses the
  negative fixture `tools/fixtures/h1_vulnerable.lock` with
  `error: supply-chain: denied RUSTSEC-TEST-0001 (demo-vulnerable v0.9.0)`
  (exit 1) — the deterministic, offline "CI fails" demonstration. The deny
  verdicts come from the pinned advisory snapshot
  (`tools/fixtures/h1_advisories.json`, D-H1-2).
- **Named control** (remove_deny): `tools/nc_h1.py` NC-1 weakens the advisory
  deny and requires the self-test case `vulnerable_lock_denied` to go red.
- **Pinning**: `rust-toolchain.toml` (1.99.1), committed `Cargo.lock` with a
  pinned digest and zero unpinned git sources.
- **SBOM**: deterministic CycloneDX 1.5 (`sbom.cdx.json`, D-H1-3) with a
  pinned digest; drift fails the gate.
- **Secrets**: gitleaks-class scanner (`tools/scan_secrets.py`, D-H1-4) with
  a planted-secret fixture proving detection and a clean-repo verdict.
- **Signatures**: Ed25519 demo-key detached signatures (D-H1-5) over
  repository artifacts (`signed-artifacts.txt`, pubkey pinned in
  `docs/release-pubkey.txt`); forged signatures are refused.
- **Vectors**: 12 self-test verdicts from the oracle
  `tools/gen_h1_vectors.py` (11/11 PASS); **mutation controls 6/6 PASS**
  (deny, toolchain pin, lockfile digest, sbom, secret rule, signature).
- **Auxiliary tools** (non-gate, versions pinned in `tool-versions.txt`):
  cargo-deny 0.20.2, gitleaks 8.30.1 (D-H1-6).

Receipts: `gates.txt`, `check.log`, `nc.log`, `tool-versions.txt`,
`signed-artifacts.txt`, `inventory.txt`, `SHA256SUMS`. Regenerate with
`make evidence-h1`. Public project material only.
