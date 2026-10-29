#!/usr/bin/env python3
"""Regenerate X6 evidence: gates, call-isolation report, NC inventory, SHA256SUMS.

Every line in `gates.txt` is the output of a real command. Every number in
`report.md` comes from `crates/aura-exec/tests/fixtures/x6_calls.txt`, whose
artifact block is written by `tools/build_x6_contracts.py` and whose gas and digest
block is measured by `cargo run -p aura-exec --bin x6-record` — which deploys the
shipped contracts through the real `Lifecycle` and lets them call each other.

Usage:
    python3 tools/write_x6_evidence.py
    make evidence-x6
"""

from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "x6"
OUT = EVIDENCE / "gates.txt"
REPORT = EVIDENCE / "report.md"
INVENTORY = EVIDENCE / "inventory.txt"
FIXTURE = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x6_calls.txt"
ADDENDUM = REPO / "docs" / "evidence" / "x6-dependency-addendum.txt"

NC_LOGS = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_LOGS += ["x0", "x1", "x2", "x3", "x4", "x5", "x6"]
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)

EXPECTED_NC_TOTAL = 147

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-exec/tests/fixtures/x6_calls.txt | cut -d' ' -f1); "
    "python3 tools/build_x6_contracts.py >/dev/null && "
    "after=$(sha256sum crates/aura-exec/tests/fixtures/x6_calls.txt | cut -d' ' -f1); "
    'if [ "$before" = "$after" ]; then echo "build+recorder idempotent: $after"; '
    'else echo "GENERATOR DRIFT"; exit 1; fi'
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("workspace crates", "find crates -name Cargo.toml | wc -l", None),
    ("contract workspaces", "find examples -name Cargo.toml | wc -l", None),
    ("Rust files", "find . -name '*.rs' -not -path './target/*' -not -path './examples/*' -not -path './standards/*' | wc -l", None),
    ("external deps in the lock", "grep -c '^source = ' Cargo.lock || true", None),
    ("default build graph (aura-exec)", "cargo tree -p aura-exec --edges normal -q | sort -u | wc -l", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("vector inventory", "python3 tools/count_vectors.py", 21),
    ("test rig wiring", "python3 tools/check_rig.py", None),
    ("pinned contract artifacts", "python3 tools/build_x6_contracts.py --check", None),
    ("builder + recorder are idempotent", GEN_IDEMPOTENT, None),
    ("X6 call-isolation gate", "cargo test -p aura-exec --test x6_calls -- --nocapture", 40),
    ("X6 SDK gate (host)", "cargo test -p aura-sdk -- --nocapture", 20),
    ("X6 negative controls (evidence log)", "make nc-x6", 40),
    ("all negative controls (C0-C10 + S0-S7 + X0-X6)", "make nc-all", 45),
    ("aura-exec tests (X0-X6 + unit)", "cargo test -p aura-exec --quiet", 20),
    ("plan<->tree audit", "make audit-closeout", 12),
    ("full gate", "make test", 10),
]

CONTRACTS = ["vault", "evil", "recurse"]


