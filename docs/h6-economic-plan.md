# H6 — Economic attacks plan (simulations, bounded damage)

Status: 🚧 IN PROGRESS — H6
Owner: core maintainer
Date: 2026-11-17
Gate: `simulations: damage bounded by limits/slashing; NC: no pause → red`

## 1. Scope and DoD

SECURITY_PLAN row H6 covers six economic attack vectors — MEV,
censorship, stake grinding, 34% attack, oracle manipulation, bridge drain —
and the gate demands simulations whose damage is bounded by protocol limits
and/or slashing, plus the negative control where removing the emergency
pause must turn the gate red. All deterministic, seeded, self-contained.

Deliverables:

- **D-H6-1 — MEV bounded by canonical ordering.** The mempool's canonical
  transaction order is a deterministic function of the tx set (metamorphic
  property: insert order is irrelevant) and follows the pinned
  `(sender, nonce)` lane key with deterministic fee-based replacement and
  eviction. A proposer cannot extract discretionary reordering MEV; only
  inclusion/exclusion remains, which D-H6-2 bounds.
- **D-H6-2 — censorship bounded by proposer rotation.** `Proposer::propose`
  is strict round-robin (`validators[round % len]`); a censoring minority
  can delay a valid tx only until the next honest round: bounded delay ≤
  `validator_count - 1` rounds, never permanent exclusion.
- **D-H6-3 — stake grinding bounded by limits + slashing.** Leader selection
  is stake-weighted over deterministic inputs (`VrfLeaderElection`); a
  forged/ground leader claim is itself slashable
  (`SlashingAttackKind::InvalidVrfLeaderClaim`) — grinding beyond elected
  slots burns stake (`SlashingReceipt.stake_after < stake_before`).
- **D-H6-4 — 34% attack: emission conserved.** A super-minority attacker
  (adversarial proposer/fee schedule, deterministic) cannot inflate supply:
  `RewardsEngine::verify_sum_invariant` (`|sum + burned - issued| <= 1`)
  holds after every step; no fund creation under 34% byzantine stake
  (safety side of the 34% attack is H5/B10).
- **D-H6-5 — oracle manipulation bounded by median reports.** Modelled
  surface (see §3): stake-weighted median of validator price reports; with
  attacker share α the manipulated median stays inside the honest report
  range — damage bounded by the report window.
- **D-H6-6 — bridge drain bounded by reserves.** Modelled surface (§3):
  peg with fixed reserves; overdraw, replay and forged-amount withdrawals
  are rejected; invariant `Σ withdrawn <= reserves` always holds.
- **D-H6-7 — emergency pause halts governance.** `GovernanceEngine`
  (`EmergencyPauseState`, 2f+1 QC): while paused, `submit_proposal` and
  `execute_proposal` must fail with `GovernanceError::EmergencyPaused`;
  the pause lifts only via a valid QC. This is the named NC gate.

## 2. Tests

| test | property | NC |
| --- | --- | --- |
| `h6_stake_grinding_bounded` (aura-consensus) | selection ∝ stake over deterministic rounds; forged claim slashed | NC-3 |
| `h6_emission_conserved_under_attack` | sum invariant under adversarial proposer/fees | NC-2 |
| `h6_oracle_manipulation_bounded` | median stays within honest report range | NC-6 |
| `h6_bridge_drain_bounded` | `Σ withdrawn <= reserves`, no replay | NC-7 |
| `h6_emergency_pause_halts_actions` | paused ⇒ governance actions rejected | NC-1 (named) |
| `h6_censorship_bounded_by_rotation` | bounded delay ≤ n-1 rounds | NC-4 |
| `h6_canonical_ordering_deterministic` (aura-mempool) | insert-order metamorphism + fee-key order | NC-5 |

## 3. Modelled surfaces and deviation record

- **Deviation (2026-11-17, scope):** AuraLite has no native oracle or
  bridge component in the shipped crates; the H6 plan models both as
  executable specifications inside the H6 suite (deterministic,
  self-contained models of the intended native behaviour). The negative
  controls for these surfaces inject the bug into the model implementation
  and require the property to go red — the same guard a future native
  implementation must pass. This is recorded here as the phase deviation
  mechanism; nothing else is out of scope.
- The 34% safety question (conflicting commits) is already gated by
  H5/B10; H6 covers the economic half (no fund creation).

## 4. Crash triage

A failing bound is a regression: reproduce with the recorded seed,
minimise the adversarial schedule, fix with a regression vector, re-run.

## 5. Gates and controls

- `tools/nc_h6.py` injects bugs (restored after each control) and requires
  the named H6 tests to go red with the named message:
  **NC-1 (named) injected bug -> no pause must turn the gate red** — pause
  enforcement removed ⇒ `H6: emergency pause must halt governance actions`;
  NC-2 sum invariant forced green ⇒ `H6: emission must stay conserved under
  attack`; NC-3 slash without burn ⇒ `H6: attacker damage must be bounded
  by slashing`; NC-4 `Proposer::propose` pinned to one validator ⇒ `H6:
  censorship must be bounded by proposer rotation`; NC-5 canonical order
  reversed ⇒ `H6: MEV extraction must stay bounded by canonical ordering`;
  NC-6 oracle median → max ⇒ `H6: oracle manipulation must stay within
  report bounds`; NC-7 bridge reserve bound disabled ⇒ `H6: bridge
  withdrawals must stay within reserves`.
- Evidence `docs/evidence/h6/` via `make evidence-h6` (`gates.txt`,
  `nc.log` with `RESULT: 7/7 negative controls PASS`, `campaign/` bundle,
  `report.md`, `inventory.txt` cumulative TOTAL: 467, `SHA256SUMS`).
