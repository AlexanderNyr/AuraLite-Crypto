# T0 — economics baseline: desired properties

**Status:** ✅ T0 closed. **Token:** none. The native asset `ALCRY` does not
exist in this phase; it is introduced by the issuance model in T1. Until
then the simulation uses abstract **testnet accounting units** with no
market value (decision D-T4) — Stage 1 issues nothing that could be offered
to the public.

Everything below is measurable: the properties are enforced by invariants in
`crates/aura-storage/src/econ.rs`, exercised by the deterministic baseline
simulation (`sim/t0-baseline.json`, 1000 steps, seed 20261180) and pinned by
the golden state digest `aec12b64…c7a51a` in both the Rust tests and the
independent Python oracle.

## Scarcity

* The total supply equals the **declared** genesis endowment plus declared
  emission — nothing else. The genesis table (`sim/t0-baseline.json`,
  3 000 000 units across five accounts) is the whole of it: **no hidden
  premine, no emission from nowhere**.
* Any creation of units outside a declared source breaks
  `sum(balances) == genesis + declared emission` and is detected at every
  simulation step (negative control NC-1 makes the gate go red literally).
* Decisions D-T1 (fixed `u128` accounting, 18 decimals reserved) and the
  T1 issuance model will extend the declared sources; changing emission
  later requires governance with timelock, not a simple commit.

## Security

* Invariants are checked **after every step** of the simulation, not only at
  the end: conservation of supply, non-negative balances, and full
  reproducibility.
* A transfer can never spend more than the sender holds
  (`InsufficientFunds`); there is no credit, no negative balance, no
  overflow path (`checked` arithmetic throughout).
* The simulation is bit-exact reproducible from its seed: the same schedule
  always yields the same state digest, in Rust and in the Python reference
  model. Mutation controls (`tools/nc_t0.py`) prove each guard can actually
  fail: six injected bugs each turn the gate red with its named message.

## Accessibility

* The baseline distribution is explicit, small and published: five named
  accounts with fixed endowments, readable in `sim/t0-baseline.json`. There
  is no separate insider table.
* Later distribution mechanisms (faucet — already testnet-only and rate
  limited per E3; validator rewards — T1/T4; grants — T7) must state their
  parameters in genesis and keep the same "everything declared" discipline.
* No promises of returns (decision D-T2): these units are accounting values
  for testnet experiments, not investments.

## What T0 deliberately does not do

* No issuance model, allocation, fees or staking — those are T1–T4.
* No legal claims (decision D-T4: the legal perimeter is T8, ⏸ Stage 2).
