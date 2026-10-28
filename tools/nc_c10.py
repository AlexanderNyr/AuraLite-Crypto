#!/usr/bin/env python3
"""Negative controls for C10 (plan<->tree close-out audit).

Six tamper tests must each turn the close-out audit red:

  NC-1: delete a phase receipt from CORE_PLAN.
  NC-2: forge a ✅ row without a receipt. While STATE_PLAN still has a pending
        phase the tamper marks that phase ✅; once the plan is complete (S7),
        the tamper removes the newest `### Sx receipt` heading instead. Both
        are the same rule: no ✅ row without a receipt.
  NC-3: forge an NC log result.
  NC-4: unindex a format spec.
  NC-5: forge a patch subject.
  NC-6: corrupt an evidence checksum.

All touched files are restored byte-for-byte in a `finally` block (several
are historical evidence pinned by SHA256SUMS). `make nc-c10` captures stdout
into docs/evidence/c10/nc.log; a final green audit restore check is mandatory.
"""

from __future__ import annotations

import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CORE_PLAN = REPO / "docs" / "plans" / "CORE_PLAN.md"
STATE_PLAN = REPO / "docs" / "plans" / "STATE_PLAN.md"
FORMAT_INDEX = REPO / "docs" / "spec" / "FORMAT_INDEX.md"
C9_NC_LOG = REPO / "docs" / "evidence" / "c9" / "nc.log"
C7_PATCH = REPO / "patches" / "0008-C7-account-model.patch"
C9_SUMS = REPO / "docs" / "evidence" / "c9" / "SHA256SUMS"

AUDIT = ["python3", "tools/audit_closeout.py"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str) -> bool:
    original = path.read_text(encoding="utf-8")
    if old not in original:
        print(f"{label}: FAIL — tamper target not found in {path.name}")
        return False
    tampered = original.replace(old, new, 1)
    if tampered == original:
        print(f"{label}: FAIL — tamper was a no-op")
        return False
    try:
        path.write_text(tampered, encoding="utf-8")
        result = run(AUDIT)
        went_red = result.returncode != 0
        snippet = "\n".join((result.stdout + result.stderr).strip().splitlines()[-4:])
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      tools/audit_closeout.py -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    print(f"C10 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print("These NCs prove the plan<->tree audit detects missing receipts.")
    print()

    first_hash = C9_SUMS.read_text(encoding="utf-8").splitlines()[0][:64]
    flipped = ("0" if first_hash[0] != "0" else "1") + first_hash[1:]

    # Pick the FIRST not-yet-done S phase dynamically, so this NC keeps
    # working as STATE_PLAN advances (it used to hard-code S1). Once every S
    # phase is shipped (STATE_PLAN complete), the same rule is exercised from
    # the other side: remove the newest receipt heading.
    state_text = STATE_PLAN.read_text(encoding="utf-8")
    pending = re.search(r"^\| \*\*(S\d+)\*\* \|", state_text, re.M)
    if pending is not None:
        old_row = pending.group(0)
        new_row = old_row.replace("**", "✅ **", 1)
        print(f"NC-2 target: {old_row.strip()} -> {new_row.strip()}")
    else:
        receipts = re.findall(r"^### (S\d+) receipt", state_text, re.M)
        if not receipts:
            print("NC-2 setup: FAIL — no S receipts and no pending S phase")
            return 1
        newest = receipts[-1]
        old_row = f"### {newest} receipt"
        new_row = f"### {newest} notes (no receipt)"
        print(f"NC-2 target (STATE_PLAN complete): '{old_row}' -> '{new_row}'")
    results = [
        expect_red(
            "NC-1 missing phase receipt",
            CORE_PLAN,
            "### C7 receipt (2026-10-26)",
            "### C7 notes (no receipt)",
        ),
        expect_red(
            "NC-2 done phase without a receipt",
            STATE_PLAN,
            old_row,
            new_row,
        ),
        expect_red(
            "NC-3 forged NC log result",
            C9_NC_LOG,
            "RESULT: 6/6 negative controls PASS",
            "RESULT: 5/6 negative controls PASS",
        ),
        expect_red(
            "NC-4 unindexed format spec",
            FORMAT_INDEX,
            "| C6 | [`merkle-proof-format.md`](merkle-proof-format.md) | compact inclusion proofs for tx and event leaves |",
            "# (spec deliberately removed by NC-4)",
        ),
        expect_red(
            "NC-5 forged patch subject",
            C7_PATCH,
            "Subject: [PATCH] C7 account model",
            "Subject: [PATCH] X7 account model",
        ),
        expect_red(
            "NC-6 corrupted evidence checksum",
            C9_SUMS,
            first_hash,
            flipped,
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    if passed != len(results):
        return 1

    audit = run(AUDIT)
    sums = run(["sh", "-c", "for f in $(find docs/evidence -name SHA256SUMS | sort); do sha256sum -c \"$f\" >/dev/null || exit 1; done"])
    ok = audit.returncode == 0 and sums.returncode == 0
    print(
        f"restore check: audit rc={audit.returncode}; checksums rc={sums.returncode} "
        f"({'GREEN as expected' if ok else 'STILL RED — FAIL'})"
    )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
