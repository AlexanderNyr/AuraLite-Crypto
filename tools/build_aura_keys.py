#!/usr/bin/env python3
"""Build the W6 `aura-keys` WASM artifact and pin its measurements.

`clients/aura-keys` is a standalone crate (own workspace and lockfile) that
includes `crates/aura-wallet/src/web.rs` verbatim and exposes it through a
deterministic C ABI. This tool builds it for `wasm32-unknown-unknown` twice
into two target directories, refuses to pin anything unless the two builds
are byte-identical (reproducible-build gate, model:
`tools/build_contracts.py`), copies the artifact to
`clients/aura-keys/wasm/aura_keys.wasm`, and writes the measured size,
sha256, imports and exports to
`crates/aura-wallet/tests/fixtures/w6_aura_keys_wasm.txt`.

Usage:
    python3 tools/build_aura_keys.py            # build, pin, write the fixture
    python3 tools/build_aura_keys.py --check     # build and compare, write nothing
    make build-aura-keys
"""

from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CRATE = REPO / "clients" / "aura-keys"
FIXTURE = REPO / "crates" / "aura-wallet" / "tests" / "fixtures" / "w6_aura_keys_wasm.txt"
SHIPPED = CRATE / "wasm" / "aura_keys.wasm"
TARGET = Path(os.environ.get("AURA_KEYS_TARGET", str(REPO.parent / "aura-keys-target")))

MAX_MEMORY_BYTES = 16 * 1024 * 1024  # bounded linear memory (X0 census rule)

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


def describe(blob: bytes) -> dict[str, object]:
    """Parse the import and export sections of a WASM module (independent
    reading of the artifact, model: `tools/build_contracts.py`)."""
    reader = Reader(blob)
    magic = reader.bytes(4)
    version = reader.bytes(4)
    if magic != b"\x00asm" or version != b"\x01\x00\x00\x00":
        raise SystemExit(f"not a wasm 1.0 module: {magic!r} {version!r}")
    imports: list[str] = []
    exports: list[str] = []
    memory_max: int | None = None
    while reader.pos < len(blob):
        section_id = reader.u8()
        size = reader.uleb()
        end = reader.pos + size
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
        elif section_id == 7:
            count = reader.uleb()
            for _ in range(count):
                name = reader.name()
                kind = reader.u8()
                reader.uleb()
                if kind == 0x00:
                    exports.append(name)
                else:
                    exports.append(f"{name}:{KINDS.get(kind, hex(kind))}")
        elif section_id == 5:
            count = reader.uleb()
            for _ in range(count):
                flag = reader.u8()
                reader.uleb()
                if flag:
                    memory_max = reader.uleb()
        reader.pos = end
    return {"imports": imports, "exports": exports, "memory_max": memory_max}


# --------------------------------------------------------------------------- build
def build(target_dir: Path) -> Path:
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = str(target_dir)
    env["RUSTFLAGS"] = RUSTFLAGS
    env["CARGO_INCREMENTAL"] = "0"
    result = subprocess.run(
        ["cargo", "build", "--release", "--target", "wasm32-unknown-unknown"],
        cwd=CRATE,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stdout + result.stderr)
        raise SystemExit("build failed for clients/aura-keys")
    artifact = target_dir / "wasm32-unknown-unknown" / "release" / "aura_keys.wasm"
    if not artifact.exists():
        raise SystemExit(f"missing artifact {artifact}")
    return artifact


def render(row: dict[str, object]) -> str:
    lines = [
        "# W6 aura-keys WASM artifact — measured from the built artifact.",
        "# Written by tools/build_aura_keys.py. Do not edit by hand.",
        "#",
        "# clients/aura-keys/wasm/aura_keys.wasm is the artifact this fixture",
        "# pins; crates/aura-wallet/src/web.rs is compiled verbatim into it.",
        "",
        "api_version=1",
        f"size={row['size']}",
        f"sha256={row['sha256']}",
        f"memory_max={row['memory_max']}",
        "imports:",
    ]
    for item in row["imports"]:
        lines.append(f"  {item}")
    lines.append("exports:")
    for item in row["exports"]:
        lines.append(f"  {item}")
    return "\n".join(lines) + "\n"


def read_fixture() -> dict[str, str]:
    pinned: dict[str, str] = {}
    if not FIXTURE.exists():
        return pinned
    for line in FIXTURE.read_text(encoding="utf-8").splitlines():
        if "=" in line and not line.startswith("#") and not line.startswith("  "):
            key, value = line.split("=", 1)
            pinned[key] = value
    return pinned


def main() -> int:
    check = "--check" in sys.argv
    one = build(TARGET / "a")
    two = build(TARGET / "b")
    blob = one.read_bytes()
    again = two.read_bytes()
    if blob != again:
        raise SystemExit(
            f"aura-keys: NOT reproducible — two builds of the same tree differ "
            f"({len(blob)} vs {len(again)} bytes); refusing to pin a hash"
        )
    info = describe(blob)
    digest = hashlib.sha256(blob).hexdigest()
    row = {
        "size": len(blob),
        "sha256": digest,
        "imports": info["imports"],
        "exports": info["exports"],
        "memory_max": info["memory_max"],
    }

    expected = {
        "aura_keys_api_version",
        "aura_keys_origin_check",
        "aura_keys_deep_link_parse",
        "aura_keys_confirm_view",
        "aura_keys_sign",
        "aura_keys_sign_without_view",
        "aura_keys_ui_version",
        "aura_keys_claim_check",
    }
    found = {entry.split(":", 1)[0] for entry in info["exports"]}
    missing = sorted(expected - found)
    if missing:
        raise SystemExit(f"aura-keys: missing exports: {', '.join(missing)}")

    print(
        f"aura-keys: {len(blob)} bytes, sha256 {digest[:16]}…, "
        f"imports {len(info['imports'])}, exports {','.join(sorted(found))}"
    )

    if check:
        pinned = read_fixture()
        if pinned.get("sha256") != digest:
            print(f"PINNED HASH DRIFT: {pinned.get('sha256')} != {digest}")
            return 1
        print("PINNED HASHES MATCH")
        return 0

    SHIPPED.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(one, SHIPPED)
    FIXTURE.write_text(render(row), encoding="utf-8")
    print(f"wrote {FIXTURE.relative_to(REPO)} and {SHIPPED.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
