# Отчёт H7 — OpSec / воспроизводимые сборки

Дата: 2026-11-17

Итог: **ВСЕ ГЕЙТЫ H7 ЗЕЛЁНЫЕ**

Гейт: NC: secret in git -> CI red; two hosts build to the same sha256.

Кампания `h7_opsec_campaign.py --schedule full` (`campaign/result.json`, `campaign/repro-result.json`);
negative controls `tools/nc_h7.py` (`nc.log`, RESULT: 7/7 negative controls PASS).
Stage 1 (H0–H7) закрывается этой фазой.

