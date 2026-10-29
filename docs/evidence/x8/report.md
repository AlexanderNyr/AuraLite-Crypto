# X8 — the static verifier (measured)

Generated (UTC): 2026-10-29T11:00:15+00:00. Every number below came out of `x8-verify` in this
tree; `gates.txt` holds the raw command output that produced it, and
`crates/aura-exec/tests/x8_verify.rs` pins the same numbers so they cannot
drift silently.

## What the phase claims

> A contract module either satisfies the accepted subset, or is refused at
> verification time with a stable code — before an engine ever sees it.

The verifier never executes the module. It reads the binary, checks it against
the subset the census (X0) and the host surface (X2, X3, X6) already
established, and answers with a `Report`: a verdict, a size, function and
instruction counts, the peak operand-stack depth, and one `Problem` per defect
carrying a byte offset into the module.

## Shipped artifacts (re-verified, not remembered)

| contract | verdict | functions | instructions | peak stack | peak ops/body |
|---|---|---|---|---|---|
| `kv` | accept | 33 | 1905 | 7 | 476 |
| `store` | accept | 29 | 1921 | 7 | 476 |
| `events` | accept | 25 | 1871 | 7 | 476 |
| `vault` | accept | 53 | 3692 | 7 | 535 |
| `evil` | accept | 61 | 4235 | 7 | 630 |
| `recurse` | accept | 51 | 3331 | 7 | 476 |

All six pass. That is the part of the claim that is cheap: a verifier that
accepts everything would print the same table. The next section is the part
that is not cheap.

## The mutation matrix (100 damaged modules, 15 kinds of damage)

`tools/gen_x8_mutants.py` is an independent implementation: its own section
walker in Python, one damage per module, and a stated expectation. If it
shared code with the verifier, agreement would prove nothing.

| kind | mutants | expected verdict | agrees |
|---|---|---|---|
| call_index — `call` names a function that does not exist | 6 | `call_index` ×6 | 6/6 |
| export_shape — the `run` export is renamed | 6 | `export_shape` ×6 | 6/6 |
| float_local — a local declaration becomes a float | 6 | `float_type` ×6 | 6/6 |
| float_type — a value type becomes `f32`/`f64` | 6 | `float_type` ×6 | 6/6 |
| function_code — the code section drops a body | 6 | `function_code_mismatch` ×6 | 6/6 |
| global_type — a global is not an integer | 6 | `global_type` ×6 | 6/6 |
| import — an import module name is not one of the ABI surfaces | 8 | `import_not_allowed` ×8 | 8/8 |
| label_depth — `br` names a label that does not exist | 8 | `label_depth` ×8 | 8/8 |
| local_index — `local.get` names a local that does not exist | 8 | `local_index` ×8 | 8/8 |
| magic — header magic byte destroyed | 6 | `bad_magic` ×6 | 6/6 |
| memory_max — linear memory declares more pages than allowed | 6 | `memory_limits` ×6 | 6/6 |
| opcode — one opcode leaves the accepted subset | 8 | `opcode_banned` ×8 | 8/8 |
| shipped — the six shipped artifacts, unmodified | 6 | `accept` ×6 | 6/6 |
| too_large — the module grows past the size ceiling | 6 | `module_too_large` ×6 | 6/6 |
| truncated — the last section is cut short | 8 | `section_truncated` ×8 | 8/8 |
| version — version word bumped to 2 | 6 | `bad_version` ×6 | 6/6 |

**100 mutants (100 refusals) and 6 unmodified modules replayed as-is. Mismatches between the expectation the
mutator stated and the verdict the verifier returned: 0.**

The plan is 6 mutants per damage kind over the six shipped artifacts, with a
second round for the five kinds whose first six do not already cover every
artifact — 100 exactly, written by the generator, not by hand.

## The 22 codes, and the witness for each

