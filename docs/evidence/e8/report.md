# E8 Report — NFT + names + DAO

Date: 2026-11-22

The result: **ALL E8 GATES GREEN**

Gate 1: **50 standard vectors** (ARC-721/1155 + `.aura` names +
identity) — the fixture `tests/fixtures/e8-standards.txt` with exact
error literals `E8: …`; the Rust test and the independent Python model give the
same digest `8cb9da94…62bd3`. Gate 2: **the first DAO vote through the
UI** — the receipt is recorded, the tally `250000/75000/25000` ALCRY is brought to
`executed`; the governance UI HTML page is byte-identical in Rust and Python
(sha `bf1486e2…1167f`). 6 named-message mutation controls.

The `crates/aura-storage/src/nft.rs` model: ARC-721 (per-slot ownership,
transfers clear approvals — the stale-approval vector v013), ARC-1155 (per-id
supply, operators, all-or-nothing batch), the `.aura` name service
(365 days + a 90-day grace, renewal by anyone, re-registration
after expiry+grace, primary/reverse), on-chain identity (a meta-digest
bound to a live name). The governance UI `src/dao_ui.rs` wraps the T6
DAO: propose/vote/finalize/execute/veto, a receipt journal, a determi-
nistic HTML page. 25 named messages. Decisions D-E6/D-E7/D-E8.

The simulation `sim/e8-dao.json`, the aggregate `39674d6a…fb313`
(`E8-VECTORS-OK 50` / `E8-UI-OK` / `E8-SIM-OK` byte-for-byte between
`examples/e8_nft_example.rs` and `tools/e8_nft_check.py`). The gate tests
`tests/e8_nft.rs` (15 tests: vectors, digests, the scenario, HTML, exact
DAO errors, metamorphic properties batch≡sequential / mint order / vote
order / determinism). Negative controls `tools/nc_e8.py` — 6
controls (`nc.log`, RESULT: 6/6 negative controls PASS).
Sources: rareskills.io ERC-1155, eco.com, webopedia, metana;
gate.com ENS, chainscorelabs.com, cryptoslate.com, docs.ens.domains —
see `docs/ecosystem/e8-nft-report.md`.
