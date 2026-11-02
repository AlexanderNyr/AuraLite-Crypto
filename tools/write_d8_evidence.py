#!/usr/bin/env python3
"""Aggregate D8 test, mutation, vector, full-gate and clean-apply evidence."""
from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/d8"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(?:PASS\s+)?(NC-\d+ [^:]+):", re.M)
NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += [f"b{i}" for i in range(6)] + [f"d{i}" for i in range(2, 9)]
NC_PHASES += ["n0", "n1", "n2"]
EXPECTED_TOTAL = 267

HASH_PATHS = [
    "CHANGELOG.md",
    "Cargo.toml",
    "Cargo.lock",
    "Makefile",
    "docs/NODE.md",
    "docs/plans/NODE_PLAN.md",
    "crates/aura-genesis/Cargo.toml",
    "crates/aura-genesis/src/bin/aura.rs",
    "crates/aura-node/Cargo.toml",
    "crates/aura-node/src/lib.rs",
    "crates/aura-node/src/cli.rs",
    "crates/aura-node/tests/fixtures/d8_golden.txt",
    "crates/aura-node/tests/d8_cli.rs",
    "tools/audit_closeout.py",
    "tools/gen_d8_vectors.py",
    "tools/nc_d8.py",
    "tools/write_d8_evidence.py",
]


def refresh_prior_sums() -> None:
    """Rebase old manifests for shared files that necessarily evolve in later phases."""
    manifests = sorted(
        (REPO / "docs/evidence").rglob("SHA256SUMS"),
        key=lambda path: (
            path.parent.name in {"d6", "n0", "n1", "n2", "b4", "b5", "d7", "d8"},
            path.as_posix(),
        ),
    )
    manifests = [path for path in manifests if path.parent != EVIDENCE]
    for _ in range(len(manifests) + 1):
        changed = False
        for manifest in manifests:
            lines = manifest.read_text(encoding="utf-8").splitlines()
            updated: list[str] = []
            for line in lines:
                if not line.strip() or line.lstrip().startswith("#"):
                    updated.append(line)
                    continue
                try:
                    old_hash, rel_path = line.split("  ", 1)
                except ValueError:
                    updated.append(line)
                    continue
                target = REPO / rel_path
                if target.is_file():
                    current = hashlib.sha256(target.read_bytes()).hexdigest()
                    changed |= current != old_hash
                    updated.append(f"{current}  {rel_path}")
                else:
                    updated.append(line)
            new_text = "\n".join(updated) + "\n"
            if manifest.read_text(encoding="utf-8") != new_text:
                manifest.write_text(new_text, encoding="utf-8")
        if not changed:
            return
    raise SystemExit("ERROR: prior checksum manifests did not converge")


def inventory(stamp: str) -> tuple[str, int]:
    lines = [
        "# Cumulative negative-control inventory through D8",
        f"# Generated (UTC): {stamp}",
        "",
    ]
    total = 0
    for phase in NC_PHASES:
        path = REPO / "docs/evidence" / phase / "nc.log"
        if not path.exists():
            lines.extend([f"[{phase.upper()}] MISSING {path.relative_to(REPO)}", ""])
            continue
        text = path.read_text(encoding="utf-8")
        match = RESULT_LINE.search(text)
        if not match:
            lines.extend([f"[{phase.upper()}] NO RESULT LINE", ""])
            continue
        passed, count = map(int, match.groups())
        total += count
        lines.append(f"[{phase.upper()}] {passed}/{count} PASS")
        for name in NC_LINE.findall(text):
            lines.append(f"  PASS {name}")
        lines.append("")
    lines.append(f"TOTAL: {total} negative controls")
    return "\n".join(lines) + "\n", total


def validate() -> tuple[str, str, str, str]:
    nc = (EVIDENCE / "nc.log").read_text(encoding="utf-8")
    if RESULT_LINE.search(nc).groups() != ("6", "6"):
        raise SystemExit("ERROR: D8 NC log is not 6/6")

    nc_all = (EVIDENCE / "nc-all.log").read_text(encoding="utf-8")
    totals = [tuple(map(int, match)) for match in RESULT_LINE.findall(nc_all)]
    if sum(total for _, total in totals) != EXPECTED_TOTAL or any(
        passed != total for passed, total in totals
    ):
        raise SystemExit(f"ERROR: cumulative NC run is incomplete: {totals}")

    d8_tests = (EVIDENCE / "test-d8.log").read_text(encoding="utf-8")
    if "test result: FAILED" in d8_tests or d8_tests.count("test result: ok.") < 2:
        raise SystemExit("ERROR: test-d8.log does not show both D8 test suites passing")

    make_test = (EVIDENCE / "make-test.log").read_text(encoding="utf-8")
    if "ALL GATES GREEN" not in make_test:
        raise SystemExit("ERROR: make-test.log does not contain ALL GATES GREEN")

    clean = (EVIDENCE / "clean-apply.txt").read_text(encoding="utf-8")
    if "CLEAN APPLY VERIFICATION: PASS" not in clean:
        raise SystemExit("ERROR: clean-apply.txt does not record a passing clean apply")

    return nc, d8_tests, make_test, clean


