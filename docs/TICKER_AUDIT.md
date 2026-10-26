# TICKER_AUDIT — choosing the AuraLite-Crypto ticker

> Date: **2026-10-25**. This is a measurement, not an opinion: every row below
> is the result of a concrete query. The audit was done **before** C1 so the
> ticker would not change after addresses and genesis exist.

## 0. Decision

| Field | Value |
|---|---|
| Project / repository | `AuraLite-Crypto` |
| Network | `AuraLite-Crypto Network` |
| **Native asset ticker** | **`ALCRY`** |
| decimals / smallest unit | 18 / `aalcry` (10^-18 ALCRY) |
| chain-id (Stage 1) | `auralite-test-1` |
| chain-id (Stage 2, reserved) | `auralite-1` |
| bech32 address prefix | `aura` (`aura1...`) |
| Crate prefix | `aura-*` (internal namespace, not tied to the ticker) |
| Reserve tickers | `AULTC`, `ALCRX`, `AURLT` |

`ALCRY` reads as **A**ura**L**ite **CRY**pto, five characters, easy to say,
and does not collide with the existing "Aura" projects.

## 1. Verification method

1. CoinGecko public search API: `GET /api/v3/search?query=<CANDIDATE>` —
   searches the **name and symbol** of coins, exchanges, ICOs, categories and
   NFTs.
2. Yahoo Finance Symbol Lookup for `<CANDIDATE>` — stocks, ETFs, funds,
   futures, currencies, cryptocurrencies, private companies.
3. Web search across aggregators (CoinMarketCap, CoinPaprika, BscScan,
   Etherscan, Kraken, Reuters, coin databases) for an exact string match.
4. The neighbouring `AUR*` / `ALC*` families were checked separately, because
   that is where most collisions live.

## 2. Raw results (receipts)

| Candidate | CoinGecko `/search` | Yahoo Finance lookup | Web search | Verdict |
|---|---|---|---|---|
| **ALCRY** | `{"coins":[],"exchanges":[],"icos":[],"categories":[],"nfts":[]}` | `No results for 'ALCRY'`, All/Stocks/ETFs/Crypto = **0** | no exact project/token matches found | ✅ **CHOSEN** |
| AULTC | `{"coins":[],"exchanges":[],"icos":[],"categories":[],"nfts":[]}` | `No results for 'AULTC'` (0) | no exact matches | ✅ reserve |
| ALCRX | empty result | not checked separately | no exact matches | ✅ reserve |
| AURLT | empty result | not checked separately | no exact matches | ✅ reserve |
| ALCR | indirect match (`SocialCrab`) | symbol page exists | **Alacrity Securities Ltd `ALCR.BO`** [3](https://www.reuters.com/markets/companies/ALCR.BO) [1](https://finance.yahoo.com/quote/ALCR/) | ❌ taken |
| AURA | — | — | Aura Network (L1) [1](https://coinmarketcap.com/currencies/aura-network/), ERC-20 Aura [9](https://etherscan.io/address/0xC0c293ce456fF0ED870ADd98a0828Dd4d2903DBF), Solana memecoin [18](https://coinmarketcap.com/currencies/aura/) | ❌ taken repeatedly |
| AUR | — | — | Auroracoin [3](https://coinmarketcap.com/currencies/auroracoin/), Aurix [12](https://coinmarketcap.com/currencies/aurix/) | ❌ taken |
| AURS | — | — | AURA DEX (`$AURS`) [17](https://coinlaunch.space/projects/aura-dex/) [19](https://medium.com/@AuraDEX.io/aura-on-solana-the-automated-on-chain-trading-ecosystem-5e1b725e52a8) | ❌ taken |
| ARA | — | — | Aura (ARA), PoW coin [5](https://blockspot.io/coin/aura-ara/) | ❌ taken |
| AURC | — | — | Aurumwaycoin [1](https://minkiz.co/coin/5133/), Aurora Acquisition Corp [6](https://stocktwits.com/symbol/AURC), tokenized stock `AURc` [10](https://coinmarketcap.com/currencies/aurora-innovation-inc-tokenized-stock-coinbase/) | ❌ taken |
| ALC | — | — | ALLCOIN [1](https://coinmarketcap.com/currencies/allcoin/), Alrightcoin [2](https://coinmarketcap.com/currencies/alrightcoin/), Alcon stock | ❌ taken |
| ALCX | — | — | Alchemix [3](https://www.kraken.com/prices/alchemix) [4](https://www.okx.com/en-us/price/alchemix-alcx) | ❌ taken |
| ALTC | — | — | ALT CRYPTO (BEP-20) [2](https://bscscan.com/token/0xB8927b3ee00e9AdEd1431fEeA6975795403873eF) [4](https://coinpaprika.com/coin/altc-alt-crypto/) | ❌ taken |
| AURY | — | — | Aurory [3](https://coinstats.app/coins/aurory/) | ❌ taken |

Reproduction commands (re-run in phase T1):

```sh
curl -s 'https://api.coingecko.com/api/v3/search?query=ALCRY'
curl -s 'https://api.coingecko.com/api/v3/search?query=AULTC'
# Yahoo Finance: https://finance.yahoo.com/lookup/?s=ALCRY  -> "No results"
```

## 3. Important caveats (honestly)

1. **There is no single global ticker registry.** In crypto the symbol is
   chosen by the project team or the exchange; the same ticker can exist on
   different venues [21](https://coinspaid.com/glossary/ticker-symbol/)
   [22](https://cryptoprocessing.com/glossary/ticker-symbol). Moreover,
   CoinMarketCap explicitly states that an asset **does not need** a unique
   ticker to be listed [18](https://support.coinmarketcap.com/hc/en-us/articles/360016553651-Ticker).
   Therefore "free" here means: **no exact matches found in the checked sources
   on the audit date**, not a legal guarantee.
2. Absence from CoinGecko/Yahoo ≠ absence from every CEX/DEX, trademark register
   or domain. Before Stage 2 (T8/O5/O6) we must check EUIPO/national trademark
   registers, domains and social handles, the listing rules of target
   exchanges, and repeat this audit (databases grow).
3. Reliable asset identification is never the ticker, it is
   `chain-id + contract address/native denom` — fixed in
   `docs/ARCHITECTURE.md` and D5 of `ROADMAP.md`.
4. Stage 1 uses `ALCRY` **only as a testnet unit with no market value**
   (decision Q5). A public offer of the token is a separate Stage 2 decision
   with a legal review.

## 4. What changes in the tree

- `README.md`, `ROADMAP.md`, `docs/ARCHITECTURE.md`, `docs/STATUS.md`,
  `CHANGELOG.md` and all 12 plans use `AuraLite-Crypto` / `ALCRY` /
  `auralite-test-1`.
- Internal crate names (`aura-types`, `aura-codec`, …) and the bech32 prefix
  `aura` are **kept**: that is a technical namespace, not a ticker. Changing it
  would break addresses for no reason.
- Phase **T1** must re-check this audit against the tree and, on any mismatch,
  open an ⚠️ AUDIT entry instead of quietly editing the number.

## Deliverable

This file is part of `patches/0001-C0-baseline-workspace.patch` (the
documentation part of C0).
