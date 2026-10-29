# X6 — call isolation (measured)

Generated (UTC): 2026-10-29T09:25:30+00:00. Sizes and `sha256` come from
`tools/build_x6_contracts.py` reading the artifact it just built; the
BLAKE3 digests and the gas numbers come from
`cargo run -p aura-exec --bin x6-record`, which deploys the shipped
contracts through the real `Lifecycle`, funds a vault, attacks it, and
descends a chain of nine copies.

## The two rules, and why they are these two

**A frame may not call an address that is already on the stack (`-14`).**
That is stricter than a per-function mutex and simpler than one: it makes
the classic withdraw-then-reenter attack *unrepresentable*, because the
attacker cannot be called back while its own frame is in flight. The cost is
named in `docs/CALLS.md` and in the vault's own doc comment: a callback into
a live frame does not exist on this chain, so a receiver that wants to be
told about a payment must be called *forward*, not back. The measured
consequence is in the gas table's neighbours — the attacker's counter stops
at **one** attempt, because the second entry was refused before the vault
ran at all.

**A stack may not exceed eight frames (`-15`).** Eight is a protocol
parameter, published in `abi_v3::MAX_CALL_DEPTH` and re-checked by the
executor. Because the first rule forbids cycles, this limit governs chains of
*distinct* contracts — the shape a router → pool → token path has.

| function | arity | gas | why |
|---|---|---|---|
| `call` | 6 | 100 + 3/byte | loads and runs another module; the callee's own instructions are metered separately |
| `returndata_len` | 0 | 20 | how much the callee answered |
| `returndata_read` | 2 | 20 + 3/byte | the answer itself, bounded by `MAX_RETURN_LEN` |
| `depth` | 0 | 20 | the caller's own frame index, so it can refuse to go deeper |

## Contracts (measured from the artifacts)

| contract | bytes | sha256 | imports | role |
|---|---|---|---|---|
| `vault` | 9759 | `9173b3594e61f068…` | 8 | pays out **before** it debits — the bug X6 exists to survive |
| `evil` | 11607 | `efcc461cb17f3de1…` | 8 | re-enters the vault from the payout callback, and probes other contracts |
| `recurse` | 9093 | `9fd2301b3db8cb20…` | 8 | calls the next contract in a chain of nine copies until refused |

Three contracts, 30459 bytes in total, each its own workspace, each built
with the same canonical command as X4/X5 and built twice into independent
target directories before anything is pinned.

## Measured gas

| operation | gas |
|---|---|
| `vault_deploy_gas` | 107369 |
| `evil_deploy_gas` | 117100 |
| `recurse_deploy_gas` | 100132 |
| `fund_gas` | 41188 |
| `attack_gas` | 87697 |
| `withdraw_after_attack_gas` | 35116 |
| `descent_gas` | 28414 |
| `attack_attempts` | 1 |
| `attack_last_status` | 0 |

## What the gate actually proves

1. **Depth** — a chain of nine copies descends; the first ran at depth 1, the
   eighth at depth 8, the eighth's attempt on the ninth was refused with
   `-15`, and the ninth contract never ran (its counter is 0).
2. **Reentrancy** — the vault pays its receiver *first* and then notifies it
   with `env3.call`; the node refuses that notification with `-14`, because the
   receiver's own frame is still on the stack. The vault publishes the status
   it got (`notify` event, asserted to be `-14`), the attacker's counter stops
   at **1** attempt, its ledger is debited once, the vault ends **empty rather
   than short**, and the honest depositor is paid in full afterwards
   (9 800 of her own 9 800).
3. **Atomicity** — a callee that writes, emits and then traps leaves no
   write, no event and no value; the caller's own frame still commits, because
   a callee's failure is a value the caller handles.
4. **Value** — the payout moves both balances, and when a callee reverts the
   sender keeps the value it sent and the callee never receives it.
5. **Identity** — the caller of a nested frame is the calling *contract*, not
   the transaction's signer.
