# Отчёт E9 — Bridge + AuraLite OS

Дата: 2026-11-22

Итог: **ВСЕ ГЕЙТЫ E9 ЗЕЛЁНЫЕ**

Критерии D-E4 выполнены буквально: **MITM forged header → reject** (4
формы атаки: чужой подписчик 99, бит-флип подписи, полностью чужой
коммит, дубль-голос — все с точным `E9: forged header signature (…)`;
каждая подпись проверяется, early-stop нет); **OS client checks
finalized checkpoint** (`OsClient::accept` — только высоты ≤ finalized →
`E9: checkpoint not finalized (6)`; `verify_checkpoint` отклоняет
подделанный чекпоинт); **emulator lane green** (`E9-LANE-OK 4`:
`aurad --version`, `aurad report --fixture`, отказ запуска без
`--chain-id` (exit 2), bounded-запуск ноды + SIGTERM → чистое
завершение; лог `lane.log`; зарегистрирован в CI через
`make emulator-lane`). Лимиты (per-tx 1000 / дневной 1200), pause,
replay-защита, инвариант резерва `locked == minted` — 6 named-message
мутационных контролей.

Модель `crates/aura-storage/src/bridge.rs`: IBC-подобный light client
(хэш-цепочка, ed25519-коммиты, >2/3 мощности = 27/40, unknown signer =
forgery), мост (velocity-окна, guardian-pause, обработка id один раз,
резерв) и `OsClient` для AuraLite OS (self-contained checkpoint API).
25 шагов сценария закреплены: дайджест `039d498b…39580`, агрегат
`4f8d9af3…575e4f`; Rust-пример ↔ Python `tools/e9_bridge_check.py`
(через `cryptography` ed25519) — `E9-STEPS-OK 25` / `E9-SIM-OK` байт-в-
байт. Гейт-тесты `tests/e9_bridge.rs` (8: три критерия DoD, точные
литералы, метаморфизмы порядка/границ/детерминизма). Negative controls
`tools/nc_e9.py` — 6 контролей (`nc.log`, RESULT: 6/6 negative
controls PASS). Источники: tendermint spec verification_002_draft,
chainscorelabs IBC light client, docs.tendermint.com accountability,
ipsa-contracts VerifyCommitLight; nomoslabs (Nomad/Wormhole), dev.to
0xrivet, zealynx checklist, blockchaindose — см.
`docs/ecosystem/e9-bridge-report.md`. Deviation: внешние AuraLite-OS /
AuraLite-Emulator заменены in-repo `OsClient` + лейн-раннер `aurad`.
