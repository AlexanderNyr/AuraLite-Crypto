# Отчёт T7 — Treasury and grants

Дата: 2026-11-21

Итог: **ВСЕ ГЕЙТЫ T7 ЗЕЛЁНЫЕ**

Гейт: у каждого платежа есть proposal + tx + report; снятие без голоса — красный.

Модель `crates/aura-storage/src/treasury.rs`: казна с подписанным балансом
(`i128`, овердрафт видим и запрещён); совет 5 голосует за платёжные предложения
(строгое большинство), мультисиг 2-of-3 (решение W4) подтверждает; жизненный
цикл proposal -> vote -> finalize -> approve -> execute. Каждый платёж получает
номер **tx** и прозрачный `Report {tx_id, proposal_id, amount, recipient,
purpose, height}` (отчёт казны печатает пример `T7-TX` строками). Снятие без
проголосованного предложения отвергается (`NoVote`, шаг 19 сценария); платёж
не исполняется дважды (`AlreadyExecuted`); отклонённое предложение не платит
(`Rejected`); сумма больше баланса — `InsufficientFunds`. Сценарий
`sim/t7-treasury.json` (10 предложений, 82 шага: 6 выплат с tx+report,
2 отклонения, попытка снятия без голоса), золотой дайджест 719c93b2…3231f7
(Rust ↔ Python ↔ пример `t7_treasury_example`). Гейт
`crates/aura-storage/tests/t7_treasury.rs` (10 тестов + 2 метаморфизма),
оракул `tools/t7_treasury_check.py`.
negative controls `tools/nc_t7.py` (`nc.log`, RESULT: 7/7 negative controls PASS)
— 7 контролей (стандарт 6 + literal NC снятия без голоса).
