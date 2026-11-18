# H8 — External audit + bounty (phase plan)

**Phase:** H8 · **Patch:** `patches/0072-H8-audit.patch` · **Status:** 🚧 IN PROGRESS
**Spec (SECURITY_PLAN):** audit of consensus/VM/bridge/wallet, public
report, fixes, bug bounty + disclosure | **0 unfixed critical/high**;
NC: **re-audit finds an old bug → red**.

## 1. Deliverables

1. `docs/audit/h8-public-audit-report.md` — public report over the four
   areas, findings table, methodology.
2. `SECURITY.md` — coordinated disclosure policy + bug bounty with a
   normative severity table.
3. `tools/fixtures/h8_known_bugs.json` — registry of old (Stage 1) bugs
   pinned by exact textual invariants.
4. `tools/fixtures/h8_findings.json` — machine-readable findings table.
5. `tools/h8_reaudit.py` — deterministic re-audit oracle, wired into
   `make test` (fail closed).
6. `tools/nc_h8.py` + `tools/write_h8_evidence.py` + `docs/evidence/h8/`.

## 2. Gates (deterministic oracles)

| Gate | Oracle | Red condition |
|---|---|---|
| 0 unfixed critical/high | `h8_findings.json` scan | any critical/high not `fixed` |
| re-audit finds an old bug → red | `h8_known_bugs.json` scan | any invariant broken in the tree |
| disclosure complete | SECURITY.md anchors | anchor missing |
| bounty pins severity table | SECURITY.md anchors | anchor missing |
| report covers all areas | report area markers | marker missing |
| fixes cite patches | findings `fix_ref` | `fixed` without `fix_ref` |

## 3. Modelled surfaces and deviation record

- **Deviation (2026-11-18, method):** no external audit firm or bounty
  platform exists in this environment. The "external" review is run as an
  independent scripted re-review pass (registry oracle + pattern
  inventory + targeted invariant reading) whose output is fully
  reproducible by `make reaudit-h8`; the report says exactly that.
  Payouts of the published bounty table run through the owner's channel.
- **Deviation (2026-11-18, scope):** the bridge is E9 (Stage 2) and is
  not implemented in this tree; its audit slot is kept open in both
  registries (`H8-F-007`) so the area cannot silently leave the scope.

## 4. Fixes shipped by this phase

- `SECURITY.md` (H8-F-008): the missing disclosure policy / bounty
  program itself, fixed by publishing both.

## 5. Negative controls (docs/evidence/h8/nc.log)

| # | Injected bug | Required red |
|---|---|---|
| NC-1 | keystore loader mode check widened (`& 0o077` → `& 0o000`) | `H8: re-audit finds an old bug` |
| NC-2 | reward accounting overflow reintroduced (`saturating_add` → `wrapping_add`) | `H8: re-audit finds an old bug` |
| NC-3 | a high finding reopened in the findings registry | `H8: 0 unfixed critical/high` |
| NC-4 | known-bug registry emptied | `H8: known-bug registry must pin every old bug` |
| NC-5 | disclosure anchor removed from SECURITY.md | `H8: disclosure policy must stay complete` |
| NC-6 | bounty anchor removed from SECURITY.md | `H8: bounty must pin the severity table` |
| NC-7 | wallet area marker removed from the report | `H8: audit report must cover consensus/VM/bridge/wallet` |

## 6. H8 receipt (2026-11-18)

- Deliverable: `patches/0072-H8-audit.patch`; evidence in
  `docs/evidence/h8/` (`campaign/` self-contained: re-audit logs,
  `result.json` with commands and source digests).
- Gate met literally: **0 unfixed critical/high** in the findings
  registry, and **re-audit finds an old bug → red** (NC-1/NC-2 inject the
  two named old bugs — wallet and consensus — and the oracle turns red
  with the named message each time).
- Public report (`docs/audit/h8-public-audit-report.md`) covers
  consensus/VM/bridge/wallet; disclosure policy and bug bounty published
  in `SECURITY.md`.
- Negative controls: `tools/nc_h8.py`, 7/7 in `docs/evidence/h8/nc.log`;
  cumulative negative control total 481.
