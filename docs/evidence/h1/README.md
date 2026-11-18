# H1 evidence — supply-chain gate (12 verdicts)

Gate (SECURITY_PLAN row H1): "CI fails on a known vulnerable version; NC:
remove deny -> red".

- `check.log` — `tools/check_supply_chain.py`: the self-test (vulnerable
  lockfile -> denied, planted secret -> detected, forged signature ->
  refused) and the repository check (pins, deny, sbom, secrets, signatures).
- `nc.log` — `tools/nc_h1.py` mutation controls (6), named = remove deny.
- `signed-artifacts.txt` — Ed25519 demo-key signatures over repository
  artifacts (D-H1-5); verify with `tools/sign_release.py verify`.
- `tool-versions.txt` — auxiliary tool pins (cargo-deny 0.20.2, gitleaks
  8.30.1); the gate itself never shells out to them (D-H1-6).
- `inventory.txt` — verdicts, digests and the cumulative NC total (436).
- `report.md`, `gates.txt`, `SHA256SUMS`.

Regenerate: `make gen-h1-vectors`, `make gen-h1-sbom`, `make evidence-h1`.
