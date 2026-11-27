# Отчёт E4 — AMM DEX

Дата: 2026-11-22

Итог: **ВСЕ ГЕЙТЫ E4 ЗЕЛЁНЫЕ**

Гейт: **инвариант `k` никогда не убывает** (256-битное сравнение, свопы и
маршруты); NC: отключение slippage guard → красный. DeFi-фазы идут после
оракула (E5) и аудита (D-E3).

Модель `crates/aura-storage/src/amm.rs`: constant-product AMM (x·y = k)
с LP-токенами. Fee-on-input 30 bps (0.3%, модель Uniswap V2), floor-округление
на каждом делении, 256-битная арифметика (`wide_mul`/`mul_div`), инвариант
`k` в 256 битах. LP: первый депозит `a + b` (объяснённое отклонение от
sqrt), далее строго пропорционально `min(a·S/ra, b·S/rb)`, burn — pro-rata
с точным round-trip. `swap_exact_in` со slippage guard, `swap_route` —
мультимаршрут A→B→C. 9 named messages. Решения D-T1..D-T4.

Симуляция `sim/e4-amm.json` (1 A→B из (100/200) = 1974316068794122597;
route 1 A→B→C = 5791186053878244695; single 2A = 3910033923564131223 ≥
split = 3909976989011504086; LP 300e18/150e18, burn (50e18, 100e18) точно),
агрегатный дайджест c1286a35…283865 (Rust-пример ↔ Python
`tools/e4_amm_check.py`). Гейт `crates/aura-storage/tests/e4_amm.rs`
(21 тест, метаморфизмы split/route/determinism). Negative controls
`tools/nc_e4.py` — 6 контролей: ослабление каждого named message →
красный, restore → зелёный (`nc.log`, RESULT: 6/6 negative controls PASS).
Источники: Uniswap V2 whitepaper, dev.to «AMM Mathematics», autheo.com,
zealynx.io — см. `docs/ecosystem/e4-amm-report.md`.
