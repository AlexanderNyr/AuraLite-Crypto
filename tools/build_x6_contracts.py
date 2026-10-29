#!/usr/bin/env python3
"""Build the X6 call-isolation contracts and pin their artifacts.

X6 needs contracts that talk to each other — a vault that pays out before it
debits, an attacker that re-enters it, and a contract that calls itself until the
node refuses — and it needs them as *shipped bytecode*, because the gate runs them
through the real `Lifecycle` and the real engine.

This tool is a deliberate copy of `tools/build_contracts.py` rather than a flag on
it: the X4 fixture is checksummed in `docs/evidence/x4/`, so adding X6's artifacts
to that file would rewrite a closed phase's evidence. Two tools, two fixtures, one
canonical build command.

Every contract in `examples/` is a real Rust crate that depends on `aura-sdk`.
This tool builds each one for `wasm32-unknown-unknown`, copies the `.wasm` into
`examples/<name>/wasm/<name>.wasm`, and writes
`crates/aura-exec/tests/fixtures/x4_contracts.txt` with the **measured** size,
hash, imports and exports of the artifact it just produced.

Why the artifact is committed: the X4 gate in `aura-exec` runs the *shipped*
bytecode through the real `Lifecycle`, so the thing under test is the thing a
deploy would store. The fixture pins its hash, so a rebuild that changes the
bytecode is a visible diff rather than a silently re-baselined number.

Determinism: the same canonical command as X4 (`RUSTFLAGS` with a path remap and
`--max-memory`, release + LTO + `opt-level=z` + `strip`), built twice into two
independent target directories; if the two hashes disagree this tool fails
instead of pinning a hash. The X4 measurement stands: the pin is exact for this
toolchain, these flags and this checkout path.

Usage:
    python3 tools/build_contracts.py            # build, pin, write the fixture
    python3 tools/build_contracts.py --check     # build and compare, write nothing
    make build-contracts
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EXAMPLES = REPO / "examples"
FIXTURE = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x6_calls.txt"
TARGET = Path(os.environ.get("AURA_X6_TARGET", str(REPO.parent / "x6-target")))

CONTRACTS = ["vault", "evil", "recurse"]

# `--max-memory` is not an optimisation: the X0 census refuses linear memory
# without a declared maximum ("without a maximum memory.grow can consume the
# whole node"), and rustc's default wasm output declares only a minimum. The
# linker flag is what makes a Rust contract *deployable* at all, so it belongs in
# the canonical build command rather than in a post-processing step.
MAX_MEMORY_BYTES = 16 * 1024 * 1024  # 256 pages, well under the 4096-page cap

RUSTFLAGS = (
    f"--remap-path-prefix={REPO}=/auralite "
    f"--remap-path-prefix={TARGET}=/target "
    f"-C link-arg=--max-memory={MAX_MEMORY_BYTES}"
)


# --------------------------------------------------------------------------- wasm
class Reader:
    def __init__(self, data: bytes) -> None:
        self.data = data
        self.pos = 0

    def u8(self) -> int:
        value = self.data[self.pos]
        self.pos += 1
        return value

    def uleb(self) -> int:
        result = 0
        shift = 0
        while True:
            byte = self.u8()
            result |= (byte & 0x7F) << shift
            shift += 7
            if not byte & 0x80:
                return result

    def name(self) -> str:
        length = self.uleb()
        raw = self.data[self.pos : self.pos + length]
        self.pos += length
        return raw.decode("utf-8", "replace")

    def bytes(self, count: int) -> bytes:
        raw = self.data[self.pos : self.pos + count]
        self.pos += count
        return raw


KINDS = {0x00: "func", 0x01: "table", 0x02: "memory", 0x03: "global"}


def type_arity(types: list[int], functions: list[int], index: int) -> object:
    """Arity of defined function `index`, or `?` when it is out of range.

    Imports occupy the first function indices, so a `call_indirect`-free module
    still needs this offset; the caller only asks about *defined* functions,
    which is what an export of a contract can be.
    """
    if 0 <= index < len(functions):
        type_index = functions[index]
        if type_index < len(types):
            return types[type_index]
    return "?"


def describe(blob: bytes) -> dict[str, object]:
    """Parse the import and export sections of a WASM module.

    Hand-written on purpose: the point of the fixture is an *independent* reading
    of the artifact, not a restatement of what `aura-exec` believes.
    """
    reader = Reader(blob)
    magic = reader.bytes(4)
    version = reader.bytes(4)
    if magic != b"\x00asm" or version != b"\x01\x00\x00\x00":
        raise SystemExit(f"not a wasm 1.0 module: {magic!r} {version!r}")
    imports: list[str] = []
    exports: list[str] = []
    types: list[int] = []
    functions: list[int] = []
    memory_max: int | None = None
    sections: list[int] = []
    while reader.pos < len(blob):
        section_id = reader.u8()
        size = reader.uleb()
        end = reader.pos + size
        sections.append(section_id)
        if section_id == 2:
            count = reader.uleb()
            for _ in range(count):
                module = reader.name()
                name = reader.name()
                kind = reader.u8()
                imports.append(f"{module}.{name}:{KINDS.get(kind, hex(kind))}")
                if kind == 0x00:
                    reader.uleb()
                elif kind == 0x01:
                    reader.u8()
                    flag = reader.u8()
                    reader.uleb()
                    if flag:
                        reader.uleb()
                elif kind == 0x02:
                    flag = reader.u8()
                    reader.uleb()
                    if flag:
                        memory_max = reader.uleb()
                elif kind == 0x03:
                    reader.u8()
                    reader.u8()
        elif section_id == 1:
            count = reader.uleb()
            for _ in range(count):
                form = reader.u8()
                if form != 0x60:
                    raise SystemExit(f"unexpected type form {form:#x}")
                params = reader.uleb()
                for _ in range(params):
                    reader.u8()
                results = reader.uleb()
                for _ in range(results):
                    reader.u8()
                types.append(params)
        elif section_id == 3:
            count = reader.uleb()
            for _ in range(count):
                functions.append(reader.uleb())
        elif section_id == 7:
            count = reader.uleb()
            for _ in range(count):
                name = reader.name()
                kind = reader.u8()
                index = reader.uleb()
                if kind == 0x00:
                    imported = sum(1 for entry in imports if entry.endswith(":func"))
                    exports.append(
                        f"{name}:{type_arity(types, functions, index - imported)}"
                    )
                else:
                    exports.append(name)
        elif section_id == 5:
            count = reader.uleb()
            for _ in range(count):
                flag = reader.u8()
                reader.uleb()
                if flag:
                    memory_max = reader.uleb()
        reader.pos = end
    return {
        "types": types,
        "imports": imports,
        "exports": exports,
        "memory_max": memory_max,
        "sections": sections,
    }


# --------------------------------------------------------------------------- build
def build(name: str, target_dir: Path) -> Path:
    crate = EXAMPLES / name
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = str(target_dir)
    env["RUSTFLAGS"] = RUSTFLAGS
    env["CARGO_INCREMENTAL"] = "0"
    result = subprocess.run(
        ["cargo", "build", "--release", "--target", "wasm32-unknown-unknown"],
        cwd=crate,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stdout + result.stderr)
        raise SystemExit(f"build failed for examples/{name}")
    artifact = (
        target_dir / "wasm32-unknown-unknown" / "release" / f"aura_example_{name}.wasm"
    )
    if not artifact.exists():
        raise SystemExit(f"missing artifact {artifact}")
    return artifact


def main() -> int:
    check = "--check" in sys.argv
    first = TARGET / "a"
    second = TARGET / "b"
    rows = []
    for name in CONTRACTS:
        one = build(name, first)
        two = build(name, second)
        blob = one.read_bytes()
        again = two.read_bytes()
        if blob != again:
            raise SystemExit(
                f"{name}: NOT reproducible — two builds of the same tree differ "
                f"({len(blob)} vs {len(again)} bytes); refusing to pin a hash"
            )
        info = describe(blob)
        import hashlib

        digest = hashlib.sha256(blob).hexdigest()
        shipped = EXAMPLES / name / "wasm" / f"{name}.wasm"
        moved = "kept" if check else "written"
        if not check:
            shipped.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(one, shipped)
            write_abi(name, blob, digest, info)
        rows.append(
            {
                "name": name,
                "size": len(blob),
                "sha256": digest,
                "imports": info["imports"],
                "exports": info["exports"],
                "memory_max": info["memory_max"],
                "moved": moved,
            }
        )
        print(
            f"{name}: {len(blob)} bytes, sha256 {digest[:16]}…, "
            f"imports {len(info['imports'])}, exports {','.join(info['exports'])}"
        )

    if check:
        pinned = read_fixture()
        drift = [row["name"] for row in rows if pinned.get(row["name"]) != row["sha256"]]
        if drift:
            print(f"PINNED HASH DRIFT: {', '.join(drift)}")
            return 1
        print("PINNED HASHES MATCH")
        return 0

    # The measured block is produced *after* the artifacts are pinned: it deploys
    # them through the real `Lifecycle` and reports gas and digests, so the numbers
    # in the report cannot drift from the bytes in the tree.
    payload = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "--package",
            "aura-exec",
            "--bin",
            "x6-record",
        ],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    if payload.returncode != 0:
        sys.stderr.write(payload.stdout + payload.stderr)
        raise SystemExit("x6-record failed; refusing to write the fixture")
    FIXTURE.write_text(
        render(rows) + "\n" + MARKER + "\n" + payload.stdout, encoding="utf-8"
    )
    print(f"wrote {FIXTURE.relative_to(REPO)}")
    return 0


MARKER = "# --- measured by `cargo run -p aura-exec --bin x6-record` ---"


def measured_block() -> str:
    """Keep the recorder's block: the base pins the artifacts, the recorder pins
    the gas measured through them, and neither tool rewrites the other."""
    if not FIXTURE.exists():
        return f"\n{MARKER}\n"
    text = FIXTURE.read_text(encoding="utf-8")
    if MARKER not in text:
        return f"\n{MARKER}\n"
    return "\n" + text[text.index(MARKER) :]


def write_abi(name: str, blob: bytes, digest: str, info: dict[str, object]) -> None:
    """Write `examples/<name>/abi.json` — the ABI of the *artifact*.

    This is what "ABI-JSON generation" means here, and the honest limit of it:
    the file describes what the bytecode can be called with (entry points and
    their arity), what it imports, how much memory it declares and which exact
    artifact it belongs to. It deliberately does **not** invent a schema for the
    contract's storage: keys are chosen by the author inside `Slot::at`, they are
    not recoverable from the binary, and a declared-storage attribute is a later
    phase's job rather than a guess here.
    """
    exports = []
    for entry in info["exports"]:
        if ":" in entry:
            entry_name, arity = entry.split(":", 1)
            exports.append({"name": entry_name, "arity": int(arity)})
        else:
            exports.append({"name": entry, "arity": None})
    document = {
        "contract": name,
        "abi_version": 1,
        "abi_module": "env",
        "code_sha256": digest,
        "code_size": len(blob),
        "memory_max_pages": info["memory_max"],
        "imports": info["imports"],
        "exports": exports,
    }
    path = EXAMPLES / name / "abi.json"
    path.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def read_fixture() -> dict[str, str]:
    pinned: dict[str, str] = {}
    if not FIXTURE.exists():
        return pinned
    for line in FIXTURE.read_text(encoding="utf-8").splitlines():
        if line.startswith("contract_") and line.endswith("_sha256"):
            name = line[len("contract_") : -len("_sha256")]
            pinned[name] = line.split("=", 1)[1]
        elif line.startswith("contract_") and "_sha256" in line:
            head, value = line.split("=", 1)
            pinned[head[len("contract_") : -len("_sha256")]] = value
    return pinned


def render(rows: list[dict[str, object]]) -> str:
    lines = [
        "# X6 call-isolation contracts — measured from the built artifacts.",
        "# Written by tools/build_x6_contracts.py. Do not edit by hand.",
        "#",
        "# The .wasm files under examples/<name>/wasm/ are the artifacts this",
        "# fixture pins; crates/aura-exec/tests/x6_calls.rs deploys them through",
        "# the real Lifecycle and lets them call each other.",
        "",
        "sdk_abi_surfaces=env,env2,env3",
        "max_call_depth=8",
        "toolchain=1.99.1",
        "profile=release,lto,opt-level=z,codegen-units=1,panic=abort,strip",
        f"link_max_memory_bytes={MAX_MEMORY_BYTES}",
        "",
    ]
    for row in rows:
        lines += [
            f"contract_{row['name']}_size={row['size']}",
            f"contract_{row['name']}_sha256={row['sha256']}",
            f"contract_{row['name']}_imports={','.join(row['imports'])}",
            f"contract_{row['name']}_exports={','.join(row['exports'])}",
            f"contract_{row['name']}_memory_max={row['memory_max']}",
            "",
        ]
    total = sum(int(row["size"]) for row in rows)
    lines += [
        f"contracts_total_bytes={total}",
        f"contracts_count={len(rows)}",
        "",
    ]
    return "\n".join(lines)


if __name__ == "__main__":
    raise SystemExit(main())
