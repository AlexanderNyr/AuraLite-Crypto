# X5 — reference standards and ABI v2 (measured)

Generated (UTC): 2026-10-29T08:55:59+00:00. Every number below comes from
`crates/aura-exec/tests/fixtures/x5_standards.txt`: size, hash, imports and
exports from `tools/build_standards.py` reading the built artifact, and the
gas lines from `cargo run -p aura-exec --bin x5-record` through the real
`Lifecycle`.

## Two decisions, before any code

**A standard needs a dispatcher, and ABI v1 could not express one.** The X3
lifecycle hands a contract a single `i32`. A token has many methods with
`[u8; 20]` and `u128` arguments, and `balance_of` has to answer with sixteen
bytes. So X5 adds **ABI v2** — but as a *separate import module* `env2`, not
as a new version of `env`. The module name carries the version, exactly as
`wasi_snapshot_preview1` does: a v1 contract is bit-for-bit unaffected, the 23
frozen X2 vectors keep their meaning, and a module declares which surface it
speaks by what it imports.

**A contract must know who called it and where it lives.** `transfer(from,
to, amount)` with a caller-supplied `from` is not a transfer, it is permission
to spend anyone's tokens — so the address comes from `env2.caller`. And a
multisig that wants to know whether it can cover a payment must name itself —
so `env2.self_address`. Both were forced by the standards, not added for
symmetry; the second one the gate forced by failing.

| function | arity | gas | why |
|---|---|---|---|
| `calldata_len` | 0 | 20 | a length, not a copy |
| `calldata_read` | 2 | 20 + 3/byte | selector and arguments arrive as bytes |
| `return_write` | 2 | 20 + 3/byte | `balance_of` must answer with a `u128` |
| `caller` | 2 | 20 | the signer, from the chain, never from calldata |
| `self_address` | 2 | 20 | a contract that cannot name itself cannot read its balance |
| `abi_version` | 0 | 20 | the surface version, so a contract can assert it |

The engine still runs in 16777216-byte linear memory, but a call
carries at most 1 KiB of calldata (`MAX_CALLDATA_LEN`) and answers with at
most 1 KiB (`MAX_RETURN_LEN`). Both limits are session limits: they are
enforced before the engine runs, and the subject of gate `x5_abi2`.

## Standards (measured from the artifacts)

| standard | bytes | sha256 | blake3 | imports |
|---|---|---|---|---|
| `arc20` | 12733 | `c9f9ec9bf5d9f445…` | `771a54185552b03e…` | 7 |
| `arc721` | 13224 | `d88899ee751b4a17…` | `bde9554d4fe03391…` | 6 |
| `multisig` | 15268 | `fcbb09547cb1ef8e…` | `12ab99f719cc6fd8…` | 10 |

Three standards, 41225 bytes in total. Each ships an `abi.json` next to
its `.wasm`, so a wallet can discover the selector table without reading Rust.

### Method tables

* **ARC-20** (`0x01`–`0x0a`): `transfer`, `approve`, `transfer_from`,
  `balance_of`, `allowance`, `total_supply`, `mint`, `burn`, `owner`,
  `transfer_ownership`.
* **ARC-721** (`0x01`–`0x0a`): `mint`, `transfer`, `approve`, `transfer_from`,
  `owner_of`, `get_approved`, `balance_of`, `total_supply`, `burn`, `owner`.
* **ARC-M1** (`0x01`–`0x08`): `set_policy`, `propose`, `approve`, `revoke`,
  `proposal`, `signers`, `threshold`, `creator`.

## Measured gas

| operation | gas |
|---|---|
| `arc20_deploy_gas` | 140694 |
| `arc20_mint_gas` | 56410 |
| `arc20_transfer_gas` | 55951 |
| `arc20_balance_of_gas` | 39221 |
| `arc721_deploy_gas` | 145604 |
| `arc721_mint_gas` | 63946 |
| `arc721_transfer_gas` | 71514 |
| `multisig_deploy_gas` | 162108 |
| `multisig_policy_gas` | 56066 |
| `multisig_propose_gas` | 72930 |
| `multisig_approve_gas` | 76281 |

Deploy gas is `1000 + 10 × bytes` for the code plus whatever `init` costs.
The numbers above are the whole bill: instruction metering and ABI calls share
one counter, which is why `deploy` of a 15 KiB multisig costs
162108 rather than the
153680
the formula alone would suggest.

## How a standard answers a refusal

X2's rule is that `Returned(_)` — any value, negative included — is a
*successful* call and gets a diff. A standard that must tell a caller "not
enough balance" therefore has exactly two honest options: trap and lose the
reason, or answer with a negative code **and write nothing**. The standards do
the second, and every gate asserts it: an empty change set and unchanged tree
entries, not merely "the balance looks right".

