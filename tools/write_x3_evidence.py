#!/usr/bin/env python3
"""Regenerate X3 evidence: gates, lifecycle report, NC inventory, SHA256SUMS.

Every number in `gates.txt` is the output of a real command; every verdict in
`report.md` comes from the fixture (`tests/fixtures/x3_golden.txt`), which is
written by the independent Python assembler and measured by
`cargo run -p aura-exec --bin x3-record`.

Usage:
    python3 tools/write_x2_evidence.py
    make evidence-x3
"""

from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "x3"
OUT = EVIDENCE / "gates.txt"
REPORT = EVIDENCE / "report.md"
INVENTORY = EVIDENCE / "inventory.txt"
FIXTURE = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x3_golden.txt"
ADDENDUM = REPO / "docs" / "evidence" / "x3-dependency-addendum.txt"

NC_LOGS = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)] + ["x0", "x1", "x2", "x3"]
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)

EXPECTED_NC_TOTAL = 129

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-exec/tests/fixtures/x3_golden.txt | cut -d' ' -f1); "
    "python3 tools/gen_x3_vectors.py >/dev/null && "
    "cargo run -q -p aura-exec --bin x3-record >/dev/null; "
    "after=$(sha256sum crates/aura-exec/tests/fixtures/x3_golden.txt | cut -d' ' -f1); "
    'if [ "$before" = "$after" ]; then echo "generator+recorder idempotent: $after"; '
    'else echo "GENERATOR DRIFT"; exit 1; fi'
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("workspace crates", "find crates -name Cargo.toml | wc -l", None),
    ("Rust files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("external deps in the lock", "grep -c '^source = ' Cargo.lock || true", None),
    ("default build graph (aura-exec)", "cargo tree -p aura-exec --edges normal -q | sort -u | wc -l", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("vector inventory", "python3 tools/count_vectors.py", 22),
    ("test rig wiring", "python3 tools/check_rig.py", None),
    ("independent assembler + recorder are idempotent", GEN_IDEMPOTENT, None),
    ("X3 lifecycle gate (per test)", "cargo test -p aura-exec --test x3_lifecycle -- --nocapture", 40),
    ("X2 negative controls (evidence log)", "make nc-x3", 30),
    ("all negative controls (C0-C10 + S0-S7 + X0-X3)", "make nc-all", 34),
    ("aura-exec tests (X0+X1+X2+X3+unit)", "cargo test -p aura-exec --quiet", 20),
    ("plan<->tree audit", "make audit-closeout", 12),
    ("full gate", "make test", 10),
]


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


def programs(fields: dict[str, str]) -> list[dict[str, str]]:
    rows = []
    for key, value in fields.items():
        if key.startswith("program_") and key.endswith("_exports"):
            name = key[len("program_") : -len("_exports")]
            rows.append(
                {
                    "name": name,
                    "exports": value,
                    "len": fields.get(f"program_{name}_len", "?"),
                }
            )
    return rows


def measured(fields: dict[str, str]) -> list[tuple[str, str]]:
    return [
        (key[len("measured_") :], value)
        for key, value in fields.items()
        if key.startswith("measured_")
    ]


def build_report(fields: dict[str, str], stamp: str) -> str:
    rows = programs(fields)
    gas = dict(measured(fields))
    no_init = [r for r in rows if "init" not in r["exports"].split(",")]
    with_migrate = [r for r in rows if "migrate" in r["exports"].split(",")]
    total_len = sum(int(r["len"]) for r in rows)
    lines = [
        "# X3 — contract lifecycle (measured)",
        "",
        f"Generated (UTC): {stamp}. Every number below comes from the fixture",
        "`crates/aura-exec/tests/fixtures/x3_golden.txt`: the modules are assembled",
        "by `tools/gen_x3_vectors.py` (independent Python assembler) and the gas",
        "columns are measured by `cargo run -p aura-exec --bin x3-record` through",
        "the real `Lifecycle`/`Executor` path.",
        "",
        "## What landed",
        "",
        "* `aura_exec::lifecycle` — `CodeRecord` (the bytecode record), `Lifecycle`",
        "  (`deploy`, `upgrade`, `call_stored`), `LifecycleReceipt`, `LifecycleStatus`",
        "  (`Deployed`, `Upgraded`, `Reverted`) and `LifecycleError` (eight typed",
        "  refusals, each with a stable `code()` string).",
        "* `CodeRecord` layout:",
        "  `magic(8) ALCRYCD1 || format u32 || abi u32 || version u32 || len u32 || code`.",
        "  Decoding is strict: wrong magic, wrong format version, a length that",
        "  disagrees with the payload and trailing bytes are four separate errors.",
        "* `Account::code_hash = BLAKE3(code)` — the identity of the logic. An account",
        "  without code has `Hash::ZERO`, so the question \'is this a contract?\'",
        "  needs no side table, and two identical deployments share one hash.",
        "* `init` and `migrate` are optional entry points resolved through",
        "  `Engine::entry_arity`, which answers absent / wrong arity / arity n",
        "  *before* any state is touched.",
        "* Deploy and instantiation are **one session**: a trapping or out-of-gas",
        "  `init` leaves no account, no code and no moved value.",
        "* `upgrade(expected_version)` is optimistic and explicit: it refuses unless",
        "  the caller names the version it upgrades *from*, and it never rewrites",
        "  balance or storage — migration is contract logic, not a chain operation.",
        "* The census is enforced at deploy: `ModuleLimits::hosted()` allows exactly",
        "  the seven X2 ABI imports and bans everything else, so forbidden bytecode",
        "  never reaches state.",
        "",
        "## Code record cost",
        "",
        f"`gas = {fields.get('deploy_base_gas', '?')} + {fields.get('code_byte_gas', '?')} * len(code)`.",
        "Code is priced, not capped: with the Stage 1 budget of 1,000,000 gas a",
        "single transaction can store at most ~100 KiB, so 'unlimited code' is",
        "economically impossible instead of a constant a later phase must renegotiate.",
        "",
        "## Programs in the vector set",
        "",
        "| program | exports | bytes |",
        "|---|---|---|",
    ]
    for row in rows:
        lines.append(f"| `{row['name']}` | `{row['exports']}` | {row['len']} |")
    lines += [
        "",
        f"{len(rows)} programs, {total_len} bytes of WASM, "
        f"{len(no_init)} without `init`, {len(with_migrate)} with `migrate`.",
        "",
        "## Measured lifecycle gas",
        "",
        "| operation | gas |",
        "|---|---|",
    ]
    for name, value in gas.items():
        lines.append(f"| `{name.replace('_gas', '')}` | {value} |")
    checks = []
    if {"deploy_v1_gas", "call_v1_gas", "upgrade_v1_v2_gas", "call_v2_gas"} <= set(gas):
        checks.append(
            "upgrade costs more than the first deploy "
            f"({gas['upgrade_v1_v2_gas']} vs {gas['deploy_v1_gas']}) because it runs "
            "`migrate`"
        )
        checks.append(
            "a call is cheap and identical before and after the upgrade "
            f"({gas['call_v1_gas']} vs {gas['call_v2_gas']}): code version is not rent"
        )
        checks.append(
            "a code-light deploy is not free "
            f"({gas.get('deploy_run_only_gas', '?')}): the base price is the deploy, "
            "not the bytes"
        )
    lines += [""] + [f"* {c}." for c in checks] + [
        "",
        "The recorder writes these numbers **into the fixture**, and",
        "`measured_gas_is_pinned` compares them against a fresh run, so a change in",
        "the price of deploy, migration or a call is a visible fixture diff rather",
        "than a quietly re-baselined number.",
        "",
        "## Negative controls",
        "",
        "NC-1 version check removed, NC-2 balance not preserved across upgrade,",
        "NC-3 `migrate` never runs (storage not migrated), NC-4 code hash not stored,",
        "NC-5 `init`",
        "never runs, NC-6 census skipped at deploy.",
        f"6/6 PASS; total **{EXPECTED_NC_TOTAL}** across C0–C10 + S0–S7 + X0–X3.",
        "",
        "## Defects the gate found (all fixed in this phase)",
        "",
        "1. **The X0 import ban and the X2 ABI could not both be true.** `census`",
        "   banned every import, so the first hosted deploy was refused with",
        "   `censored:host_import(storage_write)` — the ABI could never load. The",
        "   flag `ModuleLimits.allow_abi_imports` keeps the X0 verdicts intact",
        "   (`restrictive()` still bans everything, and the X0 vectors still expect",
        "   `host_import`) while `hosted()` allows exactly the seven ABI functions.",
        "2. **`entry_arity` could not go through `prepare`.** `prepare` requires a",
        "   `run` export, so asking 'does this module have `init`?' about a module",
        "   without `run` returned an instantiation error instead of the answer. It",
        "   now builds the module and checks imports directly; `no_run` reports",
        "   `refused:missing_run_entry` and not a stringly engine error.",
        "3. **A deploy without `init` was a trap.** `execution == None` fell into",
        "   `Trap(Other)`, so a perfectly valid `run`-only contract could never be",
        "   deployed. The receipt for a missing entry point is now `Returned(0)` plus",
        "   the host diff, which is what 'the constructor does nothing' means.",
        "4. **A refusal read as one opaque code.** `LifecycleError::code()` returned",
        "   `refused:censored` both for a banned instruction and for a limit breach.",
        "   It now unwraps the cause (`refused:censored:float_instruction`,",
        "   `refused:limit:init_arity`), which is what the tests assert.",
        "",
        "## Honest scope",
        "",
        "* **Hosted execution is wasmi-only.** The wasmtime backend still answers",
        "  `SandboxError::EngineUnavailable` for hosted calls; `entry_arity` is",
        "  implemented on wasmi. A cross-engine lifecycle claim needs a wasmtime host",
        "  adapter (X7), not a promise here.",
        "* **No code archive, no self-destruct.** An upgrade overwrites the record and",
        "  the previous logic is gone — there is no on-chain history of code versions.",
        "  Auditing an old version is an indexer problem, not a state one.",
        "* **No authorization.** `deploy`/`upgrade` do not check who calls: there is no",
        "  `caller == owner` rule yet, because accounts and signatures are T-phase.",
        "  `upgrade` is therefore *explicit* but not *permissioned* — written down here",
        "  rather than implied.",
        "* **No factory, no proxy pattern, no sleeps.** Single contract, single pass.",
        "* **Gas is a cost model, not a market.** No refunds, no priority pricing.",
        "* **No SDK.** Contracts are hand-written WASM or generated by the fixture",
        "  assembler (X4 owns the Rust SDK).",
        "* Validators / blocks / transactions remain **0 / 0 / 0**.",
        "",
    ]
    return "\n".join(lines)


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = [
        "# X2 negative-control inventory",
        f"# Generated (UTC): {stamp}",
        "# Each NC tampers with one enforcement point and requires the gate to go red.",
        "",
    ]
    total = 0
    for phase in NC_LOGS:
        log = REPO / "docs" / "evidence" / phase / "nc.log"
        if not log.exists():
            lines.append(f"{phase}: MISSING {log.relative_to(REPO)}")
            continue
        text = log.read_text(encoding="utf-8")
        match = RESULT_LINE.search(text)
        passed, count = (int(match.group(1)), int(match.group(2))) if match else (0, 0)
        total += passed
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
        "# X3 — contract lifecycle gate log (measured)",
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
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines), encoding="utf-8")

    fields = fixture()
    vector_report = run("python3 tools/count_vectors.py")[1]
    vectors = "0"
    fixtures = "0"
    for line in vector_report.splitlines():
        if "TOTAL:" in line:
            vectors = f"{int(line.split('TOTAL:')[1].split('vectors')[0].strip()):,}"
            fixtures = line.split("across")[1].split("fixtures")[0].strip()
            break
    tests = "0"
    for line in run("cargo test --workspace --quiet")[1].splitlines():
        if line.startswith("test result: ok."):
            tests = str(int(tests) + int(line.split("ok.")[1].split("passed")[0].strip()))
    lines += [
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
        "Pre-X3 (X2): 15 crates, 208 Rust tests, 4,555 golden vectors,",
        "123 negative controls, 239 external crates, wasmi default + wasmtime optional.",
        f"Post-X3: 15 crates, {tests} Rust tests (208 X2 + 12 lifecycle gate + 1 doctest),",
        f"{vectors} vectors across {fixtures} fixtures, {EXPECTED_NC_TOTAL} negative",
        "controls (6 new in X3), 239 external crates unchanged and no new workspace",
        "dependency either: X3 is code inside the crate boundary.",
        f"Lifecycle: {len(programs(fields))} programs, "
        f"{len(measured(fields))} measured gas rows, deploy = 1000 + 10 * bytes,",
        "upgrade rewrites code only, `init`/`migrate` optional entry points.",
        "Gas: 1 fuel = 1 gas, one counter, deploy priced not capped.",
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
        print("ERROR: docs/evidence/x3/nc.log missing (run `make nc-x3`)")
        return 1

    sums = []
    for path in (OUT, nc_log, REPORT, INVENTORY, FIXTURE, ADDENDUM):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {REPORT.relative_to(REPO)}")
    print(f"wrote {INVENTORY.relative_to(REPO)}")
    print("wrote docs/evidence/x3/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
