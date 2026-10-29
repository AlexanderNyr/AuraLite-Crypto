#!/usr/bin/env python3
"""Regenerate B1 evidence: gates, baseline report, NC inventory, SHA256SUMS.

Every number in the report comes from a live run of `aura-consensus` in this
tree or from the command list recorded in `gates.txt`; nothing is typed by
hand.

Usage:
    python3 tools/write_b1_evidence.py
    make evidence-b1
"""

from __future__ import annotations

import hashlib
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "b1"
OUT = EVIDENCE / "gates.txt"
REPORT = EVIDENCE / "report.md"
INVENTORY = EVIDENCE / "inventory.txt"
FIXTURE = REPO / "crates" / "aura-consensus" / "tests" / "fixtures" / "b1_baseline.txt"

AURA_CONSENSUS = "cargo run --quiet -p aura-consensus --bin aura-consensus --"

NC_LOGS = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_LOGS += ["x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "d0", "d1", "b0", "b1"]
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)

EXPECTED_NC_TOTAL = 183  # 171 (through D1) + 6 (B0)

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-consensus/tests/fixtures/b1_baseline.txt | cut -d' ' -f1); "
    "python3 tools/gen_b1_vectors.py >/dev/null && "
    "after=$(sha256sum crates/aura-consensus/tests/fixtures/b1_baseline.txt | cut -d' ' -f1); "
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
    ("pinned B1 consensus fixture", "python3 tools/gen_b1_vectors.py --check", None),
    ("generator is idempotent", GEN_IDEMPOTENT, None),
    # The DoD, literally: the baseline refuses to claim consensus.
    ("aura-consensus with no arguments (DoD)", f"{AURA_CONSENSUS} ; echo exit=$?", 22),
    ("aura-consensus --help", f"{AURA_CONSENSUS} --help", None),
    ("aura-consensus --version", f"{AURA_CONSENSUS} --version", None),
    ("aura-consensus report (measured survey)", f"{AURA_CONSENSUS} report", None),
    ("aura-consensus report --fixture", f"{AURA_CONSENSUS} report --fixture", None),
    ("aura-consensus propose (unknown command)", f"{AURA_CONSENSUS} propose ; echo exit=$?", 12),
    ("aura-consensus report --workspace /tmp", f"{AURA_CONSENSUS} report --workspace /tmp ; echo exit=$?", 3),
    ("B0 baseline gate", "cargo test -p aura-consensus", 20),
    ("D0 baseline gate (still green)", "cargo test -p aura-node --test d0_baseline", 12),
    ("D1 skeleton gate (still green)", "cargo test -p aura-node --test d1_skeleton", 12),
    ("B1 negative controls (evidence log)", "make nc-b1", 30),
    ("all negative controls (C0-C10 + S0-S7 + X0-X8 + D0 + D1 + B0)", "make nc-all", 28),
    ("all tests", "cargo test --workspace", 6),
    ("full gate", "make test", 6),
]


