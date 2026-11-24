# E9 — Bridge + AuraLite OS (phase report)

Date: 2026-11-22. Phase patch: `patches/0092-E9-Bridge.patch`.
Aggregate digest
`4f8d9af363ab88fb7a6ce0ef6af52facb886f5e123ca0a128d1693e06d575e4f`.

## DoD and criteria (D-E4)

| Criterion | Status |
|---|---|
| **MITM forged header → reject** | ✅ `verify_commit` checks EVERY signature (no early stop): an unknown signer (attack A, id 99) → `E9: forged header signature (99)`; a bit-flipped validator signature (attack B) → `E9: forged header signature (1)`; a wholly foreign commit (attack C) and a double vote → also rejected. 3 of the 4 attacks in the gate + a scenario lane (2 attempts, `forged_rejected=2`) |
| **OS client checks finalized checkpoint** | ✅ `OsClient` (`src/bridge.rs`) — the embedding point for AuraLite OS: `verify_checkpoint` (the commit is checked against the validator set), `accept` (only heights ≤ the finalized checkpoint → `E9: checkpoint not finalized (6)`), `check_root` |
| **emulator lane green** | ✅ `tools/e9_emulator_lane.py` → `E9-LANE-OK 4`: `aurad --version` ✓, `aurad report --fixture` ✓, running without `--chain-id` → exit 2 ✓, a bounded node run + SIGTERM → clean shutdown ✓ (log `docs/evidence/e9/lane.log`); registered in CI (`make emulator-lane`, part of `make test`) |
| **limits / pause (D-E4)** | ✅ a per-transaction limit of 1000 and a daily 1200 (velocity window, reset by day), guardian pause (circuit breaker), replay protection (an id is processed exactly once), the reserve invariant `locked == minted` (a withdraw cannot exceed the reserve) |
| **separate audit (D-E4)** | ✅ pinned: the bridge does not ship to mainnet without H8 (a dedicated audit + bug bounty); SECURITY_PLAN: "A bridge bug is the most expensive: E9 is not enabled on mainnet without H8" |

## The `crates/aura-storage/src/bridge.rs` model

### Light client (IBC/Tendermint Core Verification form)

A header is `height | parent | state root | time`; its hash is sha256 of the canonical
form `e9h|…`. A commit is ed25519 signatures (RFC 8032, `aura_crypto`) over the header
hash. Acceptance rules (LCV-FUNC-VALID.2):

