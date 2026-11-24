# E5 — Oracle: a decentralized price-feed network

Date: 2026-11-22. Phase **E5** per `docs/plans/ECOSYSTEM_PLAN.md`
(decision **D-E3**: the DeFi order — after the oracle and the audit; the H8 audit is closed,
so the decentralized phase after E3 is E5, then E4 AMM).

## Phase DoD

- ✅ Feed network: registered providers with ed25519 keys;
- ✅ Every report is signed by its provider (canon: `feed16 || price16BE || ts8BE`);
- ✅ Median of admitted prices (lower median — deterministic for even lengths);
- ✅ Quorum: N/2+1 (minimum 3); a single provider → red (named message);
- ✅ Staleness: a report must lie strictly within `(now − 3600s, now]`; future ones are rejected;
- ✅ Outlier band: ±500 bps from the preliminary median → the final median.

## The `crates/aura-storage/src/oracle.rs` model

- `Price = u128` (18 decimals, `E5_ONE = 10^18` — decision D-T1);
- `PriceReport::sign/verify` — ed25519 (`aura_crypto::sign_raw/verify_raw`);
- `OracleNetwork::submit(now, reports)`:
  1. **strict admission**: unknown provider, duplicate provider in a round,
     foreign feed, bad signature, `ts <= now − 3600` or `ts > now` → `Err`
     with a named message (7 of them — see below);
  2. **quorum**: `admitted.len() >= quorum` (quorum = N/2+1, minimum 3);
  3. **aggregation**: `provisional = lower_median(admitted.prices)`,
     survivors = `|p − provisional| * 10000 <= 500 * provisional`,
     `price = lower_median(survivors.prices)`,
     `timestamp = lower_median(survivors.timestamps)`.
- Constants: `E5_MAX_STALENESS_SECS = 3600` (Chainlink heartbeat),
  `E5_MIN_QUORUM = 3`, `E5_DEVIATION_BPS = 500` (0.5% — the deviation
  override of Chainlink), `E5_ONE = 10^18`.

### Named messages (for `tools/nc_e5.py`)

1. `E5: round requires at least one report`;
2. `E5: report from unregistered provider {provider}`;
3. `E5: one report per provider per round (provider {provider})`;
4. `E5: all reports must target this feed (provider {provider})`;
5. `E5: provider report signature must verify (provider {provider})`;
6. `E5: provider report must be fresh (provider {provider}, ts {timestamp}, now {now})`;
7. `E5: oracle requires a quorum of provider reports ({valid}/{required})`.

## Gate and metamorphisms

The gate `crates/aura-storage/tests/e5_oracle.rs` (14 tests):

- **literal gate**: 5 providers, 2 malicious (a x100 pump and a
  0.0001 dump) → the price stays **100.0** (the lower median of the honest 99.9/100.0/100.1)
  ; both outliers are dropped;
- **NC**: a single provider → `E5: oracle requires a quorum of provider
  reports (1/3)`;
- rejections: bad signature, stale (ts = now−3600−1), future (ts = now+1),
  duplicate provider, foreign feed, unknown provider, empty round;
- outlier: 5 reports with one x100 outlier → 1 rejected, price 101.0;
- **metamorphisms**: permuting reports does not change the aggregate; scaling
  all prices ×2 → median ×2; repeating the same round → the same aggregate.

Simulation `sim/e5-oracle.json` + `crates/aura-storage/examples/e5_oracle_example.rs`
(→ `make examples-e5`): `E5-SIM-OK {"providers": 5, "admitted": 5,
"rejected_outliers": 2, "price": "100000000000000000000", "timestamp":
999989, "quorum": 3, "staleness_secs": 3600}`. The independent Python model
`tools/e5_oracle_check.py` (→ `make check-e5`) confirms the same numbers
(`E5-MODEL-OK`). The aggregate digest:
`aee332b61c43f19d234ec79044208daa4bb1671ec1e8126b5d7ff75a4bdead24`
(canon `E5|feed=ALCRY/USD.......|price=100000000000000000000|admitted=5|
rejected=2|quorum=3|staleness=3600|deviation=500`).

## Negative controls (`tools/nc_e5.py`, 6/6)

Each mutation weakens exactly one named message (anchor counter == 1),
runs `cargo test -p aura-storage --test e5_oracle` and requires red;
`finally` restores the original bytes → green:

| # | Mutation | Expectation |
|---|---------|----------|
| NC-1 | quorum message without numbers | red |
| NC-2 | duplicate-provider message without id | red |
| NC-3 | unknown-provider message without "unregistered" | red |
| NC-4 | feed-mismatch message simplified | red |
| NC-5 | staleness message without ts/now | red |
| NC-6 | empty-round message simplified | red |

Result: `NC-GREEN restored: 6/6 mutations turned the gate red`.

## Phase decisions

- **D-E3** (from ECOSYSTEM_PLAN): the DeFi order — oracle → audit → AMM/CDP/
  lending; the H8 audit is closed, so E5 goes before E4. The order is unchanged.
- Median (not mean): robust to outliers; lower median — determinism.
- Quorum N/2+1: a Byzantine quorum (2 malicious of 5 do not move the median
  and do not block finalization — 3 honest ones suffice).
- Staleness 3600s + deviation 500 bps: the heartbeat/deviation-override
  parameters of Chainlink price feeds.

## Sources (web, 2026-11-22)

- Chainlink DON: N = 11–31 operators, off-chain aggregation (OCR 2.0),
  quorum N/2+1 signatures, heartbeat 3600s and deviation 0.5% (min responses),
  outlier removal/trimmed mean; threshold signatures t-of-n (f < n/3).
  Sources: bex.co/course/defi/6.1; everstake.com/resources/blog/
  chainlink-vs-pyth-oracle-comparison; everstake.com/resources/blog/
  what-is-blockchain-oracle; defi-explained.dev/oracles/;
  bumba.global/assets/whitepaper/Chainlink.pdf.
- Pyth: first-party publishers, a pull model (~400ms) — for architectural comparison.
- UMA: optimistic oracle (contestable answers) — for architectural comparison.

## Evidence

The tier `docs/evidence/e5/` (baseline.log, e5-oracle.log, campaign/,
gates.txt, inventory.txt, nc.log, nc-suite.log, report.md, SHA256SUMS) —
see `tools/write_e5_evidence.py`.
