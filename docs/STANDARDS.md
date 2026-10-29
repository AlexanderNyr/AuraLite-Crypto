# Reference standards — ARC-20, ARC-721, ARC-M1

**Phase:** X5 · **Status:** implemented, measured, gated
(`crates/aura-exec/tests/x5_*.rs`)

A standard is a contract whose **method table is fixed and numbered**: a wallet
calls selector `0x03` for `transfer_from` on every ARC-20 token and passes the same
arguments in the same order. The three standards here are the ones Stage 1 needs,
and each one is a real Rust contract compiled to WASM, pinned by hash, and
deployed by the gate.

## 1. The calling convention

Calldata is **4 bytes of little-endian selector, then the arguments** in order,
each fixed-width:

| type | bytes | note |
|---|---|---|
| `u8` | 1 | thresholds, kinds |
| `u128` | 16 | token amounts and ids |
| `address` | 20 | raw bytes, not a string |

Answers come back through `env2.return_write` and are empty unless the call
committed. Every standard refuses calldata that is shorter than 4 bytes, carries an
unknown selector, is missing arguments, **or has a trailing byte**: a method's
meaning must never depend on what a later version appends.

Addresses are compared byte-for-byte. There is no checksum encoding at this layer;
that belongs to the wallet and the address format, not to a contract.

## 2. Failure codes

| code | name | meaning |
|---|---|---|
| `0` | — | success |
| `-6` | insufficient balance / not the token owner | the state says no |
| `-7` | overflow | checked arithmetic refused |
| `-20` | `BAD_SELECTOR` | no known selector (SDK) |
| `-21` | `BAD_ARGS` | arguments did not decode exactly (SDK) |
| `-22` | not owner | only the contract owner may do this |
| `-23` | zero address | a zero address is not an owner and not a payee |
| `-24` | allowance | `transfer_from` above the allowance (ARC-20) |
| `-25` | no token | the token id does not exist (ARC-721) |
| `-26` | token exists | the token id is already minted (ARC-721) |
| `-27` | not approved | neither owner nor approved spender (ARC-721) |
| `-28` | not signer | the caller is not in the policy (ARC-M1) |
| `-29` | policy | the policy is installed, or the arguments are unusable (ARC-M1) |
| `-30` | no proposal | the proposal id does not exist (ARC-M1) |
| `-31` | already executed | the proposal already ran (ARC-M1) |
| `-32` | already voted | this signer already voted (ARC-M1) |

### What a refusal does to state

X2's rule is that `Returned(_)` — any value, negative included — is a *successful*
call and gets a diff. So a standard that must say "not enough balance" has two
honest options: trap and throw the reason away, or answer with a code **and write
nothing**. The standards do the second, and every gate asserts it: the change set
is empty and the entries of the state tree are unchanged. The version still
advances by one, because that is what any committed transaction does.

A trap is reserved for a bug in the contract itself (the `unwrap` family), which
is the only case where there is no meaningful code to return.

## 3. ARC-20 — fungible token (`standards/arc20`)

| selector | method | arguments | answer |
|---|---|---|---|
| `0x01` | `transfer` | `to: address`, `amount: u128` | — |
| `0x02` | `approve` | `spender: address`, `amount: u128` | — |
| `0x03` | `transfer_from` | `from: address`, `to: address`, `amount: u128` | — |
| `0x04` | `balance_of` | `who: address` | `u128` |
| `0x05` | `allowance` | `owner: address`, `spender: address` | `u128` |
| `0x06` | `total_supply` | — | `u128` |
| `0x07` | `mint` | `to: address`, `amount: u128` | — (owner only) |
| `0x08` | `burn` | `amount: u128` | — |
| `0x09` | `owner` | — | `address` |
| `0x0a` | `transfer_ownership` | `new_owner: address` | — (owner only) |

**Storage:** `t` total supply, `o` owner, `b` + address → balance, `a` + 10 bytes
of `BLAKE3(owner ‖ spender)` → allowance. The allowance key is hashed because
`owner ‖ spender` is 40 bytes and a slot is 32.

**Rules that are enforced, not documented:** `from` for `transfer` is always
`env2.caller`; a zero address is refused as a recipient; zero-amount transfers
succeed and write nothing; `transfer_from` decrements the allowance by exactly the
amount **after** a successful move, so a failure leaves it untouched; `mint` is
owner-only and checked against `u128` overflow.

**Events:** `transfer(from, to, amount)`, `approval(owner, spender, amount)`,
`mint(to, amount)`, `burn(from, amount)`, `ownership(new_owner)`.

