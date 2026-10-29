#!/usr/bin/env python3
"""Generate the X1 sandbox vectors with an INDEPENDENT Python assembler.

No Rust, no wasmtime, no wasmi, no network: every module is assembled from the
WASM specification (LEB128, section framing, opcodes) by the helpers of
`tools/gen_x0_vectors.py`, which the X0 phase already uses. The sandbox under
test never builds its own inputs.

The file has two blocks:

  * the *expectation* block, written here: program hex and the class of result
    the sandbox must produce (`return:42`, `trap:trap_out_of_fuel`,
    `censored:<Ban::code()>`, `instantiate`, `limit`);
  * the *measured* block, written by `cargo run -p aura-exec --bin x1-record`,
    which runs every program through the real sandbox and pins the exact fuel
    and trace length. A `wasmi` version bump changes the measured fuel, so the
    gate goes red and the bump becomes a reviewed protocol decision.

Usage:
    python3 tools/gen_x1_vectors.py
    make gen-x1-vectors        # generator + recorder
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import gen_x0_vectors as g  # noqa: E402  (independent assembler, shared on purpose)

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x1_golden.txt"

I32 = g.I32
I64 = g.I64

MARKER = "# --- measured by `cargo run -p aura-exec --bin x1-record` ---"


def export(name: str, index: int = 0) -> bytes:
    return g.section(7, g.vec([g.name(name) + b"\x00" + g.uleb(index)]))


def single(ins: bytes, params: list[int] | None = None, results: list[int] | None = None,
           locals_: list[tuple[int, int]] | None = None, extra: tuple[bytes, ...] = (),
           exports: tuple[str, ...] = ("run",)) -> bytes:
    """One function, optional memory, exports in order of `exports`."""
    params = params or []
    results = results if results is not None else [I32]
    types = g.section(1, g.vec([g.functype(params, results)]))
    funcs = g.section(3, g.vec([g.uleb(0)]))
    export_payload = g.vec([g.name(name) + b"\x00" + g.uleb(0) for name in exports])
    parts = [types, funcs, *extra, g.section(7, export_payload),
             g.section(10, g.vec([g.code_body(locals_ or [], ins)]))]
    return g.module(*parts)


def i32(value: int) -> bytes:
    return bytes([g.OP_I32_CONST]) + g.sleb(value)


def i64(value: int) -> bytes:
    return bytes([g.OP_I64_CONST]) + g.sleb(value)


# --- allowed programs: the sandbox must run them and return a value ----------


def prog_const42() -> bytes:
    return single(i32(42) + bytes([g.OP_END]))


def prog_add_mul() -> bytes:
    """(n * 3 + 1) — pure integer arithmetic, no memory."""
    return single(
        bytes([g.OP_LOCAL_GET]) + g.uleb(0) + i32(3) + bytes([0x6C])  # i32.mul
        + i32(1) + bytes([g.OP_I32_ADD, g.OP_END]),
        params=[I32],
    )


def prog_sum_to_n() -> bytes:
    """1 + 2 + ... + n, a loop with br_if and a local accumulator."""
    ins = (
        bytes([g.OP_BLOCK, 0x40, g.OP_LOOP, 0x40])
        + bytes([g.OP_LOCAL_GET]) + g.uleb(0) + bytes([g.OP_I32_EQZ, g.OP_BR_IF]) + g.uleb(1)
        + bytes([g.OP_LOCAL_GET]) + g.uleb(1)
        + bytes([g.OP_LOCAL_GET]) + g.uleb(0) + bytes([g.OP_I32_ADD, g.OP_LOCAL_SET]) + g.uleb(1)
        + bytes([g.OP_LOCAL_GET]) + g.uleb(0) + i32(1) + bytes([0x6B])  # i32.sub
        + bytes([g.OP_LOCAL_SET]) + g.uleb(0)
        + bytes([g.OP_BR]) + g.uleb(0)
        + bytes([0x0B, 0x0B])
        + bytes([g.OP_LOCAL_GET]) + g.uleb(1) + bytes([g.OP_END])
    )
    return single(ins, params=[I32], locals_=[(1, I32)])


def prog_mem_roundtrip() -> bytes:
    """i64 store then load through linear memory, returns the value."""
    memory = g.section(5, g.vec([g.limits(0x01, 1, 16)]))
    ins = (
        i32(0) + i64(7) + bytes([0x37]) + g.uleb(3) + g.uleb(0)  # i64.store align=8
        + i32(0) + bytes([0x29]) + g.uleb(3) + g.uleb(0)  # i64.load align=8
        + bytes([0xA7])  # i64 -> i32 (wrap)
        + bytes([g.OP_END])
    )
    return single(ins, extra=(memory,))


def prog_mem_grow() -> bytes:
    """memory.grow(1) then memory.size -> 2 pages."""
    memory = g.section(5, g.vec([g.limits(0x01, 1, 4096)]))
    ins = i32(1) + bytes([g.OP_MEMORY_GROW, 0x00, g.OP_DROP, g.OP_MEMORY_SIZE, 0x00, g.OP_END])
    return single(ins, extra=(memory,))


def prog_branchy() -> bytes:
    """A block/loop with two br_if exits; deterministic control flow."""
    ins = (
        bytes([g.OP_BLOCK, 0x40, g.OP_LOOP, 0x40])
        + bytes([g.OP_LOCAL_GET]) + g.uleb(0) + i32(4) + bytes([0x4A])  # i32.gt_s
        + bytes([g.OP_BR_IF]) + g.uleb(1)
        + bytes([g.OP_LOCAL_GET]) + g.uleb(1) + i32(1) + bytes([g.OP_I32_ADD, g.OP_LOCAL_SET]) + g.uleb(1)
        + bytes([g.OP_LOCAL_GET]) + g.uleb(0) + i32(1) + bytes([g.OP_I32_ADD, g.OP_LOCAL_SET]) + g.uleb(0)
        + bytes([g.OP_BR]) + g.uleb(0)
        + bytes([0x0B, 0x0B])
        + bytes([g.OP_LOCAL_GET]) + g.uleb(1) + bytes([g.OP_END])
    )
    return single(ins, params=[I32], locals_=[(1, I32)])


# --- trapping programs ------------------------------------------------------


def prog_spin() -> bytes:
    """An infinite loop: only the fuel budget can stop it."""
    ins = bytes([g.OP_BLOCK, 0x40, g.OP_LOOP, 0x40, g.OP_BR]) + g.uleb(0) + bytes(
        [0x0B, 0x0B]
    ) + i32(0) + bytes([g.OP_END])
    return single(ins)


def prog_unreachable() -> bytes:
    return single(bytes([0x00]) + i32(0) + bytes([g.OP_END]))


def prog_mem_oob() -> bytes:
    """Read past the end of a 1-page memory."""
    memory = g.section(5, g.vec([g.limits(0x01, 1, 16)]))
    ins = i32(65536) + bytes([0x28]) + g.uleb(2) + g.uleb(0) + bytes([g.OP_END])
    return single(ins, extra=(memory,))


def prog_div_zero() -> bytes:
    return single(i32(1) + i32(0) + bytes([0x6D]) + bytes([g.OP_END]))  # i32.div_s


def prog_deep_recursion() -> bytes:
    """run(n) = run(n + 1): no base case, so either the stack or fuel stops it."""
    ins = bytes([g.OP_LOCAL_GET]) + g.uleb(0) + i32(1) + bytes([g.OP_I32_ADD, g.OP_CALL]) + g.uleb(0) + bytes([g.OP_END])
    return single(ins, params=[I32])


# --- rejected programs ------------------------------------------------------


def prog_wasi_clock() -> bytes:
    types = g.section(1, g.vec([g.functype([I32, I32], [I32])]))
    imports = g.section(2, g.vec([
        g.name("wasi_snapshot_preview1") + g.name("clock_time_get") + b"\x00" + g.uleb(0)
    ]))
    return g.module(types, imports)


def prog_import_env_random() -> bytes:
    types = g.section(1, g.vec([g.functype([], [I64])]))
    imports = g.section(2, g.vec([g.name("env") + g.name("random_get") + b"\x00" + g.uleb(0)]))
    return g.module(types, imports)


def prog_f32_add() -> bytes:
    ins = (
        bytes([0x43]) + b"\x00\x00\x80\x3f"  # f32.const 1.0
        + bytes([0x43]) + b"\x00\x00\x00\x40"  # f32.const 2.0
        + bytes([0x92, g.OP_DROP]) + i32(0) + bytes([g.OP_END])  # f32.add
    )
    return single(ins)


def prog_start_section() -> bytes:
    body = g.code_body([], bytes([g.OP_END]))
    return g.module(
        g.section(1, g.vec([g.functype([], [])])),
        g.section(3, g.vec([g.uleb(0)])),
        g.section(8, g.uleb(0)),
        g.section(10, g.vec([body])),
    )


def prog_mutable_global() -> bytes:
    globals_ = g.section(6, g.vec([bytes([I32, 0x01, 0x41]) + g.sleb(1) + bytes([g.OP_END])]))
    return single(i32(1) + bytes([g.OP_END]), extra=(globals_,))


def prog_no_run_export() -> bytes:
    return single(i32(1) + bytes([g.OP_END]), exports=("not_run",))


def prog_run_two_params() -> bytes:
    return single(bytes([g.OP_LOCAL_GET]) + g.uleb(0) + bytes([g.OP_END]), params=[I32, I32])


def prog_truncated() -> bytes:
    return b"\x00asm\x01\x00\x00\x00\x01"


# The i32 argument passed to a `(i32) -> i32` export; empty means `() -> i32`.
INPUTS: dict[str, str] = {
    "add_mul": "10",
    "sum_to_n": "1000",
    "branchy": "0",
    "deep_recursion": "0",
}

PROGRAMS: list[tuple[str, bytes, str]] = [
    # allowed: executed, value and fuel are pinned
    ("const42", prog_const42(), "return:42"),
    ("add_mul", prog_add_mul(), "return:31"),
    ("sum_to_n", prog_sum_to_n(), "return:500500"),
    ("mem_roundtrip", prog_mem_roundtrip(), "return:7"),
    ("mem_grow", prog_mem_grow(), "return:2"),
    ("branchy", prog_branchy(), "return:5"),
    # traps: the sandbox must classify them, never let them escape
    ("spin", prog_spin(), "trap:trap_out_of_fuel"),
    ("unreachable", prog_unreachable(), "trap:trap_unreachable"),
    ("mem_oob", prog_mem_oob(), "trap:trap_memory_oob"),
    ("div_zero", prog_div_zero(), "trap:trap_div_by_zero"),
    ("deep_recursion", prog_deep_recursion(), "trap"),
    # censored by X0 before the engine sees them
    ("wasi_clock", prog_wasi_clock(), "censored:host_import"),
    ("import_env_random", prog_import_env_random(), "censored:host_import"),
    ("f32_add", prog_f32_add(), "censored:float_instruction"),
    ("start_section", prog_start_section(), "censored:start_section"),
    ("mutable_global", prog_mutable_global(), "censored:mutable_global"),
    ("truncated", prog_truncated(), "censored:truncated"),
    # structural failures
    ("no_run_export", prog_no_run_export(), "instantiate"),
    ("run_two_params", prog_run_two_params(), "limit:run_arity"),
]


def main() -> int:
    lines = [
        "# X1 sandbox vectors — independent Python assembler.",
        "# expect = return:<i32> | trap:<TrapKind::code()> | trap | censored:<Ban::code()>",
        "#          | instantiate | limit:run_arity",
        "# input = the i32 argument passed to a `(i32) -> i32` run export (empty for () -> i32)",
        f"engine=wasmi",
        f"engine_version={g.__dict__.get('WASMI_VERSION', '2.0.0')}",
        "fuel=1000000",
        "max_memory_pages=4096",
        "max_module_bytes=524288",
        "max_table_entries=1024",
        "max_trace_events=4096",
    ]
    for label, blob, expect in PROGRAMS:
        lines.append(f"program_{label}_hex={blob.hex()}")
        lines.append(f"program_{label}_expect={expect}")
        lines.append(f"program_{label}_input={INPUTS.get(label, '')}")
    lines.append(MARKER)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines, {len(PROGRAMS)} programs)")
    print("  next: cargo run -p aura-exec --bin x1-record   (writes the measured block)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
