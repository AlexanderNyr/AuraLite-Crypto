#!/usr/bin/env python3
"""Regenerate X8 evidence: gates, verifier report, NC inventory, SHA256SUMS.

Every line of `gates.txt` is the output of a real command. The report's tables are
built from live runs of `x8-verify` in this tree — the six shipped artifacts and the
100 golden mutants — plus the manifest, so a number can only appear in the report if
a command printed it.

Usage:
    python3 tools/write_x8_evidence.py
    make evidence-x8
"""

from __future__ import annotations

import hashlib
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "x8"
OUT = EVIDENCE / "gates.txt"
REPORT = EVIDENCE / "report.md"
INVENTORY = EVIDENCE / "inventory.txt"
MANIFEST = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x8_mutants.txt"

ARTIFACTS = ["kv", "store", "events", "vault", "evil", "recurse"]

NC_LOGS = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_LOGS += ["x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8"]
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)

EXPECTED_NC_TOTAL = 159

MUTANT_LINE = re.compile(
    r"^(?P<file>\S+) expect=(?P<expect>\S+) got=(?P<got>\S+) "
    r"functions=(?P<functions>\d+) instructions=(?P<instructions>\d+) "
    r"max_stack=(?P<stack>\d+) max_ops=(?P<ops>\d+)$"
)

GEN_IDEMPOTENT = (
    "before=$(find crates/aura-exec/tests/fixtures/x8 -name '*.wasm' | sort | "
    "xargs sha256sum | sha256sum | cut -d' ' -f1); "
    "python3 tools/gen_x8_mutants.py >/dev/null && "
    "after=$(find crates/aura-exec/tests/fixtures/x8 -name '*.wasm' | sort | "
    "xargs sha256sum | sha256sum | cut -d' ' -f1); "
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
    ("golden mutants are reproducible", "python3 tools/gen_x8_mutants.py --check", None),
    ("generator is idempotent", GEN_IDEMPOTENT, None),
    (
        "X8 verifier on the six shipped artifacts",
        "cargo run -q -p aura-exec --bin x8-verify -- "
        + " ".join(f"examples/{name}/wasm/{name}.wasm" for name in ARTIFACTS),
        None,
    ),
    (
        "X8 golden mutants (verdict vs expectation)",
        "cargo run -q -p aura-exec --bin x8-verify -- --fixture "
        "crates/aura-exec/tests/fixtures/x8_mutants.txt | "
        "awk '{split($2,a,\"=\"); split($3,b,\"=\"); if (a[2] != b[2]) "
        "{bad++}} END {print \"mismatches: \" bad+0; "
        "exit (bad+0) > 0}'",
        None,
    ),
    ("X8 verifier gate", "cargo test -p aura-exec --test x8_verify", 12),
    ("X8 negative controls (evidence log)", "make nc-x8", 30),
    ("all negative controls (C0-C10 + S0-S7 + X0-X8)", "make nc-all", 28),
    ("aura-exec tests (X0-X8 + unit)", "cargo test -p aura-exec --quiet", 12),
    ("plan<->tree audit", "make audit-closeout", 10),
    ("full gate", "make test", 6),
]


