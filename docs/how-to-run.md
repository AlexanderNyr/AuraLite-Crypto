# How to run — AuraLite-Crypto devnet (M0/M1)

This is the operator quickstart that ships with the O2 phase (the "how to
run" DoD). Everything here is measured by the milestone receipts
(`docs/o2-m0-m1-plan.md`); the numbers below come from the recorded run in
`docs/evidence/o2/milestone.txt`, not from aspiration.

## Prerequisites

- Rust **1.99.1** (pinned by `rust-toolchain.toml`), `make`, Python 3.
- No native services required for the devnet (RocksDB is vendored through
  `librocksdb-sys` only for the optional `s2-native` lane).

## Build

```sh
cargo build --release -p aura-node --bin aurad
cargo build --release -p aura-node --bin aurad-receipt
cargo build --release -p aura-testkit --bin aura-milestone
```

The release binary is reproducible: `make repro-check` builds `aurad` twice
into independent target directories and requires identical sha256 (O1).

## Run the 4-validator devnet (M1)

One process per validator; the devnet chain-id is `auralite-devnet-1`.
Validator keys are **devnet seeds** (`docs/evidence/d7/` shows the same
shape) — never reuse them outside a devnet.

```sh
mkdir -p devnet/validator-0 devnet/validator-1 devnet/validator-2 devnet/validator-3

# dev-only key material (the four fixed devnet seeds, 32 raw bytes each)
python3 - <<'PY'
for i, byte in enumerate((11, 22, 33, 44)):
    open(f"devnet/validator-{i}/validator.key", "wb").write(bytes([byte]) * 32)
PY

for i in 0 1 2 3; do
  AURA_DEVNET_DIR="$PWD/devnet" AURA_VALIDATOR_INDEX="$i" \
  ./target/release/aurad \
    --chain-id auralite-devnet-1 \
    --data-dir "$PWD/devnet/validator-$i" \
    --rpc-addr "127.0.0.1:$((26600 + i))" \
    --grpc-addr "127.0.0.1:$((26700 + i))" \
    --validator-key "$PWD/devnet/validator-$i/validator.key" \
    > "devnet/validator-$i.log" 2>&1 &
done
```

The full acceptance gate (4 processes, 100 blocks, identical heads and
`state_root`, RPC balance after a faucet transfer, survival of a 1-of-4
outage) is automated as `cargo test -p aura-node --test d7_devnet` — the
same flow the paragraphs above reproduce by hand.

## Query the RPC (M1: "RPC returns a balance")

```sh
curl -s http://127.0.0.1:26600 -d '{"jsonrpc":"2.0","id":1,"method":"get_balance","params":{"address":"<bech32 address>"}}'
curl -s http://127.0.0.1:26600 -d '{"jsonrpc":"2.0","id":1,"method":"get_block","params":null}'
```

`aurad balance <address> --rpc-addr …` wraps the same call.

## Measure the milestones (M0/M1 receipts)

```sh
make milestone-receipts   # aurad-receipt: genesis -> 1000 tx replay x2 + 4-validator run
make milestone-check      # aura-milestone check: the ROADMAP §5 criteria
```

M0 (genesis lib) is satisfied when the two replay state roots are
byte-identical after 1000 transactions; M1 (devnet) when the receipt shows
4 validators, >= 100 committed blocks, 0 desync, a positive RPC balance and
a block interval <= 1000 ms. A receipt that misses any criterion is
refused (exit 65) — including the named gate `m0 state root drift`.

## Full gate

```sh
make test        # everything, including the d7 multi-process devnet acceptance
make evidence-o2 # the O2 receipt bundle under docs/evidence/o2/
```
