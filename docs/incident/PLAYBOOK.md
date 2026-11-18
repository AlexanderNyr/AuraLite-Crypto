# AuraLite — incident playbook (H9)

Scope: mainnet/testnet operations of AuraLite-Crypto (W1-FORMAT-v1).
Drills: `tools/h9_incident_drill.py` over `tools/fixtures/h9_drills.json`.
SLOs (normative): **halt ≤ 15 min**, **recover ≤ 240 min**, post-mortem
draft ≤ 480 min after the incident starts.

Severity classes: S1 (funds at risk / consensus break), S2 (liveness
loss / halt needed), S3 (degraded service), S4 (informational).

## 1. Detection

<!-- H9: detect -->

- Monitors: finality lag, mempool growth, validator heartbeat, balance
  invariants (H6 conservation checks), CI/supply-chain alarms (H1).
- Any S1/S2 signal pages the on-call operator immediately (≤ 5 min).
- First action after confirmation: declare the incident and start the
  clock (t=0 for the SLOs above).

## 2. Halt — emergency network stop

<!-- H9: halt -->

- Trigger: double-signing (H0-S10), state divergence, bridge anomaly
  (E9), or operator call for any S1.
- Action: validators sign a halt attestation; the chain stops at
  threshold ≥ 2/3 of the active set (see `aura-consensus` halt path).
- SLO: network halted within **15 minutes** of t=0. The drill
  "Byzantine validator + bridge bug" must prove this literally.

## 3. Pause — partial stop

<!-- H9: pause -->

- Trigger: S2/S3 where full halt is not needed (contract bug, spam).
- Action: governance pause of the affected tx class (H6 parameter
  change), mempool admission filter, or bridge lane freeze (E9).

## 4. Upgrade — coordinated fix

<!-- H9: upgrade -->

- Action: patch the defect, run `make test` + `make reaudit-h8`,
  build reproducibly (H7 two-host sha256), sign the release (H1
  `tools/sign_release.py`), publish the advisory (`SECURITY.md`
  disclosure), coordinate the validator set upgrade window.

## 5. Rollback — last known good state

<!-- H9: rollback -->

- Action: roll back to the last finalized checkpoint (H0-S12
  protections), re-sync validators from the checkpoint, verify state
  roots against the pinned fixtures (W0/H4 invariants).
- Rollback is preferred over forward-fix when funds are at risk.

## 6. Recovery — service restore

<!-- H9: recover -->

- Action: restart validators on the fixed/rolled-back version, watch
  finality for ≥ 10 min, unfreeze paused lanes, re-enable bridges (E9).
- SLO: network recovered within **240 minutes** of t=0; the drill
  "Byzantine validator + bridge bug" must prove this literally.

## 7. Communication

<!-- H9: communicate -->

- Internal: incident channel + on-call bridge; status every 30 minutes
  during S1/S2.
- External: status page entry at t=0, updates at least every 4 hours;
  disclosure follows `SECURITY.md` (ack ≤ 3 working days, disclosure
  ≤ 90 days or with the fix).
- One voice: the release manager publishes; validators mirror.

## 8. Post-mortem

<!-- H9: post-mortem -->

Template (draft ≤ 480 min after t=0; final within 5 working days):

1. Timeline (detection, halt, pause, upgrade/rollback, recovery).
2. Root cause (with the failing invariant / H0 scenario id).
3. Blast radius (funds, liveness, data).
4. Corrective actions (fix, test, NC, runbook update) with owners.
5. What the drill/monitoring missed.

## 9. Drills

- Scheduled drills live in `tools/fixtures/h9_drills.json`; the runner
  validates playbook coverage, the incident state machine order and the
  SLO timeline, and names the phase gate: drill **"Byzantine validator
  + bridge bug"** — network halted/recovered within SLO.
- Every drill is tied to the H0 threat-model scenarios it exercises.