A code that no test can trigger is documentation, not a check. Every code below
has either a focused unit test in the gate (a hand-assembled module, so it does
not depend on how today's rustc lays out a contract) or a golden mutant:

| code | witness |
|---|---|
| `bad_magic` | unit `code_bad_magic` + mutant kind `magic` |
| `bad_version` | unit `code_bad_version` + mutant kind `version` |
| `module_too_large` | unit `code_module_too_large` + mutant kind `too_large` |
| `section_order` | unit `code_section_order`, `code_section_order_on_a_duplicate` |
| `section_unknown` | unit `code_section_unknown` |
| `section_truncated` | unit `code_section_truncated` + mutant kind `truncated` |
| `float_type` | unit `code_float_type`, `code_float_local` + mutant kinds |
| `import_not_allowed` | unit `code_import_not_allowed` + restrictive config |
| `global_type` | unit `code_global_type` + mutant kind `global_type` |
| `memory_limits` | unit `code_memory_limits_*` (no maximum, above ceiling) |
| `function_code_mismatch` | unit `code_function_code_mismatch` + mutant kind |
| `body_truncated` | unit `code_body_truncated`, `…_on_trailing_bytes` |
| `opcode_banned` | unit `code_opcode_banned`, `…_on_a_bulk_sub_opcode` |
| `stack_underflow` | unit `code_stack_underflow` (and NC-2) |
| `stack_too_deep` | unit `code_stack_too_deep` (budget lowered to 1) |
| `label_depth` | unit `code_label_depth` + mutant kind `label_depth` |
| `local_index` | unit `code_local_index` + mutant kind `local_index` |
| `global_index` | unit `code_global_index` |
| `call_index` | unit `code_call_index` + mutant kind `call_index` |
| `opcode_budget` | unit `code_opcode_budget` (budget lowered to 2) |
| `locals_budget` | unit `code_locals_budget` (budget lowered to 1) |
| `export_shape` | unit `code_export_shape`, `…_on_a_wrong_name` + mutant kind |

## What depth this actually is

*It is a depth check, not a type check.* The operand stack is tracked as a
**length**, not as a stack of types: `i32.add` pops two and pushes one, and the
verifier trusts the opcode to mean what the spec says it means without tracking
`i32` vs `i64` through every instruction. A module that type-confuses two
integers of different widths can therefore pass X8 and still be refused by the
engine at instantiation — which is the correct division of labour: **the engine
remains the authority on instantiation, X8 only guarantees that the engine is
never asked to instantiate something outside the accepted subset.**

What it does check, in full:

* section order, uniqueness, id range, declared lengths, and the `function` /
  `code` counts against each other;
* every value type in the type section, every local declaration, every global;
* the import whitelist (three ABI surfaces, function imports only) with a
  restrictive configuration that refuses all imports;
* per body: the instruction budget, the local budget, the operand-stack depth
  with callee signatures read out of the type section, label depth for
  `br`/`br_if`/`br_table`, `local`/`global`/`call` index bounds, `call_indirect`
  against the type section, and the two allowed `0xfc` bulk sub-opcodes;
* the export shape (`run` must exist);
* memory: a maximum must be declared and must not exceed the census ceiling.

## Honest limits

* **The accepted opcode and value-type subset is a whitelist, not the full
  grammar.** An instruction the whitelist does not know is `opcode_banned`, even
  if the spec allows it. That is deliberate — the whitelist is the same shape as
  the census — but it means a contract built by a different toolchain can be
  refused for using an instruction this SDK never emits. The refusal is loud and
  carries an offset, which is the property that matters.
* **`else` without `if` is caught, but arithmetic reachability is not proved.**
  Unreachable code gets the spec's polymorphic-stack treatment: it is not
  evaluated, so a body after `unreachable` cannot manufacture a false failure.
* **Nothing here is a prover.** There is no formal model of the ABI, no gas
  bound derived from the bytecode, and no claim that a verified contract is
  correct — only that it is inside the subset the rest of the system assumes.
* **The engine backends are not tested by this gate.** `wasmi` is what `make
  test` exercises; the optional `wasmtime` backend is CI-only and refuses
  modules that import the host ABI, because this workspace never implemented the
  ABI on that backend. X8 does not change that.

## Defects the phase found (both fixed here)

1. **`call` never read the callee's signature.** The first version of the body
   check popped nothing and pushed nothing for `0x10`, so `store.wasm` was
   refused with `stack_underflow` at two real instructions. Every subsequent
   stack number was arithmetic on a fiction. The verifier now resolves the
   callee through the type section — imported functions through the import
   section, defined ones through the function section — and the gate has a test
   that fails if that resolution is removed (NC-1).
2. **Body offsets were relative to the body slice, not the module.** Every
   problem inside a function pointed at the wrong byte. The body cursor now
   carries the module base, so an offset a reader is given is an offset they can
   check against the file.

## Negative controls

Six tampers, each reddening a *named* test: `call` ignoring the callee
signature, a stack underflow that never reaches the report, float value types
read as `i32`, any export name counting as `run`, the `function`/`code` counts
allowed to disagree, and the size ceiling unenforced. A green baseline runs
first, so a red caused by a broken tree can never be mistaken for a successful
control.

6/6 PASS; total **159** across C0–C10 + S0–S7 + X0–X8.

## What is still missing before this runs on a chain

* **Nothing calls the verifier yet.** X8 is a library and a binary; the node
  phase (D) is what wires it into the deploy path, and until then the claim is
  "a verifier exists and is measured", not "deploys are verified".
* **The budget constants are policy, not measurement.** 200 000 instructions
  per body, 4 096 stack slots, 1 024 locals — all far above what the shipped
  contracts use (measured peak: 630 instructions in one body, 7 stack slots),
  and set so that a legitimate contract is never refused by accident. They will
  be tightened when a gas model can price them.
* **No differential run against a reference validator.** `wasmparser` is in the
  dependency graph through `wasmi`, and a future phase should compare verdicts
  on a fuzz corpus; today the corpus is the 100 mutants plus 6 shipped modules.

Tests: **359** in the workspace, 37 of them in the X8 gate.
Validators / blocks / transactions remain **0 / 0 / 0**.
