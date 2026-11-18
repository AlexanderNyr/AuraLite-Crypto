# H2 evidence — crypto review (50 vectors)

Gate (SECURITY_PLAN row H2): "50 crypto vectors + side-channel checklist;
NC: shared domain -> signature passes, red".

- `check.log` — oracle drift check, `tools/check_h2_review.py` and the h2
  vector suite (`crates/aura-crypto/tests/h2_crypto.rs`).
- `nc.log` — `tools/nc_h2.py` mutation controls (6), named = shared domain.
- `inventory.txt` — vector kinds, checklist statuses and the cumulative NC
  total (442).
- `report.md`, `gates.txt`, `SHA256SUMS`.

Regenerate: `make gen-h2-vectors`, `make evidence-h2`.
