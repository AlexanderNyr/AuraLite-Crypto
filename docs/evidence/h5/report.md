# H5 Report — consensus safety

Date: 2026-11-17

The result: **ALL H5 GATES GREEN**

Gate: model finds a counterexample when 2f+1 is removed; NC documented.

The campaign `h5_consensus_campaign.py --schedule full` (`campaign/result.json`, logs in `campaign/`);
negative controls `tools/nc_h5.py` (`nc.log`, RESULT: 6/6 negative controls PASS).
The TLA+ model `docs/h5/HotStuff3ChainSafetyMC.tla` (TLC, safe/`WeakenQuorum`):
safe — no violations; weakened — a counterexample is mandatory.

