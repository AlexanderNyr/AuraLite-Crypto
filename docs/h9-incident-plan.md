# H9 — Incident drills (phase plan)

**Phase:** H9 · **Patch:** `patches/0073-H9-incident.patch` · **Status:** 🚧 IN PROGRESS
**Spec (SECURITY_PLAN):** playbook: halt/pause/upgrade/rollback,
communication, post-mortem, drills | **drill "Byzantine validator +
bridge bug": network halted/recovered within SLO**.

## 1. Deliverables

1. `docs/incident/PLAYBOOK.md` — halt/pause/upgrade/rollback procedures,
   detection, recovery, communication plan, post-mortem template;
   normative SLOs (halt ≤ 15 min, recover ≤ 240 min, post-mortem ≤ 480 min).
2. `tools/fixtures/h9_drills.json` — three deterministic drill timelines
   (simulated minutes), each tied to H0 threat-model scenarios.
3. `tools/h9_incident_drill.py` — deterministic drill runner (playbook
   coverage, incident state-machine order, SLO bounds), wired into
   `make test`.
4. `tools/nc_h9.py` + `tools/write_h9_evidence.py` + `docs/evidence/h9/`.

## 2. Gates (deterministic oracles)

| Gate | Oracle | Red condition |
|---|---|---|
| **drill "Byzantine validator + bridge bug": halted/recovered within SLO** | D1 timeline vs SLO | halt missing/late or recover missing/late |
| steps come from the playbook | action ⊆ playbook anchors | unknown action |
| playbook covers halt/pause/upgrade/rollback | anchors | anchor missing |
| playbook covers communication + post-mortem | anchors | anchor missing |
| drills cover their H0 scenarios | H0 registry | unknown id |
| every drill ends in a verified recovery | steps | recover missing |
| post-mortem within SLO | steps vs SLO | missing/late |
| state-machine order | step order | illegal order |

## 3. Modelled surfaces and deviation record

- **Deviation (2026-11-18, method):** no live multi-validator testnet is
  running in this environment; "halt/recover" is exercised as a
  deterministic scheduled-timeline drill validated against the playbook
  and the H0 scenario registry by `tools/h9_incident_drill.py`
  (reproducible via `make drill-h9`). The named gate is checked
  literally over the D1 timeline: the network state machine walks
  detect → halt → communicate → recover → post-mortem with SLO bounds.
- **Deviation (2026-11-18, scope):** the bridge half of the named drill
  is E9 (not implemented; see `H8-F-007`); the drill keeps the combined
  scenario the spec names so the bridge cannot leave the incident scope.

## 4. Fixes shipped by this phase

- The missing incident capability itself: playbook + communication +
  post-mortem + scheduled drills (H9-F-001: no playbook existed).

## 5. Negative controls (docs/evidence/h9/nc.log)

| # | Injected bug | Required red |
|---|---|---|
| NC-1 | D1 halt step removed | `H9: drill Byzantine validator + bridge bug must halt/recover within SLO` |
| NC-2 | D1 recover step removed | `H9: drill Byzantine validator + bridge bug must halt/recover within SLO` |
| NC-3 | rollback anchor removed from the playbook | `H9: playbook must cover halt/pause/upgrade/rollback` |
| NC-4 | communication anchor removed from the playbook | `H9: playbook must cover communication and post-mortem` |
| NC-5 | D2 H0 ref swapped to a nonexistent id | `H9: drills must cover the H0 scenarios they claim` |
| NC-6 | D2 recover step removed | `H9: every drill must end in a verified recovery` |
| NC-7 | D1 post-mortem delayed past SLO | `H9: post-mortem must be produced within SLO` |

## 6. H9 receipt (2026-11-18)

- Deliverable: `patches/0073-H9-incident.patch`; evidence in
  `docs/evidence/h9/` (`campaign/` self-contained: drill logs,
  `result.json` with commands and source digests).
- Gate met literally: drill **"Byzantine validator + bridge bug"**
  halts the network at t+5 min (SLO ≤ 15) and recovers it at t+180 min
  (SLO ≤ 240); NC-1/NC-2 break each half and the runner turns red with
  the named message both times.
- Playbook covers halt/pause/upgrade/rollback + communication +
  post-mortem; three drills tied to H0 scenarios (D1 → H0-S10/S11,
  D2 → H0-S02/S13, D3 → H0-S15/S18).
- Negative controls: `tools/nc_h9.py`, 7/7 in `docs/evidence/h9/nc.log`;
  cumulative negative control total 488.
