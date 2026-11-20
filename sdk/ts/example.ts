// AuraLite SDK example (TypeScript): same demo as the Rust and Python
// examples. Usage: node example.mjs http://127.0.0.1:PORT

import { connect } from "./aura_lite";

async function main() {
  const base: string = process.argv[2] || "http://127.0.0.1:8080";
  const client = connect({ base: base });

  const first = await client.faucetRequest(3600, "testnet", "alice");
  if (!first.body.includes('"outcome":"paid"')) {
    throw new Error("first request must pay");
  }
  const repeat = await client.faucetRequest(4000, "testnet", "alice");
  if (!repeat.body.includes('"outcome":"denied_rate_limit"')) {
    throw new Error("same-day repeat must be denied");
  }
  const nextDay = await client.faucetRequest(86400 + 3600, "testnet", "alice");
  if (!nextDay.body.includes('"outcome":"paid"')) {
    throw new Error("next day must pay");
  }
  const wrongNet = await client.faucetRequest(86400 + 4000, "mainnet", "bob");
  if (!wrongNet.body.includes('"outcome":"denied_wrong_network"')) {
    throw new Error("mainnet must be refused");
  }
  const status = await client.faucetStatus();
  if (!status.body.includes('"payouts":2')) {
    throw new Error("two payouts in the ledger");
  }

  console.log('E3-EXAMPLE-OK {"lang":"ts","payouts":2,"denied_rate_limit":1,"denied_wrong_network":1}');
}

main();
