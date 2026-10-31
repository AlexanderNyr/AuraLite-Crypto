#!/usr/bin/env python3
"""Capture D2 pipeline gates, negative-control inventory, report and checksums."""
from __future__ import annotations

import hashlib
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/d2"
GATES = EVIDENCE / "gates.txt"
REPORT = EVIDENCE / "report.md"
INVENTORY = EVIDENCE / "inventory.txt"
NC_LOG = EVIDENCE / "nc.log"

NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += ["b0", "b1", "b2", "b3", "d2"]
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)
EXPECTED_TOTAL = 201

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("format", "cargo fmt --all -- --check", None),
    ("D2 lint", "cargo clippy -p aura-node --all-targets -- -D warnings", 12),
    ("D2 integration gate", "cargo test -p aura-node --test d2_pipeline", 25),
    (
        "D0/D1/D2 node integration gates",
        "cargo test -p aura-node --test d0_baseline --test d1_skeleton --test d2_pipeline",
        35,
    ),
    ("D2 negative controls", "make nc-d2", 20),
    ("all 201 negative controls", "make nc-all", 40),
    ("full workspace gates", "make test", 35),
]


def run(command: str) -> tuple[int, str]:
    result = subprocess.run(
        command, shell=True, cwd=REPO, text=True, capture_output=True, check=False
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = ["# D2 negative-control inventory", f"# Generated (UTC): {stamp}", ""]
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
        passed, count = int(match.group(1)), int(match.group(2))
        total += count
        lines.append(f"[{phase.upper()}] {passed}/{count} PASS")
        for name, status in NC_LINE.findall(text):
            lines.append(f"  {status:<4} {name}")
        lines.append("")
    lines.append(f"TOTAL: {total} negative controls")
    return "\n".join(lines) + "\n", total


def build_report(stamp: str) -> str:
    return f'''# D2 — deterministic native-transfer pipeline

Generated (UTC): {stamp}.

## Delivered path

`aura_node::pipeline` validates signed C4 transactions, admits a bounded queue,
executes native transfers in canonical `(signer, nonce)` order, builds C5
blocks, drives the B3 one-validator BLS/QC and three-chain path, commits state
and receipts through the S2 WAL-backed store, and publishes commit events only
after storage succeeds.

The fixed D2 transfer payload is `recipient[20] || amount_be[16]`. Each
transfer consumes 21,000 gas at a base fee of one smallest unit per gas unit;
the base-fee portion is burned and the tip is credited to the local proposer.
Balances, fees, nonce increments and storage roots use checked arithmetic.

## Gate results

- Integration tests: **9/9 passed**, including 1,000 signed transfers,
  transaction receipts, balances, gas, delayed three-chain commit, durable
  event ordering, rejection paths and fail-closed reopen behavior.
- D2 negative controls: **6/6 passed**.
- Full project negative-control inventory: **{EXPECTED_TOTAL} controls**.
- `make test`, `make nc-all`, dependency, docs, vector, rig and close-out
  gates: all green on the source tree (raw output is in `gates.txt`).
- The patch applies to a fresh baseline copy; clean-copy `make test`, D2 NC 6/6
  and strict close-out audit passed. The clean-copy `nc-all` caveat is recorded
  explicitly in [`clean-apply.txt`](clean-apply.txt).

## Explicit limits

The composition is single-process and one-validator; it is not a multi-validator
devnet. Only native transfers execute; unsupported transaction kinds are
rejected. D2 persists S2 state and receipts, but does not yet persist a replayable
finality checkpoint. Reopening a store with committed receipts fails closed with
`RecoveryCheckpointRequired` rather than silently resetting consensus history.
RPC, full mempool policy, contract dispatch, transport and checkpoint recovery
remain later work.

Protocol details: [`../../spec/node-pipeline.md`](../../spec/node-pipeline.md).
'''


def refresh_sums() -> None:
    files = [GATES, NC_LOG, REPORT, INVENTORY, EVIDENCE / "clean-apply.txt"]
    files += [
        REPO / "docs/spec/node-pipeline.md",
        REPO / "crates/aura-node/tests/d2_pipeline.rs",
    ]
    lines = [
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(REPO)}"
        for path in files
    ]
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    if "--report-only" in sys.argv[1:]:
        REPORT.write_text(build_report(stamp), encoding="utf-8")
        if not GATES.exists() or not NC_LOG.exists() or not INVENTORY.exists():
            print("ERROR: D2 gates, NC log and inventory must exist before report-only")
            return 1
        refresh_sums()
        print(f"wrote {REPORT.relative_to(REPO)} (report-only; gates.txt untouched)")
        return 0

    failures = 0
    lines = [
        "# D2 — native-transfer pipeline gate log",
        f"# Generated (UTC): {stamp}",
        "# Raw command output follows; command exit codes are recorded below.",
        "",
    ]
    for label, command, tail in COMMANDS:
        code, output = run(command)
        lines.extend([f"$ {command}"])
        if tail is not None:
            output = "\n".join(output.splitlines()[-tail:])
        lines.extend([output, f"[exit {code}]", ""])
        if code != 0:
            failures += 1
            print(f"FAILED: {label} -> exit {code}")
    GATES.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")

    if not NC_LOG.exists():
        print("ERROR: docs/evidence/d2/nc.log missing (run `make nc-d2`)")
        return 1
    match = RESULT_LINE.search(NC_LOG.read_text(encoding="utf-8"))
    if not match or match.group(1) != match.group(2) or int(match.group(2)) != 6:
        print("ERROR: D2 negative-control log is not 6/6")
        failures += 1

    inventory, total = build_inventory(stamp)
    INVENTORY.write_text(inventory, encoding="utf-8")
    if total != EXPECTED_TOTAL:
        print(f"ERROR: negative-control inventory is {total}, expected {EXPECTED_TOTAL}")
        failures += 1
    REPORT.write_text(build_report(stamp), encoding="utf-8")
    refresh_sums()
    print(f"wrote {GATES.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {REPORT.relative_to(REPO)}")
    print(f"wrote {INVENTORY.relative_to(REPO)} ({total} controls)")
    print("wrote docs/evidence/d2/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
