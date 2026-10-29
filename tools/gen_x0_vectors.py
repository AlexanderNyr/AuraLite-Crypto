#!/usr/bin/env python3
"""Generate the X0 determinism-census vectors from an INDEPENDENT assembler.

No Rust code, no wasmtime, no network. This file contains a tiny WASM binary
assembler written from the specification (LEB128, sections, opcodes) and emits
20 small modules, each exercising exactly one census rule, together with the
verdict the Rust gate must produce.

Verdicts are `allowed` or `banned:<code>`, where `<code>` matches
`Ban::code()` in `crates/aura-exec/src/census.rs`.

Usage:
    python3 tools/gen_x0_vectors.py
    make gen-x0-vectors
"""

from __future__ import annotations

import struct
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x0_census.txt"

MAGIC = b"\x00asm"
VERSION = struct.pack("<I", 1)

# value types
I32, I64, F32, F64, FUNCREF = 0x7F, 0x7E, 0x7D, 0x7C, 0x70
# opcodes used by the vectors
OP_END, OP_RETURN, OP_CALL, OP_DROP = 0x0B, 0x0F, 0x10, 0x1A
OP_LOCAL_GET, OP_LOCAL_SET, OP_LOCAL_TEE = 0x20, 0x21, 0x22
OP_I32_LOAD, OP_I32_STORE = 0x28, 0x36
OP_MEMORY_SIZE, OP_MEMORY_GROW = 0x3F, 0x40
OP_I32_CONST, OP_I64_CONST = 0x41, 0x42
OP_I32_ADD, OP_I32_EQZ, OP_I64_ADD = 0x6A, 0x45, 0x7C
OP_F32_ADD, OP_F64_ADD, OP_F64_CONVERT = 0x92, 0xA0, 0xB9
OP_LOOP, OP_BLOCK, OP_BR, OP_BR_IF = 0x03, 0x02, 0x0C, 0x0D
OP_CALL_INDIRECT = 0x11
OP_PREFIX_BULK, OP_PREFIX_SIMD, OP_PREFIX_ATOMIC = 0xFC, 0xFD, 0xFE


def uleb(value: int) -> bytes:
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        out.append(byte | (0x80 if value else 0))
        if not value:
            return bytes(out)


def sleb(value: int) -> bytes:
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        done = (value == 0 and not byte & 0x40) or (value == -1 and byte & 0x40)
        out.append(byte | (0 if done else 0x80))
        if done:
            return bytes(out)


def vec(items: list[bytes]) -> bytes:
    return uleb(len(items)) + b"".join(items)


def name(text: str) -> bytes:
    raw = text.encode()
    return uleb(len(raw)) + raw


def section(section_id: int, payload: bytes) -> bytes:
    return bytes([section_id]) + uleb(len(payload)) + payload


def functype(params: list[int], results: list[int]) -> bytes:
    return b"\x60" + vec([bytes([p]) for p in params]) + vec([bytes([r]) for r in results])


def code_body(locals_: list[tuple[int, int]], instructions: bytes) -> bytes:
    body = vec([uleb(count) + bytes([kind]) for count, kind in locals_]) + instructions
    return uleb(len(body)) + body


def module(*sections: bytes) -> bytes:
    return MAGIC + VERSION + b"".join(sections)


def limits(flag: int, minimum: int, maximum: int | None = None) -> bytes:
    out = bytes([flag]) + uleb(minimum)
    if maximum is not None:
        out += uleb(maximum)
    return out


def program_i32_arith() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    body = code_body(
        [(1, I32)],
        bytes([OP_I32_CONST]) + sleb(40) + bytes([OP_I32_CONST]) + sleb(2)
        + bytes([OP_I32_ADD]) + bytes([OP_LOCAL_TEE]) + uleb(0) + bytes([OP_END]),
    )
    return module(types, functions, section(10, vec([body])))


def program_i64_memory_roundtrip() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    memory = section(5, vec([limits(0x01, 1, 16)]))
    body = code_body(
        [],
        bytes([OP_I32_CONST]) + sleb(0)  # address
        + bytes([OP_I64_CONST]) + sleb(-7)
        + bytes([0x37]) + uleb(3) + uleb(8)  # i64.store align=8 offset=8
        + bytes([OP_I32_CONST]) + sleb(0)
        + bytes([0x29]) + uleb(3) + uleb(8)  # i64.load align=8 offset=8
        + bytes([0x1A, OP_I32_CONST]) + sleb(1)
        + bytes([OP_END]),
    )
    return module(types, functions, memory, section(10, vec([body])))


