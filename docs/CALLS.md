# Calls — how one contract runs another

**Phase:** X6 · **Surface:** `env3` (see [`ABI_V3.md`](ABI_V3.md)) · **Status:** implemented

This is the operator's and the contract author's view of cross-contract calls: what
is allowed, what is refused, and what happens to storage when something fails.

## The model

A transaction has one **top-level frame** at depth 1. Every `env3.call` pushes a
child frame; when it returns, the frame pops. A frame is `(contract address, store,
fuel budget, base version)`. Nothing else is shared — a child cannot read the
parent's linear memory and the parent cannot read the child's except through return
data.

## Two refusals, both in the host

| refusal | code | why this rule and not another |
|---|---|---|
| target is already on the stack | `-14` | makes the withdraw-then-reenter attack **unrepresentable**, not merely detected; one scan of at most 8 addresses |
| stack already holds 8 frames | `-15` | bounds a chain of *distinct* contracts, which is what a router → pool → token path is |

`MAX_CALL_DEPTH = 8` is a protocol parameter: it is published in
`abi_v3::MAX_CALL_DEPTH`, re-checked by the executor, and every node must agree on
it or two nodes disagree on which call runs.

Two further refusals are about the target rather than the stack: `-16` (`no_code`)
when the address is an account with no contract, and `-17` (`callee_failed`) when
the child trapped or ran out of gas.

## The consequence every author must know

**A callback into a live frame does not exist on this chain.** A contract that has
just been paid cannot be called back to react in the same transaction. A receiver
that wants to be told about a payment is called **forward** — `env3.call` with a
zero value and an explicit selector — after the money has moved.

That is why the reference vault in `examples/vault` does two things, not one:

```
env.transfer(receiver, amount)?;      // money moves, and this is final
env3.call(receiver, on_receive, 0);   // control moves, isolated, may fail harmlessly
```

The first call is a transfer, not a call: an account with no code can be paid, and
its receiver logic is never required. The second is a call: it can fail, and its
failure does not undo the payment. Trying to pay *through* `env3.call` on this chain
returns `-16` for an EOA and `-14` for the caller itself — a mistake the X6 recorder
made on its first run, and the reason the vault's doc comment says which function
does which.

## What happens to state

* **Callee succeeds.** Its diff is merged into the parent's, its events are appended
  after the parent's existing events, its return data becomes the parent's
  `returndata`.
* **Callee fails.** Its diff is discarded, its events are discarded, the value it was
  given is returned to the sender, and `returndata` is cleared. The parent continues
  and sees the status.
* **Parent fails afterwards.** Nothing the children did survives, because the parent's
  own diff is the only one a receipt can carry.

So a callee can never leave a half-applied effect, and a parent can never inherit one.
The X6 gate asserts all three cases against tree roots, not against balances.

## What a node charges

`call` is 100 gas + 3/byte of calldata, `returndata_len` 20, `returndata_read`
20 + 3/byte, `depth` 20. The child's instructions are metered on the same counter
the parent uses: the parent's remaining fuel *is* the child's budget, and the total
appears in one receipt. Multi-frame fee accounting — reserving gas before a call and
refunding after — belongs to the transaction phase, and is listed as an honest limit
in `docs/evidence/x6/report.md`.

## Writing one

```rust
use aura_sdk::{abi, abi3, prelude::*};

#[entrypoint]
pub fn run(selector: u32) -> i32 {
    match selector {
        0x01 => {
            let mut buf = [0u8; 36];
            buf[..4].copy_from_slice(&0x02u32.to_le_bytes());
            buf[4..24].copy_from_slice(&who);
            buf[24..].copy_from_slice(&amount.to_le_bytes());
            let status = abi3::call_contract(&target, &buf, 0);
            if status != 0 {
                // The callee refused. Its writes are gone; ours are ours.
                return status;
            }
            0
        }
        _ => abi::BAD_SELECTOR,
    }
}
```

Two rules for the author: check the returned status (a callee's refusal is not an
exception), and do not assume anything the callee wrote is visible before the call
returns, because a reverted callee leaves no trace.
