#!/usr/bin/env python3
"""Fail closed on unreviewed changes to the local Unicode 15.1 normalization fork.

All unmodified files are checked against exact SHA-256 digests from published
crates.io `unicode-normalization` 0.1.23
(archive SHA-256: a56d1686db2308d901306f92a263857ef59ea39678a5458e7cb17f01415101f5).
`Cargo.toml` and `src/lib.rs` are reversed through the exact documented build/lint
adaptations and checked against their upstream digests. Extra files or symlinks
are rejected. This checks provenance only, not cryptographic or Unicode correctness.
"""
from __future__ import annotations

import hashlib
from pathlib import Path
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
FORK = ROOT / "third_party/unicode-normalization-15-1"

HASHES = {
    ".cargo_vcs_info.json": "e92f0f7454bcb0d1799f3355f02eb54792e63f9a1e474bf4d87f0c21bd66ed86",
    ".github/workflows/rust.yml": "11979c4019cdbee20701edfa1048c96cdbe865c328e18f0be18239c2705927de",
    ".gitignore": "b2e67ce99bec6186eede939ac3867a2e6a508adbac43ebe021ba17e2fd0df49a",
    ".travis.yml": "a7b41b8c3a730ae331409fc96b45a0baded22923150a8f30cf2d6a9aa37fe30a",
    "COPYRIGHT": "23860c2a7b5d96b21569afedf033469bab9fe14a1b24a35068b8641c578ce24d",
    "Cargo.toml.orig": "cc32aa67ca802c3b0bdd3a57c89740a6c5177515eaf57ce4f416db15e83ae442",
    "LICENSE-APACHE": "a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2",
    "LICENSE-MIT": "7b63ecd5f1902af1b63729947373683c32745c16a10e8e6292e2e2dcd7e90ae0",
    "README.md": "5e6af58a3100ef18063422cb2fdf70afac62b5d60d828c6cde6344236d795be9",
    "benches/bench.rs": "827e5343b059a732904be29717c2797203bfd0a633edf08042afea65372a3e2c",
    "scripts/unicode.py": "648b5df0f5dd5db92ed17169456975aeca73d44cd83760bb68b7a7b6e70097c7",
    "src/__test_api.rs": "78e21bfa0b98894f545c8ed3e31cec20d7a48951a7f3ed69a6130c4b3d463aee",
    "src/decompose.rs": "c0eb774843a545356e63bbcd7fb926f80d3c97ef4601ca3701fc34154f2e9905",
    "src/lookups.rs": "962f9909b32e02b8a2a05836135d9cd39bb1ce01f7c659de99cbd8a3a3c78574",
    "src/no_std_prelude.rs": "602e81e67b8952b6571826f431e3b6787be3073bc10f38a0d3374278f81a6a1f",
    "src/normalize.rs": "de2670b4437d335d42884af844a750f70e541467ecd34077dfe032103cb9b041",
    "src/perfect_hash.rs": "400c84e2f467f61bd55d55d08672da6a9ad7a57c938ce5d0c701a6994b1b273b",
    "src/quick_check.rs": "9756312d75fc31b67fca954e44a4812945a7e436b03ba18b9a2441f6de570f6f",
    "src/recompose.rs": "a6228ad7561a5c7a1ef1d510159bdde1eea8a161007c80e470432e9b844d5536",
    "src/replace.rs": "b24c904f3e00851a78820e30ddfa4ff10c795f8925fd0ee7f5870f31fdfa770b",
    "src/stream_safe.rs": "383d71f0da401af8e735877e43855c7e16cb06deb2263539cdec2a407dbe257d",
    "src/tables.rs": "b3b3fb0784a3c8ec171135601f9e750825ced09ec6882aeb6692b19624ec81cb",
    "src/test.rs": "3af8ad8c6bd2cc1ca44660bd265ad813c88d3074b448df4d9ff376b25fb77d26",
}

UPSTREAM_MANIFEST = "376feb646a85a5b2b5c45d5e817cdcf9fd79c0cb4b924fde03687da3e6ea0f39"
MANIFEST_PATCH = (
    (
        'name = "unicode-normalization"\nversion = "0.1.23"\n',
        'name = "unicode-normalization-15-1"\nversion = "0.1.23"\nautobenches = false\n',
    ),
    ('version = "1"\nfeatures = ["alloc"]', 'version = "=1.13.3"\nfeatures = ["alloc"]'),
)

