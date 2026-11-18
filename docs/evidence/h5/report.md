# Отчёт H5 — consensus safety

Дата: 2026-11-17

Итог: **ВСЕ ГЕЙТЫ H5 ЗЕЛЁНЫЕ**

Гейт: model finds a counterexample when 2f+1 is removed; NC documented.

Кампания `h5_consensus_campaign.py --schedule full` (`campaign/result.json`, логи в `campaign/`);
negative controls `tools/nc_h5.py` (`nc.log`, RESULT: 6/6 negative controls PASS).
TLA+ модель `docs/h5/HotStuff3ChainSafetyMC.tla` (TLC, safe/`WeakenQuorum`):
safe — без нарушений; weakened — контрпример обязателен.

