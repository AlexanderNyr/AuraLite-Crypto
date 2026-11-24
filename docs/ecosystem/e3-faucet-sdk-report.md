# E3 report — Faucet + SDK (2026-11-19)

Phase **E3** per ECOSYSTEM_PLAN (the "Faucet + SDK" row): a rate-limited
faucet (testnet only), a Rust/TS/Python SDK, a docs site. The phase gate
is met literally: **1000 requests: 1 payout/address/day; SDK examples
pass in CI**.

## The gate (in detail)

- **1000 requests → 1 payout per address per day**: the gate script is
  250 addresses × 3 days (750 first requests) + 250 repeat requests
  on the same day = exactly 1000 requests. Result: **750 payouts, 250 rejections
  on rate limit, no (address, day) key paid twice**.
  The independent Python model (`tools/e3_faucet_check.py`) replays
  the same script and matches the numbers (`tools/fixtures/e3_faucet_script.json`).
- **SDK examples pass in CI**: three examples (Rust/TS/Python) run
  `tools/e3_examples_check.py` against the live reference faucet and
  print the same `E3-EXAMPLE-OK` summary: 2 payouts, 1 rejection on
  rate limit, 1 network rejection. **3/3 PASS.**

## Named messages of the gate

- `E3: faucet must pay 1 payout per address per day` (the gate literal)
- `E3: faucet must refuse non-testnet requests`
- `E3: faucet must stop when out of funds`
- `E3: the daily window must roll over`
- `E3: the faucet HTTP API must apply the same policy as the library`
- `E3: the faucet must validate addresses`

## Negative controls (6/6)

| # | Injected bug | Named message |
|---|---|---|
| NC-1 | the same-day repeat check removed (the gate literal) | `E3: faucet must pay 1 payout per address per day` |
| NC-2 | the testnet-only guard removed | `E3: faucet must refuse non-testnet requests` |
| NC-3 | the out-of-funds stop removed | `E3: faucet must stop when out of funds` |
| NC-4 | the daily window does not roll over | `E3: the daily window must roll over` |
| NC-5 | the HTTP route bypasses the policy | `E3: the faucet HTTP API must apply the same policy as the library` |
| NC-6 | address validation removed | `E3: the faucet must validate addresses` |

## Artifacts

| Artifact | Path |
|---|---|
| Faucet (deterministic policy + stdlib HTTP) | `crates/aura-storage/src/faucet.rs` |
| Rust SDK | `crates/aura-storage/src/sdk.rs` |
| TypeScript SDK | `sdk/ts/aura_lite.ts` (+ `sdk/ts/example.ts`) |
| Python SDK | `sdk/python/aura_lite.py` (+ `sdk/python/example.py`) |
| Gate test | `crates/aura-storage/tests/e3_faucet.rs` (`make test-e3`) |
| Policy oracle | `tools/e3_faucet_check.py` + `tools/fixtures/e3_faucet_script.json` |
| SDK examples in CI | `tools/e3_examples_check.py` (`make examples-e3`) |
| Docs site | `docs/sdk/index.md` |
| Evidence | `docs/evidence/e3/` (gates.txt, nc.log, campaign/result.json → `TOTAL: 512`) |

## Deviations from the plan (pinned by a plan-doc edit)

1. A payout is a faucet-ledger entry with fund debiting; an on-chain transfer
   is not performed while the W1-FORMAT-v1 testnet freeze is in force.
2. The demo HTTP protocol carries an explicit `now` (CI determinism).
3. The TS example runs in CI via deterministic type-strip + `node`
   (the TypeScript toolchain is not pinned in CI).
4. The docs site is static pages under `docs/sdk/` with no publishing pipeline.

The final counter: **512 negative controls cumulative** (506 + 6).