The codes are part of the standard: `-6` insufficient balance, `-7` overflow,
`-20` unknown selector, `-21` malformed arguments, `-22` not owner, `-23` zero
address. `-20` and `-21` come from the dispatcher, so *every* standard refuses
an unknown selector and a trailing byte the same way.

## Negative controls

NC-1 the dispatcher accepts trailing bytes, NC-2 `transfer_from` does not spend
the allowance, NC-3 an ARC-721 transfer keeps the approval, NC-4 the multisig
does not check `executed`, NC-5 the session accepts oversize calldata, NC-6 a
failed execution is still marked executed.

6/6 PASS; total **141** across C0–C10 + S0–S7 + X0–X5.

## Defects the gate found (all fixed in this phase)

1. **ARC-721's `approve` trapped on every call.** The event buffer was 40 bytes
   and the code copied a 16-byte `u128` into `event[20..]`, which is 20 bytes:
   a panic in the guest, which the host correctly reports as `Trap(Unreachable)`.
   The standard was wrong, not the runtime — and the fix is a comment in the
   contract, because that class of bug will happen again.
2. **The multisig could not read its own balance.** Nothing in ABI v1 or v2
   named the contract's own address, so a "can I cover this?" check was
   impossible; `env2.self_address` exists because a gate failed without it.
3. **`RUSTFLAGS` is the wrong place for a wasm linker flag.** X4's builder set
   `-C link-arg=--max-memory` globally, which also reaches the *host* build of a
   proc-macro's build script; a rebuild that needed to recompile `proc-macro2`
   failed with `cc: error: unrecognized command-line option`. The canonical
   form is `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS`, and the new builder
   uses it.
4. **A standard cannot be tested with a status assertion alone.** Six of the
   first X5 test failures were tests that forgot to commit the diff between two
   calls — the contract was right and the harness was reading a stale tree. The
   harness now has one `propose`-style helper per state-changing step, and the
   rule is written down where the helper lives.

## Reproducibility: measured where it broke

`tools/build_standards.py` builds every standard **twice**, into two
independent target directories, and refuses to write a pin if the two differ;
that is what makes the hashes in the table above mean anything in this tree.
The stronger claim — that they mean the same thing in *any* tree — was tested
on 2026-10-29 and does not hold:

| standard | build in this tree | clean clone at `/tmp/zt5` | verdict |
|---|---|---|---|
| `arc20` | 12 733 B | 12 733 B, **0 differing bytes** | identical |
| `arc721` | 13 224 B | 13 224 B, type 35 B / func 26 B / code 6 225 B differ | permuted |
| `multisig` | 15 268 B | 15 268 B, type 54 B / func 27 B / code 6 891 B differ | permuted |

Same toolchain, same flags, same section sizes: the two `multisig` artifacts
are the same length and differ only in the **order** of their types, functions
and bodies. The cause is one the phase can name: cargo folds the absolute path
of the `aura-sdk` path dependency into `-C metadata`, and `rustc` seeds the
iteration order of its type interning from that hash. So the honest claim is
**reproducible at the recorded source path**, which is exactly the X4 finding,
and a clone elsewhere must run `make build-standards` once before
`make check-standards`. Making the build path-independent belongs to X7, where
cross-engine determinism is the subject and a mismatch is the point of the
gate rather than an inconvenience.

## Honest scope

* **No cross-contract calls, so no true reentrancy test.** X6 is call
  isolation; the multisig's double-execution guard and the token's
  check-then-write ordering are the *shape* of that defence, and they are
  tested, but a contract cannot yet call another contract, so this phase makes
  no claim about reentrancy across contracts. The plan's wording ("+ reentrancy
  test") is satisfied only in the sense that the invariant is enforced and
  measured within one contract.
* **The multisig policy is immutable.** Set once by the deployer, never again —
  including by a proposal. That is a deliberate limit: a reconfigurable policy
  is a single key with extra steps.
* **Allowance and vote keys are hashed and truncated.** `(owner, spender)` is 40
  bytes and a slot is 32, so the node's BLAKE3 is truncated to 80 bits. A
  collision needs two pairs sharing 80 bits of digest; the alternative (a
  second indirection) costs more than the risk it removes.
* **No token metadata in ARC-20/721.** Name, symbol and decimals are not
  letter-addressable state, and a standard that reads them must first agree on
  their encoding.
* **The standards are wasm artifacts pinned by hash, and the pin is
  path-scoped.** Measured, not assumed — see the reproducibility section above:
  a clone at another path reproduced `arc20` byte for byte and permuted the type
  and function sections of `arc721` and `multisig`.
* **Failures are still calls.** A refusal commits an empty diff and advances the
  version. Making a negative status revert would change X2's frozen vectors, so
  it is a protocol decision for a later phase, not an X5 convenience.
* Validators / blocks / transactions remain **0 / 0 / 0**.