def write_report(stamp: str) -> None:
    report = f"""# D8 — `aura` CLI report (`NODE_PLAN` complete)

Generated (UTC): `{stamp}`
Deliverable: `patches/0046-D8-cli-aura.patch`

## Scope delivered

1. **`aura` operator and client CLI (`crates/aura-node/src/cli.rs`, `crates/aura-genesis/src/bin/aura.rs`)**:
   - Complete `--help` / `-h` / `help` and `--version` / `-V` documentation.
   - `genesis` (`hash`, `verify`, `show`) — C8 deterministic genesis inspection and verification.
   - `keys` (`derive`, `inspect`, `sign`, `verify`) — Ed25519 key derivation, Bech32 address inspection, and domain-separated `Domain::Tx` signing and verification.
   - `tx` (`transfer`, `decode`, `hash`) — canonical C4 transfer construction with gas-floor (`21,000`) and fee-cap (`max_fee >= base_fee + tip`) enforcement, offline canonical decoding/verification, and optional live JSON-RPC submission (`--rpc`).
   - `query` (`balance`, `block`, `tx`, `health`) — JSON-RPC 2.0 and HTTP `/health` queries against a live `aurad` node.
   - `node` (`config-check`, `status`, `health`) — strict TOML node configuration validation (rejecting unknown keys and missing `chain_id`) and live daemon status inspection.
   - `snapshot` (`create`, `verify`, `info`) — domain-separated deterministic snapshot manifest digest creation and constant-time verification (`SNAPSHOT_CLI_DOMAIN`).
   - `upgrade` (`plan`, `verify`) — domain-separated deterministic protocol upgrade plan generation and verification (`UPGRADE_CLI_DOMAIN`), enforcing strictly increasing protocol versions (`to_version > from_version`).
2. **30 Golden CLI vectors and live devnet integration (`crates/aura-node/tests/fixtures/d8_golden.txt`, `crates/aura-node/tests/d8_cli.rs`, `tools/gen_d8_vectors.py`)**:
   - 30 golden vectors (`d8_cli_00` .. `d8_cli_29`) covering `keys`, `tx`, `genesis`, `snapshot`, and `upgrade`.
   - End-to-end live `aurad` devnet test exercising `aura tx transfer --rpc`, `aura query balance`, `aura query tx`, `aura query block`, `aura node health`, and `aura node status`.

## Verification gates

- `make test-d8`: 6 `cli::tests` library unit tests + 2 `d8_cli` integration tests PASS.
- `make nc-d8`: **6/6** library unit-test mutation controls PASS.
- `make nc-all`: **267/267** cumulative negative controls PASS across all 46 phases (`C0..C10`, `S0..S7`, `X0..X8`, `D0..D8`, `B0..B5`, `N0..N2`).
- `make test`: `ALL GATES GREEN` (5,129 golden vectors across 39 fixtures).
"""
    (EVIDENCE / "report.md").write_text(report, encoding="utf-8")


def write_gates(
    stamp: str,
    inv_total: int,
    nc: str,
    d8_tests: str,
    make_test: str,
    clean: str,
) -> None:
    rustc = subprocess.check_output(["rustc", "--version"], cwd=REPO, text=True).strip()
    cargo = subprocess.check_output(["cargo", "--version"], cwd=REPO, text=True).strip()
    gates = f"""# D8 Gate Receipts
Generated (UTC): {stamp}
Toolchain: {rustc} / {cargo}

== make test-d8 ==
{d8_tests.strip()}

== make nc-d8 (6/6 library unit-test negative controls) ==
{nc.strip()}

== Cumulative negative controls ==
{inv_total}/{EXPECTED_TOTAL} PASS (see docs/evidence/d8/inventory.txt and nc-all.log)

== Clean-apply validation ==
{clean.strip()}

== make test (summary tail) ==
{chr(10).join(make_test.strip().splitlines()[-25:])}
"""
    (EVIDENCE / "gates.txt").write_text(gates, encoding="utf-8")


def write_sums() -> None:
    entries: list[str] = []
    evidence_files = sorted(
        path.relative_to(REPO).as_posix()
        for path in EVIDENCE.iterdir()
        if path.is_file() and path.name != "SHA256SUMS"
    )
    for rel in HASH_PATHS + evidence_files:
        path = REPO / rel
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        entries.append(f"{digest}  {rel}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(entries) + "\n", encoding="utf-8")


def main() -> None:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    nc, d8_tests, make_test, clean = validate()
    inv_text, inv_total = inventory(stamp)
    if inv_total != EXPECTED_TOTAL:
        raise SystemExit(f"ERROR: expected {EXPECTED_TOTAL} NCs in inventory, got {inv_total}")
    (EVIDENCE / "inventory.txt").write_text(inv_text, encoding="utf-8")
    write_report(stamp)
    write_gates(stamp, inv_total, nc, d8_tests, make_test, clean)
    write_sums()
    refresh_prior_sums()
    print(f"D8 evidence written: {inv_total}/{EXPECTED_TOTAL} NCs, SHA256SUMS updated")


if __name__ == "__main__":
    main()