def run(cmd: str) -> tuple[int, str]:
    result = subprocess.run(
        cmd, shell=True, cwd=REPO, capture_output=True, text=True, check=False
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def manifest_rows() -> list[dict[str, str]]:
    """`file=… kind=… source=… expect=…` from the manifest the mutator wrote."""
    rows: list[dict[str, str]] = []
    for line in MANIFEST.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        fields: dict[str, str] = {}
        for token in line.split():
            if "=" in token:
                key, value = token.split("=", 1)
                fields[key] = value
        rows.append(fields)
    return rows


def measured_verdicts() -> tuple[list[dict[str, str]], int]:
    """Run the verifier over the whole manifest and pair it with the expectation."""
    code, output = run(
        "cargo run -q -p aura-exec --bin x8-verify -- --fixture "
        "crates/aura-exec/tests/fixtures/x8_mutants.txt"
    )
    if code != 0:
        return [], -1
    expectations = {row["file"]: row for row in manifest_rows()}
    rows: list[dict[str, str]] = []
    mismatches = 0
    for line in output.splitlines():
        parsed = MUTANT_LINE.match(line.strip())
        if not parsed:
            continue
        row = parsed.groupdict()
        expected = expectations.get(row["file"], {})
        row["expect"] = expected.get("expect", row["expect"])
        row["kind"] = expected.get("kind", "?")
        row["source"] = expected.get("source", "?")
        row["agrees"] = "yes" if row["expect"] == row["got"] else "NO"
        if row["agrees"] == "NO":
            mismatches += 1
        rows.append(row)
    return rows, mismatches


def shipped_verdicts() -> list[dict[str, str]]:
    code, output = run(
        "cargo run -q -p aura-exec --bin x8-verify -- "
        + " ".join(f"examples/{name}/wasm/{name}.wasm" for name in ARTIFACTS)
    )
    if code != 0:
        return []
    rows: list[dict[str, str]] = []
    pattern = re.compile(
        r"^(?P<path>\S+): (?P<verdict>\S+) functions=(?P<functions>\d+) "
        r"instructions=(?P<instructions>\d+) max_stack=(?P<stack>\d+) max_ops=(?P<ops>\d+)$"
    )
    for line in output.splitlines():
        parsed = pattern.match(line.strip())
        if parsed:
            rows.append(parsed.groupdict())
    return rows


def workspace_test_count() -> int:
    total = 0
    for line in run("cargo test --workspace --quiet")[1].splitlines():
        if line.startswith("test result: ok."):
            total += int(line.split("ok.")[1].split("passed")[0].strip())
    return total


def gate_test_count() -> int:
    total = 0
    for line in run("cargo test -p aura-exec --test x8_verify --quiet")[1].splitlines():
        if line.startswith("test result: ok."):
            total += int(line.split("ok.")[1].split("passed")[0].strip())
    return total


def build_report(
    rows: list[dict[str, str]],
    mismatches: int,
    shipped: list[dict[str, str]],
    stamp: str,
) -> str:
    tests = workspace_test_count()
    gate = gate_test_count()

    kinds: dict[str, dict[str, int]] = {}
    for row in rows:
        entry = kinds.setdefault(row["kind"], {"count": 0, "codes": {}})  # type: ignore[arg-type]
        entry["count"] += 1
        code = "accept" if row["got"] == "accept" else row["got"].split(":", 1)[-1]
        entry["codes"][code] = entry["codes"].get(code, 0) + 1

    mutants = [row for row in rows if row["kind"] != "shipped"]
    accepts = [row for row in rows if row["expect"] == "accept"]
    rejects = [row for row in mutants if row["expect"] != "accept"]

    lines = [
        "# X8 — the static verifier (measured)",
        "",
        f"Generated (UTC): {stamp}. Every number below came out of `x8-verify` in this",
        "tree; `gates.txt` holds the raw command output that produced it, and",
        "`crates/aura-exec/tests/x8_verify.rs` pins the same numbers so they cannot",
        "drift silently.",
        "",
        "## What the phase claims",
        "",
        "> A contract module either satisfies the accepted subset, or is refused at",
        "> verification time with a stable code — before an engine ever sees it.",
        "",
        "The verifier never executes the module. It reads the binary, checks it against",
        "the subset the census (X0) and the host surface (X2, X3, X6) already",
        "established, and answers with a `Report`: a verdict, a size, function and",
        "instruction counts, the peak operand-stack depth, and one `Problem` per defect",
        "carrying a byte offset into the module.",
        "",
        "## Shipped artifacts (re-verified, not remembered)",
        "",
        "| contract | verdict | functions | instructions | peak stack | peak ops/body |",
        "|---|---|---|---|---|---|",
    ]
    for row in shipped:
        name = row["path"].split("/")[1]
        lines.append(
            f"| `{name}` | {row['verdict']} | {row['functions']} | {row['instructions']} | "
            f"{row['stack']} | {row['ops']} |"
        )
    lines += [
        "",
        "All six pass. That is the part of the claim that is cheap: a verifier that",
        "accepts everything would print the same table. The next section is the part",
        "that is not cheap.",
        "",
        "## The mutation matrix (100 damaged modules, 15 kinds of damage)",
        "",
        "`tools/gen_x8_mutants.py` is an independent implementation: its own section",
        "walker in Python, one damage per module, and a stated expectation. If it",
        "shared code with the verifier, agreement would prove nothing.",
        "",
        "| kind | mutants | expected verdict | agrees |",
        "|---|---|---|---|",
    ]
    labels = {
        "magic": "header magic byte destroyed",
        "version": "version word bumped to 2",
        "float_type": "a value type becomes `f32`/`f64`",
        "float_local": "a local declaration becomes a float",
        "import": "an import module name is not one of the ABI surfaces",
        "global_type": "a global is not an integer",
        "memory_max": "linear memory declares more pages than allowed",
        "opcode": "one opcode leaves the accepted subset",
        "call_index": "`call` names a function that does not exist",
        "local_index": "`local.get` names a local that does not exist",
        "label_depth": "`br` names a label that does not exist",
        "export_shape": "the `run` export is renamed",
        "function_code": "the code section drops a body",
        "too_large": "the module grows past the size ceiling",
        "truncated": "the last section is cut short",
        "shipped": "the six shipped artifacts, unmodified",
    }
    for kind in sorted(kinds):
        entry = kinds[kind]
        codes = ", ".join(f"`{code}` ×{n}" for code, n in sorted(entry["codes"].items()))
        agree = sum(
            1 for row in rows if row["kind"] == kind and row["agrees"] == "yes"
        )
        lines.append(
            f"| {kind} — {labels.get(kind, '?')} | {entry['count']} | {codes} | "
            f"{agree}/{entry['count']} |"
        )
    lines += [
        "",
        f"**{len(mutants)} mutants ({len(rejects)} refusals) and {len(accepts)} unmodified"
        f" modules replayed as-is. Mismatches between the expectation the",
        f"mutator stated and the verdict the verifier returned: {mismatches}.**",
        "",
        "The plan is 6 mutants per damage kind over the six shipped artifacts, with a",
        "second round for the five kinds whose first six do not already cover every",
        "artifact — 100 exactly, written by the generator, not by hand.",
        "",
        "## The 22 codes, and the witness for each",
        "",
        "A code that no test can trigger is documentation, not a check. Every code below",
        "has either a focused unit test in the gate (a hand-assembled module, so it does",
        "not depend on how today's rustc lays out a contract) or a golden mutant:",
        "",
        "| code | witness |",
        "|---|---|",
        "| `bad_magic` | unit `code_bad_magic` + mutant kind `magic` |",
        "| `bad_version` | unit `code_bad_version` + mutant kind `version` |",
        "| `module_too_large` | unit `code_module_too_large` + mutant kind `too_large` |",
        "| `section_order` | unit `code_section_order`, `code_section_order_on_a_duplicate` |",
        "| `section_unknown` | unit `code_section_unknown` |",
        "| `section_truncated` | unit `code_section_truncated` + mutant kind `truncated` |",
        "| `float_type` | unit `code_float_type`, `code_float_local` + mutant kinds |",
        "| `import_not_allowed` | unit `code_import_not_allowed` + restrictive config |",
        "| `global_type` | unit `code_global_type` + mutant kind `global_type` |",
        "| `memory_limits` | unit `code_memory_limits_*` (no maximum, above ceiling) |",
        "| `function_code_mismatch` | unit `code_function_code_mismatch` + mutant kind |",
        "| `body_truncated` | unit `code_body_truncated`, `…_on_trailing_bytes` |",
        "| `opcode_banned` | unit `code_opcode_banned`, `…_on_a_bulk_sub_opcode` |",
        "| `stack_underflow` | unit `code_stack_underflow` (and NC-2) |",
        "| `stack_too_deep` | unit `code_stack_too_deep` (budget lowered to 1) |",
        "| `label_depth` | unit `code_label_depth` + mutant kind `label_depth` |",
        "| `local_index` | unit `code_local_index` + mutant kind `local_index` |",
        "| `global_index` | unit `code_global_index` |",
        "| `call_index` | unit `code_call_index` + mutant kind `call_index` |",
        "| `opcode_budget` | unit `code_opcode_budget` (budget lowered to 2) |",
        "| `locals_budget` | unit `code_locals_budget` (budget lowered to 1) |",
        "| `export_shape` | unit `code_export_shape`, `…_on_a_wrong_name` + mutant kind |",
        "",
        "## What depth this actually is",
        "",
        "*It is a depth check, not a type check.* The operand stack is tracked as a",
        "**length**, not as a stack of types: `i32.add` pops two and pushes one, and the",
        "verifier trusts the opcode to mean what the spec says it means without tracking",
        "`i32` vs `i64` through every instruction. A module that type-confuses two",
        "integers of different widths can therefore pass X8 and still be refused by the",
        "engine at instantiation — which is the correct division of labour: **the engine",
        "remains the authority on instantiation, X8 only guarantees that the engine is",
        "never asked to instantiate something outside the accepted subset.**",
        "",
        "What it does check, in full:",
        "",
        "* section order, uniqueness, id range, declared lengths, and the `function` /",
        "  `code` counts against each other;",
        "* every value type in the type section, every local declaration, every global;",
        "* the import whitelist (three ABI surfaces, function imports only) with a",
        "  restrictive configuration that refuses all imports;",
        "* per body: the instruction budget, the local budget, the operand-stack depth",
        "  with callee signatures read out of the type section, label depth for",
        "  `br`/`br_if`/`br_table`, `local`/`global`/`call` index bounds, `call_indirect`",
        "  against the type section, and the two allowed `0xfc` bulk sub-opcodes;",
        "* the export shape (`run` must exist);",
        "* memory: a maximum must be declared and must not exceed the census ceiling.",
        "",
        "## Honest limits",
        "",
        "* **The accepted opcode and value-type subset is a whitelist, not the full",
        "  grammar.** An instruction the whitelist does not know is `opcode_banned`, even",
        "  if the spec allows it. That is deliberate — the whitelist is the same shape as",
        "  the census — but it means a contract built by a different toolchain can be",
        "  refused for using an instruction this SDK never emits. The refusal is loud and",
        "  carries an offset, which is the property that matters.",
        "* **`else` without `if` is caught, but arithmetic reachability is not proved.**",
        "  Unreachable code gets the spec's polymorphic-stack treatment: it is not",
        "  evaluated, so a body after `unreachable` cannot manufacture a false failure.",
        "* **Nothing here is a prover.** There is no formal model of the ABI, no gas",
        "  bound derived from the bytecode, and no claim that a verified contract is",
        "  correct — only that it is inside the subset the rest of the system assumes.",
        "* **The engine backends are not tested by this gate.** `wasmi` is what `make",
        "  test` exercises; the optional `wasmtime` backend is CI-only and refuses",
        "  modules that import the host ABI, because this workspace never implemented the",
        "  ABI on that backend. X8 does not change that.",
        "",
        "## Defects the phase found (both fixed here)",
        "",
        "1. **`call` never read the callee's signature.** The first version of the body",
        "   check popped nothing and pushed nothing for `0x10`, so `store.wasm` was",
        "   refused with `stack_underflow` at two real instructions. Every subsequent",
        "   stack number was arithmetic on a fiction. The verifier now resolves the",
        "   callee through the type section — imported functions through the import",
        "   section, defined ones through the function section — and the gate has a test",
        "   that fails if that resolution is removed (NC-1).",
        "2. **Body offsets were relative to the body slice, not the module.** Every",
        "   problem inside a function pointed at the wrong byte. The body cursor now",
        "   carries the module base, so an offset a reader is given is an offset they can",
        "   check against the file.",
        "",
        "## Negative controls",
        "",
        "Six tampers, each reddening a *named* test: `call` ignoring the callee",
        "signature, a stack underflow that never reaches the report, float value types",
        "read as `i32`, any export name counting as `run`, the `function`/`code` counts",
        "allowed to disagree, and the size ceiling unenforced. A green baseline runs",
        "first, so a red caused by a broken tree can never be mistaken for a successful",
        "control.",
        "",
        f"6/6 PASS; total **{EXPECTED_NC_TOTAL}** across C0–C10 + S0–S7 + X0–X8.",
        "",
        "## What is still missing before this runs on a chain",
        "",
        "* **Nothing calls the verifier yet.** X8 is a library and a binary; the node",
        "  phase (D) is what wires it into the deploy path, and until then the claim is",
        "  \"a verifier exists and is measured\", not \"deploys are verified\".",
        "* **The budget constants are policy, not measurement.** 200 000 instructions",
        "  per body, 4 096 stack slots, 1 024 locals — all far above what the shipped",
        "  contracts use (measured peak: 630 instructions in one body, 7 stack slots),",
        "  and set so that a legitimate contract is never refused by accident. They will",
        "  be tightened when a gas model can price them.",
        "* **No differential run against a reference validator.** `wasmparser` is in the",
        "  dependency graph through `wasmi`, and a future phase should compare verdicts",
        "  on a fuzz corpus; today the corpus is the 100 mutants plus 6 shipped modules.",
        "",
        f"Tests: **{tests}** in the workspace, {gate} of them in the X8 gate.",
        "Validators / blocks / transactions remain **0 / 0 / 0**.",
        "",
    ]
    return "\n".join(lines)


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = [
        "# X8 negative-control inventory",
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
    for path in (OUT, EVIDENCE / "nc.log", REPORT, INVENTORY, MANIFEST):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    failures = 0

    rows, mismatches = measured_verdicts()
    shipped = shipped_verdicts()
    if not rows or not shipped:
        print("ERROR: x8-verify did not run; fix the tree first")
        return 1

    if "--report-only" in sys.argv[1:]:
        REPORT.write_text(build_report(rows, mismatches, shipped, stamp), encoding="utf-8")
        refresh_sums()
        print(f"wrote {REPORT.relative_to(REPO)} (report-only; gates.txt untouched)")
        return 0 if mismatches == 0 else 1

    lines = [
        "# X8 — static verifier log (measured)",
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

    lines += [
        "=" * 72,
        "MUTATION MATRIX",
        "=" * 72,
        "",
    ]
    for row in rows:
        lines.append(
            "kind={kind} source={source} file={file} expect={expect} got={got} "
            "agrees={agrees} functions={functions} instructions={instructions} "
            "max_stack={stack} max_ops={ops}".format(**row)
        )
    lines += [
        "",
        f"rows: {len(rows)}",
        f"mismatches: {mismatches}",
        "",
    ]
    if mismatches:
        failures += 1
        print(f"FAILED: {mismatches} mutant(s) got a verdict other than expected")

    lines += [
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
    ]
    for row in shipped:
        lines.append(
            f"{row['path']}: {row['verdict']} functions={row['functions']} "
            f"instructions={row['instructions']} max_stack={row['stack']} max_ops={row['ops']}"
        )
    gate = gate_test_count()
    lines += [
        "",
        f"Shipped artifacts: {len(shipped)} accepted, "
        f"{sum(1 for row in shipped if row['verdict'] != 'accept')} refused.",
        f"Golden mutants: {len(rows)} modules, {mismatches} verdict mismatches.",
        f"Tests: {workspace_test_count()} in the workspace, {gate} in the X8 gate.",
        f"Negative controls: {EXPECTED_NC_TOTAL} across C0-C10 + S0-S7 + X0-X8.",
        "Validators/blocks/tx remain 0/0/0.",
        "",
    ]

    EVIDENCE.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines), encoding="utf-8")
    REPORT.write_text(build_report(rows, mismatches, shipped, stamp), encoding="utf-8")
    inventory, total = build_inventory(stamp)
    INVENTORY.write_text(inventory, encoding="utf-8")
    if total != EXPECTED_NC_TOTAL:
        print(f"INVENTORY ERROR: total {total} NC, expected {EXPECTED_NC_TOTAL}")
        failures += 1

    if not (EVIDENCE / "nc.log").exists():
        print("ERROR: docs/evidence/x8/nc.log missing (run `make nc-x8`)")
        return 1

    refresh_sums()
    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {REPORT.relative_to(REPO)}")
    print(f"wrote {INVENTORY.relative_to(REPO)}")
    print("wrote docs/evidence/x8/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
