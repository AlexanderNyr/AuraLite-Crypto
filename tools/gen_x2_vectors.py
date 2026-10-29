#!/usr/bin/env python3
"""Generate the X2 (gas + ABI v1) vectors with an INDEPENDENT Python assembler.

Nothing here uses Rust, wasmi or the repository's own gas code. Every module is
assembled from the WASM specification by the helpers of `tools/gen_x0_vectors.py`
(the same independent assembler X0 and X1 already use), and the expected host
gas is recomputed from a **second, hand-written copy of the published gas
table**. If the Rust table drifts, the `expect_host_gas` columns go red, which is
exactly the point: a gas table that is only checked against itself is not
checked at all.

Two blocks per vector:

  * the *expectation* block, written here — module hex, pre-state, and what the
    executor must report (`returned:N`, `out_of_gas`, `trap:<code>`,
    `rejected`), plus the host gas the table implies and the resulting state;
  * the *measured* block, written by `cargo run -p aura-exec --bin x2-record`,
    which runs every vector through the real executor and pins the exact total
    gas. A `wasmi` bump changes instruction metering, so the measured column
    goes red and the bump becomes a reviewed protocol decision.

Usage:
    python3 tools/gen_x2_vectors.py
    make gen-x2-vectors        # generator + recorder
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import gen_x0_vectors as g  # noqa: E402  (independent assembler, shared on purpose)

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x2_golden.txt"

MARKER = "# --- measured by `cargo run -p aura-exec --bin x2-record` ---"

I32 = g.I32

# --- the published gas table, copied by hand (X2 spec, ABI v1) --------------
#
# The Rust table lives in `crates/aura-exec/src/abi.rs`; the X2 test asserts the
# two agree row by row, and every `expect_host_gas` below is the sum of these
# numbers. A typo on either side is a red gate.
GAS_TABLE = {
    "storage_read": (100, 10),
    "storage_write": (200, 25),
    "balance": (50, 0),
    "transfer": (300, 0),
    "emit": (200, 15),
    "hash": (100, 5),
    "block_ctx": (20, 0),
}

ARITY = {
    "storage_read": 4,
    "storage_write": 4,
    "balance": 3,
    "transfer": 3,
    "emit": 4,
    "hash": 3,
    "block_ctx": 2,
}

# --- memory layout shared by every vector -----------------------------------
KEY = 0  # 7 bytes: "counter"
VALUE = 16  # 5 bytes: "hello"
OUT = 64  # 64-byte output buffer, zero-filled
ADDR_OTHER = 128  # 20 bytes of 0x33
AMT = 160  # 16-byte little-endian u128 = 100
AMT_BIG = 176  # 16-byte little-endian u128 = 1_000_000
TOPIC = 192  # 4 bytes: "note"
ADDR_SELF = 224  # 20 bytes of 0x11
BLOB_LEN = 244

CONTRACT = "11" * 20
OTHER = "33" * 20


def blob() -> bytes:
    data = bytearray(BLOB_LEN)
    data[KEY : KEY + 7] = b"counter"
    data[VALUE : VALUE + 5] = b"hello"
    data[TOPIC : TOPIC + 4] = b"note"
    data[ADDR_OTHER : ADDR_OTHER + 20] = b"\x33" * 20
    data[ADDR_SELF : ADDR_SELF + 20] = b"\x11" * 20
    data[AMT : AMT + 16] = (100).to_bytes(16, "little")
    data[AMT_BIG : AMT_BIG + 16] = (1_000_000).to_bytes(16, "little")
    return bytes(data)


def i32(value: int) -> bytes:
    return bytes([g.OP_I32_CONST]) + g.sleb(value)


def call(index: int, args: list[int]) -> bytes:
    out = b"".join(i32(arg) for arg in args)
    return out + bytes([g.OP_CALL]) + g.uleb(index)


def build(
    names: list[str],
    body: bytes,
    *,
    memory: bool = True,
    export_memory: bool = True,
    raw_imports: tuple[bytes, ...] = (),
) -> bytes:
    """One `run() -> i32` module that imports `env` functions in the given order."""
    types: list[bytes] = [g.functype([], [I32])]
    type_of: dict[int, int] = {}
    for name in names:
        arity = ARITY[name]
        if arity not in type_of:
            type_of[arity] = len(types)
            types.append(g.functype([I32] * arity, [I32]))

    imports = [
        g.name("env") + g.name(name) + b"\x00" + g.uleb(type_of[ARITY[name]])
        for name in names
    ]
    imports.extend(raw_imports)
    sections = [g.section(1, g.vec(types)), g.section(2, g.vec(imports)),
                g.section(3, g.vec([g.uleb(0)]))]
    if memory:
        sections.append(g.section(5, g.vec([g.limits(0x01, 1, 16)])))

    # only *function* imports occupy the function index space
    entries = [g.name("run") + b"\x00" + g.uleb(len(names))]
    if memory and export_memory:
        entries.append(g.name("memory") + b"\x02" + g.uleb(0))
    sections.append(g.section(7, g.vec(entries)))
    sections.append(g.section(10, g.vec([g.code_body([], body + bytes([g.OP_END]))])))

    if memory:
        offset = b"\x41" + g.sleb(0) + b"\x0b"
        segment = g.uleb(0) + offset + g.uleb(len(blob())) + blob()
        sections.append(g.section(11, g.vec([segment])))
    return g.module(*sections)


SPIN = (
    bytes([g.OP_BLOCK, 0x40, g.OP_LOOP, 0x40, g.OP_BR]) + g.uleb(0) + bytes([0x0B, 0x0B])
    + i32(0)
)
TRAP = bytes([0x00])  # unreachable
LOAD_I32 = i32(OUT) + bytes([g.OP_I32_LOAD]) + g.uleb(2) + g.uleb(0)


class Vector:
    """One X2 vector plus the plan that produced it."""

    def __init__(self, label: str, wasm: bytes, *, gas: int = 1_000_000, arg: str = ""):
        self.label = label
        self.wasm = wasm
        self.gas = gas
        self.arg = arg
        self.pre_accounts: list[tuple[str, int]] = [(CONTRACT, 1000)]
        self.pre_slots: list[tuple[str, str, bytes]] = []
        self.post_slots: list[tuple[str, str, bytes | None]] = []
        self.post_balances: list[tuple[str, int]] = []
        self.status = "returned:0"
        self.host_gas = 0
        self.writes = 0
        self.events = 0

    def charge(self, name: str, byte_count: int | None) -> None:
        """Bill one ABI call, or nothing when it fails before the charge."""
        base, per_byte = GAS_TABLE[name]
        if byte_count is None:
            return
        self.host_gas += base + per_byte * byte_count

    def lines(self) -> list[str]:
        out = [f"vector={self.label}", f"wasm={self.wasm.hex()}", f"gas={self.gas}",
               f"arg={self.arg}"]
        for address, units in self.pre_accounts:
            out.append(f"pre_account={address}:{units}")
        for address, key, value in self.pre_slots:
            out.append(f"pre_slot={address}:{key}:{value.hex()}")
        out.append(f"expect_status={self.status}")
        out.append(f"expect_host_gas={self.host_gas}")
        out.append(f"expect_writes={self.writes}")
        out.append(f"expect_events={self.events}")
        for address, key, value in self.post_slots:
            encoded = "-" if value is None else value.hex()
            out.append(f"expect_slot={address}:{key}:{encoded}")
        for address, units in self.post_balances:
            out.append(f"expect_balance={address}:{units}")
        return out


def simple(label: str, names: list[str], calls: list[tuple[str, list[int], int | None]],
           *, status: str, tail: bytes = b"", **kwargs) -> Vector:
    """A vector whose body is a sequence of ABI calls, all but the last dropped."""
    body = b""
    for index, (_name, args, _bytes) in enumerate(calls):
        body += call(index, args)
        if index + 1 < len(calls) or tail:
            body += bytes([g.OP_DROP])
    body += tail
    core = {
        key: kwargs.pop(key)
        for key in ("memory", "export_memory", "raw_imports")
        if key in kwargs
    }
    vector = Vector(label, build(names, body, **core), **kwargs)
    vector.status = status
    for name, _args, byte_count in calls:
        vector.charge(name, byte_count)
    return vector


def vectors() -> list[Vector]:
    out: list[Vector] = []

    # --- storage ------------------------------------------------------------
    vector = simple(
        "read_absent", ["storage_read"],
        [("storage_read", [KEY, 7, OUT, 64], 0)],
        status="returned:-9",
    )
    out.append(vector)

    vector = simple(
        "write_then_read", ["storage_write", "storage_read"],
        [("storage_write", [KEY, 7, VALUE, 5], 12), ("storage_read", [KEY, 7, OUT, 64], 5)],
        status="returned:5",
    )
    vector.writes = 1
    vector.post_slots = [(CONTRACT, "counter", b"hello")]
    out.append(vector)

    vector = simple(
        "read_existing", ["storage_read"],
        [("storage_read", [KEY, 7, OUT, 64], 5)],
        status="returned:5",
    )
    vector.pre_slots = [(CONTRACT, "counter", b"hello")]
    out.append(vector)

    vector = simple(
        "key_too_long", ["storage_read"],
        [("storage_read", [KEY, 33, OUT, 64], None)],
        status="returned:-2",
    )
    out.append(vector)

    vector = simple(
        "value_too_long", ["storage_write"],
        [("storage_write", [KEY, 7, 1000, 4097], None)],
        status="returned:-3",
    )
    out.append(vector)

    vector = simple(
        "ptr_out_of_bounds", ["storage_read"],
        [("storage_read", [65530, 100, OUT, 64], None)],
        status="returned:-1",
    )
    out.append(vector)

    # --- balances and transfers --------------------------------------------
    vector = simple(
        "balance_other", ["balance"], [("balance", [ADDR_OTHER, 20, OUT], 0)],
        status="returned:5000", tail=LOAD_I32,
    )
    vector.pre_accounts = [(CONTRACT, 1000), (OTHER, 5000)]
    out.append(vector)

    vector = simple(
        "balance_self", ["balance"], [("balance", [ADDR_SELF, 20, OUT], 0)],
        status="returned:1000", tail=LOAD_I32,
    )
    out.append(vector)

    vector = simple(
        "transfer_ok", ["transfer"], [("transfer", [ADDR_OTHER, 20, AMT], 0)],
        status="returned:0",
    )
    vector.writes = 2
    vector.post_balances = [(CONTRACT, 900), (OTHER, 100)]
    out.append(vector)

    vector = simple(
        "transfer_insufficient", ["transfer"], [("transfer", [ADDR_OTHER, 20, AMT_BIG], 0)],
        status="returned:-6",
    )
    vector.post_balances = [(CONTRACT, 1000), (OTHER, 0)]
    out.append(vector)

    # --- events and hashing -------------------------------------------------
    vector = simple(
        "emit_event", ["emit"], [("emit", [TOPIC, 4, VALUE, 5], 9)],
        status="returned:0",
    )
    vector.events = 1
    out.append(vector)

    vector = simple(
        "hash_blake3", ["hash"], [("hash", [VALUE, 5, OUT], 5)],
        status="returned:0",
    )
    out.append(vector)

    # --- consensus context --------------------------------------------------
    vector = simple(
        "block_height", ["block_ctx"], [("block_ctx", [0, OUT], 0)],
        status="returned:7", tail=LOAD_I32,
    )
    out.append(vector)

    vector = simple(
        "block_bad_field", ["block_ctx"], [("block_ctx", [9, OUT], 0)],
        status="returned:-10",
    )
    out.append(vector)

    # --- gas: refusals, out-of-gas, rollback --------------------------------
    vector = simple(
        "out_of_gas_first_call", ["storage_write"],
        [("storage_write", [KEY, 7, VALUE, 5], None)],
        status="out_of_gas", gas=150,
    )
    out.append(vector)

    # enough gas for the write, not enough for the read that follows it
    vector = simple(
        "oog_between_two_calls", ["storage_write", "storage_read"],
        [("storage_write", [KEY, 7, VALUE, 5], 12), ("storage_read", [KEY, 7, OUT, 64], None)],
        status="out_of_gas", gas=700,
    )
    vector.writes = 1
    out.append(vector)

    # write, then burn the rest of the budget in pure computation
    vector = Vector(
        "write_then_spin",
        build(["storage_write"], call(0, [KEY, 7, VALUE, 5]) + bytes([g.OP_DROP]) + SPIN),
    )
    vector.status = "out_of_gas"
    vector.charge("storage_write", 12)
    vector.writes = 1
    out.append(vector)

    vector = Vector(
        "write_then_trap",
        build(["storage_write"], call(0, [KEY, 7, VALUE, 5]) + bytes([g.OP_DROP]) + TRAP),
    )
    vector.status = "trap:trap_unreachable"
    vector.charge("storage_write", 12)
    vector.writes = 1
    out.append(vector)

    vector = Vector("emit_then_trap", build(["emit"], call(0, [TOPIC, 4, VALUE, 5]) + bytes([g.OP_DROP]) + TRAP))
    vector.status = "trap:trap_unreachable"
    vector.charge("emit", 9)
    out.append(vector)

    # --- import census ------------------------------------------------------
    wasi = g.name("wasi_snapshot_preview1") + g.name("clock_time_get") + b"\x00" + g.uleb(0)
    vector = Vector(
        "import_bad_module",
        build([], i32(0), raw_imports=(wasi,)),
    )
    vector.status = "rejected"
    out.append(vector)

    unknown = g.name("env") + g.name("random") + b"\x00" + g.uleb(0)
    vector = Vector("import_unknown_env", build([], i32(0), raw_imports=(unknown,)))
    vector.status = "rejected"
    out.append(vector)

    memory_import = g.name("env") + g.name("memory") + b"\x02" + g.limits(0x00, 1)
    vector = Vector(
        "import_memory",
        build([], i32(0), memory=False, raw_imports=(memory_import,)),
    )
    vector.status = "rejected"
    out.append(vector)

    # imports ABI v1 but never exports its memory: the ABI must refuse, not trap
    vector = simple(
        "no_memory_export", ["storage_read"],
        [("storage_read", [KEY, 7, OUT, 64], None)],
        status="returned:-1", export_memory=False,
    )
    out.append(vector)

    return out


def main() -> int:
    items = vectors()
    lines = [
        "# X2 golden vectors — gas model and ABI v1.",
        "#",
        "# Expectation block: tools/gen_x2_vectors.py (independent assembler + an",
        "# independent copy of the gas table). Measured block: x2-record.",
        "# Do not edit by hand.",
        "",
        "abi_module=env",
        "abi_version=1",
        "max_key_len=32",
        "max_value_len=4096",
        "max_topic_len=64",
        "",
    ]
    for name, (base, per_byte) in GAS_TABLE.items():
        lines.append(f"gas_table={name}:{base}:{per_byte}")
    lines.append("")
    for vector in items:
        lines.extend(vector.lines())
        lines.append("")
    FIXTURE.write_text("\n".join(lines))
    print(f"wrote {FIXTURE} ({len(items)} vectors)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
