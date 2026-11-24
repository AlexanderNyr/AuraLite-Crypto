# T8 Report — White paper + law

Date: 2026-11-22

The result: **ALL T8 GATES GREEN**

Gate: a checklist against MiCA Art. 4/6/8/9, notification >= 20 business days,
publication; **not legal advice**.

The `crates/aura-storage/src/whitepaper.rs` model: a checklist machine for a
crypto-asset white paper under MiCA (Regulation (EU) 2023/1114).
**Art. 4** — offer to the public: the offeror = a legal entity; the white
paper is drafted (**Art. 6**: project/technology/risks/holder sections
rights + statements traced to the T0–T7 model pins), notified
(**Art. 8**: at least 20 business days before publication — the boundary: exactly 20
passes, 19 — red) and published (**Art. 9**: before the offer starts,
the version = the notified one). Marketing (**Art. 7**) — only with a reference to the
published white paper and after publication. The "not legal
advice" disclaimer is mandatory (D-T4); income promises are forbidden (D-T2). The project
documents: `docs/whitepaper.md`, `docs/marketing.md`,
`docs/legal-review.md` (a checklist + the EU/Latvia jurisdiction). The scenario
`sim/t8-whitepaper.json` (8 documents, 77 steps: 1 fully valid,
each rejector fails exactly one item), the golden digest
03a85943…a89396f (Rust ↔ Python ↔ the `t8_whitepaper_example` example). The gate
`crates/aura-storage/tests/t8_whitepaper.rs` (10 tests + 2 metamorphic properties),
the oracle `tools/t8_whitepaper_check.py`.
negative controls `tools/nc_t8.py` (`nc.log`, RESULT: 7/7 negative
controls PASS) — 7 controls (the standard 6 + the literal NC of the 20-day window).
