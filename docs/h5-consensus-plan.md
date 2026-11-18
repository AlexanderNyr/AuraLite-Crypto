# H5 — Consensus safety plan (TLA+, Jepsen-like runs)

Status: 🚧 IN PROGRESS — H5
Owner: core maintainer
Date: 2026-11-17
Gate: `model finds a counterexample when 2f+1 is removed; NC documented`

## 1. Scope and DoD

SECURITY_PLAN row H5: TLA+/Apalache spec, Jepsen-like runs (together with
B10/N8). B10 shipped `HotStuff3ChainSafety.tla`, the bounded Rust checker
`aura_consensus::safety_model` and the 100-node simulation; N8 shipped the
chaos stand (latency/loss/partition/eclipse, 20 scenarios × 3 runs). H5
turns that into a **security-phase gate**: the model must *find the
counterexample* the moment `2f+1` is removed, formal spec and executable
model must not drift apart, and the Jepsen-like campaign must be recorded
with its divergence monitor proven sensitive.

Deliverables:

- **D-H5-1 — model-checkable TLA+ spec.** `docs/h5/HotStuff3ChainSafetyMC.tla`
  is a behaviour spec (Init/Next over vote sets) whose invariant is the
  3-chain accountable-safety essence of `HotStuff3ChainSafety.tla`
  (single honest vote per view + quorum intersection + no two conflicting
  commits). TLC runs it in two configurations: `h5-safe.cfg`
  (Threshold = 2F+1 → no violation) and `h5-weakened.cfg`
  (Threshold = F+1 → the model must report a counterexample).
  `tools-bin/tla2tools.jar` is the pinned checker (sha256 recorded).
- **D-H5-2 — executable model gate.** `BftSafetyModelChecker::check` over
  the validator-set table (4,1)/(7,2)/(10,3)/(13,4)/(31,10)/(100,33):
  Safe with `min_honest_quorum_overlap >= 1`; with
  `enforce_quorum_check = false` the checker must return a
  `Counterexample` with a non-empty `equivocating_validators` and
  `extract_conflicting_qc_offenders` must name the double-signers.
- **D-H5-3 — spec integrity.** `docs/evidence/b10/HotStuff3ChainSafety.tla`
  must stay byte-identical to `safety_model::TLA_SPEC_SOURCE` and keep
  `QuorumThreshold == 2 * F + 1`; the MC spec must keep the same threshold.
- **D-H5-4 — Jepsen-like runs (with B10/N8).** The N8 chaos matrix
  (20 scenarios × 3 runs, latency/loss/partition/eclipse) under the H5
  umbrella: zero finalized-head divergence, and the divergence monitor
  (`verify_cluster_convergence`) must reject a crafted divergent cluster —
  a monitor that never fires is not a monitor.
- **D-H5-5 — campaign runner.** `tools/h5_consensus_campaign.py`:
  recorded, resumable (h3/h4 pattern) — model table + chaos matrix + TLC
  runs (both configs) with commands, seeds, source manifests and run.log
  SHA-256 into `<out>/result.json`.
- **D-H5-6 — mutation controls** (`tools/nc_h5.py`, 6): §5.

## 2. Tests

| test | property | NC |
| --- | --- | --- |
| `h5_model_safety_and_counterexample` | Safe for f < n/3 (table); weakened quorum ⇒ counterexample + named offenders | NC-1, NC-2, NC-3 |
| `h5_tla_spec_integrity` | spec file == pinned source; threshold stays `2 * F + 1` | NC-4 |
| `h5_divergence_monitor_fires` | crafted divergent cluster is rejected | NC-5 |
| `h5_chaos_campaign_zero_divergence` | 20 scenarios × 3 runs, zero divergence, 60 reports | NC-6 |

## 3. Campaigns and deviation record

- `make model-h5` runs the in-tree tests (cheap gate, in `make test` via
  `cargo test --workspace`). `make model-h5-full` records the campaign
  (model table + chaos matrix + TLC both configs).
- **Deviation (2026-11-17, session environment).** Apalache is not run
  here (its distribution is a heavier toolchain); the gate uses **TLC**
  over the H5 behaviour spec and the pinned bounded Rust checker. If
  `tools-bin/tla2tools.jar` is absent, the campaign records the TLC steps
  as `skipped`, never as passes. The B10 spec (`HotStuff3ChainSafety.tla`)
  is an invariant specification without Init/Next — it is checked by the
  integrity test and realised executable by `HotStuff3ChainSafetyMC.tla`.

## 4. Crash triage

A failing model run or a TLC counterexample in the SAFE configuration is a
regression: reproduce with the recorded config/seed, minimise the trace,
fix with a regression vector in `b10_safety_model.txt`/the MC spec, re-run.

## 5. Gates and controls

- `tools/nc_h5.py` injects bugs (restored after each control) and requires
  the H5 tests to go red with the named message:
  **NC-1 (named) injected bug -> model must find counterexample** — the
  checker always reports Safe ⇒ `H5: model must find a counterexample when
  2f+1 is removed`; NC-2 `required_quorum` weakened to f+1 ⇒ `H5: safety
  must hold for f < n/3`; NC-3 offender extraction returns empty ⇒ `H5:
  accountable safety extraction must name offenders`; NC-4 TLA threshold
  changed to `F + 1` ⇒ `H5: TLA spec drift`; NC-5 divergence monitor never
  fires ⇒ `H5: divergence monitor must fire`; NC-6 chaos matrix truncated
  below 20 scenarios ⇒ `H5: chaos campaign must cover 20 scenarios`.
- Evidence `docs/evidence/h5/` via `make evidence-h5` (`gates.txt`,
  `nc.log` with `RESULT: 6/6 negative controls PASS`, `campaign/` bundle
  incl. TLC logs, `report.md`, `inventory.txt` cumulative TOTAL: 460,
  `SHA256SUMS`).
