#!/usr/bin/env python3
"""Build the X5 reference standards and pin their artifacts.

Every contract under `standards/` is a real Rust crate that depends only on
`aura-sdk`. This tool builds each one for `wasm32-unknown-unknown`, copies the
`.wasm` to `standards/<name>/wasm/<name>.wasm`, writes `standards/<name>/abi.json`
and pins the **measured** size and hash in
`crates/aura-exec/tests/fixtures/x5_standards.txt`.

Why the artifacts are committed: the X5 gates in `aura-exec` deploy the *shipped*
bytecode through the real `Lifecycle`, so the thing under test is the thing a
deploy would store. The fixture pins its hash, so a rebuild that changes the
bytecode is a visible diff instead of a silently re-baselined number.

# The linker flag has to be target-scoped

X4 set `RUSTFLAGS` for the whole build and it worked, because every host build
script in its dependency graph had already been compiled. It is not correct: the
same flags reach the *host* build of a proc-macro's build script, and
`-C link-arg=--max-memory=…` is not a flag `cc` understands. The canonical way is
`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS`, which cargo applies only when
compiling for that target. MEASURED (2026-10-29): with the global `RUSTFLAGS`
form, a build that needs to recompile `proc-macro2`'s build script fails with
`cc: error: unrecognized command-line option '--max-memory=16777216'`; with the
target-scoped form it succeeds.

`--max-memory` is not an optimisation: the X0 census refuses linear memory without
a declared maximum, and rustc declares only a minimum. 16 MiB = 256 pages, well
under the 4096-page cap.

Usage:
    python3 tools/build_standards.py            # build, pin, write the fixture
    python3 tools/build_standards.py --check    # build and compare, write nothing
    make build-standards / make check-standards
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
STANDARDS = REPO / "standards"
FIXTURE = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x5_standards.txt"
TARGET = Path(os.environ.get("AURA_STANDARDS_TARGET", str(REPO.parent / "standards-target")))

# (directory, crate name, wasm file name, ABI functions the contract imports)
STANDARDS_LIST = [
    ("arc20", "aura-standard-arc20", "arc20"),
    ("arc721", "aura-standard-arc721", "arc721"),
    ("multisig", "aura-standard-multisig", "multisig"),
]

MAX_MEMORY_BYTES = 16 * 1024 * 1024
WASM_TARGET = "wasm32-unknown-unknown"

TARGET_RUSTFLAGS = (
    f"--remap-path-prefix={REPO}=/auralite "
    f"--remap-path-prefix={TARGET}=/target "
    f"-C link-arg=--max-memory={MAX_MEMORY_BYTES}"
)

# The ABI v2 surface a standard may import. Kept here as a second, independent
# copy: the census in `aura-exec` is the authority, and a divergence between the
# two is a failure, not a difference of opinion.
ABI2_FUNCTIONS = [
    "calldata_len",
    "calldata_read",
    "return_write",
    "caller",
    "self_address",
    "abi_version",
]
ABI1_FUNCTIONS = [
    "storage_read",
    "storage_write",
    "balance",
    "transfer",
    "emit",
    "hash",
    "block_ctx",
]

METHODS = {
    "arc20": [
        (0x01, "transfer", ["address", "u128"], None),
        (0x02, "approve", ["address", "u128"], None),
        (0x03, "transfer_from", ["address", "address", "u128"], None),
        (0x04, "balance_of", ["address"], "u128"),
        (0x05, "allowance", ["address", "address"], "u128"),
        (0x06, "total_supply", [], "u128"),
        (0x07, "mint", ["address", "u128"], None),
        (0x08, "burn", ["u128"], None),
        (0x09, "owner", [], "address"),
        (0x0A, "transfer_ownership", ["address"], None),
    ],
    "arc721": [
        (0x01, "mint", ["address", "u128"], None),
        (0x02, "transfer", ["address", "u128"], None),
        (0x03, "approve", ["address", "u128"], None),
        (0x04, "transfer_from", ["address", "address", "u128"], None),
        (0x05, "owner_of", ["u128"], "address"),
        (0x06, "get_approved", ["u128"], "address"),
        (0x07, "balance_of", ["address"], "u128"),
        (0x08, "total_supply", [], "u128"),
        (0x09, "burn", ["u128"], None),
        (0x0A, "owner", [], "address"),
    ],
    "multisig": [
        (0x01, "set_policy", ["u8", "address", "address", "address"], None),
        (0x02, "propose", ["u8", "address", "u128"], "u128"),
        (0x03, "approve", ["u128"], None),
        (0x04, "revoke", ["u128"], None),
        (0x05, "proposal", ["u128"], "bytes:33"),
        (0x06, "signers", [], "bytes:60"),
        (0x07, "threshold", [], "u8"),
        (0x08, "creator", [], "address"),
    ],
}


def run(cmd: list[str], cwd: Path, env: dict[str, str], quiet: bool = True) -> str:
    result = subprocess.run(
        cmd,
        cwd=cwd,
        env=env,
        capture_output=quiet,
        text=True,
    )
    if result.returncode != 0:
        sys.stdout.write(result.stdout or "")
        sys.stderr.write(result.stderr or "")
        raise SystemExit(f"FAILED: {' '.join(cmd)} -> exit {result.returncode}")
    return result.stdout or ""


def build_env() -> dict[str, str]:
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = str(TARGET)
    env["CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS"] = TARGET_RUSTFLAGS
    env.pop("RUSTFLAGS", None)
    # Respect an explicit Rust installation, and otherwise use rustup's actual
    # defaults. Pointing at a fresh private cache makes rustup shims select an
    # empty toolchain home and fails with E0463 (missing target `core`).
    env.setdefault("CARGO_HOME", str(Path.home() / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(Path.home() / ".rustup"))
    env["PATH"] = str(Path(env["CARGO_HOME"]) / "bin") + os.pathsep + env.get("PATH", "")
    return env


def build(name: str, crate: str, env: dict[str, str]) -> Path:
    directory = STANDARDS / name
    run(
        ["cargo", "build", "--target", WASM_TARGET, "--release", "--quiet"],
        cwd=directory,
        env=env,
    )
    artifact = TARGET / WASM_TARGET / "release" / f"{crate.replace('-', '_')}.wasm"
    if not artifact.exists():
        raise SystemExit(f"FAILED: {artifact} was not produced")
    return artifact


def read_imports(wasm: bytes) -> list[str]:
    """Minimal wasm import reader: (module, name) pairs, in order.

    Deliberately literal: this tool must be able to disagree with the engine, so it
    does not share the engine's parser. A wasm module starts with the magic and
    version, then sections; section 2 is the import section.
    """
    if wasm[:4] != b"\x00asm":
        raise SystemExit("FAILED: not a wasm module")

    def uleb(data: bytes, pos: int) -> tuple[int, int]:
        result = shift = 0
        while True:
            byte = data[pos]
            pos += 1
            result |= (byte & 0x7F) << shift
            if not byte & 0x80:
                return result, pos
            shift += 7

    pos = 8
    imports: list[str] = []
    while pos < len(wasm):
        section_id = wasm[pos]
        pos += 1
        size, pos = uleb(wasm, pos)
        end = pos + size
        if section_id == 2:
            count, pos = uleb(wasm, pos)
            for _ in range(count):
                mod_len, pos = uleb(wasm, pos)
                module = wasm[pos : pos + mod_len].decode("utf-8")
                pos += mod_len
                name_len, pos = uleb(wasm, pos)
                name = wasm[pos : pos + name_len].decode("utf-8")
                pos += name_len
                kind = wasm[pos]
                pos += 1
                if kind == 0x00:  # function: type index
                    _, pos = uleb(wasm, pos)
                else:
                    raise SystemExit("FAILED: unexpected non-function import")
                imports.append(f"{module}.{name}")
        pos = end
    return imports


def read_exports(wasm: bytes) -> list[str]:
    """Exported function names the standards contract must provide."""

    def uleb(data: bytes, pos: int) -> tuple[int, int]:
        result = shift = 0
        while True:
            byte = data[pos]
            pos += 1
            result |= (byte & 0x7F) << shift
            if not byte & 0x80:
                return result, pos
            shift += 7

    pos = 8
    names: list[str] = []
    while pos < len(wasm):
        section_id = wasm[pos]
        pos += 1
        size, pos = uleb(wasm, pos)
        end = pos + size
        if section_id == 7:
            count, pos = uleb(wasm, pos)
            for _ in range(count):
                name_len, pos = uleb(wasm, pos)
                names.append(wasm[pos : pos + name_len].decode("utf-8"))
                pos += name_len
                kind = wasm[pos]
                pos += 1
                _, pos = uleb(wasm, pos)
        pos = end
    return names


def abi_json(name: str, wasm: bytes, code_size: int, digest: str) -> dict:
    return {
        "standard": name,
        "abi_version": 2,
        "abi_modules": ["env", "env2"],
        "abi_functions_v1": ABI1_FUNCTIONS,
        "abi_functions_v2": ABI2_FUNCTIONS,
        "dispatch": "4-byte little-endian selector, arguments after it",
        "args_encoding": "fixed layout, little endian; `address` = 20 bytes, `u128` = 16",
        "methods": [
            {
                "selector": f"0x{selector:02x}",
                "name": method,
                "args": args,
                "reply": reply,
            }
            for selector, method, args, reply in METHODS[name]
        ],
        "code_size": code_size,
        "code_sha256": digest,
        "imports": read_imports(wasm),
        "exports": read_exports(wasm),
        "toolchain": {
            "rustc": run(["rustc", "--version"], cwd=REPO, env=build_env()).strip(),
            "target": WASM_TARGET,
            "profile": "release + lto + opt-level=z + panic=abort + strip",
            "max_memory_bytes": MAX_MEMORY_BYTES,
        },
    }


def main() -> int:
    check = "--check" in sys.argv
    env = build_env()

    # Two independent target directories: a build that is not reproducible is not
    # a build, and a pinned hash that only one toolchain can produce is a trap.
    artifacts: dict[str, tuple[bytes, bytes]] = {}
    for name, crate, _ in STANDARDS_LIST:
        first = build(name, crate, env)
        second_dir = TARGET.with_name(TARGET.name + "-second")
        env_second = dict(env)
        env_second["CARGO_TARGET_DIR"] = str(second_dir)
        env_second["CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS"] = (
            TARGET_RUSTFLAGS.replace(str(TARGET), str(second_dir))
        )
        run(
            ["cargo", "build", "--target", WASM_TARGET, "--release", "--quiet"],
            cwd=STANDARDS / name,
            env=env_second,
        )
        second = second_dir / WASM_TARGET / "release" / f"{crate.replace('-', '_')}.wasm"
        first_bytes = first.read_bytes()
        second_bytes = second.read_bytes()
        if first_bytes != second_bytes:
            digest_first = hashlib.sha256(first_bytes).hexdigest()
            digest_second = hashlib.sha256(second_bytes).hexdigest()
            raise SystemExit(
                f"FAILED: {name} is not reproducible across target dirs\n"
                f"  {digest_first}\n  {digest_second}"
            )
        artifacts[name] = (first_bytes, first_bytes)

    lines = [
        "# X5 standards — measured artifact sizes, hashes, imports and exports.",
        "#",
        "# Written by tools/build_standards.py. Do not edit by hand.",
        "#",
        "# The gates in crates/aura-exec/tests/x5_*.rs deploy the shipped bytecode,",
        "# so this file is what makes a stale artifact impossible to test by accident.",
        "",
        f"target={WASM_TARGET}",
        f"max_memory_bytes={MAX_MEMORY_BYTES}",
        "abi_version=2",
        "abi_modules=env,env2",
    ]
    for name, crate, wasm_name in STANDARDS_LIST:
        wasm, _ = artifacts[name]
        digest = hashlib.sha256(wasm).hexdigest()
        imports = read_imports(wasm)
        exports = read_exports(wasm)
        lines.append("")
        lines.append(f"standard_{name}_crate={crate}")
        lines.append(f"standard_{name}_size={len(wasm)}")
        lines.append(f"standard_{name}_sha256={digest}")
        lines.append(f"standard_{name}_imports={','.join(imports)}")
        lines.append(f"standard_{name}_exports={','.join(exports)}")
        for function in imports:
            module, _, short = function.partition(".")
            if module == "env2" and short not in ABI2_FUNCTIONS:
                raise SystemExit(f"FAILED: {name} imports an unknown env2 function {short}")
            if module == "env" and short not in ABI1_FUNCTIONS:
                raise SystemExit(f"FAILED: {name} imports an unknown env function {short}")
            if module not in ("env", "env2"):
                raise SystemExit(f"FAILED: {name} imports from {module}")
        if "run" not in exports:
            raise SystemExit(f"FAILED: {name} does not export run")

        if not check:
            destination = STANDARDS / name / "wasm" / f"{wasm_name}.wasm"
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(wasm)
            (STANDARDS / name / "abi.json").write_text(
                json.dumps(abi_json(name, wasm, len(wasm), digest), indent=2) + "\n"
            )

    text = "\n".join(lines) + "\n"
    # The measured block comes from the node itself: `x5-record` deploys each
    # standard through the real lifecycle and reports BLAKE3 digests and gas. It is
    # appended verbatim, the same way X4 pins its measured numbers, so a hand-edited
    # number is a visible diff.
    measured = run(
        ["cargo", "run", "--quiet", "--release", "-p", "aura-exec", "--bin", "x5-record"],
        cwd=REPO,
        env=env,
    ).strip()
    text = text + "\n# --- measured by `cargo run -p aura-exec --bin x5-record` ---\n" + measured + "\n"

    # `--check` compares the *whole* fixture, the measured block included: a check
    # that ignores half the file would have reported a clean pin over a stale gas
    # number, which is exactly the drift it exists to catch.
    if check:
        current = FIXTURE.read_text() if FIXTURE.exists() else ""
        if current != text:
            names = ", ".join(name for name, _, _ in STANDARDS_LIST)
            print(f"PINNED HASH DRIFT: {names}")
            for line in text.splitlines():
                if line not in current.splitlines():
                    print("  +", line)
            return 1
        print(
            f"standards: {len(STANDARDS_LIST)} artifacts reproduce the pinned fixture"
        )
        return 0

    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    FIXTURE.write_text(text)
    total = sum(len(artifacts[name][0]) for name, _, _ in STANDARDS_LIST)
    print(f"wrote {FIXTURE.relative_to(REPO)}")
    for name, _, _ in STANDARDS_LIST:
        wasm = artifacts[name][0]
        print(
            f"  {name}: {len(wasm)} bytes, sha256 {hashlib.sha256(wasm).hexdigest()[:16]}…"
        )
    print(f"total {total} bytes across {len(STANDARDS_LIST)} standards")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