def program_locals_and_calls() -> bytes:
    types = section(1, vec([functype([], [I32]), functype([I32], [I32])]))
    functions = section(3, vec([uleb(0), uleb(1)]))
    outer = code_body(
        [(1, I32)],
        bytes([OP_I32_CONST]) + sleb(7) + bytes([OP_CALL]) + uleb(1)
        + bytes([OP_LOCAL_SET]) + uleb(0) + bytes([OP_LOCAL_GET]) + uleb(0) + bytes([OP_END]),
    )
    inner = code_body([], bytes([OP_LOCAL_GET]) + uleb(0) + bytes([OP_I32_CONST]) + sleb(1)
                      + bytes([OP_I32_ADD]) + bytes([OP_END]))
    return module(types, functions, section(10, vec([outer, inner])))


def program_block_loop_branch() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    body = code_body(
        [(1, I32)],
        bytes([OP_BLOCK, 0x40, OP_LOOP, 0x40])
        + bytes([OP_LOCAL_GET]) + uleb(0) + bytes([OP_I32_CONST]) + sleb(9)
        + bytes([OP_I32_ADD]) + bytes([OP_LOCAL_TEE]) + uleb(0)
        + bytes([OP_I32_CONST]) + sleb(3) + bytes([0x4B])  # i32.gt_u
        + bytes([OP_BR_IF]) + uleb(1) + bytes([OP_BR]) + uleb(0)
        + bytes([OP_END, OP_END, OP_LOCAL_GET]) + uleb(0) + bytes([OP_END]),
    )
    return module(types, functions, section(10, vec([body])))


def program_mutable_global() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    globals_ = section(6, vec([bytes([I32, 0x01, OP_I32_CONST]) + sleb(1) + bytes([OP_END])]))
    body = code_body([], bytes([OP_I32_CONST]) + sleb(1) + bytes([OP_END]))
    return module(types, functions, globals_, section(10, vec([body])))


def program_memory_no_max() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    memory = section(5, vec([limits(0x00, 1)]))
    body = code_body([], bytes([OP_MEMORY_SIZE, 0x00, OP_END]))
    return module(types, functions, memory, section(10, vec([body])))


def program_memory_too_large() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    memory = section(5, vec([limits(0x01, 1, 5_000)]))
    body = code_body([], bytes([OP_MEMORY_GROW, 0x00, OP_END]))
    return module(types, functions, memory, section(10, vec([body])))


def program_import_wasi_clock() -> bytes:
    types = section(1, vec([functype([I32, I32], [I32])]))
    imports = section(2, vec([name("wasi_snapshot_preview1") + name("clock_time_get")
                              + bytes([0x00]) + uleb(0)]))
    return module(types, imports)


def program_import_env_random() -> bytes:
    types = section(1, vec([functype([], [I64])]))
    imports = section(2, vec([name("env") + name("random_get") + bytes([0x00]) + uleb(0)]))
    return module(types, imports)


def program_f32_add() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    body = code_body(
        [], bytes([0x43]) + struct.pack("<f", 1.5) + bytes([0x43]) + struct.pack("<f", 2.5)
        + bytes([OP_F32_ADD, OP_DROP, OP_I32_CONST]) + sleb(0) + bytes([OP_END]),
    )
    return module(types, functions, section(10, vec([body])))


def program_f64_add() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    body = code_body(
        [], bytes([0x44]) + struct.pack("<d", 1.0) + bytes([0x44]) + struct.pack("<d", 2.0)
        + bytes([OP_F64_ADD, OP_DROP, OP_I32_CONST]) + sleb(0) + bytes([OP_END]),
    )
    return module(types, functions, section(10, vec([body])))


def program_f64_nan_const() -> bytes:
    """0.0/0.0 is the canonical NaN: its payload is not specified by the spec."""
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    body = code_body(
        [], bytes([0x44]) + struct.pack("<d", float("nan")) + bytes([OP_DROP, OP_I32_CONST])
        + sleb(0) + bytes([OP_END]),
    )
    return module(types, functions, section(10, vec([body])))


def program_f64_reinterpret() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    body = code_body(
        [], bytes([0x44]) + struct.pack("<d", 0.5) + bytes([OP_F64_CONVERT, OP_DROP, OP_I32_CONST])
        + sleb(0) + bytes([OP_END]),
    )
    return module(types, functions, section(10, vec([body])))


def program_start_section() -> bytes:
    types = section(1, vec([functype([], [])]))
    functions = section(3, vec([uleb(0)]))
    start = section(8, uleb(0))
    body = code_body([], bytes([OP_END]))
    return module(types, functions, start, section(10, vec([body])))


def program_shared_memory() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    memory = section(5, vec([limits(0x03, 1, 16)]))
    body = code_body([], bytes([OP_MEMORY_SIZE, 0x00, OP_END]))
    return module(types, functions, memory, section(10, vec([body])))