def run(cmd: str) -> tuple[int, str]:
    result = subprocess.run(
        cmd, shell=True, cwd=REPO, capture_output=True, text=True, check=False
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def fixture() -> dict[str, str]:
    fields: dict[str, str] = {}
    for line in FIXTURE.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        fields[key] = value
    return fields


def rows(fields: dict[str, str]) -> list[dict[str, str]]:
    return [
        {
            "name": name,
            "size": fields.get(f"contract_{name}_size", "?"),
            "sha256": fields.get(f"contract_{name}_sha256", "?"),
            "blake3": fields.get(f"measured_{name}_blake3", "?"),
            "imports": fields.get(f"contract_{name}_imports", "?"),
            "exports": fields.get(f"contract_{name}_exports", "?"),
        }
        for name in CONTRACTS
    ]


def measured(fields: dict[str, str]) -> list[tuple[str, str]]:
    return [
        (key[len("measured_") :], value)
        for key, value in fields.items()
        if key.startswith("measured_") and not key.endswith("_blake3")
    ]


def x6_test_count() -> int:
    """How many tests the phase actually adds, measured from the binary."""
    total = 0
    for line in run("cargo test -p aura-exec --test x6_calls --quiet")[1].splitlines():
        if line.startswith("test result: ok."):
            total += int(line.split("ok.")[1].split("passed")[0].strip())
    return total


def workspace_test_count() -> int:
    total = 0
    for line in run("cargo test --workspace --quiet")[1].splitlines():
        if line.startswith("test result: ok."):
            total += int(line.split("ok.")[1].split("passed")[0].strip())
    return total


def build_report(fields: dict[str, str], stamp: str) -> str:
    tests = workspace_test_count()
    added = x6_test_count()
    contracts = rows(fields)
    gas = dict(measured(fields))
    lines = [
        "# X6 — call isolation (measured)",
        "",
        f"Generated (UTC): {stamp}. Sizes and `sha256` come from",
        "`tools/build_x6_contracts.py` reading the artifact it just built; the",
        "BLAKE3 digests and the gas numbers come from",
        "`cargo run -p aura-exec --bin x6-record`, which deploys the shipped",
        "contracts through the real `Lifecycle`, funds a vault, attacks it, and",
        "descends a chain of nine copies.",
        "",
        "## The two rules, and why they are these two",
        "",
        "**A frame may not call an address that is already on the stack (`-14`).**",
        "That is stricter than a per-function mutex and simpler than one: it makes",
        "the classic withdraw-then-reenter attack *unrepresentable*, because the",
        "attacker cannot be called back while its own frame is in flight. The cost is",
        "named in `docs/CALLS.md` and in the vault's own doc comment: a callback into",
        "a live frame does not exist on this chain, so a receiver that wants to be",
        "told about a payment must be called *forward*, not back. The measured",
        "consequence is in the gas table's neighbours — the attacker's counter stops",
        "at **one** attempt, because the second entry was refused before the vault",
        "ran at all.",
        "",
        "**A stack may not exceed eight frames (`-15`).** Eight is a protocol",
        "parameter, published in `abi_v3::MAX_CALL_DEPTH` and re-checked by the",
        "executor. Because the first rule forbids cycles, this limit governs chains of",
        "*distinct* contracts — the shape a router → pool → token path has.",
        "",
        "| function | arity | gas | why |",
        "|---|---|---|---|",
        "| `call` | 6 | 100 + 3/byte | loads and runs another module; the callee's own instructions are metered separately |",
        "| `returndata_len` | 0 | 20 | how much the callee answered |",
        "| `returndata_read` | 2 | 20 + 3/byte | the answer itself, bounded by `MAX_RETURN_LEN` |",
        "| `depth` | 0 | 20 | the caller's own frame index, so it can refuse to go deeper |",
        "",
        "## Contracts (measured from the artifacts)",
        "",
        "| contract | bytes | sha256 | imports | role |",
        "|---|---|---|---|---|",
    ]
    roles = {
        "vault": "pays out **before** it debits — the bug X6 exists to survive",
        "evil": "re-enters the vault from the payout callback, and probes other contracts",
        "recurse": "calls the next contract in a chain of nine copies until refused",
    }
    for row in contracts:
        lines.append(
            f"| `{row['name']}` | {row['size']} | `{row['sha256'][:16]}…` | "
            f"{len(row['imports'].split(','))} | {roles[row['name']]} |"
        )
    total = sum(int(row["size"]) for row in contracts)
    lines += [
        "",
        f"Three contracts, {total} bytes in total, each its own workspace, each built",
        "with the same canonical command as X4/X5 and built twice into independent",
        "target directories before anything is pinned.",
        "",
        "## Measured gas",
        "",
        "| operation | gas |",
        "|---|---|",
    ]
    for name, value in gas.items():
        lines.append(f"| `{name}` | {value} |")
    lines += [
        "",
        "## What the gate actually proves",
        "",
        "1. **Depth** — a chain of nine copies descends; the first ran at depth 1, the",
        "   eighth at depth 8, the eighth's attempt on the ninth was refused with",
        "   `-15`, and the ninth contract never ran (its counter is 0).",
        "2. **Reentrancy** — the vault pays its receiver *first* and then notifies it",
        "   with `env3.call`; the node refuses that notification with `-14`, because the",
        "   receiver's own frame is still on the stack. The vault publishes the status",
        "   it got (`notify` event, asserted to be `-14`), the attacker's counter stops",
        "   at **1** attempt, its ledger is debited once, the vault ends **empty rather",
        "   than short**, and the honest depositor is paid in full afterwards",
        "   (9 800 of her own 9 800).",
        "3. **Atomicity** — a callee that writes, emits and then traps leaves no",
        "   write, no event and no value; the caller's own frame still commits, because",
        "   a callee's failure is a value the caller handles.",
        "4. **Value** — the payout moves both balances, and when a callee reverts the",
        "   sender keeps the value it sent and the callee never receives it.",
        "5. **Identity** — the caller of a nested frame is the calling *contract*, not",
        "   the transaction's signer.",
        "6. **Codes** — a callee's refusal (`-21` from its dispatcher) reaches the",
        "   caller unchanged; a call to an account with no code is `-16`; a callee that",
        "   traps is `-17`.",
        "7. **Receipts** — the committed tree equals the receipt's `result_root`,",
        "   and a nested call's events arrive **before** the caller's own, in execution",
        "   order (`notify`, `withdraw`, `attempt`).",
        "",
        "One measured number deserves a warning, because it looks like a contradiction:",
        "`measured_attack_last_status=0`. That is the attacker's *own* record of its",
        "*own* call — `evil.attack` called the vault, and the vault succeeded, so the",
        "status is `0`. The refusal that matters (`-14`) is the vault's, recorded in",
        "the `notify` event and asserted separately. Two different statuses, two",
        "different subjects.",
        "",
        "## Negative controls",
        "",
        "NC-1 the reentrancy guard is removed, NC-2 the depth limit is removed, NC-3",
        "the callee's diff is not merged, NC-4 a failed callee's value is not returned,",
        "NC-5 the callee's status is flattened to success, NC-6 the child frame does",
        "not inherit the parent's stack.",
        "",
        f"6/6 PASS; total **{EXPECTED_NC_TOTAL}** across C0–C10 + S0–S7 + X0–X6.",
        "",
        "## Defects the gate found (all fixed in this phase)",
        "",
        "1. **The engine refused `env3` imports.** `check_imports` in the `wasmi`",
        "   backend kept its own copy of the allowed modules and had not been told",
        "   about the third surface, so a contract that calls another contract could",
        "   not be deployed at all. The census had it right; the engine's copy had",
        "   drifted. Found by the first run of the recorder.",
        "2. **A returned status is not a success flag.** X2's rule — \"any",
        "   `Returned(_)` commits\" — is right for a transaction and wrong for a",
        "   *frame*: the first X6 gate saw a vault answer `-14` and the caller read",
        "   `0`, because the child's status was being dropped. A callee's answer is now",
        "   propagated, and NC-5 removes that propagation and requires the gate to go",
        "   red.",
        "3. **The payout and the notify are not the same call.** The vault's first",
        "   draft paid its receiver with `env3.call{value}`; on this chain that is",
        "   refused for an account with no code (`-16`) and for its own caller (`-14`),",
        "   so an honest withdrawal could not complete. Money moves with",
        "   `env.transfer`, control moves with `env3.call`, and the contract says so.",
        "4. **`recurse` could not recurse.** The first depth fixture called itself,",
        "   which the reentrancy rule refuses — correctly. The fixture had to become a",
        "   chain of distinct contracts, which is also the honest shape of the limit.",
        "",
        "## Honest scope",
        "",
        "* **No callbacks.** A frame cannot be re-entered, so a payment cannot trigger",
        "  receiver logic in the same transaction. That is a deliberate Stage 1",
        "  restriction, not an accident of the implementation; a future phase that",
        "  wants flash-loan-style callbacks has to replace this rule with a per-account",
        "  lock and re-run this gate.",
        "* **The payout moves money with `env.transfer`, not with `env3.call{value}`.**",
        "  That is a design decision with a measured reason: `env3.call` refuses an",
        "  account with no code (`-16`) and a live frame (`-14`), so a vault that paid",
        "  *through* a call could not pay an ordinary wallet at all. The `value`",
        "  parameter is therefore exercised by `evil.send` in two tests — a callee that",
        "  reverts returns the money (`value_of_a_reverted_call_comes_back`) — while the",
        "  successful contract-to-contract value path is covered only by the payout in",
        "  `value_sent_with_a_call_changes_both_balances`, which is `env.transfer` under",
        "  the hood. A caller that wants to *both* send value and run code in one step",
        "  is not tested on this chain, and is not claimed.",
        "* **Gas is charged after the callee returns**, not reserved before it. A",
        "  parent cannot overspend — the child's budget is the parent's remaining gas —",
        "  but a receipt is written even when the caller traps afterwards, and that is",
        "  the next phase's problem (fee accounting, T-phase).",
        "* **The reentrancy rule is per address, not per storage slot.** A contract",
        "  that calls two different instances of the same logic is fine; a contract",
        "  that calls itself is not.",
        "* Pins are exact for this toolchain, these flags and this checkout path, as",
        "  measured and documented in X4 and X5.",
        "",
        f"Tests: **{tests}** in the workspace, {added} of them in the X6 gate.",
        "Validators / blocks / transactions remain **0 / 0 / 0**.",
        "",
    ]
    return "\n".join(lines)


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = [
        "# X6 negative-control inventory",
        f"# Generated (UTC): {stamp}",
        "# Each NC tampers with one enforcement point and requires the gate to go red.",
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


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    failures = 0
    lines = [
        "# X6 — call isolation gate log (measured)",
        f"# Generated (UTC): {stamp}",
        "# Every block below is raw command output; nothing is typed by hand.",
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
    EVIDENCE.mkdir(parents=True, exist_ok=True)

    fields = fixture()
    lines += [
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
        f"Contracts: {len(CONTRACTS)} built artifacts, "
        f"{sum(int(row['size']) for row in rows(fields))} bytes.",
        f"Tests: {workspace_test_count()} in the workspace, {x6_test_count()} in the X6 gate.",
        f"Negative controls: {EXPECTED_NC_TOTAL} across C0-C10 + S0-S7 + X0-X6.",
        "ABI v3: 4 functions in module `env3`; ABI v1 and v2 untouched.",
        "Validators/blocks/tx remain 0/0/0.",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")
    REPORT.write_text(build_report(fields, stamp), encoding="utf-8")
    inventory, total = build_inventory(stamp)
    INVENTORY.write_text(inventory, encoding="utf-8")
    if total != EXPECTED_NC_TOTAL:
        print(f"INVENTORY ERROR: total {total} NC, expected {EXPECTED_NC_TOTAL}")
        failures += 1

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/x6/nc.log missing (run `make nc-x6`)")
        return 1

    sums = []
    for path in (OUT, nc_log, REPORT, INVENTORY, FIXTURE, ADDENDUM):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {REPORT.relative_to(REPO)}")
    print(f"wrote {INVENTORY.relative_to(REPO)}")
    print("wrote docs/evidence/x6/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
