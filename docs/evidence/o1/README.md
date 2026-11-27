# O1 evidence — Releases (10 scenarios)

Gate (OPS_PLAN row O1): "two independent hosts -> identical binary sha256".
Named control: repro_mismatch_refused. Two independent implementations (Python hashlib vs
the std-only SHA-256 of `aura_testkit::release`); signatures are structurally
checked here and verified by the wired `tools/sign_release.py verify`
(H1 demo key).

- `test.log`: `cargo test -p aura-testkit --test o1_release` (11 tests).
- `nc.log`: `tools/nc_o1.py` mutation controls (6/6).
- `sbom.log`: the deterministic SBOM regeneration.
- `repro-check.log`: two independent builds of `aurad` with identical sha256.
- `release-demo.log` / `release-check.log`: signed release assembly and the
  release check (incl. the RFC 8032 verify step).
- `inventory.txt`: case -> NC mapping and sha256 of fixture/generator/runner;
  ends with `TOTAL: 680`.
- `gates.txt` / `report.md` / `SHA256SUMS`: the receipts.

Regenerate with `make evidence-o1`; every sha256 in `SHA256SUMS` can be
recomputed with `sha256sum -c`.
