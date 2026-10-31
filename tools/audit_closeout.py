#!/usr/bin/env python3
"""Plan<->tree close-out audit (C10, extended for STATE_PLAN S0).

Fails when any ✅ phase row lacks a receipt, patch, NC driver/log, evidence,
CHANGELOG entry or spec — i.e. "not a single ✅ line without a receipt".
Also verifies every evidence SHA256SUMS, the total NC count, the spec index
and that no phase is marked done in the wrong plan.

Usage:
    python3 tools/audit_closeout.py [--strict]
    make audit-closeout

`--strict` additionally requires the current phase's patch and evidence.
Default mode tolerates them as "pending" while that phase is being produced.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
PLANS = REPO / "docs" / "plans"
SPEC = REPO / "docs" / "spec"
EVIDENCE = REPO / "docs" / "evidence"
PATCHES = REPO / "patches"

CORE_PHASES = [f"C{index}" for index in range(11)]      # C0..C10 (closed)
STATE_PHASES = ["S0", "S1", "S2", "S3", "S4", "S5", "S6", "S7"]  # STATE_PLAN complete
EXEC_PHASES = ["X0", "X1", "X2", "X3", "X4", "X5", "X6", "X7", "X8"]                # grows with each X phase
NODE_PHASES = ["D0", "D1", "D2", "D3", "D4", "D5", "D6"]                   # grows with each D phase
CONSENSUS_PHASES = ["B0", "B1", "B2", "B3"]                        # grows with each B phase
NET_PHASES = []                                                   # grows with each N phase
PHASES = CORE_PHASES + STATE_PHASES + EXEC_PHASES + NODE_PHASES + CONSENSUS_PHASES + NET_PHASES
SHIPPED = CORE_PHASES + [
    "S0",
    "S1",
    "S2",
    "S3",
    "S4",
    "S5",
    "S6",
    "S7",
    "X0",
    "X1",
    "X2",
    "X3",
    "X4",
    "X5",
    "X6",
    "X7",
    "X8",
    "D0",
    "D1",
    "D2",
    "D3",
    "D4",
    "D5",
    "B0",
    "B1",
    "B2",
    "B3",
]
CURRENT = "D6"

PLAN_FOR_PREFIX = {
    "C": "CORE_PLAN.md",
    "S": "STATE_PLAN.md",
    "X": "EXEC_PLAN.md",
    "B": "CONSENSUS_PLAN.md",
    "N": "NET_PLAN.md",
    "D": "NODE_PLAN.md",
    "W": "WALLET_PLAN.md",
    "E": "ECOSYSTEM_PLAN.md",
    "T": "ECONOMICS_PLAN.md",
    "H": "SECURITY_PLAN.md",
    "P": "PERF_PLAN.md",
    "O": "OPS_PLAN.md",
}

NC_PER_PHASE = {f"C{i}": 6 for i in range(4, 11)}
NC_PER_PHASE.update({"C0": 2, "C1": 3, "C2": 4, "C3": 5, "S0": 6, "S1": 6, "S2": 6, "S3": 6, "S4": 6, "S5": 6, "S6": 6, "S7": 6, "X0": 6, "X1": 6, "X2": 7, "X3": 6, "X4": 6, "X5": 6, "X6": 6, "X7": 6, "X8": 6, "D0": 6, "D1": 6, "D2": 6, "D3": 6, "D4": 6, "D5": 6, "D6": 6, "B0": 6, "B1": 6, "B2": 6, "B3": 6})
SHIPPED_NC_TOTAL = sum(NC_PER_PHASE[phase] for phase in SHIPPED)  # 207 (through D3)
EXPECTED_TOTAL_NC = SHIPPED_NC_TOTAL + (0 if CURRENT in SHIPPED else NC_PER_PHASE[CURRENT])

DONE_ROW = re.compile(r"^\| ✅ \*\*([A-Z]+\d+)\*\* \|", re.M)
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)


# Patches follow landing order across plans. D2 follows the already shipped
# B0-B3 series, rather than colliding with B0's historical 0031 number.
PATCH_ORDER = CORE_PHASES + STATE_PHASES + EXEC_PHASES + ["D0", "D1"]
PATCH_ORDER += ["B0", "B1", "B2", "B3"] + [f"D{i}" for i in range(2, 9)]
PATCH_ORDER += [f"B{i}" for i in range(4, 11)]
PATCH_NUMBERS = {phase: index + 1 for index, phase in enumerate(PATCH_ORDER)}


def patch_prefix(phase: str) -> str:
    number = PATCH_NUMBERS.get(phase)
    if number is None:
        # No numbered patch slot exists yet; never accept another plan's file.
        return f"----{phase}-"
    return f"{number:04d}-{phase}-"


def patch_path(phase: str) -> Path | None:
    matches = sorted(PATCHES.glob(f"{patch_prefix(phase)}*.patch"))
    return matches[0] if matches else None


def nc_log(phase: str) -> Path:
    return EVIDENCE / phase.lower() / "nc.log"


def sha_files() -> list[Path]:
    return sorted(EVIDENCE.rglob("SHA256SUMS"))


def validate_patch(phase: str, path: Path, errors: list[str]) -> None:
    text = path.read_text(encoding="utf-8")
    subject = re.search(r"^Subject: \[PATCH\] (.*)$", text, re.M)
    if not subject:
        errors.append(f"{phase}: {path.name} has no 'Subject: [PATCH]' header")
    elif phase not in subject.group(1):
        errors.append(
            f"{phase}: {path.name} subject does not mention {phase}: {subject.group(1)!r}"
        )
    if "diff --git" not in text:
        errors.append(f"{phase}: {path.name} has no diff hunks")


def check(strict: bool) -> tuple[list[str], list[str]]:
    errors: list[str] = []
    notes: list[str] = []
    changelog = (REPO / "CHANGELOG.md").read_text(encoding="utf-8")
    plan_texts = {p.name: p.read_text(encoding="utf-8") for p in PLANS.glob("*_PLAN.md")}

    # 1. Which phases are marked done, and in the right plan?
    done: list[tuple[str, str]] = []
    for plan_name, text in plan_texts.items():
        for phase in DONE_ROW.findall(text):
            done.append((plan_name, phase))
    found = [phase for _, phase in done]
    for plan_name, phase in done:
        expected_plan = PLAN_FOR_PREFIX.get(phase[0])
        if plan_name != expected_plan:
            errors.append(
                f"{plan_name}: phase {phase} is ✅ but belongs in {expected_plan}"
            )
        if phase not in PHASES:
            errors.append(f"{plan_name}: unexpected ✅ phase {phase}")
    for phase in PHASES:
        if found.count(phase) != 1:
            errors.append(
                f"phase {phase} has {found.count(phase)} ✅ rows, expected 1"
            )

    # 2. Receipts, CHANGELOG, NC drivers and NC logs.
    all_plan_text = "\n".join(plan_texts.values())
    shipped_nc = 0
    current_nc: int | None = None
    for phase in PHASES:
        lower = phase.lower()
        if f"### {phase} receipt" not in all_plan_text:
            errors.append(f"{phase}: missing '### {phase} receipt' in a plan")
        if f"## [{phase}" not in changelog:
            errors.append(f"{phase}: missing CHANGELOG entry '## [{phase}'")
        driver = REPO / "tools" / f"nc_{lower}.py"
        if not driver.exists():
            errors.append(f"{phase}: missing {driver.relative_to(REPO)}")

        log = nc_log(phase)
        if not log.exists():
            if phase == CURRENT and not strict:
                notes.append(f"{phase} NC log: pending (created by make nc-{lower})")
                continue
            errors.append(f"{phase}: missing NC log {log.relative_to(REPO)}")
            continue
        match = RESULT_LINE.search(log.read_text(encoding="utf-8"))
        if not match:
            if phase == CURRENT and not strict:
                notes.append(f"{phase} NC log: pending (no RESULT line yet)")
                continue
            errors.append(f"{phase}: NC log has no 'RESULT: n/n negative controls PASS' line")
            continue
        passed, total = int(match.group(1)), int(match.group(2))
        if passed != total or total != NC_PER_PHASE[phase]:
            errors.append(
                f"{phase}: NC log says {passed}/{total}, expected "
                f"{NC_PER_PHASE[phase]}/{NC_PER_PHASE[phase]}"
            )
        if phase == CURRENT:
            current_nc = passed
        if phase != CURRENT or CURRENT in SHIPPED:
            shipped_nc += passed

    if shipped_nc != SHIPPED_NC_TOTAL:
        errors.append(f"shipped NC total = {shipped_nc}, expected {SHIPPED_NC_TOTAL}")
    total_nc = shipped_nc if CURRENT in SHIPPED else shipped_nc + (current_nc or 0)
    if current_nc is None:
        notes.append(f"NC total so far: {total_nc} of {EXPECTED_TOTAL_NC} ({CURRENT} pending)")
        if strict:
            errors.append(f"{CURRENT} NC log has no valid RESULT line")
    elif total_nc != EXPECTED_TOTAL_NC:
        errors.append(f"total NCs = {total_nc}, expected {EXPECTED_TOTAL_NC}")

    # 3. Shipped phases need a valid patch and evidence receipts.
    for phase in SHIPPED:
        lower = phase.lower()
        path = patch_path(phase)
        if path is None:
            errors.append(f"{phase}: missing patch patches/{patch_prefix(phase)}*.patch")
        else:
            validate_patch(phase, path, errors)

        if phase == "C0":
            required = [
                EVIDENCE / "c0-baseline.txt",
                EVIDENCE / "c0" / "nc.log",
                EVIDENCE / "SHA256SUMS",
            ]
        else:
            required = [
                EVIDENCE / lower / "gates.txt",
                EVIDENCE / lower / "nc.log",
                EVIDENCE / lower / "SHA256SUMS",
            ]
        for item in required:
            if not item.exists():
                errors.append(f"{phase}: missing evidence {item.relative_to(REPO)}")

    # 4. The current phase's own patch/evidence (cut after this audit runs).
    current_patch = patch_path(CURRENT)
    if current_patch is None:
        message = f"{CURRENT} patch: pending (cut at commit time)"
        if strict:
            errors.append(message)
        else:
            notes.append(message)
    else:
        validate_patch(CURRENT, current_patch, errors)
        notes.append(f"{CURRENT} patch: {current_patch.name} OK")

    current_gates = EVIDENCE / CURRENT.lower() / "gates.txt"
    if not current_gates.exists():
        message = f"{CURRENT} evidence: pending (generated by make evidence-{CURRENT.lower()})"
        if strict:
            errors.append(message)
        else:
            notes.append(message)
    else:
        notes.append(f"{CURRENT} evidence: gates.txt present")
        current_report = "baseline-report.md" if CURRENT == "S0" else "report.md"
        for name in ("gates.txt", "nc.log", "SHA256SUMS", current_report, "inventory.txt"):
            if not (EVIDENCE / CURRENT.lower() / name).exists():
                errors.append(f"{CURRENT}: missing docs/evidence/{CURRENT.lower()}/{name}")

    # 5. Every evidence checksum must verify (paths are repo-root relative).
    for sums in sha_files():
        result = subprocess.run(
            ["sha256sum", "-c", str(sums.relative_to(REPO))],
            cwd=REPO,
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            bad = [line for line in result.stdout.splitlines() if not line.endswith(": OK")]
            errors.append(
                f"{sums.relative_to(REPO)}: checksum mismatch "
                f"({'; '.join(bad[:3]) if bad else result.stderr.strip()})"
            )

    # 6. Every spec file must be indexed (FORMAT_INDEX.md is the frozen CORE
    # index; S0 stores its report in evidence, not in docs/spec/).
    index_path = SPEC / "FORMAT_INDEX.md"
    if not index_path.exists():
        errors.append("docs/spec/FORMAT_INDEX.md is missing")
    else:
        index_text = index_path.read_text(encoding="utf-8")
        for item in sorted(SPEC.iterdir()):
            if item.is_file() and item.name not in index_text:
                errors.append(f"FORMAT_INDEX.md does not list docs/spec/{item.name}")

    # 7. Consolidated NC inventories, when present.
    closeout = EVIDENCE / "c10" / "closeout.txt"
    if closeout.exists():
        text = closeout.read_text(encoding="utf-8")
        for phase in CORE_PHASES:
            if f"{phase}:" not in text:
                errors.append(f"closeout.txt has no {phase} inventory line")
        if "TOTAL: 56 NC" not in text:
            errors.append("closeout.txt does not report the CORE total 56 NC")
    # Per-phase inventories are historical snapshots: each reports the NC
    # total AS OF that phase, not the current project-wide total.
    cumulative: list[tuple[str, int]] = []
    running = 56  # CORE closed with 56 NCs; S0 is the first phase after it
    inventory_order = STATE_PHASES + EXEC_PHASES + ["D0", "D1"]
    inventory_order += CONSENSUS_PHASES + ["D2"]
    inventory_order += [f"D{i}" for i in range(3, 9)] + [f"B{i}" for i in range(4, 11)]
    inventory_order += NET_PHASES
    for phase in inventory_order:
        if phase not in NC_PER_PHASE:
            continue
        running += NC_PER_PHASE[phase]
        cumulative.append((phase.lower(), running))
    for phase, expected in cumulative:
        path = EVIDENCE / phase / "inventory.txt"
        text = path.read_text(encoding="utf-8") if path.exists() else ""
        if path.exists() and f"TOTAL: {expected} " not in text:
            errors.append(f"{phase}/inventory.txt does not report TOTAL: {expected} NC")

    return errors, notes


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--strict", action="store_true", help="require the current phase patch and evidence too"
    )
    args = parser.parse_args()

    errors, notes = check(args.strict)
    print("CLOSE-OUT PLAN<->TREE AUDIT (CORE + STATE)")
    print(f"  phases audited: {len(PHASES)} ({PHASES[0]}-{PHASES[-1]})")
    current_label = (
        f"{SHIPPED_NC_TOTAL} shipped through {CURRENT}"
        if CURRENT in SHIPPED
        else f"{SHIPPED_NC_TOTAL} shipped + {NC_PER_PHASE[CURRENT]} {CURRENT}"
    )
    print(f"  expected NC total: {EXPECTED_TOTAL_NC} ({current_label})")
    print(f"  evidence checksum files: {len(sha_files())}")
    print(f"  spec files indexed: {len([p for p in SPEC.iterdir() if p.is_file()])}")
    for note in notes:
        print(f"  note: {note}")
    if errors:
        print(f"AUDIT: FAIL ({len(errors)} problem(s))")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("AUDIT: OK — every ✅ phase has a receipt, patch, NC log and evidence; all checksums verify")
    return 0


if __name__ == "__main__":
    sys.exit(main())
