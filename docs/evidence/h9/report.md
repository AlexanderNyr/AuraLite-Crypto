# Отчёт H9 — инцидентные дриллы

Дата: 2026-11-18

Итог: **ВСЕ ГЕЙТЫ H9 ЗЕЛЁНЫЕ**

Гейт: drill "Byzantine validator + bridge bug": network halted/recovered within SLO.

Playbook `docs/incident/PLAYBOOK.md` (halt/pause/upgrade/rollback, communication, post-mortem); дриллы `tools/fixtures/h9_drills.json`, раннер `tools/h9_incident_drill.py` (`campaign/result.json`, `campaign/drill.log`).
negative controls `tools/nc_h9.py` (`nc.log`, RESULT: 7/7 negative controls PASS).

