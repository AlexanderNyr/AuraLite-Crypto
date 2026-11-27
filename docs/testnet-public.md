# AuraLite public testnet (M2) — operator and developer guide

The public testnet is the M2 milestone (`ROADMAP.md` §5): faucet,
explorer, docs, bug bounty, public metrics and >= 10 external
validators, measured over a 30-day window (>= 99% uptime, >= 100 000
transactions, 0 unfixed criticals). The measurements are receipts, not
adjectives: `release/testnet.txt`, `release/metrics-30d.txt` and
`release/triage.txt`, checked by `aura-testnet` (`rule testnet-v1`).

## Faucet

`aurad-faucet` (`crates/aura-node/src/bin/aurad-faucet.rs`) is the
public faucet service over the node `RpcBackend`. It reads
`payout <bech32-address> <units> <nonce>` request lines and pays each
one through `submit_tx`, subject to a deterministic per-run budget
(`--limit`):

```
$ aurad-faucet --limit 1000 --requests requests.txt
payout aura1... 100 ok
payout aura1... 100 refused: faucet limit exceeded
```

The faucet key never leaves the node; payouts are ordinary
`build_faucet_transfer` transactions.

## Explorer

`aurad-explorer` (`crates/aura-node/src/bin/aurad-explorer.rs`) is the
public block explorer over the same `RpcBackend`. It reads query lines
(`head`, `block <height>`, `tx <txid>`, `balance <bech32>`) and prints a
deterministic one-line answer per query:

```
$ aurad-explorer --queries queries.txt
head height=12 txs=1680
block 3 txs=140 root=...
balance aura1... 9998320
```

## Public metrics

The metrics ledger `release/metrics-30d.txt` is published with every
release: one `metrics <d> <h> <height> <txs_total> <root-hex-64>` row
per operator poll. `aura-testnet metrics` re-derives
`metrics ledger: days=30 uptime=100 txs=100800` and refuses any
regressing window (`tx regression at <d> <h>`). The on-call runbook
(`docs/oncall-runbook.md`) defines the alert thresholds.

## Bug bounty

The bounty program is `docs/testnet-bounty.md` (it inherits the H8
program in `SECURITY.md`). Findings are triaged in the public journal
`release/triage.txt`; the published gate is **0 unfixed criticals** —
`aura-testnet triage` refuses the window while any open critical exists
(`open critical <id>`).

## Validators

Joining the validator set is an organizational process: run a node from
the pinned release, prove your genesis root matches the published one
(the ten-validator window of `aurad-testnet` is exactly this check), and
get invited by the on-call operators. There is no on-chain validator
admission before B4/B5 governance lands (Stage 2).

## Reproducing the receipts

```
make testnet-receipts   # aurad-testnet measures the window
make testnet-check      # aura-testnet check/metrics/triage validate it
```
