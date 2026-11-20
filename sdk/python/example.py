"""AuraLite SDK example (Python): same demo as the Rust and TypeScript
examples. Usage: python3 example.py http://127.0.0.1:PORT
"""

import sys

from aura_lite import AuraLiteClient

DAY = 86_400


def main():
    base = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8080"
    client = AuraLiteClient(base)

    first = client.faucet_request(3600, "testnet", "alice")
    assert '"outcome":"paid"' in first.body, "first request must pay"
    repeat = client.faucet_request(4000, "testnet", "alice")
    assert '"outcome":"denied_rate_limit"' in repeat.body, "same-day repeat must be denied"
    next_day = client.faucet_request(DAY + 3600, "testnet", "alice")
    assert '"outcome":"paid"' in next_day.body, "next day must pay"
    wrong_net = client.faucet_request(DAY + 4000, "mainnet", "bob")
    assert '"outcome":"denied_wrong_network"' in wrong_net.body, "mainnet must be refused"
    status = client.faucet_status()
    assert '"payouts":2' in status.body, "two payouts in the ledger"

    print('E3-EXAMPLE-OK {"lang":"python","payouts":2,"denied_rate_limit":1,"denied_wrong_network":1}')


if __name__ == "__main__":
    main()
