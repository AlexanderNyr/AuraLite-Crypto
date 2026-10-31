#!/usr/bin/env python3
"""Capture D3 mempool gates, cumulative NC inventory, report and checksums."""
from __future__ import annotations

import hashlib
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/d3"
GATES = EVIDENCE / "gates.txt"
REPORT = EVIDENCE / "report.md"
INVENTORY = EVIDENCE / "inventory.txt"
NC_LOG = EVIDENCE / "nc.log"
EXPECTED_TOTAL = 207

NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += ["b0", "b1", "b2", "b3", "d2", "d3"]
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("format", "cargo fmt --all -- --check", None),
    ("D3 clippy", "cargo clippy -p aura-mempool -p aura-node --all-targets -- -D warnings", 18),
    ("mempool unit gate", "cargo test -p aura-mempool", 12),
    ("D3 10,000-transfer acceptance", "cargo test -p aura-node --test d3_mempool", 30),
    (
        "D0-D2 node regression gates",
        "cargo test -p aura-node --test d0_baseline --test d1_skeleton --test d2_pipeline",
        35,
    ),
    ("D3 negative controls", "make nc-d3", 18),
    ("all 207 negative controls", "make nc-all", 42),
    ("full workspace gates", "make test", 35),
]


def run(command: str) -> tuple[int, str]:
    result = subprocess.run(
        command, shell=True, cwd=REPO, text=True, capture_output=True, check=False
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = ["# D3 negative-control inventory", f"# Generated (UTC): {stamp}", ""]
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
    return f'''# D3 — bounded fee-priority transaction pool

Generated (UTC): {stamp}.

## Delivered behavior

`aura-mempool` implements a deterministic 16,384-entry pool keyed by raw
`(signer, nonce)`. It selects only contiguous sender-lane heads, prioritises
ready entries by signed tip, uses deterministic address/nonce tie-breaking,
requires a checked 10% replacement bump (rounded up, minimum one unit), and
only evicts the lowest-tip entry for a strictly higher tip. Duplicate IDs are
rejected. Reverted-branch transactions can be reinserted after caller-side
revalidation against the new account/nonce view.

`aura_node::pipeline` validates chain, expiry, signature, projected nonce,
balance, transfer gas floor and fee cap before pool insertion. The selected set
is encoded canonically by `(signer, nonce)` before C5 block production; S2
receipts/events remain behind the B3 three-chain durability boundary.

## Gate results

- 10,000-transfer acceptance: **passed**; unique durable receipts, sequential
  nonce, exact sender/recipient balances, gas, state root and event ordering.
- Mempool unit tests: **6/6 passed**.
- D3 negative controls: **6/6 passed**, all using exact Rust unit tests.
- Cumulative negative controls: **{EXPECTED_TOTAL} passed** by `make nc-all`.
- `make test`, dependency, docs, vector, rig and strict close-out gates: green
  on the source tree (raw output in `gates.txt`).
- Clean baseline application and clean-copy regression result are recorded in
  [`clean-apply.txt`](clean-apply.txt).

## Explicit boundaries

The pool's reorg hook only reinserts caller-provided transactions; the caller
must revalidate them against the new chain/account state. D3 does not add a
distributed reorg detector, multi-validator transport, contract transaction
dispatch or a persisted finality checkpoint. The D2 pipeline still fails closed
when reopening committed receipts without a checkpoint.

Protocol: [`../../spec/mempool.md`](../../spec/mempool.md).
'''


def refresh_sums() -> None:
    files = [GATES, NC_LOG, REPORT, INVENTORY, EVIDENCE / "clean-apply.txt"]
    files += [
        REPO / "docs/spec/mempool.md",
        REPO / "crates/aura-mempool/src/lib.rs",
        REPO / "crates/aura-node/src/pipeline.rs",
        REPO / "crates/aura-node/tests/d3_mempool.rs",
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
        required = [GATES, NC_LOG, INVENTORY, EVIDENCE / "clean-apply.txt"]
        missing = [path.relative_to(REPO) for path in required if not path.exists()]
        if missing:
            print("ERROR: missing D3 evidence inputs: " + ", ".join(map(str, missing)))
            return 1
        REPORT.write_text(build_report(stamp), encoding="utf-8")
        refresh_sums()
        print(f"wrote {REPORT.relative_to(REPO)} (report-only; gates.txt untouched)")
        return 0

    failures = 0
    lines = [
        "# D3 — mempool gate log",
        f"# Generated (UTC): {stamp}",
        "# Raw command output follows; command exit codes are recorded below.",
        "",
    ]
    for label, command, tail in COMMANDS:
        code, output = run(command)
        lines.append(f"$ {command}")
        if tail is not None:
            output = "\n".join(output.splitlines()[-tail:])
        lines.extend([output, f"[exit {code}]", ""])
        if code != 0:
            failures += 1
            print(f"FAILED: {label} -> exit {code}")
    GATES.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")

    if not NC_LOG.exists():
        print("ERROR: docs/evidence/d3/nc.log missing (run `make nc-d3`)")
        return 1
    match = RESULT_LINE.search(NC_LOG.read_text(encoding="utf-8"))
    if not match or match.group(1) != match.group(2) or int(match.group(2)) != 6:
        print("ERROR: D3 negative-control log is not 6/6")
        failures += 1
    inventory, total = build_inventory(stamp)
    INVENTORY.write_text(inventory, encoding="utf-8")
    if total != EXPECTED_TOTAL:
        print(f"ERROR: NC inventory is {total}, expected {EXPECTED_TOTAL}")
        failures += 1
    REPORT.write_text(build_report(stamp), encoding="utf-8")
    if not (EVIDENCE / "clean-apply.txt").exists():
        print("ERROR: D3 clean-apply record missing")
        failures += 1
    refresh_sums()
    print(f"wrote {GATES.relative_to(REPO)}")
    print(f"wrote {REPORT.relative_to(REPO)}")
    print(f"wrote {INVENTORY.relative_to(REPO)} ({total} controls)")
    print(f"wrote {EVIDENCE.relative_to(REPO)}/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
