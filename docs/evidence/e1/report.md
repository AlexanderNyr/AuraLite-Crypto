# E1 Report — chain indexer

Date: 2026-11-19

The result: **ALL E1 GATES GREEN**

Gate: 10 000 blocks + an artificial reorg → 0 duplicates/losses; NC: without rollback → red.

The `crates/aura-storage/src/indexer.rs` code (idempotent ingest + reorg rollback over local storage — the rejection of Postgres is recorded in ECOSYSTEM_PLAN); the gate test `crates/aura-storage/tests/e1_indexer.rs` (`campaign/indexer.log`, `campaign/result.json`).
negative controls `tools/nc_e1.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

