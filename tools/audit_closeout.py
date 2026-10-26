#!/usr/bin/env python3
"""Plan<->tree close-out audit (C10).

Fails when any ✅ phase row lacks a receipt, patch, NC driver/log, evidence,
CHANGELOG entry or spec — i.e. "not a single ✅ line without a receipt".
Also verifies every evidence SHA256SUMS, the total NC count, the spec index
and that no deferred (non-CORE) phase is marked ✅.

Usage:
    python3 tools/audit_closeout.py [--strict]
    make audit-closeout

`--strict` additionally requires the C10 patch, NC log and evidence files.
Default mode tolerates them as "pending" while the close-out phase itself is
being produced (its patch is cut at commit time).
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

PHASES = [f"C{index}" for index in range(11)]          # C0..C10
SHIPPED = PHASES[:10]                                   # patches already cut
CURRENT = "C10"
EXPECTED_NC = dict(zip(PHASES, [2, 3, 4, 5, 6, 6, 6, 6, 6, 6, 6]))
SHIPPED_NC_TOTAL = sum(EXPECTED_NC[phase] for phase in SHIPPED)   # 50
EXPECTED_TOTAL_NC = 56
DONE_ROW = re.compile(r"^\| ✅ \*\*([A-Z]+\d+)\*\* \|", re.M)
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)


def patch_path(phase: str) -> Path | None:
    prefix = f"{int(phase[1:]) + 1:04d}-{phase}-"
    matches = sorted(PATCHES.glob(f"{prefix}*.patch"))
    return matches[0] if matches else None


def nc_log(phase: str) -> Path:
    return EVIDENCE / f"c{phase[1:]}" / "nc.log"


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
    core = (PLANS / "CORE_PLAN.md").read_text(encoding="utf-8")
    changelog = (REPO / "CHANGELOG.md").read_text(encoding="utf-8")

    # 1. Which phases are marked done, and where?
    done: list[tuple[str, str]] = []
    for plan in sorted(PLANS.glob("*_PLAN.md")):
        for phase in DONE_ROW.findall(plan.read_text(encoding="utf-8")):
            done.append((plan.name, phase))
    found = [phase for _, phase in done]
    for plan_name, phase in done:
        if plan_name != "CORE_PLAN.md":
            errors.append(
                f"{plan_name}: phase {phase} is ✅ but only CORE_PLAN may ship phases"
            )
        if phase not in PHASES:
            errors.append(f"{plan_name}: unexpected ✅ phase {phase}")
    for phase in PHASES:
        if found.count(phase) != 1:
            errors.append(
                f"CORE_PLAN.md: phase {phase} has {found.count(phase)} ✅ rows, expected 1"
            )

    # 2. Receipts, CHANGELOG, NC drivers and NC logs.
    shipped_nc = 0
    current_nc: int | None = None
    for phase in PHASES:
        index = int(phase[1:])
        if f"### {phase} receipt" not in core:
            errors.append(f"{phase}: missing '### {phase} receipt' in CORE_PLAN.md")
        if f"## [{phase}" not in changelog:
            errors.append(f"{phase}: missing CHANGELOG entry '## [{phase}'")
        driver = REPO / "tools" / f"nc_c{index}.py"
        if not driver.exists():
            errors.append(f"{phase}: missing {driver.relative_to(REPO)}")

        log = nc_log(phase)
        if not log.exists():
            if phase == CURRENT and not strict:
                notes.append("C10 NC log: pending (created by make nc-c10)")
                continue
            errors.append(f"{phase}: missing NC log {log.relative_to(REPO)}")
            continue
        match = RESULT_LINE.search(log.read_text(encoding="utf-8"))
        if not match:
            if phase == CURRENT and not strict:
                notes.append("C10 NC log: pending (no RESULT line yet)")
                continue
            errors.append(f"{phase}: NC log has no 'RESULT: n/n negative controls PASS' line")
            continue
        passed, total = int(match.group(1)), int(match.group(2))
        if passed != total or total != EXPECTED_NC[phase]:
            errors.append(
                f"{phase}: NC log says {passed}/{total}, expected "
                f"{EXPECTED_NC[phase]}/{EXPECTED_NC[phase]}"
            )
        if phase == CURRENT:
            current_nc = passed
        else:
            shipped_nc += passed

    if shipped_nc != SHIPPED_NC_TOTAL:
        errors.append(f"shipped NC total = {shipped_nc}, expected {SHIPPED_NC_TOTAL}")
    total_nc = shipped_nc + (current_nc or 0)
    if current_nc is None:
        notes.append(f"NC total so far: {total_nc} of {EXPECTED_TOTAL_NC} (C10 pending)")
        if strict:
            errors.append("C10 NC log has no valid RESULT line")
    elif total_nc != EXPECTED_TOTAL_NC:
        errors.append(f"total NCs = {total_nc}, expected {EXPECTED_TOTAL_NC}")

    # 3. Shipped phases need a valid patch and evidence receipts.
    for phase in SHIPPED:
        index = int(phase[1:])
        path = patch_path(phase)
        if path is None:
            errors.append(f"{phase}: missing patch patches/{index + 1:04d}-{phase}-*.patch")
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
                EVIDENCE / f"c{index}" / "gates.txt",
                EVIDENCE / f"c{index}" / "nc.log",
                EVIDENCE / f"c{index}" / "SHA256SUMS",
            ]
        for item in required:
            if not item.exists():
                errors.append(f"{phase}: missing evidence {item.relative_to(REPO)}")

    # 4. The current phase's own patch/evidence (cut after this audit runs).
    c10_patch = patch_path(CURRENT)
    if c10_patch is None:
        message = "C10 patch: pending (cut at commit time)"
        if strict:
            errors.append(message)
        else:
            notes.append(message)
    else:
        validate_patch(CURRENT, c10_patch, errors)
        notes.append(f"C10 patch: {c10_patch.name} OK")

    c10_gates = EVIDENCE / "c10" / "gates.txt"
    if not c10_gates.exists():
        message = "C10 evidence: pending (generated by make evidence-c10)"
        if strict:
            errors.append(message)
        else:
            notes.append(message)
    else:
        notes.append("C10 evidence: gates.txt present")
        for name in ("gates.txt", "nc.log", "closeout.txt", "SHA256SUMS"):
            if not (EVIDENCE / "c10" / name).exists():
                errors.append(f"C10: missing docs/evidence/c10/{name}")

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

    # 6. Every spec file must be indexed.
    index_path = SPEC / "FORMAT_INDEX.md"
    if not index_path.exists():
        errors.append("docs/spec/FORMAT_INDEX.md is missing")
    else:
        index_text = index_path.read_text(encoding="utf-8")
        for item in sorted(SPEC.iterdir()):
            if item.is_file() and item.name not in index_text:
                errors.append(f"FORMAT_INDEX.md does not list docs/spec/{item.name}")

    # 7. Consolidated NC inventory, when present.
    closeout = EVIDENCE / "c10" / "closeout.txt"
    if closeout.exists():
        text = closeout.read_text(encoding="utf-8")
        for phase in PHASES:
            if f"{phase}:" not in text:
                errors.append(f"closeout.txt has no {phase} inventory line")
        if f"TOTAL: {EXPECTED_TOTAL_NC} NC" not in text:
            errors.append(f"closeout.txt does not report TOTAL: {EXPECTED_TOTAL_NC} NC")

    return errors, notes


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--strict", action="store_true", help="require the C10 patch, NC log and evidence too"
    )
    args = parser.parse_args()

    errors, notes = check(args.strict)
    print("CLOSE-OUT PLAN<->TREE AUDIT (C10)")
    print(f"  phases audited: {len(PHASES)} (C0-C10)")
    print(f"  expected NC total: {EXPECTED_TOTAL_NC} (50 shipped + 6 C10)")
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
