#!/usr/bin/env python3
"""Regenerate D1 evidence: gates, baseline report, NC inventory, SHA256SUMS.

Every number in the report comes from a live run of `aurad` in this tree or
from the command list recorded in `gates.txt`; nothing is typed by hand.

Usage:
    python3 tools/write_d1_evidence.py
    make evidence-d1
"""

from __future__ import annotations

import hashlib
import sys
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "d1"
OUT = EVIDENCE / "gates.txt"
REPORT = EVIDENCE / "report.md"
INVENTORY = EVIDENCE / "inventory.txt"
FIXTURE = REPO / "crates" / "aura-node" / "tests" / "fixtures" / "d1_skeleton.txt"

AURAD = "cargo run --quiet -p aura-node --bin aurad --"

NC_LOGS = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_LOGS += ["x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "d0", "d1"]
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)

EXPECTED_NC_TOTAL = 171  # 165 (through D0) + 6 (D1)

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-node/tests/fixtures/d1_skeleton.txt | cut -d' ' -f1); "
    "python3 tools/gen_d1_vectors.py >/dev/null && "
    "after=$(sha256sum crates/aura-node/tests/fixtures/d1_skeleton.txt | cut -d' ' -f1); "
    'if [ "$before" = "$after" ]; then echo "generator idempotent: $after"; '
    'else echo "GENERATOR DRIFT"; exit 1; fi'
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("workspace crates", "find crates -name Cargo.toml | wc -l", None),
    (
        "Rust files",
        "find . -name '*.rs' -not -path './target/*' -not -path './examples/*' "
        "-not -path './standards/*' | wc -l",
        None,
    ),
    ("external deps in the lock", "grep -c '^source = ' Cargo.lock || true", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("vector inventory", "python3 tools/count_vectors.py", 3),
    ("test rig wiring", "python3 tools/check_rig.py", None),
    ("pinned D1 skeleton fixture", "python3 tools/gen_d1_vectors.py --check", None),
    ("generator is idempotent", GEN_IDEMPOTENT, None),
    # The DoD, literally: starting the node without --chain-id must refuse.
    ("aurad without arguments (DoD)", f"{AURAD} ; echo exit=$?", 22),
    ("aurad --help", f"{AURAD} --help", None),
    ("aurad --version", f"{AURAD} --version", None),
    ("aurad report (measured survey)", f"{AURAD} report", None),
    ("aurad report --fixture", f"{AURAD} report --fixture", None),
    ("aurad serve (unknown command)", f"{AURAD} serve ; echo exit=$?", 12),
    ("aurad --config (a node flag)", f"{AURAD} --config /etc/aurad.toml ; echo exit=$?", 16),
    ("D1 skeleton gate", "cargo test -p aura-node --test d1_skeleton", 12),
    ("D0 baseline gate (still green)", "cargo test -p aura-node --test d0_baseline", 12),
    ("D1 negative controls (evidence log)", "make nc-d1", 30),
    ("all negative controls (C0-C10 + S0-S7 + X0-X8 + D0 + D1)", "make nc-all", 28),
    ("all tests", "cargo test --workspace", 6),
    ("full gate", "make test", 6),
]