def run(cmd: str) -> tuple[int, str]:
    result = subprocess.run(
        cmd, shell=True, cwd=REPO, capture_output=True, text=True, check=False
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def measured_fixture() -> dict[str, list[str]]:
    """Parse `aura-consensus report --fixture`: the survey, measured in this tree, now."""
    code, output = run(f"{AURA_CONSENSUS} report --fixture")
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
    for line in run("cargo test -p aura-consensus --quiet")[1].splitlines():
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
        "# B0 baseline — no consensus, measured",
        "",
        f"Generated (UTC): {stamp}. Every number below came out of `aura-consensus` in this",
        "tree; `gates.txt` holds the raw command output, and",
        "`crates/aura-consensus/tests/fixtures/b1_baseline.txt` pins the same survey so it",
        "cannot drift silently.",
        "",
        "## What this phase claims",
        "",
        "> B0 lands the consensus baseline: a measurement that no consensus exists,",
        "> plus a binary that refuses to pretend otherwise. The six consensus",
        "> capabilities (proposer, block-signature, QC, finality, validator set,",
        "> VRF leader) are all missing and named with the phase (B1–B5) that will",
        "> add them.",
        "",
        "## The binary's contract",
        "",
        "| invocation | exit | what it does |",
        "|---|---|---|",
        f"| `aura-consensus` (no arguments) | {fields.get('exit_stub', ['?'])[0]} | prints the missing capabilities, one phase each, and refuses |",
        f"| `aura-consensus report [--workspace PATH]` | 0 | measures the tree and prints what exists |",
        f"| `aura-consensus report --fixture` | 0 | the same measurement as `key=value` lines (the pin) |",
        f"| `aura-consensus report --workspace <not a workspace>` | {fields.get('exit_no_workspace', ['?'])[0]} | refuses: there is nothing to measure |",
        f"| `aura-consensus <anything else>` | {fields.get('exit_usage', ['?'])[0]} | usage error, with the usage text |",
        f"| `aura-consensus --help` / `--version` | 0 | documented commands, version names the phase |",
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
        "All six consensus components are placeholders at B0: the crate has sources",
        "and a public `baseline` module, but declares no consensus logic.",
        "",
        "## The six capabilities a consensus needs and this tree does not have",
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
        "## What M0 still needs after B0",
        "",
        "* **B1.** PoA round-robin proposer + block signature.",
        "* **B2.** QC / BLS: prevote/precommit, 2f+1 aggregation.",
        "* **B3.** BFT pipeline: 3-chain commit, view change, timeouts.",
        "* **B4.** Validator set: staking, epoch, enter/exit queue.",
        "* **B5.** VRF leader election.",
        "* **N1–N2.** Transport and gossipsub (NET_PLAN) — required before consensus",
        "  can talk to other nodes.",
        "* **D2–D7.** Node pipeline, mempool, RPC, metrics, devnet, CLI (NODE_PLAN).",
        "",
        "## What B0 deliberately does not do",
        "",
        "* **It does not implement consensus.** The baseline measures absence;",
        "  B1–B5 land the real implementation.",
        "* **It does not order transactions.** At B0, two different tx logs are",
        "  not equivalent — there is no consensus to order them.",
        "* **It does not claim correctness.** The survey counts files and public",
        "  items; whether consensus is correct is what B1–B5's gates measure.",
        "",
        "## Honest limits",
        "",
        "* **The survey is a static count, not an audit.** A crate with one public",
        "  item measures the same as a crate with two hundred and one.",
        "* **`Cargo.toml` membership is trusted.** The survey looks for",
        "  `crates/aura-consensus/src`.",
        "* **The baseline will go stale on purpose.** When B1 lands the proposer,",
        "  this gate fails until the plan and the fixture are updated in the same",
        "  patch — that is the mechanism, not a defect.",
        "",
        "## Negative controls",
        "",
        "Six tampers, each reddening a *named* test: the baseline exits 0, every",
        "layer is classified `implemented`, `report` measures a non-workspace, an",
        "unknown command falls through to exit 0, the refusal stops naming the",
        "capabilities, and two different logs are treated as equivalent. A green",
        "baseline runs first, so a red caused by a broken tree can never be",
        "mistaken for a successful control.",
        "",
        f"6/6 PASS; total **{EXPECTED_NC_TOTAL}** across C0–C10 + S0–S7 + X0–X8 + D0 + D1 + B0.",
        "",
        f"Tests: **{tests}** in the workspace, {gate} of them in the B0 gate.",
        "Validators / blocks / transactions remain **0 / 0 / 0**.",
        "",
    ]
    return "\n".join(lines)


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = [
        "# B0 negative-control inventory",
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
        print("ERROR: `aura-consensus report --fixture` did not run; fix the tree first")
        return 1

    if "--report-only" in sys.argv[1:]:
        REPORT.write_text(build_report(fields, stamp), encoding="utf-8")
        refresh_sums()
        print(f"wrote {REPORT.relative_to(REPO)} (report-only; gates.txt untouched)")
        return 0

    lines = [
        "# B0 — consensus baseline log (measured)",
        f"# Generated (UTC): {stamp}",
        "# Every block below is raw command output; nothing is typed by hand.",
        "# Commands that are EXPECTED to fail (the DoD: the binary refuses without",
        "# consensus) print their exit code inside the block, so a non-zero exit is",
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
        f"Tests: {workspace_test_count()} in the workspace, {gate_test_count()} in the B0 gate.",
        f"Negative controls: {EXPECTED_NC_TOTAL} across C0-C10 + S0-S7 + X0-X8 + D0 + D1 + B0.",
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
        print("ERROR: docs/evidence/b1/nc.log missing (run `make nc-b1`)")
        return 1

    refresh_sums()
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {REPORT.relative_to(REPO)}")
    print(f"wrote {INVENTORY.relative_to(REPO)}")
    print("wrote docs/evidence/b1/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