def program_atomic_wait() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    memory = section(5, vec([limits(0x01, 1, 16)]))
    body = code_body(
        [], bytes([OP_I32_CONST]) + sleb(0) + bytes([OP_I32_CONST]) + sleb(0)
        + bytes([OP_I32_CONST]) + sleb(0) + bytes([OP_PREFIX_ATOMIC]) + uleb(0)
        + bytes([0x00, OP_DROP, OP_I32_CONST]) + sleb(0) + bytes([OP_END]),
    )
    return module(types, functions, memory, section(10, vec([body])))


def program_simd_v128() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    body = code_body(
        [], bytes([OP_PREFIX_SIMD]) + uleb(12) + bytes([OP_DROP, OP_I32_CONST]) + sleb(0)
        + bytes([OP_END]),
    )
    return module(types, functions, section(10, vec([body])))


def program_bulk_memory_copy() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    memory = section(5, vec([limits(0x01, 1, 16)]))
    body = code_body(
        [], bytes([OP_I32_CONST]) + sleb(0) + bytes([OP_I32_CONST]) + sleb(0)
        + bytes([OP_I32_CONST]) + sleb(0) + bytes([OP_PREFIX_BULK]) + uleb(10)
        + bytes([0x00, 0x00, OP_I32_CONST]) + sleb(0) + bytes([OP_END]),
    )
    return module(types, functions, memory, section(10, vec([body])))


def program_call_indirect() -> bytes:
    types = section(1, vec([functype([], [])]))
    functions = section(3, vec([uleb(0)]))
    table = section(4, vec([bytes([FUNCREF]) + limits(0x01, 1, 1)]))
    body = code_body(
        [], bytes([OP_I32_CONST]) + sleb(0) + bytes([OP_CALL_INDIRECT]) + uleb(0) + b"\x00"
        + bytes([OP_END]),
    )
    return module(types, functions, table, section(10, vec([body])))


def program_unknown_opcode() -> bytes:
    types = section(1, vec([functype([], [I32])]))
    functions = section(3, vec([uleb(0)]))
    body = code_body([], bytes([0x27, OP_I32_CONST]) + sleb(0) + bytes([OP_END]))
    return module(types, functions, section(10, vec([body])))


def program_empty() -> bytes:
    return module()


PROGRAMS: list[tuple[str, bytes, str]] = [
    ("empty", program_empty(), "allowed"),
    ("i32_arith", program_i32_arith(), "allowed"),
    ("i64_memory_roundtrip", program_i64_memory_roundtrip(), "allowed"),
    ("locals_and_calls", program_locals_and_calls(), "allowed"),
    ("block_loop_branch", program_block_loop_branch(), "allowed"),
    ("mutable_global", program_mutable_global(), "banned:mutable_global"),
    ("memory_no_max", program_memory_no_max(), "banned:unbounded_memory"),
    ("memory_too_large", program_memory_too_large(), "banned:memory_too_large"),
    ("import_wasi_clock", program_import_wasi_clock(), "banned:host_import"),
    ("import_env_random", program_import_env_random(), "banned:host_import"),
    ("f32_add", program_f32_add(), "banned:float_instruction"),
    ("f64_add", program_f64_add(), "banned:float_instruction"),
    ("f64_nan_const", program_f64_nan_const(), "banned:float_instruction"),
    ("f64_reinterpret", program_f64_reinterpret(), "banned:float_instruction"),
    ("start_section", program_start_section(), "banned:start_section"),
    ("shared_memory", program_shared_memory(), "banned:shared_memory"),
    ("atomic_wait", program_atomic_wait(), "banned:atomics"),
    ("simd_v128", program_simd_v128(), "banned:simd"),
    ("bulk_memory_copy", program_bulk_memory_copy(), "banned:bulk_memory"),
    ("call_indirect", program_call_indirect(), "banned:call_indirect"),
    ("unknown_opcode", program_unknown_opcode(), "banned:unknown_opcode"),
]


def main() -> int:
    lines = [
        "# X0 determinism-census vectors — independent Python assembler.",
        "# Each program exercises exactly one census rule.",
        "# verdict = allowed | banned:<Ban::code()>",
        f"program_count={len(PROGRAMS)}",
        f"allowed_count={sum(1 for _, _, v in PROGRAMS if v == 'allowed')}",
        f"banned_count={sum(1 for _, _, v in PROGRAMS if v != 'allowed')}",
        "max_memory_pages=4096",
        "max_module_bytes=524288",
        "max_table_entries=1024",
    ]
    for label, blob, verdict in PROGRAMS:
        lines.append(f"program_{label}_hex={blob.hex()}")
        lines.append(f"program_{label}_verdict={verdict}")
        lines.append(f"program_{label}_size={len(blob)}")
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"  programs: {len(PROGRAMS)} "
          f"({sum(1 for _, _, v in PROGRAMS if v == 'allowed')} allowed, "
          f"{sum(1 for _, _, v in PROGRAMS if v != 'allowed')} banned)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
