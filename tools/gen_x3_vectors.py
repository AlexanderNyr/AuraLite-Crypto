#!/usr/bin/env python3
"""Generate the X3 (contract lifecycle) programs with an INDEPENDENT assembler.

The lifecycle gate needs modules that export more than one entry point (`init`,
`run`, `migrate`) and that behave differently across code versions. Every module
here is assembled from the WASM specification by `tools/gen_x0_vectors.py`
helpers — no Rust, no wasmi, no network — so the executor never builds its own
inputs.

The file has two blocks:

  * the *expectation* block, written here: program hex, the entry points each
    module exports, and the storage keys its code writes;
  * the *measured* block, written by `cargo run -p aura-exec --bin x3-record`,
    which runs the canonical lifecycle scenarios (deploy → call → upgrade → call)
    and pins the exact gas of each step. A metering change turns the gate red.

Usage:
    python3 tools/gen_x3_vectors.py
    make gen-x3-vectors        # generator + recorder
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import gen_x0_vectors as g  # noqa: E402  (independent assembler, shared on purpose)

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x3_golden.txt"

MARKER = "# --- measured by `cargo run -p aura-exec --bin x3-record` ---"

I32 = g.I32

# --- memory layout shared by the lifecycle programs -------------------------
KEY_VERSION = 0  # 7 bytes  "version"
VALUE_ONE = 16  # 1 byte   "1"
VALUE_TWO = 24  # 1 byte   "2"
KEY_MIGRATED = 32  # 8 bytes  "migrated"
VALUE_YES = 48  # 3 bytes  "yes"
OUT = 64  # output buffer
BLOB_LEN = 96

STORAGE_WRITE = "storage_write"
STORAGE_READ = "storage_read"
ARITY = {STORAGE_WRITE: 4, STORAGE_READ: 4}


def blob() -> bytes:
    data = bytearray(BLOB_LEN)
    data[KEY_VERSION : KEY_VERSION + 7] = b"version"
    data[VALUE_ONE] = ord("1")
    data[VALUE_TWO] = ord("2")
    data[KEY_MIGRATED : KEY_MIGRATED + 8] = b"migrated"
    data[VALUE_YES : VALUE_YES + 3] = b"yes"
    return bytes(data)


def i32(value: int) -> bytes:
    return bytes([g.OP_I32_CONST]) + g.sleb(value)


def call(index: int, args: list[int]) -> bytes:
    out = b"".join(i32(arg) for arg in args)
    return out + bytes([g.OP_CALL]) + g.uleb(index)


def module(
    imports: list[str],
    funcs: list[tuple[str, list[int], bytes]],
    *,
    memory: bool = True,
    data: bytes | None = None,
) -> bytes:
    """Assemble a module from imported ABI functions and local functions.

    `funcs` is `(export_name, param_types, body_without_final_end)`; the body may
    call the imported functions by their index in `imports` and local functions
    by `len(imports) + position`.
    """
    types: list[bytes] = []
    type_index: dict[tuple[tuple[int, ...], tuple[int, ...]], int] = {}

    def register(params: list[int], results: list[int]) -> int:
        key = (tuple(params), tuple(results))
        if key not in type_index:
            type_index[key] = len(types)
            types.append(g.functype(params, results))
        return type_index[key]

    import_entries = [
        g.name("env") + g.name(name) + b"\x00" + g.uleb(register([I32] * ARITY[name], [I32]))
        for name in imports
    ]
    local_types = [register(params, [I32]) for _name, params, _body in funcs]

    sections = [
        g.section(1, g.vec(types)),
        g.section(2, g.vec(import_entries)),
        g.section(3, g.vec([g.uleb(index) for index in local_types])),
    ]
    if memory:
        sections.append(g.section(5, g.vec([g.limits(0x01, 1, 16)])))

    export_entries = [g.name("memory") + b"\x02" + g.uleb(0)] if memory else []
    for position, (name, _params, _body) in enumerate(funcs):
        export_entries.insert(
            0, g.name(name) + b"\x00" + g.uleb(len(imports) + position)
        )
    sections.append(g.section(7, g.vec(export_entries)))
    sections.append(
        g.section(
            10,
            g.vec(
                [g.code_body([], body + bytes([g.OP_END])) for _name, _p, body in funcs]
            ),
        )
    )
    if memory:
        payload = data if data is not None else blob()
        offset = b"\x41" + g.sleb(0) + b"\x0b"
        segment = g.uleb(0) + offset + g.uleb(len(payload)) + payload
        sections.append(g.section(11, g.vec([segment])))
    return g.module(*sections)


def write_slot(key: int, key_len: int, value: int, value_len: int) -> bytes:
    return call(0, [key, key_len, value, value_len])


# --- programs ---------------------------------------------------------------


def program_v1() -> bytes:
    """`init` stamps version=1, `run` returns 1. No `migrate`."""
    return module(
        [STORAGE_WRITE],
        [
            ("init", [], write_slot(KEY_VERSION, 7, VALUE_ONE, 1)),
            ("run", [], i32(1)),
        ],
    )


def program_v2() -> bytes:
    """`init` stamps version=2, `migrate(from,to)` stamps version=to and a
    `migrated` marker, `run` returns 2."""
    return module(
        [STORAGE_WRITE],
        [
            ("init", [], write_slot(KEY_VERSION, 7, VALUE_TWO, 1)),
            (
                "migrate",
                [I32, I32],
                write_slot(KEY_VERSION, 7, VALUE_TWO, 1)
                + bytes([g.OP_DROP])
                + write_slot(KEY_MIGRATED, 8, VALUE_YES, 3),
            ),
            ("run", [], i32(2)),
        ],
    )


def program_v3() -> bytes:
    """`run` returns 3 and writes a marker, `migrate` clears the marker."""
    return module(
        [STORAGE_WRITE],
        [
            (
                "migrate",
                [I32, I32],
                write_slot(KEY_MIGRATED, 8, VALUE_YES, 3),
            ),
            (
                "run",
                [],
                write_slot(KEY_VERSION, 7, VALUE_TWO, 1) + bytes([g.OP_DROP]) + i32(3),
            ),
        ],
    )


def program_run_only() -> bytes:
    """No `init`, no `migrate`: deploy must succeed without any constructor."""
    return module([STORAGE_WRITE], [("run", [], i32(7))])


def program_trap_init() -> bytes:
    """`init` traps: the whole deploy must roll back."""
    return module(
        [STORAGE_WRITE],
        [
            ("init", [], write_slot(KEY_VERSION, 7, VALUE_ONE, 1) + bytes([g.OP_DROP, 0x00])),
            ("run", [], i32(9)),
        ],
    )


def program_bad_init_arity() -> bytes:
    """`init` with three parameters: refused at deploy, never executed."""
    return module(
        [STORAGE_WRITE],
        [
            ("init", [I32, I32, I32], write_slot(KEY_VERSION, 7, VALUE_ONE, 1)),
            ("run", [], i32(10)),
        ],
    )


def program_no_run() -> bytes:
    """No `run` export: the module is not callable, so deploy refuses it."""
    return module([], [("other", [], i32(11))])


def program_float() -> bytes:
    """Contains an f32 instruction: the X0 census refuses it at deploy."""
    body = i32(0) + bytes([g.OP_F32_ADD, g.OP_DROP]) + i32(12)
    return module([], [("run", [], body)])


def program_out_of_gas() -> bytes:
    """`init` loops forever: the deploy must fail as out-of-gas and roll back."""
    spin = bytes([g.OP_BLOCK, 0x40, g.OP_LOOP, 0x40, g.OP_BR]) + g.uleb(0) + bytes(
        [0x0B, 0x0B]
    )
    return module([], [("init", [], spin + i32(0)), ("run", [], i32(13))])


# --- emit -------------------------------------------------------------------

PROGRAMS = [
    ("v1", program_v1(), "init,run"),
    ("v2", program_v2(), "init,migrate,run"),
    ("v3", program_v3(), "migrate,run"),
    ("run_only", program_run_only(), "run"),
    ("trap_init", program_trap_init(), "init,run"),
    ("bad_init_arity", program_bad_init_arity(), "init,run"),
    ("no_run", program_no_run(), "other"),
    ("float", program_float(), "run"),
    ("oog_init", program_out_of_gas(), "init,run"),
]


def main() -> int:
    lines = [
        "# X3 golden vectors — contract lifecycle (deploy, code hash, version).",
        "#",
        "# Expectation block: tools/gen_x3_vectors.py (independent assembler).",
        "# Measured block: x3-record. Do not edit by hand.",
        "",
        f"code_format_version=1",
        f"code_magic=ALCRYCD1",
        f"deploy_base_gas=1000",
        f"code_byte_gas=10",
        f"key_version=version",
        f"key_migrated=migrated",
        f"value_one=1",
        f"value_two=2",
        "",
    ]
    for name, wasm, exports in PROGRAMS:
        lines.append(f"program_{name}_hex={wasm.hex()}")
        lines.append(f"program_{name}_exports={exports}")
        lines.append(f"program_{name}_len={len(wasm)}")
        lines.append("")
    FIXTURE.write_text("\n".join(lines))
    print(f"wrote {FIXTURE} ({len(PROGRAMS)} programs)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
