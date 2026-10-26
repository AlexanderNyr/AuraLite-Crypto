# AuraLite-Crypto

**A from-scratch L1 blockchain and cryptocurrency ecosystem in Rust.**

- Project / repository: **`AuraLite-Crypto`**
- Network: `AuraLite-Crypto Network`, chain-id `auralite-test-1` (M1/M2),
  `auralite-1` reserved for mainnet (Stage 2)
- Native asset: **`ALCRY`** (18 decimals, smallest unit `aalcry`)
- Internal crate prefix / bech32 addresses: `aura-*` / `aura1...`

Name and ticker were locked by the owner on 2026-10-25. The ticker was checked
for collisions — see [`docs/TICKER_AUDIT.md`](docs/TICKER_AUDIT.md).

> **Status: CORE COMPLETE (C10, 2026-10-26).** Eleven phases shipped:
> types, codec, crypto, tx, block, Merkle proofs, accounts, genesis, test rig
> and a plan↔tree close-out audit. Every ✅ phase row now has a patch, a
> receipt, a CHANGELOG entry, an NC log and evidence with verified checksums —
> **56 negative controls**, 109 tests, 956 vectors. See
> [`docs/CLOSEOUT_AUDIT.md`](docs/CLOSEOUT_AUDIT.md) for what is real and
> what is not: there is still **no consensus, VM, P2P, node, wallet, state
> tree or network**. M0 also needs STATE S0–S2 and EXEC X0–X2. Everything
> else below is a plan, not a report about an implementation. Every claim of
> readiness appears only after a measurable test gate, as in
> [AuraLite-Emulator](https://github.com/AlexanderNyr/AuraLite-Emulator) and
> [AuraLite-OS](https://github.com/AlexanderNyr/AuraLite-OS).

---

## North star

```
AuraLite-Crypto mainnet (long-term goal, Stage 2 — a separate decision):
  >= 21 independent validators, no single entity controls >= 1/3 of the stake,
  finality < 2 s with 100+ nodes,
  wallet + explorer + DEX + staking + bridge + contracts from this repository,
  external audit, reproducible builds, published measured TPS.

Stage 1 (ACTIVE, current focus): M0-M2 — core, devnet, public testnet.
```

This is **not** "a token on someone else's chain" and not a wrapper around an
existing node. It is our own consensus, state machine, transaction format, P2P,
wallet, explorer and contract standards — in Rust.

## What is in the ecosystem

| Layer | Crate(s) | Provides | Plan |
|---|---|---|---|
| Primitives | `aura-types`, `aura-codec`, `aura-crypto` | hashes, addresses, signatures, deterministic codec | `CORE_PLAN` |
| State | `aura-state`, `aura-storage` | versioned Merkle tree, RocksDB, snapshots, pruning | `STATE_PLAN` |
| Execution | `aura-exec`, `aura-sdk`, `contracts/` | deterministic WASM runtime, gas, ABI, contract SDK | `EXEC_PLAN` |
| Consensus | `aura-consensus`, `aura-staking` | BFT-PoS, QC, VRF, slashing, finality, on-chain upgrades | `CONSENSUS_PLAN` |
| Network | `aura-p2p`, `aura-sync` | libp2p/gossipsub/Kademlia, sync, light clients | `NET_PLAN` |
| Node | `aurad`, `aura-rpc`, `aura-mempool`, `aura` (CLI) | node binary, RPC/WS/gRPC, metrics, one-command devnet | `NODE_PLAN` |
| Clients | `aura-wallet`, `aura-keys`, `wallet-web` | BIP-39/32/44, keystore, offline signing, multisig, hardware keys | `WALLET_PLAN` |
| Applications | `aura-indexer`, `aura-explorer`, `faucet`, `bridge`, `defi/` | explorer, faucet, DEX, stablecoin, oracles, lending, NFT, DAO | `ECOSYSTEM_PLAN` |
| Economics | `aura-economics`, `sim/` | emission, fees, staking, treasury, governance, white paper | `ECONOMICS_PLAN` |
| Security | `fuzz/`, `tests/`, `spec/` | threat model, TLA+, fuzzing, audit, bug bounty, incidents | `SECURITY_PLAN` |
| Performance | `bench/` | Criterion benches, profiles, SLOs, honest numbers | `PERF_PLAN` |
| Operations | `deploy/`, `.github/workflows/` | CI, releases, testnet, mainnet, monitoring, docs | `OPS_PLAN` |

## Architecture at a glance

```
        wallet / dApp / SDK
                |
        JSON-RPC / WS / gRPC              <- aura-rpc
                |
   +------------ aurad --------------------------------+
   |  mempool -> exec(WASM) -> state overlay -> root   |
   |       ^          |              |                |
   |       |      consensus(BFT PoS / QC / VRF)        |
   |       |          |              |                |
   |     p2p <---- votes/blocks --> storage(RocksDB)   |
   +---------------------------------------------------+
                |
     indexer -> Postgres -> explorer / analytics
```

Details: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Quickstart

**Now (C10)** the CORE plan is closed and auditable:

```sh
make deps-check          # rustc / cargo / rustfmt / clippy
make test                # full gate including the close-out audit
make audit-closeout      # no ✅ row without a patch/receipt/NC log/evidence
make nc-all              # all 56 negative controls (C0-C10)
make fuzz-smoke          # 60 s fuzz over 6 decoders, 0 crashes
make vector-inventory    # 956 golden vectors
```

**The devnet does not exist yet — and that is honest.** After phase D7 (devnet)
it will look like this:

```sh
make deps-check          # rustc, cargo, clang, protoc, docker
make test                # unit + property + fuzz-smoke + integration
make devnet              # 4 validators, 1 block/s, one terminal
aura wallet new alice
aura tx transfer --to bob --amount 1.5ALCRY --sign alice
aura query balance alice
```

Until C0/D7 these commands do not exist; their appearance is a measurable gate,
not a promise.

## Repository layout

```
AuraLite-Crypto/
  README.md              <- this file: what we are building
  ROADMAP.md             <- north star, plan ledger, decisions, rules, audit
  CHANGELOG.md           <- what actually changed (ISO 8601 dates, Europe/Moscow)
  CONTRIBUTING.md        <- how to land a phase: one patch, one gate, one NC
  Makefile               <- single entry point (C0 OK)
  Cargo.toml             <- 15 crates (C9), 71 pinned external deps
  rust-toolchain.toml    <- pinned 1.99.1 + rustfmt/clippy (C0 OK)
  tools/                 <- docs drift guard + negative-control drivers (C0 OK)
  docs/
    STATUS.md            <- measured current state
    ARCHITECTURE.md      <- invariants and data flow
    TICKER_AUDIT.md      <- measured ticker collision audit
    plans/*_PLAN.md      <- 12 plans, dependency-ordered, DoD + gate per phase
    evidence/            <- gate logs / receipts (C0-C10)
    spec/                <- C4-C9 specs + FORMAT_INDEX.md, config, allowlists
  patches/               <- one .patch per phase (0001-C0 ... 0011-C10)
  crates/*               <- 15 crates: 8 real APIs (C9) + 7 empty placeholders
  contracts/*            <- reference contracts (ARC-20, ARC-721, AMM, ...)
  tests/                 <- unit/property/integration/consensus/soak
  fuzz/                  <- 4 libFuzzer source targets (C9; H3 runs 24 h)
  sim/ bench/ deploy/ web/
```

## Principles (inherited from your repositories)

1. **Measured, not claimed.** A TPS number, a finality number or "audit passed"
   appears only together with a gate log in `docs/evidence/`.
2. **One phase = one patch = DoD + test gate + negative control.**
3. **`make test` green at every phase boundary.** A gate that cannot go red is
   not a gate: every phase gets an NC (revert the fix → test goes red).
4. **Docs sync in the same patch** as the code: plan row, `docs/STATUS.md`,
   `CHANGELOG.md`.
5. **Honest scope.** Partial work is labelled partial; "experimental" is never
   sold as "stable". See the `MATURITY_AUDIT`-style audits in `SECURITY_PLAN`.

## Reality check

Code ≠ "a real economy". A real network needs validators, users, liquidity and
a legal perimeter. The plan covers the technical and organizational part;
exchange listings, token price and market adoption **are not guaranteed by any
phase** and are not counted in gates (see `ECONOMICS_PLAN` T8 and `OPS_PLAN`
O9).

**Owner decision (2026-10-25):** the first stage is devnet + public testnet
(M0–M2), **with no publicly traded token**. Mainnet, white paper and external
audit phases (O5–O9, T5–T8, H8–H9) stay in the plan but are marked ⏸ Stage 2
and start by a separate decision.

## Relationship to AuraLite-OS / AuraLite-Emulator

An optional but native-to-the-ecosystem track (not the critical path): an
`ALCRY` light client inside AuraLite OS (it already has TCP/TLS 1.3 and the
`X25519MLKEM768` PQ hybrid), a CI run of `aurad` inside AuraLite-Emulator, and
a hardware keystore on AuraLite OS. See `ECOSYSTEM_PLAN` E9.

---

Next: [`ROADMAP.md`](ROADMAP.md) → [`docs/STATUS.md`](docs/STATUS.md) →
[`docs/plans/CORE_PLAN.md`](docs/plans/CORE_PLAN.md).
