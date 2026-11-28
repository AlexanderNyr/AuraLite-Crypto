# E9 Report — Bridge + AuraLite OS

Date: 2026-11-22

The result: **ALL E9 GATES GREEN**

The D-E4 criteria are met literally: **MITM forged header → reject** (4
attack shapes: a foreign signer 99, a signature bit-flip, a fully foreign
commit, a duplicate vote — all with the exact `E9: forged header signature (…)`;
every signature is verified, no early stop); **OS client checks
finalized checkpoint** (`OsClient::accept` — only heights ≤ finalized →
`E9: checkpoint not finalized (6)`; `verify_checkpoint` rejects a
forged checkpoint); **emulator lane green** (`E9-LANE-OK 4`:
`aurad --version`, `aurad report --fixture`, refusing to start without
`--chain-id` (exit 2), a bounded node run + SIGTERM → a clean
shutdown; the log `lane.log`; registered in CI via
`make emulator-lane`). Limits (per-tx 1000 / daily 1200), pause,
replay protection, the reserve invariant `locked == minted` — 6 named-message
mutation controls.

The `crates/aura-storage/src/bridge.rs` model: an IBC-like light client
(a hash chain, ed25519 commits, >2/3 power = 27/40, an unknown signer =
forgery), a bridge (velocity windows, guardian pause, processing an id once,
a reserve) and `OsClient` for AuraLite OS (a self-contained checkpoint API).
The 25 scenario steps are pinned: the digest `039d498b…39580`, the aggregate
`4f8d9af3…575e4f`; the Rust example ↔ Python `tools/e9_bridge_check.py`
(via `cryptography` ed25519) — `E9-STEPS-OK 25` / `E9-SIM-OK` byte-for-
byte. The gate tests `tests/e9_bridge.rs` (8: the three DoD criteria, exact
literals, order/boundary/determinism metamorphic properties). Negative controls
`tools/nc_e9.py` — 6 controls (`nc.log`, RESULT: 6/6 negative
controls PASS). Sources: the tendermint spec verification_002_draft,
chainscorelabs IBC light client, docs.tendermint.com accountability,
ipsa-contracts VerifyCommitLight; nomoslabs (Nomad/Wormhole), dev.to
0xrivet, the zealynx checklist, blockchaindose — see.
`docs/ecosystem/e9-bridge-report.md`. Deviation: the external AuraLite-OS /
AuraLite-Emulator were replaced by the in-repo `OsClient` + the lane runner `aurad`.