6. **Codes** — a callee's refusal (`-21` from its dispatcher) reaches the
   caller unchanged; a call to an account with no code is `-16`; a callee that
   traps is `-17`.
7. **Receipts** — the committed tree equals the receipt's `result_root`,
   and a nested call's events arrive **before** the caller's own, in execution
   order (`notify`, `withdraw`, `attempt`).

One measured number deserves a warning, because it looks like a contradiction:
`measured_attack_last_status=0`. That is the attacker's *own* record of its
*own* call — `evil.attack` called the vault, and the vault succeeded, so the
status is `0`. The refusal that matters (`-14`) is the vault's, recorded in
the `notify` event and asserted separately. Two different statuses, two
different subjects.

## Negative controls

NC-1 the reentrancy guard is removed, NC-2 the depth limit is removed, NC-3
the callee's diff is not merged, NC-4 a failed callee's value is not returned,
NC-5 the callee's status is flattened to success, NC-6 the child frame does
not inherit the parent's stack.

6/6 PASS; total **147** across C0–C10 + S0–S7 + X0–X6.

## Defects the gate found (all fixed in this phase)

1. **The engine refused `env3` imports.** `check_imports` in the `wasmi`
   backend kept its own copy of the allowed modules and had not been told
   about the third surface, so a contract that calls another contract could
   not be deployed at all. The census had it right; the engine's copy had
   drifted. Found by the first run of the recorder.
2. **A returned status is not a success flag.** X2's rule — "any
   `Returned(_)` commits" — is right for a transaction and wrong for a
   *frame*: the first X6 gate saw a vault answer `-14` and the caller read
   `0`, because the child's status was being dropped. A callee's answer is now
   propagated, and NC-5 removes that propagation and requires the gate to go
   red.
3. **The payout and the notify are not the same call.** The vault's first
   draft paid its receiver with `env3.call{value}`; on this chain that is
   refused for an account with no code (`-16`) and for its own caller (`-14`),
   so an honest withdrawal could not complete. Money moves with
   `env.transfer`, control moves with `env3.call`, and the contract says so.
4. **`recurse` could not recurse.** The first depth fixture called itself,
   which the reentrancy rule refuses — correctly. The fixture had to become a
   chain of distinct contracts, which is also the honest shape of the limit.

## Honest scope

* **No callbacks.** A frame cannot be re-entered, so a payment cannot trigger
  receiver logic in the same transaction. That is a deliberate Stage 1
  restriction, not an accident of the implementation; a future phase that
  wants flash-loan-style callbacks has to replace this rule with a per-account
  lock and re-run this gate.
* **The payout moves money with `env.transfer`, not with `env3.call{value}`.**
  That is a design decision with a measured reason: `env3.call` refuses an
  account with no code (`-16`) and a live frame (`-14`), so a vault that paid
  *through* a call could not pay an ordinary wallet at all. The `value`
  parameter is therefore exercised by `evil.send` in two tests — a callee that
  reverts returns the money (`value_of_a_reverted_call_comes_back`) — while the
  successful contract-to-contract value path is covered only by the payout in
  `value_sent_with_a_call_changes_both_balances`, which is `env.transfer` under
  the hood. A caller that wants to *both* send value and run code in one step
  is not tested on this chain, and is not claimed.
* **Gas is charged after the callee returns**, not reserved before it. A
  parent cannot overspend — the child's budget is the parent's remaining gas —
  but a receipt is written even when the caller traps afterwards, and that is
  the next phase's problem (fee accounting, T-phase).
* **The reentrancy rule is per address, not per storage slot.** A contract
  that calls two different instances of the same logic is fine; a contract
  that calls itself is not.
* Pins are exact for this toolchain, these flags and this checkout path, as
  measured and documented in X4 and X5.

Tests: **311** in the workspace, 15 of them in the X6 gate.
Validators / blocks / transactions remain **0 / 0 / 0**.
