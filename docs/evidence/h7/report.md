# H7 Report — OpSec / reproducible builds

Date: 2026-11-17

The result: **ALL H7 GATES GREEN**

Gate: NC: secret in git -> CI red; two hosts build to the same sha256.

The campaign `h7_opsec_campaign.py --schedule full` (`campaign/result.json`, `campaign/repro-result.json`);
negative controls `tools/nc_h7.py` (`nc.log`, RESULT: 7/7 negative controls PASS).
Stage 1 (H0–H7) is closed by this phase.