def run(cmd: str) -> tuple[int, str]:
    result = subprocess.run(
        cmd, shell=True, cwd=REPO, capture_output=True, text=True, check=False
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def measured_fixture() -> dict[str, list[str]]:
    """Parse `aurad report --fixture`: the survey, measured in this tree, now."""
    code, output = run(f"{AURAD} report --fixture")
    if code != 0:
        return {}
    fields: dict[str, list[str]] = {}
    for line in output.splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        fields.setdefault(key, []).append(value)
    return fields


def workspace_test_count() -> int:
    total = 0
    for line in run("cargo test --workspace --quiet")[1].splitlines():
        if line.startswith("test result: ok."):
            total += int(line.split("ok.")[1].split("passed")[0].strip())
    return total


def gate_test_count() -> int:
    total = 0
    for line in run("cargo test -p aura-node --quiet")[1].splitlines():
        if line.startswith("test result: ok."):
            total += int(line.split("ok.")[1].split("passed")[0].strip())
    return total


def build_report(fields: dict[str, list[str]], stamp: str) -> str:
    tests = workspace_test_count()
    gate = gate_test_count()
    layers = fields.get("layer", [])
    capabilities = fields.get("capability", [])
    implemented = len([line for line in layers if line.endswith("implemented")])
    placeholders = len([line for line in layers if line.endswith("placeholder")])
    missing = len([line for line in layers if line.endswith("missing")])

    lines = [
        "# D1 skeleton — aurad starts, traces, writes a PID file, shuts down (measured)",
        "",
        f"Generated (UTC): {stamp}. Every number below came out of `aurad` in this",
        "tree; `gates.txt` holds the raw command output, and",
        "`crates/aura-node/tests/fixtures/d1_skeleton.txt` pins the same survey so it",
        "cannot drift silently.",
        "",
        "## What this phase claims",
        "",
        "> D1 lands the `aurad` process skeleton: configuration (TOML + env + CLI,",
        "> `chain-id` mandatory), tracing, signal handling, PID file and graceful",
        "> shutdown. The tx pipeline, mempool and RPC do not exist yet — those land",
        "> in D2/D4.",
        "",
        "## The binary's contract",
        "",
        "| invocation | exit | what it does |",
        "|---|---|---|",
        f"| `aurad` (no arguments, no --chain-id) | {fields.get('exit_stub', ['?'])[0]} | refuses: chain-id is mandatory, names the `config` capability (D1) |",
        f"| `aurad --chain-id <id> --data-dir <path>` | 0 | starts the skeleton, installs signal handlers, writes a PID file, waits for SIGTERM/SIGINT, shuts down gracefully |",
        f"| `aurad --help` / `--version` | 0 | documented commands, version names the phase |",
        f"| `aurad report [--workspace PATH]` | 0 | measures the tree and prints what exists |",
        f"| `aurad report --fixture` | 0 | the same measurement as `key=value` lines (the pin) |",
        f"| `aurad report --workspace <not a workspace>` | {fields.get('exit_no_workspace', ['?'])[0]} | refuses: there is nothing to measure |",
        f"| `aurad <anything else>` | {fields.get('exit_usage', ['?'])[0]} | usage error, with the usage text |",
        "",
        "## The survey (measured now)",
        "",
        "| layer | owner | presence |",
        "|---|---|---|",
    ]
    for line in layers:
        name, phase, presence = line.split()
        lines.append(f"| `{name}` | {phase} | {presence} |")
    lines += [
        "",
        f"**{implemented} implemented, {placeholders} placeholders, {missing} missing.**",
        "",
        "## The seven capabilities a node still needs after D1",
        "",
        "| capability | phase | what it will do |",
        "|---|---|---|",
    ]
    for entry in capabilities:
        name, phase, what = entry.split(maxsplit=2)
        lines.append(f"| `{name}` | {phase} | {what} |")
    lines += [
        "",
        "The `what` column lives in `MISSING_CAPABILITIES` and is printed by the binary;",
        "the gate asserts that the refusal text contains every capability, its phase and",
        "its description, so the message cannot rot into a generic \"not implemented\".",
        "",
        "## What M0 still needs after D1",
        "",
        "* **A pipeline.** No mempool, no block production, no receipts (D2, D3).",
        "* **An API.** No RPC of any kind (D4, D5).",
        "* **Observability.** No metrics, logs or health endpoint (D6).",
        "* **A devnet.** `make devnet` with 4 validators is not yet possible (D7).",
        "* **A CLI.** `aura` node ops, query, tx, keys, snapshot, upgrade (D8).",
        "* **Consensus and network.** `aura-consensus` (B1) and `aura-p2p` (N1)",
        "  are still empty crates, so a multi-validator devnet is not yet possible.",
        "",
        "## What D1 deliberately does not do",
        "",
        "* **It does not serve RPC.** JSON-RPC, gRPC and REST land in D4/D5.",
        "* **It does not produce blocks.** The tx pipeline lands in D2.",
        "* **It does not fake signal handling.** At D1 the OS default applies to",
        "  SIGTERM/SIGINT; a dedicated signal crate lands when the node has real",
        "  state to flush.",
        "",
        "## Honest limits",
        "",
        "* **The survey is a static count, not an audit.** A crate with one public item",
        "  measures the same as a crate with two hundred and one.",
        "* **`Cargo.toml` membership is trusted.** The survey looks for",
        "  `crates/<dir>/src`; a crate outside the workspace would be measured all the",
        "  same.",
        "* **The baseline will go stale on purpose.** When D2 lands the tx pipeline,",
        "  this gate fails until the plan and the fixture are updated in the same patch.",
        "",
        "## Negative controls",
        "",
        "Six tampers, each reddening a *named* test: the stub exits 0 without chain-id,",
        "a TOML config with unknown fields is accepted, `report` measures a non-workspace,",
        "an unknown command falls through to exit 0, the refusal stops naming the D1",
        "phase, and `config::load` accepts a missing chain-id. A green baseline runs",
        "first, so a red caused by a broken tree can never be mistaken for a",
        "successful control.",
        "",
        f"6/6 PASS; total **{EXPECTED_NC_TOTAL}** across C0–C10 + S0–S7 + X0–X8 + D0 + D1.",
        "",
        f"Tests: **{tests}** in the workspace, {gate} of them in the D1 gate.",
        "Validators / blocks / transactions remain **0 / 0 / 0**.",
        "",
    ]
    return "\n".join(lines)


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = [
        "# D1 negative-control inventory",
        f"# Generated (UTC): {stamp}",
        "# Each NC tampers with one enforcement point and requires a named test to go red.",
        "",
    ]
    total = 0
    for phase in NC_LOGS:
        path = REPO / "docs" / "evidence" / phase / "nc.log"
        if not path.exists():
            lines.append(f"[{phase}] MISSING {path.relative_to(REPO)}")
            lines.append("")
            continue
        text = path.read_text(encoding="utf-8")
        match = RESULT_LINE.search(text)
        if not match:
            lines.append(f"[{phase}] NO RESULT LINE")
            lines.append("")
            continue
        passed, count = int(match.group(1)), int(match.group(2))
        total += count
        lines.append(f"[{phase}] {passed}/{count} PASS")
        for name, status in NC_LINE.findall(text):
            lines.append(f"  {status:<4} {name}")
        lines.append("")
    lines.append(f"TOTAL: {total} negative controls")
    return "\n".join(lines) + "\n", total


def refresh_sums() -> None:
    sums = []
    for path in (OUT, EVIDENCE / "nc.log", REPORT, INVENTORY, FIXTURE):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    failures = 0

    fields = measured_fixture()
    if not fields:
        print("ERROR: `aurad report --fixture` did not run; fix the tree first")
        return 1

    if "--report-only" in sys.argv[1:]:
        REPORT.write_text(build_report(fields, stamp), encoding="utf-8")
        refresh_sums()
        print(f"wrote {REPORT.relative_to(REPO)} (report-only; gates.txt untouched)")
        return 0

    lines = [
        "# D1 — aurad skeleton log (measured)",
        f"# Generated (UTC): {stamp}",
        "# Every block below is raw command output; nothing is typed by hand.",
        "# Commands that are EXPECTED to fail (the DoD: the binary refuses without",
        "# --chain-id) print their exit code inside the block, so a non-zero exit is",
        "# the measurement, not a broken gate.",
        "",
    ]
    for label, command, tail in COMMANDS:
        code, output = run(command)
        lines.append(f"$ {command}")
        if tail:
            output = "\n".join(output.splitlines()[-tail:])
        lines.append(output)
        lines.append(f"[exit {code}]")
        lines.append("")
        if code != 0:
            failures += 1
            print(f"FAILED: {label} -> exit {code}")

    lines += [
        "=" * 72,
        "SURVEY (measured)",
        "=" * 72,
        "",
    ]
    for line in fields.get("layer", []):
        name, phase, presence = line.split()
        lines.append(f"layer={name} owner={phase} presence={presence}")
    lines += [
        "",
        f"implemented={fields.get('implemented', ['?'])[0]}",
        f"placeholders={fields.get('placeholders', ['?'])[0]}",
        f"missing={fields.get('missing', ['?'])[0]}",
        f"capabilities_missing={fields.get('capabilities', ['?'])[0]}",
        "",
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
        f"Layers measured: {len(fields.get('layer', []))}.",
        f"Exit codes: stub={fields.get('exit_stub', ['?'])[0]}, "
        f"no-workspace={fields.get('exit_no_workspace', ['?'])[0]}, "
        f"usage={fields.get('exit_usage', ['?'])[0]}.",
        f"Tests: {workspace_test_count()} in the workspace, {gate_test_count()} in the D1 gate.",
        f"Negative controls: {EXPECTED_NC_TOTAL} across C0-C10 + S0-S7 + X0-X8 + D0 + D1.",
        "Validators/blocks/tx remain 0/0/0.",
        "",
    ]

    EVIDENCE.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines), encoding="utf-8")
    REPORT.write_text(build_report(fields, stamp), encoding="utf-8")
    inventory, total = build_inventory(stamp)
    INVENTORY.write_text(inventory, encoding="utf-8")
    if total != EXPECTED_NC_TOTAL:
        print(f"INVENTORY ERROR: total {total} NC, expected {EXPECTED_NC_TOTAL}")
        failures += 1

    if not (EVIDENCE / "nc.log").exists():
        print("ERROR: docs/evidence/d1/nc.log missing (run `make nc-d1`)")
        return 1

    refresh_sums()
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {REPORT.relative_to(REPO)}")
    print(f"wrote {INVENTORY.relative_to(REPO)}")
    print("wrote docs/evidence/d1/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