UPSTREAM_LIB = "1983769ea083caa36b0736c87cf2a98e91c2b900f1d5dec64e327360fa862386"
LIB_PATCH = (
    (
        "//! extern crate unicode_normalization;\n",
        "//! extern crate unicode_normalization_15_1 as unicode_normalization;\n",
    ),
    (
        "#![deny(missing_docs, unsafe_code)]\n",
        "// Keep the audited upstream 0.1.23 algorithm/table source intact; these lints\n"
        "// are stylistic modernizations and are suppressed rather than changing the\n"
        "// versioned normalization implementation.\n"
        "#![allow(\n"
        "    clippy::manual_is_multiple_of,\n"
        "    clippy::question_mark,\n"
        "    clippy::match_like_matches_macro\n"
        ")]\n"
        "#![deny(missing_docs, unsafe_code)]\n",
    ),
    (
        "use core::{\n    str::Chars,\n    option,\n};\n",
        "use core::{option, str::Chars};\n",
    ),
    (
        "    }\n}\n\n\nimpl UnicodeNormalization<option::IntoIter<char>> for char {\n",
        "    }\n}\n\nimpl UnicodeNormalization<option::IntoIter<char>> for char {\n",
    ),
)

EXPECTED_FILES = set(HASHES) | {"Cargo.toml", "src/lib.rs", "LOCAL-FORK.md"}


def _check_patched(path: Path, patches: tuple[tuple[str, str], ...], upstream_digest: str, label: str) -> list[str]:
    errors: list[str] = []
    try:
        content = path.read_text(encoding="utf-8")
        for old, new in patches:
            if content.count(new) != 1:
                errors.append(f"{label}: expected adaptation missing or duplicated")
            else:
                content = content.replace(new, old, 1)
        if hashlib.sha256(content.encode("utf-8")).hexdigest() != upstream_digest:
            errors.append(f"{label}: does not reproduce exact upstream 0.1.23 after reversing adaptations")
    except (OSError, UnicodeError) as exc:
        errors.append(f"{label}: {exc}")
    return errors


def check(fork: Path = FORK) -> list[str]:
    errors: list[str] = []
    if fork.is_symlink() or not fork.is_dir():
        return ["fork root is missing or a symlink"]
    present: set[str] = set()
    for entry in sorted(fork.rglob("*")):
        rel = entry.relative_to(fork).as_posix()
        if entry.is_symlink():
            errors.append(f"{rel}: symlink not allowed in vendored fork")
        elif entry.is_file():
            present.add(rel)
        elif not entry.is_dir():
            errors.append(f"{rel}: non-regular entry not allowed")
    if present != EXPECTED_FILES:
        missing = sorted(EXPECTED_FILES - present)
        extra = sorted(present - EXPECTED_FILES)
        if missing:
            errors.append(f"missing expected files: {missing}")
        if extra:
            errors.append(f"unexpected files in fork: {extra}")
    for name, expected in HASHES.items():
        try:
            actual = hashlib.sha256((fork / name).read_bytes()).hexdigest()
            if actual != expected:
                errors.append(f"{name}: SHA-256 mismatch")
        except OSError as exc:
            errors.append(f"{name}: {exc}")
    try:
        tables = (fork / "src/tables.rs").read_text(encoding="utf-8")
        if "pub const UNICODE_VERSION: (u8, u8, u8) = (15, 1, 0);" not in tables:
            errors.append("src/tables.rs: expected UNICODE_VERSION = (15, 1, 0) declaration missing")
    except (OSError, UnicodeError) as exc:
        errors.append(f"src/tables.rs: {exc}")
    errors.extend(_check_patched(fork / "Cargo.toml", MANIFEST_PATCH, UPSTREAM_MANIFEST, "Cargo.toml"))
    errors.extend(_check_patched(fork / "src/lib.rs", LIB_PATCH, UPSTREAM_LIB, "src/lib.rs"))
    return errors


def check_bindings(
    workspace_manifest: Path = ROOT / "Cargo.toml",
    wallet_manifest: Path = ROOT / "crates/aura-wallet/Cargo.toml",
) -> list[str]:
    errors: list[str] = []
    try:
        ws = tomllib.loads(workspace_manifest.read_text(encoding="utf-8"))
        ws_deps = ws.get("workspace", {}).get("dependencies", {})
        if ws_deps.get("unicode-normalization") != {"version": "=0.1.26"}:
            errors.append("workspace Cargo.toml must pin unicode-normalization to exact =0.1.26")
        if ws_deps.get("unicode-normalization-15-1") != {"path": "third_party/unicode-normalization-15-1"}:
            errors.append("workspace Cargo.toml must bind unicode-normalization-15-1 to third_party/unicode-normalization-15-1")
        wallet = tomllib.loads(wallet_manifest.read_text(encoding="utf-8"))
        deps = wallet.get("dependencies", {})
        for crate_name in ("unicode-normalization", "unicode-normalization-15-1"):
            if deps.get(crate_name) != {"workspace": True}:
                errors.append(f"aura-wallet Cargo.toml must bind {crate_name} via workspace = true")
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as exc:
        errors.append(f"manifest binding check: {exc}")
    return errors


def main() -> int:
    errors = check() + check_bindings()
    if errors:
        print("UNICODE 15.1 FORK GUARD: FAIL")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("UNICODE 15.1 FORK GUARD: OK — exact upstream 0.1.23 tables/source plus approved build adaptations")
    return 0


if __name__ == "__main__":
    sys.exit(main())
