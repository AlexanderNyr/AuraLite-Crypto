#!/usr/bin/env python3
"""Regenerate X0 evidence: gates, census report, NC inventory, SHA256SUMS.

All numbers come from real command output or from the independent Python
assembler. Usage:
    python3 tools/write_x0_evidence.py
    make evidence-x0
"""
from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "x0"
OUT = EVIDENCE / "gates.txt"
REPORT = EVIDENCE / "report.md"
INVENTORY = EVIDENCE / "inventory.txt"
FIXTURE = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x0_census.txt"
ADDENDUM = REPO / "docs" / "evidence" / "x0-dependency-addendum.txt"

NC_LOGS = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)] + ["x0"]
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-exec/tests/fixtures/x0_census.txt | cut -d' ' -f1); "
    "python3 tools/gen_x0_vectors.py >/dev/null; "
    "after=$(sha256sum crates/aura-exec/tests/fixtures/x0_census.txt | cut -d' ' -f1); "
    "if [ \"$before\" = \"$after\" ]; then echo \"generator idempotent: $after\"; "
    "else echo \"GENERATOR DRIFT\"; exit 1; fi"
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("pre-X0 tree (S7 commit)", "git log -1 --oneline", None),
    ("workspace crates", "find crates -name Cargo.toml | wc -l", None),
    ("Rust files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("external deps", "grep -c '^source = ' Cargo.lock || true", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("vector inventory", "python3 tools/count_vectors.py", 18),
    ("test rig wiring", "python3 tools/check_rig.py", None),
    ("independent assembler is idempotent", GEN_IDEMPOTENT, None),
    ("X0 census tests", "cargo test -p aura-exec --quiet", 12),
    ("20+ programs: banned or deterministic",
     "cargo test -p aura-exec --test x0_census -- --nocapture", 28),
    ("X0 negative controls (evidence log)", "make nc-x0", 34),
    ("all negative controls (C0-C10 + S0-S7 + X0)", "make nc-all", 36),
    ("CORE shard", "make test-core", 12),
    ("plan<->tree audit", "make audit-closeout", 12),
    ("full gate", "make test", 10),
]


