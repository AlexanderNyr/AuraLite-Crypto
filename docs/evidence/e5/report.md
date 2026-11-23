# Отчёт E5 — Oracle

Дата: 2026-11-22

Итог: **ВСЕ ГЕЙТЫ E5 ЗЕЛЁНЫЕ**

Гейт: **5 провайдеров, 2 вредоносных → цена не двигается**; NC: один
провайдер → красный. Порядок D-E3: DeFi (AMM/CDP/lending) — после
оракула и аудита; E5 закрывает оракул.

Модель `crates/aura-storage/src/oracle.rs`: децентрализованная сеть
ценовых фидов. Каждый провайдер подписывает отчёт (ed25519 по
каноничному сообщению feed16||price16BE||ts8BE); admission — строгий
(неизвестный провайдер, дубль в раунде, чужой фид, плохая подпись,
устаревший/будущий отчёт → named message). Агрегация: кворум N/2+1
(минимум 3), нижняя медиана admitted-цен, полоса выбросов ±500 bps от
предварительной медианы, финальная медиана выживших (lower median и по
timestamp). Константы: Price=u128 (18 decimals), staleness 3600s
(heartbeat Chainlink), deviation 500bps. Решения D-E3 (порядок: оракул →
аудит → DeFi) и стандарт E-фаз.

Симуляция `sim/e5-oracle.json` (5 отчётов: честные 100.0/100.1/99.9 +
памп x100 + дамп 0.0001 → цена 100.0, отброшено 2 выброса), агрегатный
дайджест aee332b6…dead24 (Rust-пример ↔ Python `tools/e5_oracle_check.py`).
Гейт `crates/aura-storage/tests/e5_oracle.rs` (14 тестов, включая
метаморфизмы перестановки и масштабирования). Negative controls
`tools/nc_e5.py` — 6 контролей: ослабление каждого named message →
красный, restore → зелёный (`nc.log`, RESULT: 6/6 negative controls PASS).
Источники: Chainlink DON (OCR, quorum N/2+1, heartbeat 3600s, deviation
0.5%), Pyth, UMA — см. `docs/ecosystem/e5-oracle-report.md`.
