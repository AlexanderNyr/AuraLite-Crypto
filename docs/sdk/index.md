# AuraLite SDK & Faucet — docs site

One page for the E3 deliverables: the rate-limited faucet (testnet
only) and the client SDKs in Rust, TypeScript and Python. Every SDK
example runs in CI (`tools/e3_examples_check.py`).

## Faucet

- Policy: **1 payout per address per day**, testnet only, stops at zero
  funds. Addresses are 1..64 characters.
- HTTP API (`crates/aura-storage/src/faucet.rs`, stdlib server):
  - `POST /faucet` — body
    `{"now":<unix-seconds>,"network":"testnet"|"mainnet","address":"..."}`
    → `{"outcome":"paid","day":N,"payout":P}` or
    `{"outcome":"denied_rate_limit"|"denied_wrong_network"|"denied_out_of_funds"|"denied_bad_address"}`.
    The demo protocol carries an explicit `now` so CI stays
    deterministic.
  - `GET /faucet/status` → `{"funds":..,"payouts":..,"network":"testnet"}`.
- Deviations from the plan are recorded in
  `docs/plans/ECOSYSTEM_PLAN.md` → `### E3 receipt` (ledger payouts, no
  on-chain transfer while W1-FORMAT-v1 is frozen).

## SDKs

All three clients speak the same JSON protocol (explorer routes from
E2: `/blocks`, `/block/<h>`, `/tx/<hash>`, `/account/<addr>`,
`/search?q=`, `/health`; faucet routes above).

### Rust — `aura_storage::sdk`

```rust
use aura_storage::sdk::AuraLiteClient;
let client = AuraLiteClient::new("http://127.0.0.1:8080");
let (status, body) = client.faucet_request(3600, "testnet", "alice")?;
```

Example: `crates/aura-storage/examples/e3_sdk_example.rs`
(`cargo run --locked -p aura-storage --example e3_sdk_example`).

### TypeScript — `sdk/ts/aura_lite.ts`

```ts
import { connect } from "./aura_lite";
const client = connect({ base: "http://127.0.0.1:8080" });
const first = await client.faucetRequest(3600, "testnet", "alice");
```

Example: `sdk/ts/example.ts` (CI runs it via a deterministic type-strip
+ `node`, since no TypeScript toolchain is pinned in CI).

### Python — `sdk/python/aura_lite.py`

```python
from aura_lite import AuraLiteClient
client = AuraLiteClient("http://127.0.0.1:8080")
first = client.faucet_request(3600, "testnet", "alice")
```

Example: `sdk/python/example.py <base-url>`.

## CI gates

- `make test-e3` — the 1000-request faucet gate
  (`crates/aura-storage/tests/e3_faucet.rs`).
- `make check-e3` — the independent policy oracle
  (`tools/e3_faucet_check.py`).
- `make examples-e3` — SDK examples (rust/ts/python) against a live
  reference faucet (`tools/e3_examples_check.py`).