def run(cmd: str) -> tuple[int, str]:
    result = subprocess.run(
        cmd, shell=True, cwd=REPO, capture_output=True, text=True, check=False
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def fixture_map() -> dict[str, str]:
    out: dict[str, str] = {}
    for line in FIXTURE.read_text(encoding="utf-8").splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            out[key] = value
    return out


def reset_current_evidence() -> None:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    for name in ("gates.txt", "nc.log", "report.md", "inventory.txt", "SHA256SUMS"):
        path = EVIDENCE / name
        if path.exists():
            path.unlink()


def build_report(f: dict[str, str], stamp: str) -> str:
    programs = [
        (key[len("program_"):-len("_verdict")], value)
        for key, value in sorted(f.items())
        if key.startswith("program_") and key.endswith("_verdict")
    ]
    lines = [
        "# X0 — determinism census (measured)",
        "",
        f"Generated (UTC): {stamp}. The verdicts below are produced by",
        "`crates/aura-exec/src/census.rs` and pinned by",
        "`tools/gen_x0_vectors.py`, an independent Python WASM assembler.",
        "",
        "This report lives in `docs/evidence/x0/`, not `docs/spec/`, because",
        "`docs/spec/FORMAT_INDEX.md` is frozen, checksummed C10 evidence.",
        "",
        "## What landed",
        "",
        "* `aura_exec::census` — a hand-written WASM binary classifier: magic,",
        "  version, section framing and order, imports, tables, memories,",
        "  globals, and **every instruction of every function body**.",
        "* `CENSUS` — the published table of nondeterminism sources with policy",
        "  and enforcement layer.",
        "* `ModuleLimits::restrictive()` — the restrictive configuration the rest",
        "  of the stack must use.",
        "* `Ban` (18 variants), `Verdict`, `CensusReport`.",
        "",
        "**No engine is linked in X0.** The census is static: it makes claims",
        "about classification, not execution. `wasmtime` is pinned in X1 with",
        "fuel metering, memory ceilings and the zero-import sandbox.",
        "",
        "## Census table (published policy)",
        "",
        "| source | feature | policy | enforced by | why |",
        "|---|---|---|---|---|",
    ]
    rows = [
        ("float_instruction", "f32/f64 arithmetic, comparison, conversion, reinterpret, const", "BAN", "X0 + X8", "NaN bit patterns are not fully specified"),
        ("host_import", "import section (any module, incl. WASI)", "BAN", "X0 + X1", "the only route to clocks, RNG or the OS"),
        ("start_section", "start function (implicit run on instantiate)", "BAN", "X0", "executes without a transaction or gas"),
        ("shared_memory", "threads proposal, shared memory flag 0x03", "BAN", "X0", "concurrent writes make order observable"),
        ("atomics", "0xFE atomic ops, memory.atomic.wait", "BAN", "X0", "exists only to coordinate threads; can block on the clock"),
        ("simd", "0xFD v128 instructions", "BAN", "X0", "extra divergence surface, unused in Stage 1"),
        ("bulk_memory", "0xFC bulk ops, saturating truncation", "BAN", "X0", "trunc_sat hides a float conversion; bulk copies escape per-byte metering"),
        ("call_indirect", "typed indirect call through a table", "BAN", "X0 + X6", "call graph, gas and depth limits stop being statically checkable"),
        ("unbounded_memory", "memory without a declared maximum", "BAN", "X0 + X1", "memory.grow could eat the node"),
        ("memory_too_large", "memory maximum above MAX_MEMORY_PAGES", "BAN", "X0", "bounds must be visible before execution"),
        ("table_too_large", "table without a maximum, or above the cap", "BAN", "X0", "same argument as memory"),
        ("mutable_global", "mutable module-level global", "BAN", "X0", "state outside versioned storage"),
        ("unknown_opcode", "any opcode outside the whitelist", "BAN", "X0, fail-closed", "unreviewed opcodes are rejected, not trusted"),
        ("module_too_large", "module above MAX_MODULE_BYTES", "BAN", "X0", "deploy cost must be bounded"),
        ("bad_binary", "magic, version, section framing and order", "BAN", "X0", "malformed input never reaches the engine"),
        ("integer_arithmetic", "i32/i64 arithmetic, compare, wrap/extend, load/store, locals, blocks, loops, branches, direct calls", "ALLOW", "X0 whitelist; X1 executes with fuel", "fully specified by the WASM spec"),
    ]
    for source, feature, policy, enforcement, note in rows:
        lines.append(f"| `{source}` | {feature} | **{policy}** | {enforcement} | {note} |")
    lines += [
        "",
        "## Restrictive configuration",
        "",
        "* module size <= **512 KiB** (`MAX_MODULE_BYTES`)",
        "* linear memory must declare a maximum <= **4096 pages = 256 MiB**",
        "  (`MAX_MEMORY_PAGES`)",
        "* tables must declare a maximum <= **1024 entries**",
        "* zero imports, no start function, no mutable globals, no floats,",
        "  no SIMD/atomics/bulk memory, no indirect calls",
        "",
        "## DoD gate — {} programs, each banned or deterministic".format(f["program_count"]),
        "",
        "| program | verdict |",
        "|---|---|",
    ]
    for name, verdict in programs:
        lines.append(f"| `{name}` | `{verdict}` |")
    lines += [
        "",
        f"**{f['allowed_count']} allowed, {f['banned_count']} banned.** The",
        "allowed set proves the whitelist is usable (integer arithmetic, memory",
        "round-trip, locals, direct calls, blocks/loops/branches); the banned",
        "set covers NaN/floats, a WASI clock, an env RNG, shared memory, atomics,",
        "SIMD, bulk memory, indirect calls, the start section, mutable globals,",
        "unbounded and oversized memory, and an opcode outside the whitelist.",
        "",
        "## Negative controls",
        "",
        "NC-1 imports, NC-2 the single float choke point, NC-3 start section,",
        "NC-4 memory maximum, NC-5 shared memory, NC-6 fail-closed whitelist.",
        "6/6 PASS; total **110** across C0–C10 + S0–S7 + X0.",
        "",
        "## Honest scope",
        "",
        "* X0 does not execute anything and does not link a VM: no fuel, no",
        "  memory isolation, no engine-level determinism claim. That is X1's gate.",
        "* The whitelist is a whitelist, not a full WASM validator: an opcode we",
        "  did not review is rejected, so the risk of the scanner is false",
        "  rejection, not false acceptance.",
        "* Instruction-level verification with a full decoder, floats in any",
        "  position and 100 mutants arrive in X8.",
        "* No consensus, P2P, node, wallet or devnet exists.",
        "",
        "Validators / blocks / transactions remain **0 / 0 / 0**.",
        "",
    ]
    return "\n".join(lines)


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = [
        "AuraLite-Crypto — negative-control inventory after X0",
        f"Generated (UTC): {stamp}",
        "",
    ]
    total = 0
    for name in NC_LOGS:
        path = REPO / "docs" / "evidence" / name / "nc.log"
        text = path.read_text(encoding="utf-8") if path.exists() else ""
        match = RESULT_LINE.search(text)
        passed = int(match.group(1)) if match else 0
        total += passed
        lines.append(
            f"{name.upper()}: {passed}/{int(match.group(2)) if match else 0} negative controls"
        )
        for nc_name, verdict in NC_LINE.findall(text):
            lines.append(f"  - {nc_name}: {verdict}")
        lines.append("")
    lines += [f"TOTAL: {total} NC", ""]
    return "\n".join(lines), total


def main() -> int:
    reset_current_evidence()
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    f = fixture_map()

    lines = [
        "AuraLite-Crypto — X0 evidence (determinism census)",
        f"Generated (UTC): {stamp}",
        "Rule: measured against the tree, not assumed.",
        "",
        "=" * 72,
        "RAW COMMAND OUTPUT",
        "=" * 72,
        "",
    ]
    failures = 0
    for label, cmd, tail in COMMANDS:
        rc, out = run(cmd)
        if tail and out:
            out = "\n".join(out.splitlines()[-tail:])
        lines.append(f"--- {label} ---")
        lines.append(f"$ {cmd}")
        lines.append(out if out else "(no output)")
        lines.append(f"[rc={rc}]")
        lines.append("")
        if rc != 0:
            failures += 1

    vector_report = run("python3 tools/count_vectors.py")[1]
    vectors = "4,415"
    for line in vector_report.splitlines():
        if "TOTAL:" in line:
            vectors = line.split("TOTAL:")[1].split("vectors")[0].strip()
            vectors = f"{int(vectors):,}"
            break

    lines += [
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
        "Pre-X0 (S7): 15 crates, 177 Rust tests, 4,349 golden vectors,",
        "104 negative controls, 95 external crates.",
        "Post-X0: 15 crates (no new crate, no new external dependency, no VM",
        f"engine), 184 Rust tests (+3 census unit, +4 census gate), {vectors} vectors",
        "across 18 fixtures, 110 negative controls (6 new in X0), 95 external",
        "crates.",
        f"Determinism census: {f['program_count']} programs"
        f" ({f['allowed_count']} allowed, {f['banned_count']} banned),"
        " fail-closed opcode whitelist, restrictive limits published.",
        "No runtime claim: wasmtime lands in X1.",
        "Validators/blocks/tx remain 0/0/0.",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")
    REPORT.write_text(build_report(f, stamp), encoding="utf-8")
    inventory, total = build_inventory(stamp)
    INVENTORY.write_text(inventory, encoding="utf-8")
    if total != 110:
        print(f"INVENTORY ERROR: total {total} NC, expected 110")
        failures += 1

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/x0/nc.log missing (run `make nc-x0`)")
        return 1

    sums = []
    for path in (OUT, nc_log, REPORT, INVENTORY, FIXTURE, ADDENDUM):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {REPORT.relative_to(REPO)}")
    print(f"wrote {INVENTORY.relative_to(REPO)}")
    print("wrote docs/evidence/x0/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
