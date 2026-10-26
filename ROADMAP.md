# ROADMAP — AuraLite-Crypto

> **House style** (adopted from
> [`AuraLite-OS/docs/plans`](https://github.com/AlexanderNyr/AuraLite-OS/tree/main/docs/plans)
> and [`AuraLite-Emulator/ROADMAP.md`](https://github.com/AlexanderNyr/AuraLite-Emulator/blob/main/ROADMAP.md)):
> each effort is a dependency-ordered plan in `docs/plans/` with a definition
> of done and a test gate per phase, and ships as **one `.patch` per phase**
> under `patches/`. Every claim in these docs is measured against the tree,
> not assumed.

## North star

```
  AuraLite-Crypto runs as a real public L1:
  third-party validators, real transactions, immutable history,
  working applications, and not one unmeasured claim.
```

North star spelled out (these are gates, not slogans):

| Metric | Target | Where measured |
|---|---|---|
| Mainnet validators | >= 21 independent operators; max share of one entity < 1/3 of stake | `OPS_PLAN` O6 (⏸ Stage 2) |
| Finality | p99 < 2 s with 100 nodes | `PERF_PLAN` P6 (⏸ Stage 2) |
| Throughput | >= 1000 simple transfers/s on 8 vCPU, p99 latency < 5 s | `PERF_PLAN` P6 (⏸ Stage 2) |
| Reproducibility | same genesis + tx stream => byte-identical state root on N nodes | `EXEC_PLAN` X7 |
| Security | external audit + bug bounty + 0 unfixed critical findings | `SECURITY_PLAN` H8 (⏸ Stage 2) |
| Applications | wallet, explorer, DEX, staking, bridge live on mainnet | `ECOSYSTEM_PLAN` E9 (⏸ Stage 2) |

---

## 0. D0 decisions — locked 2026-10-25

The owner answered the key questions; the ticker was chosen from a measured
audit (raw results in [`docs/TICKER_AUDIT.md`](docs/TICKER_AUDIT.md)).
Q1–Q3 and Q5 are **closed**; Q4, Q6, Q7 are accepted as defaults and can be
revised without rewriting the other plans.

| # | Decision | Adopted | Status | Blocks |
|---|---|---|---|---|
| Q1 | Name / ticker / chain-id | Project `AuraLite-Crypto`; network `AuraLite-Crypto Network`; ticker **`ALCRY`**; `auralite-test-1` (M1/M2), `auralite-1` (mainnet reserved) | ✅ LOCKED | C1, T1, O5 |
| Q2 | Consensus | **BFT-PoS**: HotStuff-like pipeline, 2f+1 QC, VRF leader | ✅ LOCKED | B1–B10 |
| Q3 | VM | **WASM** (`wasmtime`) + our ABI and Rust SDK; EVM (`revm`) is optional X9 | ✅ LOCKED | X1–X9 |
| Q4 | State model | accounts + versioned JMT-like tree | ✅ default | C7, S1–S7 |
| Q5 | How "real" | **Stage 1 = devnet + public testnet (M0–M2)** with no market token; mainnet, MiCA white paper and external audit are Stage 2 by separate decision | ✅ LOCKED | T8, O5, O6 |
| Q6 | Documentation language | Russian original; **English as of 2026-10-25** (owner request) | ✅ default | CONTRIBUTING |
| Q7 | AuraLite OS / Emulator integration | optional track E9, Stage 2 | ✅ default | E9 |

### Stage split

| Stage | Composition | Status |
|---|---|---|
| **Stage 1 — ACTIVE** | C0–C10, S0–S7, X0–X8, B0–B10, N0–N8, D0–D8, W0–W4, E0–E3, T0–T4, H0–H7, P0–P3, O0–O4 → milestones **M0, M1, M2** | current focus |
| **Stage 2 — ⏸ DEFERRED** | X9, W5–W7, E4–E9, T5–T8, H8–H9, P4–P6, O5–O9 → milestones **M3, M4, M5** | plans retained, start by separate decision |

The north star remains the long-term goal; Stage 1 delivers a real, measurable
network with third-party validators, Stage 2 delivers the public economy and
mainnet. Deferred phases do not block M2 and are never presented as done.

## 1. Frozen plan decisions

**D1. Our own L1, not contracts on someone else's chain.** Otherwise
"ecosystem" collapses into a set of contracts and the north star is
unreachable.

**D2. Rust-only on the consensus path.** The core (`types`, `codec`, `crypto`,
`state`, `exec`, `consensus`) is Rust only, no FFI in the critical path, no
`unsafe` without a separate justification and test. Other languages are
clients/SDKs only.

**D3. Determinism over speed.** No `HashMap` in consensus structures, no `f64`
in the state machine, no `Clock`/`Random`/`syscall` in contracts. Any
nondeterminism is a consensus-level bug.

**D4. Canonical serialization is our `aura-codec`.** One byte to one place:
field order fixed, length-prefixed `Vec`, tagged `Option`, non-canonical
encodings rejected. `serde` is allowed only outside consensus (RPC, configs).

**D5. Signature domain separation.** `chain-id || domain || payload`. A testnet
signature is invalid on mainnet; a vote is not valid as a transaction.

**D6. Security before convenience.** Slashing/evidence/DoS/threat model first,
then UX. A wallet without offline signing is not considered ready.

**D7. Hybrid PQ horizon.** Classical crypto (Ed25519/secp256k1) at launch, but
address/signature formats are versioned for ML-DSA (FIPS 204) and SLH-DSA
(FIPS 205), finalized by NIST on 2024-08-13
[7](https://csrc.nist.gov/news/2024/postquantum-cryptography-fips-approved).
Not "set and forget" — a migration path in `SECURITY_PLAN` H2.

**D8. One patch = one phase.** Name `patches/NNNN-<PHASE>-<slug>.patch`,
containing code + tests + NC + docs + CHANGELOG. No "big commits".

**D9. Documentation is part of the gate.** A phase is not closed until its plan
row, `docs/STATUS.md` and `CHANGELOG.md` are updated in the same patch.

**D10. Honest economics.** No hidden premines and no "guaranteed returns". All
emission/allocation parameters live in genesis and the white paper and are
checkable by script (`sim/`).

**D11. The legal perimeter is not code.** (⏸ Stage 2 per Q5; mandatory before
mainnet.) A public offer of a crypto-asset in the EU requires a crypto-asset
white paper notified to the competent authority at least 20 working days in
advance under Regulation (EU) 2023/1114 (MiCA), Art. 4/6/8/9
[2](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX%3A32023R1114),
[3](https://www.amf-france.org/en/news-publications/depth/mica).
This is **not legal advice**: T8/O6 needs a lawyer in your jurisdiction.

**D12. "Real" means reproducible.** Anyone must be able to build the node from
source, verify the binary hash and arrive at the same state root. Otherwise the
network is only real on paper.

---

## 2. Plan ledger

Status of every row on 2026-10-25: **📋 PLAN (not started)**. Status changes
only inside a phase patch, never "just because". Each plan has an `**Stage:**`
line splitting Stage 1 / ⏸ Stage 2; deferred phase rows are marked `⏸` in the
phase tables (decision Q5).

| Plan | Status | Scope | Phases |
|---|---|---|---|
| [`docs/plans/CORE_PLAN.md`](docs/plans/CORE_PLAN.md) | ✅ COMPLETE (C0–C10 ✅) | primitives, codec, crypto, tx/block, genesis, test rig | C0–C10 |
| [`docs/plans/STATE_PLAN.md`](docs/plans/STATE_PLAN.md) | 📋 PLAN | Merkle state, RocksDB, WAL, snapshots, pruning, recovery | S0–S7 |
| [`docs/plans/EXEC_PLAN.md`](docs/plans/EXEC_PLAN.md) | 📋 PLAN | WASM runtime, gas, ABI, contract SDK, determinism | X0–X9 |
| [`docs/plans/CONSENSUS_PLAN.md`](docs/plans/CONSENSUS_PLAN.md) | 📋 PLAN | BFT-PoS, QC, VRF, staking, slashing, finality, governance | B0–B10 |
| [`docs/plans/NET_PLAN.md`](docs/plans/NET_PLAN.md) | 📋 PLAN | libp2p, gossipsub, Kademlia, sync, light client, chaos tests | N0–N8 |
| [`docs/plans/NODE_PLAN.md`](docs/plans/NODE_PLAN.md) | 📋 PLAN | `aurad`, mempool, RPC/WS/gRPC, metrics, CLI, devnet | D0–D8 |
| [`docs/plans/WALLET_PLAN.md`](docs/plans/WALLET_PLAN.md) | 📋 PLAN | keys, keystore, offline signing, multisig, web wallet | W0–W7 |
| [`docs/plans/ECOSYSTEM_PLAN.md`](docs/plans/ECOSYSTEM_PLAN.md) | 📋 PLAN | indexer, explorer, faucet, SDK, DEX, stablecoin, NFT, bridge | E0–E9 |
| [`docs/plans/ECONOMICS_PLAN.md`](docs/plans/ECONOMICS_PLAN.md) | 📋 PLAN | tokenomics, fees, staking, treasury, voting, white paper | T0–T8 |
| [`docs/plans/SECURITY_PLAN.md`](docs/plans/SECURITY_PLAN.md) | 📋 PLAN | threat model, TLA+, fuzzing, audit, bug bounty, incidents | H0–H9 |
| [`docs/plans/PERF_PLAN.md`](docs/plans/PERF_PLAN.md) | 📋 PLAN | benchmarks, profiling, parallelism, SLOs | P0–P6 |
| [`docs/plans/OPS_PLAN.md`](docs/plans/OPS_PLAN.md) | 📋 PLAN | CI, releases, devnet → testnet → mainnet, monitoring, docs | O0–O9 |

### Dependency order

```
CORE (C0-C10) ──> STATE (S0-S7) ──┬──> EXEC (X0-X9) ──┬──> NODE (D0-D8)
                                   │                    │
                                   └──> CONSENSUS (B0-B10) <── NET (N0-N8)
                                                  │
                    WALLET ──> ECOSYSTEM ──> ECONOMICS ──> OPS
                    SECURITY (H) and PERF (P) run in parallel with CORE,
                    but their gates are mandatory before O5 (mainnet).
```

Clarification: B1–B3 use an **in-process transport** (votes inside the
simulator/devnet), so NET does not block the first consensus phases; real
libp2p (N1–N2) connects at B5+ and is mandatory for M1. This is the same trick
as the `--kernel=` lane in AuraLite-Emulator: isolated stand first, real
machine later.

Critical path: `C0 → C4 → S2 → X2 → B3 → N4 → D7 → O4 → H8 → O6`.

---

## 3. Standing rules (per `CORE_PLAN` §2/§5)

1. **Measure before claiming.** A defect/regression/number is filed only with a
   reproducible log in `docs/evidence/`.
2. **Fix + regression vector in one patch**, with one NC per phase (revert the
   fix → test goes red).
3. **`make test` green at every phase boundary** — unit, property, integration,
   determinism, sanitizers (Miri/ASan where applicable), `cargo clippy -D warnings`.
4. **Consensus code does not change "along the way":** a tx/block/state-root
   format change = new phase + migration test + protocol version.
5. **Docs sync is part of DoD** (plan row + `STATUS.md` + `CHANGELOG.md`, same
   patch).
6. **No secrets in the repository:** genesis validator keys are generated
   offline, only public parts land in the tree; CI must not sign releases.
7. **All external dependencies pinned**, `cargo deny` and SBOM in CI.
8. **Never present testnet as mainnet.** Until O6 only the testnet token without
   market value is published.

---

## 4. Audit 2026-10-25 — state of the tree (measured)

| What | Value | How checked |
|---|---|---|
| Rust crates | 0 | `find . -name Cargo.toml` |
| Lines of Rust | 0 | `find . -name '*.rs' \| xargs wc -l` |
| Tests | 0 | `cargo test` (does not exist yet) |
| Patches | 0 | `ls patches/` (directory does not exist yet) |
| Plans | 12 | `ls docs/plans/*_PLAN.md \| wc -l` |
| Validators | 0 | no network |
| `make test` | ❌ does not exist | `make test` → no Makefile |

This is a normal baseline: the plan is written **before** code so that every
later number has a reference point. C0 records this audit in
`docs/evidence/c0-baseline.txt`.

**Post-C0 measurement (2026-10-25, same commands):** 11 crates, 12 `.rs`
files, `cargo test --workspace` → 5 passed / 0 failed, `make test` → ALL GATES
GREEN, `make nc-c0` → 2/2 negative controls PASS. Validators/blocks/tx are
still 0/0/0. The pre-C0 zero table above stays as the historical reference —
it is deliberately not rewritten.

**Post-C1 measurement (2026-10-25):** 17 `.rs` files, 17 tests passing,
`make nc-c1` → 3/3 negative controls PASS, still zero external dependencies.
Receipts: `docs/evidence/c1/`.

**Post-C2 measurement (2026-10-25):** 22 `.rs` files, 28 tests passing,
`make nc-c2` → 4/4 negative controls PASS, 0 external dependencies (one
internal path dependency). Receipts: `docs/evidence/c2/`.

**Post-C3 measurement (2026-10-25):** 28 `.rs` files, 39 tests passing,
`make nc-c3` → 5/5 negative controls PASS, 54 external crates (exact pins,
checksummed, allowlisted via `make check-deps`). Receipts:
`docs/evidence/c3/`. Note: C3 is where the "zero external dependencies" line
ends — deliberately, with a new gate rather than silently.

**Post-C4 measurement (2026-10-25):** 12 crates (new `aura-tx`), 34 `.rs`
files, 49 tests passing, `make nc-all` → **20/20 negative controls PASS**,
6/6 new in C4, still 54 external crates (no new dependency), drift guard now
covers 20 markdown files. Receipts: `docs/evidence/c4/`. Validators/blocks/tx
are still 0/0/0 — C4 is a format, not a network.

**Post-C5 measurement (2026-10-25):** 13 crates (new `aura-block`), 41 `.rs`
files, 60 tests passing, `make nc-all` → **26/26 negative controls PASS**,
6/6 new in C5, still 54 external crates (no new dependency), drift guard now
covers 21 markdown files. Receipts: `docs/evidence/c5/`. Validators/blocks/tx
are still 0/0/0 — C5 is a format, not a network.

**Post-C6 measurement (2026-10-25):** still 13 crates (proof module inside
`aura-block`), 43 `.rs` files, 71 tests passing, `make nc-all` → **32/32
negative controls PASS**, 6/6 new in C6, still 54 external crates, drift guard
now covers 22 markdown files. The 1000-leaf DoD gate verifies all proofs and
rejects one substituted leaf. Receipts: `docs/evidence/c6/`.

**Post-C7 measurement (2026-10-26):** still 13 crates (`aura-state` now has a
real API; 7 empty placeholders remain), 47 `.rs` files, 82 tests passing,
`make nc-all` → **38/38 negative controls PASS**, 6/6 new in C7, still 54
external crates, drift guard now covers 23 markdown files. DoD gates: 1000
addresses → 5000 distinct keys/digests, all 256 prefixes checked with only 5
reachable. Receipts: `docs/evidence/c7/`.

**Post-C8 measurement (2026-10-26):** 14 crates (new `aura-genesis`), 54 `.rs`
files, 96 tests passing, `make nc-all` → **44/44 negative controls PASS**,
6/6 new in C8, 58 external crates (+4 recorded in a reviewed addendum),
drift guard now covers 24 markdown files. DoD: three JSON representations →
one hash; validator order change → different hash; `aura genesis` CLI agrees
with the library. Receipts: `docs/evidence/c8/`.

**Post-C9 measurement (2026-10-26):** 15 crates (new `aura-testkit`), 63
`.rs` files (including 4 libFuzzer targets), 109 tests passing,
`make nc-all` → **50/50 negative controls PASS**, 6/6 new in C9, 71 external
crates (+13 dev-only), drift guard now covers 26 markdown files. **956
golden vectors** across 9 fixtures (>= 200 required); 60 s portable fuzz
over 6 decoders with zero crashes; 4 `libfuzzer-sys` source targets ready for
H3's 24-hour campaign. Receipts: `docs/evidence/c9/`.

**Post-C10 measurement (2026-10-26):** `CORE_PLAN` is **closed**. 11 phase
receipts, 11 numbered patches, 56 negative controls, 109 tests, 956 vectors,
71 external crates, 15 crates (8 implemented). `make audit-closeout` proves
no ✅ row lacks a receipt, patch, NC log or verified evidence. Index:
`docs/spec/FORMAT_INDEX.md`; maturity: [`docs/CLOSEOUT_AUDIT.md`](docs/CLOSEOUT_AUDIT.md);
NC inventory: `docs/evidence/c10/closeout.txt`. Receipts:
`docs/evidence/c10/`. Validators/blocks/tx are still 0/0/0 — M0 also needs
S0–S2 and X0–X2.

### Measured gaps to the north star

| # | Gap | Evidence in the tree | Plan |
|---|---|---|---|
| 1 | No transaction, block or address format | no `crates/aura-types` | CORE C1–C5 |
| 2 | No state and no state root | no `aura-state` | STATE S1–S7 |
| 3 | No contract execution | no `aura-exec` | EXEC X1–X8 |
| 4 | No consensus and no validators | no `aura-consensus` | CONSENSUS B1–B10 |
| 5 | No P2P and no sync | no `aura-p2p` | NET N1–N8 |
| 6 | No node, RPC or wallet | no `aurad`/`aura-wallet` | NODE, WALLET |
| 7 | No applications or economics | no `contracts/`, `sim/` | ECOSYSTEM, ECONOMICS |
| 8 | No security evidence | no `fuzz/`, audit | SECURITY, PERF, OPS |

The gap table above is the **pre-C0 snapshot**, kept as a historical
reference. For the current tree see the post-C0/C1/C2/C3/C4 measurements
above and [`docs/STATUS.md`](docs/STATUS.md).

---

## 5. Milestones

| Milestone | Stage | Composition | Exit criterion (measurable) |
|---|---|---|---|
| **M0 — Genesis lib** | ✅ Stage 1 | C0–C10, S0–S2, X0–X2 | `make test` green; genesis → 1000 tx → byte-identical state root on 2 runs |
| **M1 — Devnet** | ✅ Stage 1 | +B0–B4, N0–N4, D0–D7 | 4 validators, 1 block/s, 100 blocks without desync, RPC returns a balance |
| **M2 — Public testnet** | ✅ Stage 1 | +B5–B10, X3–X8, W0–W4, E0–E3, T0–T4, H0–H7, P0–P3, O0–O4 | >= 10 external validators, explorer, faucet, 7 days uptime, 0 state losses |
| **M3 — Mainnet candidate** | ⏸ Stage 2 | +E4–E8, T5–T8, H8–H9, O5 | external audit closed, 0 criticals, reproducible build, white paper |
| **M4 — Mainnet** | ⏸ Stage 2 | O6 | >= 21 validators, genesis ceremony, 24/7 monitoring, incident playbook |
| **M5 — Ecosystem** | ⏸ Stage 2 | X9, W5–W7, E9, P4–P6, O7–O9 | first on-chain governance upgrade, bridge, published SLOs |

This table is the single source of truth for milestone composition.
Stage 1 ends at M2. Milestones are not dates; dates appear in `CHANGELOG.md`
after the fact, as in AuraLite OS.

---

## 6. Next-block queue (dependency-ordered)

1. **CORE_PLAN C0–C4** — workspace, CI, canonical types, `aura-codec`, crypto,
   tx format. Nothing works without it: block hash, signature, QC, address all
   depend on C1–C4. First deliverable:
   `patches/0001-C0-baseline-workspace.patch`.
2. **STATE_PLAN S1–S2** — state tree and RocksDB: Execution and Consensus get a
   shared `state_root`.
3. **EXEC_PLAN X1–X2** — wasmtime in a sandbox + gas + ABI.
4. **CONSENSUS_PLAN B1–B3** — PoA → QC → BFT pipeline.
5. **NET_PLAN N1–N4 + NODE_PLAN D1–D7** — devnet (M1): the first network you
   can touch with your hands.
6. **WALLET + ECOSYSTEM E0–E3** — so people other than the authors can use it.
7. **ECONOMICS T0–T4 + SECURITY H0–H7 + PERF P0–P3 + OPS O0–O4** — public
   testnet (M2), end of Stage 1. Mainnet/audit/white paper are ⏸ Stage 2, by
   separate decision (Q5).

---

## 7. Reality check (honestly)

- Scale: this is a **multi-person-year** project for one developer (consensus +
  VM + P2P + clients + applications + audit). In your style: 100 small measured
  phases beat one "big release".
- Bottlenecks **not accelerated by code**: external audit (weeks-months and
  money), recruiting independent validators, the legal perimeter, liquidity.
- That is why the plan has separate organizational gates (O5, O6, T8, H8), not
  only technical ones. Skip them and you get a "real network" only in `README`.
- No phase promises a token price, a listing or returns. That is outside
  technical control and is honestly kept outside the north star.
- By owner decision Stage 1 **issues no market token**: in testnet `ALCRY` is a
  technical unit with no value. This removes the legal risk of a premature
  public offer, but does not cancel T8/O5 in Stage 2.

---

## 8. Immediate queue

**C0, C1, C2, C3, C4, C5, C6, C7, C8, C9 and C10 shipped**
(`patches/0001-C0-baseline-workspace.patch`,
`patches/0002-C1-types.patch`, `patches/0003-C2-codec.patch`,
`patches/0004-C3-crypto.patch`, `patches/0005-C4-tx-format.patch`,
`patches/0006-C5-block-header.patch`, `patches/0007-C6-merkle-proofs.patch`,
`patches/0008-C7-account-model.patch`, `patches/0009-C8-genesis.patch`,
`patches/0010-C9-test-rig.patch`, `patches/0011-C10-close-out.patch`):
workspace, CI, gates, identity constants, canonical types, codec, crypto,
tx format, block format, Merkle proofs, accounts, genesis, a wired test rig
and the plan↔tree close-out audit — all pinned against independent references
and guarded by **56 negative controls**. `CORE_PLAN` is closed; the maturity
level is **L1 (core libraries + test rig, no network)**. Right now:
**S0** — `STATE_PLAN` baseline: an in-memory state, a measured "no root / no
persistence" report, and NC documenting that two nodes with different
insertion order produce different balances. M0 also needs S1–S2 and X0–X2.
The next patch is `patches/0012-S0-state-baseline.patch`.

See also: [`docs/STATUS.md`](docs/STATUS.md),
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md),
[`docs/TICKER_AUDIT.md`](docs/TICKER_AUDIT.md),
[`CHANGELOG.md`](CHANGELOG.md), [`CONTRIBUTING.md`](CONTRIBUTING.md).
