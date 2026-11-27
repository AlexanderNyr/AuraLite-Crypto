# On-call runbook — AuraLite-Crypto closed testnet (O3)

Audience: the invited operators of the closed testnet. This runbook is the
"invited operators / monitoring / on-call" half of the O3 DoD; its measured
half is the uptime ledger and the backup/restore drill
(`docs/o3-testnet-ops-plan.md`).

## Scope and roles

- **Operator**: runs one validator (`docs/how-to-run.md`), keeps the node
  healthy, records heartbeats in the uptime ledger.
- **On-call**: one rotating operator per week; owns incident response and
  the restore drill; hands over with a written note (below).
- **Owner** (non-code): approves chain-id changes and any restart that
  touches validator keys.

## Monitoring — the uptime ledger

The ledger `ops/uptime.txt` (one row per hourly poll) is the source of
truth for the O3 gate ("7 days uptime, 0 state losses"):

```
uptime <day> <hour> <height> <state-root-hex>
```

- Every hour: query `aurad status` / `get_block` and append one row
  (`make testnet-ops-receipts` automates a scripted run; operators append
  live rows with the same format).
- **Alert thresholds** (act if any fires):
  - two consecutive missing polls (heartbeat gap > 2 h);
  - `height` not advancing across 3 polls (chain stalled — page);
  - `height` decreasing, or the same height with a different root
    (**state loss — page immediately**, see Incidents);
  - `aura-ops ledger` refusing a row (`bad heartbeat`) — the ledger must
    stay machine-checkable.
- The rollup is never hand-edited: `make testnet-ops-check` re-derives
  `uptime ledger: days=<n> state_losses=<n>` from the file.

## Backups

- **What**: the shipped S2 backup bundle
  (`aura_storage::create_backup` — manifest + WAL records + sha256), not
  ad-hoc file copies. The bundle is deterministic: the same store always
  produces the same bytes.
- **Cadence**: daily, plus before every upgrade or host migration.
- **Storage**: at least two media, one off-host; the bundle filename
  carries the chain-id and the head version.
- **Verification (every time)**: `restore_backup` into a scratch dir —
  the format itself rejects a bundle whose declared roots do not match
  the recomputed ones (`RootMismatch`), so a backup that restores is a
  proven backup.

## Restore drill

- **Cadence**: weekly (the O3 gate requires ≥ 1 completed drill).
- **Procedure** (`aurad-ops` automates the measured variant):
  1. take a backup of a live store and record its head (height + state
     root);
  2. destroy the original store (in the drill: deliberately);
  3. restore from the bundle — every record is replayed through the real
     commit path and every root recomputed;
  4. compare the restored head with the recorded one — **byte-identical
     or the drill failed** (named refusal
     `restore_mismatch_refused`);
  5. record `restore drills +1` in the receipt.
- Note (honest boundary): the D2 pipeline refuses to reopen a
  receipt-bearing store by design (`Pipeline::open` — restart
  checkpointing is a later node integration task), so restoration is
  verified at the S2 storage layer the backup format targets.

## Incidents

1. **Chain stall** (height frozen > 3 polls): check all validators'
   logs; identify the missing proposer; restart the missing node from its
   backup if its store is suspect; record the incident and the lost
   polls in the ledger (gaps reduce `uptime days` honestly — never forge
   rows).
2. **State loss signal** (height regression or same-height root change):
   **stop all writes**; take backups of every node before touching
   anything; diff the heads; restore the minority node from the majority
   head's backup; document which head won and why. `state losses` in the
   receipt must reflect reality — a non-zero value fails the O3 gate and
   that is the correct outcome.
3. **Suspect backup**: run the verification restore; a `RootMismatch` or
   checksum refusal means the bundle is bad — re-take it from the live
   store and quarantine the bad bundle.

## Handover note (weekly, written)

- ledger window covered, drills completed, incidents and their receipts;
- bundle inventory (filename, head version, verification result);
- open alerts and who owns them.