1. `header.height > trusted.height` (else `header not newer`);
2. `header.parent == trusted.hash` (else `header does not chain`);
3. every commit signature is from a **known** validator and **verifies**
   (an unknown signer or a bad signature → `forged header signature`;
   a double vote is forgery too); **no early stop** — a bad "extra" signature
   is a failure, not noise (stricter than CometBFT's early abort);
4. signature power is **strictly greater than 2/3** (`total·2/3 + 1` = 27 of 40),
   else `commit lacks quorum (…)`.

Every accepted header is a **finalized checkpoint** (instant
2/3 finality). The validator set is fixed (4 × power 10);
rotation is a separate task on top of the same scheme (NextValidatorsHash).

### Bridge protections (incident lessons)

- **Replay** (Nomad: replaying one valid message, $190M) —
  `message already processed (id)`, processed exactly once.
- **Limits** (Nomad: $190M in 150 minutes with no circuit breaker) —
  per-tx 1000 + daily velocity 1200 (both checks with exact boundaries:
  exactly the limit — ok, +1 wei — reject).
- **Pause** (zealynx/CCIP: kill switch 1-of-N guardians) —
  `bridge is paused`, guardian pause/release.
- **Reserve invariant** (Wormhole: mint without a deposit, $326M) —
  `locked == minted` after every step, a withdraw beyond the reserve →
  `withdraw exceeds reserve`.
- **Root binding** (a relative of Nomad's zero-root bug) — a message is
  proven against the specific `state root` of a finalized height
  (`message root mismatch`).

### AuraLite OS client

`OsClient` is self-contained (sha256 + ed25519 only), a minimal
surface for embedding into AuraLite OS: `verify_checkpoint` +
`accept`/`check_root` against the **finalized** checkpoint. Full integration
(hardware keystore, TLS channel) lives in the AuraLite-OS repository —
**deviation**: the external AuraLite-OS / AuraLite-Emulator are unavailable from
this repository; instead of the emulator there is a deterministic in-repo lane
`tools/e9_emulator_lane.py` fulfilling the "CI run of `aurad`" contract.

## Gates and tests

`crates/aura-storage/tests/e9_bridge.rs` — 8 tests: the three DoD criteria
(MITM ×4 attacks; OS checkpoint + forged checkpoint; the full scenario with a
line-pinned step log), exact literals of the 6 NC targets,
the remaining named messages, metamorphisms (signature order in a commit does not
matter; limit boundaries; `locked == minted`; double-run determinism;
quorum 27/40). The scenario: emulator H1–H5 (commits 0,1,2) →
an attack lane (stale header, short quorum, 2 MITM forgeries, a fork) →
transfers (500+400 deposit, 100 withdraw) → replay/limit/root rejections →
pause/unpause → OS checkpoint (5 ok / 6 reject) → day 2 → a beyond-
reserve rejection → the invariant. The steps and digest
`039d498ba6fcd17e7640094492283a94be2c3447f8ebec89f9306eb438339580`
are pinned in Rust and in the independent Python model
`tools/e9_bridge_check.py` (ed25519 via `cryptography`, RFC 8032) —
output `E9-STEPS-OK 25` / `E9-SIM-OK …` byte-for-byte.

## Negative controls

`tools/nc_e9.py` — 6 mutations with named messages: `bad-forged-header`,
`bad-quorum`, `bad-chain`, **`bad-finalized` (the gate one — "OS client
checks finalized checkpoint»)**, `bad-replay`, `bad-pause`.
**6/6 RED + GREEN.** Cumulative: **608 NC / 31 phases**.

## Sources

- **Tendermint/IBC light client**: github.com/tendermint/tendermint
  spec `verification_002_draft.md` — Core Verification: a commit carries
  signatures of >2/3 of validators and none from outsiders; a forged
  header from a faulty node must never persist.
  chainscorelabs.com «IBC Light Client Verification» — trusted
  height/validators anchor, 2/3+ against the set hash, misbehaviour →
  freeze. docs.tendermint.com accountability — attack scenarios (phantom
  validators, equivocation, forged header). GitHub ipsadev/ipsa-contracts
  — VerifyCommitLight: tallying to >2/3, rejecting "not newer than trusted",
  rejecting a commit at a different height.
- **Bridge incidents**: nomoslabs.io (Nomad $190M — zero-root init,
  no rate limit/circuit breaker; Wormhole $326M — a signature-verification bug,
  the minted↔deposited invariant), dev.to/0xrivet (5 defense layers;
  Bybit $1.46B with no rate limit), zealynx.io checklist (pause 1-of-N,
  volume limits, CCIP rate limiting), blockchaindose.com (replay nonce,
  dynamic rate limiting + auto-pause).
- **Internal**: `crates/aura-crypto/src/ed25519.rs` (RFC 8032,
  `sign_raw`/`verify_raw` + canonical-S guard), `crates/aura-node`
  (`aurad`: report/fixture, SIGTERM shutdown, exit codes 0/2/64),
  SECURITY_PLAN (D-E4, H6 bridge drill, "E9 does not ship without H8").

## Phase decisions

- **D-E9**: a commit is verified in full (all signatures, duplicates = forgery,
  no early stop) — stricter than CometBFT's early abort; an unknown
  signer is `forged header signature`, not an ignore.
- **D-E9b**: both velocity limits (per-tx + daily) are counted for
  every message including withdrawals; the window resets at day change.
- **D-E9c**: the external AuraLite-OS/AuraLite-Emulator are replaced by
  in-repo components (OsClient + the `aurad` lane runner) — the deviation
  is recorded; integration is tracked in their repositories.