## 4. ARC-721 — non-fungible token (`standards/arc721`)

| selector | method | arguments | answer |
|---|---|---|---|
| `0x01` | `mint` | `to: address`, `token_id: u128` | — (owner only) |
| `0x02` | `transfer` | `to: address`, `token_id: u128` | — |
| `0x03` | `approve` | `spender: address`, `token_id: u128` | — |
| `0x04` | `transfer_from` | `from: address`, `to: address`, `token_id: u128` | — |
| `0x05` | `owner_of` | `token_id: u128` | `address` (`-25` if absent) |
| `0x06` | `get_approved` | `token_id: u128` | `address` (zero if none) |
| `0x07` | `balance_of` | `who: address` | `u128` (tokens held) |
| `0x08` | `total_supply` | — | `u128` |
| `0x09` | `burn` | `token_id: u128` | — |
| `0x0a` | `owner` | — | `address` |

**Storage:** `t` supply, `o` contract owner, `w` + id → owner (**absent = does not
exist**), `a` + id → approved spender, `b` + address → holdings.

"Exists" is not a separate flag: a token exists exactly when its owner slot is
present. A second source of truth is the first thing that goes wrong in a
per-token contract.

**Every transfer clears the approval.** Keeping it would let a spender move the
token again after the owner handed it to someone else — the classic stale-approval
bug, and one of the X5 negative controls.

## 5. ARC-M1 — minimal multisig (`standards/multisig`)

Three signers, a threshold, one action: send native ALCRY out of the contract's
own balance.

| selector | method | arguments | answer |
|---|---|---|---|
| `0x01` | `set_policy` | `threshold: u8`, `s1`, `s2`, `s3: address` | — (creator only, once) |
| `0x02` | `propose` | `kind: u8`, `to: address`, `amount: u128` | `u128` (proposal id) |
| `0x03` | `approve` | `pid: u128` | — (executes at the threshold) |
| `0x04` | `revoke` | `pid: u128` | — |
| `0x05` | `proposal` | `pid: u128` | 33 bytes: `executed u8`, `approvals u128`, `amount u128` |
| `0x06` | `signers` | — | 60 bytes: three addresses |
| `0x07` | `threshold` | — | 1 byte |
| `0x08` | `creator` | — | `address` |

**Rules:**

* the policy is set **once**, by the deployer, and can never be changed — a
  multisig that can be reconfigured by its own proposal is a single key with extra
  steps;
* only a signer may propose or vote, and never twice: the vote is a state flag per
  `(proposal, signer)`, keyed by a truncated `BLAKE3`;
* a proposal executes **exactly once**. The `executed` flag is written before the
  transfer and cleared again if the transfer fails, so a failure is retryable and a
  success cannot repeat;
* execution requires the wallet's own balance to cover the amount, checked with
  `env2.self_address` **before** the flag — a proposal cannot be marked executed by
  a call that had nothing to send.

**Missing on purpose:** no arbitrary call payload (that is a cross-contract call,
X6), no signer replacement, no timelock, no expiry. Each is a design decision with
its own failure mode, and none belongs in a "minimal" contract.

## 6. What the gate proves, and what it does not

Proved, per standard, by tests that deploy the **shipped artifact** through the
real lifecycle:

* every method's success path, and its refusal path with **zero writes**;
* the invariants (allowance conservation, supply = Σ holdings, executed-once,
  unique proposal ids);
* the dispatcher's refusals (`-20`, `-21`);
* determinism: the same calldata twice produces the same root.

Not proved, and not claimed:

* **cross-contract reentrancy.** With no cross-contract calls (X6) there is no
  second frame to re-enter from. The check-then-write ordering and the executed
  flag are the *shape* of the defence, and they are what X6 will build on;
* **that the standards are free of bugs.** They are free of the bugs the gate
  looked for — including one it found (a 40-byte event buffer holding a 16-byte
  amount), which is recorded in `docs/evidence/x5/report.md`.

## Reproducibility

`tools/build_standards.py` builds every standard twice into independent target
directories and pins size, `sha256` and `blake3` only if the two agree. The pin is
therefore a statement about **this source path**: measured on a clean clone
(2026-10-29), `arc20` reproduced byte for byte while `arc721` and `multisig` kept
their size but permuted their type and function sections, because cargo folds the
absolute path of the `aura-sdk` dependency into `-C metadata`. Run
`make build-standards` once after cloning to a different path.
