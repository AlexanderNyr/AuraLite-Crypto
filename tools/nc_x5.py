#!/usr/bin/env python3
"""Negative controls for X5 (reference standards + ABI v2).

Six tampers must each turn an X5 gate red. They are chosen so that each one is a
mistake a real implementer makes, not a typo:

  NC-1 the dispatcher accepts trailing bytes      -> a method's meaning depends on
                                                     what a later version appends.
  NC-2 `transfer_from` does not spend the allowance -> a spender can spend it twice.
  NC-3 an ARC-721 transfer keeps the approval       -> the stale-approval bug: the old
                                                     spender moves the token again.
  NC-4 the multisig does not check `executed`       -> a proposal pays out twice.
  NC-5 the session accepts oversize calldata        -> a host pays for work it should
                                                     have refused before the engine.
  NC-6 a failed execution is still marked executed  -> a proposal the wallet could not
                                                     fund can never be retried.

Rule (CONTRIBUTING.md §1): a gate that cannot go red is not a gate.

Usage:
    python3 tools/nc_x5.py
    make nc-x5
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SDK_LIB = REPO / "crates" / "aura-sdk" / "src" / "lib.rs"
ARC20 = REPO / "standards" / "arc20" / "src" / "lib.rs"
ARC721 = REPO / "standards" / "arc721" / "src" / "lib.rs"
MULTISIG = REPO / "standards" / "multisig" / "src" / "lib.rs"
CONTRACT_RS = REPO / "crates" / "aura-exec" / "src" / "contract.rs"

ENGINE = ["cargo", "test", "-p", "aura-exec", "--quiet"]
ARC20_TEST = ["cargo", "test", "-p", "aura-exec", "--test", "x5_arc20", "--quiet"]
ARC721_TEST = ["cargo", "test", "-p", "aura-exec", "--test", "x5_arc721", "--quiet"]
MULTISIG_TEST = ["cargo", "test", "-p", "aura-exec", "--test", "x5_multisig", "--quiet"]
ABI2_TEST = ["cargo", "test", "-p", "aura-exec", "--test", "x5_abi2", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def rebuild_standard(_name: str) -> subprocess.CompletedProcess[str]:
    """Rebuild every standard after tampering with a source it depends on."""
    return subprocess.run(
        ["python3", "tools/build_standards.py"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )


def expect_red(label: str, path: Path, old: str, new: str, cmd: list[str]) -> bool:
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
        os.utime(path, None)
        result = run(cmd)
        went_red = result.returncode != 0
        shown = " ".join(cmd)
        raw = "\n".join((result.stdout + result.stderr).strip().splitlines()[-2:])
        snippet = re.sub(r"finished in [\d.]+s", "finished", raw)
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      {shown} -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        os.utime(path, None)


STANDARDS_DIR = REPO / "standards"
FIXTURE = REPO / "crates" / "aura-exec" / "tests" / "fixtures" / "x5_standards.txt"


def build_outputs() -> dict[Path, bytes]:
    """Every file `tools/build_standards.py` writes, so a tamper can be undone."""
    files = [FIXTURE]
    files += sorted(STANDARDS_DIR.rglob("wasm/*.wasm"))
    files += sorted(STANDARDS_DIR.rglob("abi.json"))
    return {path: path.read_bytes() for path in files if path.exists()}


def restore_outputs(snapshot: dict[Path, bytes]) -> None:
    for path, data in snapshot.items():
        path.write_bytes(data)


def expect_red_after_rebuild(
    label: str, path: Path, old: str, new: str, cmd: list[str]
) -> bool:
    """Tamper a source file, rebuild the standards, then run a gate.

    The standards are compiled to wasm that the gate deploys, so a tamper in a
    contract's source — or in the SDK it is compiled against — only reaches the gate
    through a rebuild. Both the tampered source and every build output are restored
    afterwards, which is what keeps the tree clean and the fixture honest.
    """
    original = path.read_text(encoding="utf-8")
    snapshot = build_outputs()
    if old not in original:
        print(f"{label}: FAIL — tamper target not found in {path.name}")
        return False
    try:
        path.write_text(original.replace(old, new, 1), encoding="utf-8")
        os.utime(path, None)
        build = rebuild_standard("all")
        if build.returncode != 0:
            print(f"{label}: FAIL — the tampered tree did not build")
            print("      " + "\n      ".join(build.stderr.strip().splitlines()[-3:]))
            return False
        result = run(cmd)
        went_red = result.returncode != 0
        shown = " ".join(cmd)
        raw = "\n".join((result.stdout + result.stderr).strip().splitlines()[-2:])
        snippet = re.sub(r"finished in [\d.]+s", "finished", raw)
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      {shown} -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        os.utime(path, None)
        restore_outputs(snapshot)


def main() -> int:
    # No timestamp: this log is checksummed in docs/evidence/x5/SHA256SUMS.
    print("X5 NEGATIVE CONTROLS (reference standards + ABI v2)")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = [
        expect_red_after_rebuild(
            "NC-1 the dispatcher accepts trailing bytes",
            SDK_LIB,
            "                if !$reader.is_empty() {\n                    $crate::abi_status::BAD_ARGS\n                } else {\n                    $method(a, b) as i32\n                }",
            "                if false {\n                    $crate::abi_status::BAD_ARGS\n                } else {\n                    $method(a, b) as i32\n                }",
            ARC20_TEST,
        ),
        expect_red_after_rebuild(
            "NC-2 `transfer_from` does not spend the allowance",
            ARC20,
            "            set_allowance(&from, &spender, allowed - amount);",
            "            let _ = allowed;",
            ARC20_TEST,
        ),
        expect_red_after_rebuild(
            "NC-3 an ARC-721 transfer keeps the approval",
            ARC721,
            "    set_approved(token_id, [0u8; 20]);",
            "",
            ARC721_TEST,
        ),
        expect_red_after_rebuild(
            "NC-4 the multisig does not check `executed`",
            MULTISIG,
            "    if executed(pid) {\n        return ERR_ALREADY_EXECUTED as u32;\n    }\n    if voted(pid, &caller) {",
            "    if voted(pid, &caller) {",
            MULTISIG_TEST,
        ),
        expect_red(
            "NC-5 the session accepts oversize calldata",
            CONTRACT_RS,
            "        if calldata.len() > crate::abi_v2::MAX_CALLDATA_LEN {",
            "        if false {",
            ABI2_TEST,
        ),
        expect_red_after_rebuild(
            "NC-6 a failed execution is still marked executed",
            MULTISIG,
            """    if funds < amount {
        return ERR_INSUFFICIENT_BALANCE as u32;
    }
    Slot::<u8>::with_key(pid_key(b"e", pid)).write(1);
    if let Err(error) = transfer(&to, amount) {
        // The money did not move, so the flag must not stay set: a proposal that
        // failed is retryable.
        unwrap(Slot::<u8>::with_key(pid_key(b"e", pid)).clear());
        return error.code() as u32;
    }""",
            """    let _ = funds;
    Slot::<u8>::with_key(pid_key(b"e", pid)).write(1);
    if let Err(error) = transfer(&to, amount) {
        return error.code() as u32;
    }""",
            MULTISIG_TEST,
        ),
    ]
    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
