# H9 Report — incident drills

Date: 2026-11-18

The result: **ALL H9 GATES GREEN**

Gate: drill "Byzantine validator + bridge bug": network halted/recovered within SLO.

Playbook `docs/incident/PLAYBOOK.md` (halt/pause/upgrade/rollback, communication, post-mortem); the drills `tools/fixtures/h9_drills.json`, the runner `tools/h9_incident_drill.py` (`campaign/result.json`, `campaign/drill.log`).
negative controls `tools/nc_h9.py` (`nc.log`, RESULT: 7/7 negative controls PASS).

