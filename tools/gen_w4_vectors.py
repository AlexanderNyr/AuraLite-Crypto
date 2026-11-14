#!/usr/bin/env python3
"""W4 oracle — 22 CLI vectors for the wallet-level M-of-N multisig contract.

Design of record: docs/w4-multisig-plan.md (D-W4-1..D-W4-4). Generates
`crates/aura-wallet/tests/fixtures/w4_multisig_vectors.json`, asserted
byte-for-byte by `tests/w4_multisig.rs`.

Independent implementation: reuses the self-checked W2/W3 oracle primitives
(BLAKE3, Ed25519 RFC 8032, domain_message, BIP-39/SLIP-0010, bech32) and adds
the multisig descriptor/proposal/approval documents. PUBLIC TEST DATA ONLY -
DO NOT USE FOR FUNDS.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, "/home/user/aura-python-deps")

from tools import gen_c4_vectors as c4  # noqa: E402
from tools import gen_w2_vectors as g2  # noqa: E402

DESCRIPTOR_PREFIX = b"AuraLite-Crypto/wallet-msig-v1\x00"
PROPOSAL_PREFIX = b"AuraLite-Crypto/wallet-msig-proposal-v1\x00"
CHAIN = g2.CHAIN
NOTICE = g2.NOTICE
ENTROPY_00 = "00" * 32

# Cross-check pin: computed by the Rust smoke run of `msig-init` (3-of-5,
# members = accounts 0..4 of the W1 smoke key, timeout 1000).
KNOWN_CONTRACT_3OF5 = "a3ed446e112c368e49e44553e7fbc288a6e8fc3c75bc35b508d08ab1e2ff9eb6"


def descriptor_bytes(chain: str, threshold: int, members: list[bytes], timeout: int) -> bytes:
    keys = sorted(members)
    return (
        DESCRIPTOR_PREFIX
        + bytes([len(chain)])
        + chain.encode()
        + bytes([threshold])
        + bytes([len(keys)])
        + b"".join(keys)
    )


def contract_id(chain: str, threshold: int, members: list[bytes], timeout: int) -> str:
    return c4.blake3_short(descriptor_bytes(chain, threshold, members, timeout)).hex()


def proposal_bytes(chain: str, contract: str, pid: int, expiry: int, kind: str,
                   to_raw: bytes = b"", amount: int = 0,
                   new_members: list[bytes] | None = None,
                   new_threshold: int = 0, new_timeout: int = 0) -> bytes:
    out = (
        PROPOSAL_PREFIX
        + bytes([len(chain)])
        + chain.encode()
        + bytes.fromhex(contract)
        + pid.to_bytes(8, "little")
        + expiry.to_bytes(8, "little")
    )
    if kind == "transfer":
        out += b"\x00" + to_raw + amount.to_bytes(8, "little")
    else:
        out += b"\x01" + descriptor_bytes(chain, new_threshold, new_members or [], new_timeout)
    return out


def approval_json(member_pk: bytes, priv: bytes, chain: str, prop: bytes) -> str:
    digest = c4.blake3_short(c4.domain_message(chain, "contract", prop))
    sig = c4.ed25519_sign(digest, priv)
    doc = {
        "format": "AuraLite-MSIG-APPROVAL-V1",
        "member": member_pk.hex(),
        "proposal": prop.hex(),
        "sig": sig.hex(),
    }
    return json.dumps(doc, indent=2, sort_keys=True) + "\n"


def descriptor_json(chain: str, threshold: int, members: list[bytes], timeout: int) -> str:
    doc = {
        "format": "AuraLite-MSIG-V1",
        "chain": chain,
        "threshold": threshold,
        "members": [m.hex() for m in sorted(members)],
        "timeout": timeout,
    }
    return json.dumps(doc, indent=2, sort_keys=True) + "\n"


def proposal_json(chain: str, contract: str, pid: int, expiry: int, kind: str,
                  to_raw: bytes = b"", amount: int = 0,
                  new_members: list[bytes] | None = None,
                  new_threshold: int = 0, new_timeout: int = 0) -> str:
    doc = {
        "format": "AuraLite-MSIG-PROPOSAL-V1",
        "chain": chain,
        "contract": contract,
        "id": pid,
        "expiry": expiry,
        "kind": kind,
    }
    if kind == "transfer":
        doc["to"] = bech32_address(to_raw)
        doc["amount"] = amount
    else:
        doc["new_multisig"] = json.loads(
            descriptor_json(chain, new_threshold, new_members or [], new_timeout)
        )
    return json.dumps(doc, indent=2, sort_keys=True) + "\n"


from tools.w1_interop_reference import bech32_address  # noqa: E402


def case(name: str, argv: list[str], *, files: dict[str, str] | None = None,
         setup: list[list[str]] | None = None, expect_exit: int = 0,
         expect_stdout: list[str] | None = None,
         expect_stderr: list[str] | None = None) -> dict:
    return {
        "name": name,
        "argv": argv,
        "setup": setup or [],
        "files": files or {},
        "expect_exit": expect_exit,
        "expect_stdout": expect_stdout or [],
        "expect_stderr": expect_stderr or [],
    }


def build_cases() -> list[dict]:
    g2.self_check()
    seed = g2.seed_from_mnemonic(g2.MNEMO.to_mnemonic(bytes(32)))
    accts = {i: g2.account(seed, i) for i in (0, 1, 2, 3, 4, 8)}
    assert accts[0]["addr"] == g2.KNOWN_ADDR_0

    members = [accts[i]["pub"] for i in (0, 1, 2, 3, 4)]
    msig_members_arg = ",".join(m.hex() for m in members)
    members2 = [accts[i]["pub"] for i in (1, 2, 3)]
    msig2_members_arg = ",".join(m.hex() for m in members2)

    TIMEOUT = 1000
    assert contract_id(CHAIN, 3, members, TIMEOUT) == KNOWN_CONTRACT_3OF5
    CID = KNOWN_CONTRACT_3OF5
    CID2 = contract_id(CHAIN, 2, members2, 2000)

    msig_json = descriptor_json(CHAIN, 3, members, TIMEOUT)
    msig2_json = descriptor_json(CHAIN, 2, members2, 2000)
    msig_foreign_json = descriptor_json("other-net", 3, members, TIMEOUT)

    to_raw = accts[0]["raw"]
    PROP = proposal_bytes(CHAIN, CID, 1, 500, "transfer", to_raw=to_raw, amount=1000)
    prop_json = proposal_json(CHAIN, CID, 1, 500, "transfer", to_raw=to_raw, amount=1000)
    prop2 = proposal_bytes(CHAIN, CID, 2, 500, "transfer", to_raw=to_raw, amount=2000)
    ROT = proposal_bytes(CHAIN, CID, 7, 900, "rotate",
                         new_members=members2, new_threshold=2, new_timeout=2000)
    rot_json = proposal_json(CHAIN, CID, 7, 900, "rotate",
                             new_members=members2, new_threshold=2, new_timeout=2000)

    ap0 = approval_json(accts[0]["pub"], accts[0]["priv"], CHAIN, PROP)
    ap1 = approval_json(accts[1]["pub"], accts[1]["priv"], CHAIN, PROP)
    ap2 = approval_json(accts[2]["pub"], accts[2]["priv"], CHAIN, PROP)
    rot0 = approval_json(accts[0]["pub"], accts[0]["priv"], CHAIN, ROT)
    rot1 = approval_json(accts[1]["pub"], accts[1]["priv"], CHAIN, ROT)
    rot2 = approval_json(accts[2]["pub"], accts[2]["priv"], CHAIN, ROT)

    new0 = ["new", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
            "--chain", CHAIN, "--entropy-file", "{sandbox}/entropy", "--account", "0",
            "--kdf", "testing"]
    new8 = ["new", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
            "--chain", CHAIN, "--entropy-file", "{sandbox}/entropy", "--account", "8",
            "--kdf", "testing"]
    base_files = {"pw": "pw\n", "entropy": ENTROPY_00}

    approve_argv = [
        "msig-approve", "--proposal", "{sandbox}/prop.json", "--multisig", "{sandbox}/msig.json",
        "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw", "--account", "0",
        "--chain", CHAIN, "--height", "100", "--out", "{sandbox}/ap.json",
    ]

    cases: list[dict] = []

    # --- msig-init (4) ---
    init_out = ["status=created", f"chain={CHAIN}", "threshold=3", "members=5",
                f"multisig_id={CID}"]
    cases.append(case("init_basic",
                      ["msig-init", "--chain", CHAIN, "--threshold", "3",
                       "--members", msig_members_arg, "--timeout", "1000",
                       "--out", "{sandbox}/msig.json"],
                      expect_stdout=init_out))
    cases.append(case("init_threshold_zero",
                      ["msig-init", "--chain", CHAIN, "--threshold", "0",
                       "--members", msig_members_arg, "--timeout", "1000",
                       "--out", "{sandbox}/msig.json"],
                      expect_exit=64,
                      expect_stderr=["error: usage: --threshold must be 1..=255"]))
    cases.append(case("init_threshold_gt_n",
                      ["msig-init", "--chain", CHAIN, "--threshold", "6",
                       "--members", msig_members_arg, "--timeout", "1000",
                       "--out", "{sandbox}/msig.json"],
                      expect_exit=64,
                      expect_stderr=["error: usage: --threshold must not exceed members"]))
    cases.append(case("init_duplicate_member",
                      ["msig-init", "--chain", CHAIN, "--threshold", "3",
                       "--members", msig_members_arg + "," + accts[0]["pub"].hex(),
                       "--timeout", "1000", "--out", "{sandbox}/msig.json"],
                      expect_exit=64,
                      expect_stderr=["error: usage: duplicate member"]))

    # --- msig-propose (5) ---
    propose_out = ["status=proposed", f"chain={CHAIN}", f"multisig_id={CID}",
                   "proposal_id=1", "kind=transfer"]
    cases.append(case("propose_transfer_basic",
                      ["msig-propose", "--multisig", "{sandbox}/msig.json", "--chain", CHAIN,
                       "--proposal-id", "1", "--expiry", "500", "--action", "transfer",
                       "--to", accts[0]["addr"], "--amount", "1000", "--out", "{sandbox}/prop.json"],
                      files={"msig.json": msig_json},
                      expect_stdout=propose_out))
    cases.append(case("propose_unknown_action",
                      ["msig-propose", "--multisig", "{sandbox}/msig.json", "--chain", CHAIN,
                       "--proposal-id", "1", "--expiry", "500", "--action", "burn",
                       "--out", "{sandbox}/prop.json"],
                      files={"msig.json": msig_json},
                      expect_exit=64,
                      expect_stderr=["error: usage: --action must be transfer or rotate"]))
    cases.append(case("propose_foreign_chain",
                      ["msig-propose", "--multisig", "{sandbox}/msig.json", "--chain", CHAIN,
                       "--proposal-id", "1", "--expiry", "500", "--action", "transfer",
                       "--to", accts[0]["addr"], "--amount", "1000", "--out", "{sandbox}/prop.json"],
                      files={"msig.json": msig_foreign_json},
                      expect_exit=65,
                      expect_stderr=[f"error: foreign chain-id: tx=other-net, wallet={CHAIN}"]))
    cases.append(case("propose_expiry_gt_timeout",
                      ["msig-propose", "--multisig", "{sandbox}/msig.json", "--chain", CHAIN,
                       "--proposal-id", "1", "--expiry", "1001", "--action", "transfer",
                       "--to", accts[0]["addr"], "--amount", "1000", "--out", "{sandbox}/prop.json"],
                      files={"msig.json": msig_json},
                      expect_exit=64,
                      expect_stderr=["error: usage: --expiry must not exceed contract timeout"]))
    cases.append(case("propose_rotate",
                      ["msig-propose", "--multisig", "{sandbox}/msig.json", "--chain", CHAIN,
                       "--proposal-id", "7", "--expiry", "900", "--action", "rotate",
                       "--new-multisig", "{sandbox}/msig2.json", "--out", "{sandbox}/prop.json"],
                      files={"msig.json": msig_json, "msig2.json": msig2_json},
                      expect_stdout=["status=proposed", f"chain={CHAIN}", f"multisig_id={CID}",
                                     "proposal_id=7", "kind=rotate"]))

    # --- msig-approve (4) ---
    cases.append(case("approve_basic", approve_argv,
                      files={"msig.json": msig_json, "prop.json": prop_json, **base_files},
                      setup=[new0],
                      expect_stdout=["status=approved", f"chain={CHAIN}", "proposal_id=1",
                                     f"member={accts[0]['pub'].hex()}"]))
    cases.append(case("approve_not_a_member",
                      ["msig-approve", "--proposal", "{sandbox}/prop.json",
                       "--multisig", "{sandbox}/msig.json", "--keystore", "{sandbox}/ks",
                       "--password-file", "{sandbox}/pw", "--account", "8",
                       "--chain", CHAIN, "--height", "100", "--out", "{sandbox}/ap.json"],
                      files={"msig.json": msig_json, "prop.json": prop_json, **base_files},
                      setup=[new8],
                      expect_exit=65,
                      expect_stderr=["error: multisig: not a member"]))
    cases.append(case("approve_expired",
                      ["msig-approve", "--proposal", "{sandbox}/prop.json",
                       "--multisig", "{sandbox}/msig.json", "--keystore", "{sandbox}/ks",
                       "--password-file", "{sandbox}/pw", "--account", "0",
                       "--chain", CHAIN, "--height", "600", "--out", "{sandbox}/ap.json"],
                      files={"msig.json": msig_json, "prop.json": prop_json, **base_files},
                      setup=[new0],
                      expect_exit=65,
                      expect_stderr=["error: multisig: proposal expired"]))
    cases.append(case("approve_foreign_chain",
                      ["msig-approve", "--proposal", "{sandbox}/prop.json",
                       "--multisig", "{sandbox}/msig.json", "--keystore", "{sandbox}/ks",
                       "--password-file", "{sandbox}/pw", "--account", "0",
                       "--chain", "other-net", "--height", "100", "--out", "{sandbox}/ap.json"],
                      files={"msig.json": msig_json, "prop.json": prop_json, **base_files},
                      setup=[new0],
                      expect_exit=65,
                      expect_stderr=[f"error: foreign chain-id: tx={CHAIN}, wallet=other-net"]))

    # --- msig-collect (7) ---
    collect_argv = ["msig-collect", "--proposal", "{sandbox}/prop.json",
                    "--multisig", "{sandbox}/msig.json", "--approvals", "{sandbox}/ap",
                    "--chain", CHAIN, "--height", "100"]
    passed_out = ["status=passed", f"chain={CHAIN}", f"multisig_id={CID}", "proposal_id=1",
                  "signatures=3", "threshold=3"]
    files3 = {"msig.json": msig_json, "prop.json": prop_json,
              "ap/00.json": ap0, "ap/01.json": ap1, "ap/02.json": ap2}
    files2 = {"msig.json": msig_json, "prop.json": prop_json, "ap/00.json": ap0, "ap/01.json": ap1}
    cases.append(case("collect_3_of_5_passed", collect_argv, files=files3,
                      expect_stdout=passed_out))
    cases.append(case("collect_2_of_5_rejected", collect_argv, files=files2,
                      expect_exit=65,
                      expect_stderr=["error: multisig: threshold not met (2/3)"]))
    cases.append(case("collect_duplicate_member", collect_argv,
                      files={"msig.json": msig_json, "prop.json": prop_json,
                             "ap/00.json": ap0, "ap/01.json": ap0, "ap/02.json": ap1},
                      expect_exit=65,
                      expect_stderr=["error: multisig: duplicate approval"]))
    forged = json.loads(ap2)
    forged["sig"] = forged["sig"][:-2] + ("00" if forged["sig"][-2:] != "00" else "01")
    cases.append(case("collect_forged_signature", collect_argv,
                      files={"msig.json": msig_json, "prop.json": prop_json,
                             "ap/00.json": ap0, "ap/01.json": ap1,
                             "ap/02.json": json.dumps(forged, indent=2, sort_keys=True) + "\n"},
                      expect_exit=65,
                      expect_stderr=["error: signature verification failed"]))
    foreign_ap = approval_json(accts[2]["pub"], accts[2]["priv"], CHAIN, prop2)
    cases.append(case("collect_foreign_proposal", collect_argv,
                      files={"msig.json": msig_json, "prop.json": prop_json,
                             "ap/00.json": ap0, "ap/01.json": ap1, "ap/02.json": foreign_ap},
                      expect_exit=65,
                      expect_stderr=["error: multisig: approval for a different proposal"]))
    cases.append(case("collect_expired",
                      ["msig-collect", "--proposal", "{sandbox}/prop.json",
                       "--multisig", "{sandbox}/msig.json", "--approvals", "{sandbox}/ap",
                       "--chain", CHAIN, "--height", "600"],
                      files=files3,
                      expect_exit=65,
                      expect_stderr=["error: multisig: proposal expired"]))
    cases.append(case("collect_wrong_contract",
                      ["msig-collect", "--proposal", "{sandbox}/prop.json",
                       "--multisig", "{sandbox}/msig2.json", "--approvals", "{sandbox}/ap",
                       "--chain", CHAIN, "--height", "100"],
                      files={"msig2.json": msig2_json, "prop.json": prop_json,
                             "ap/00.json": ap0, "ap/01.json": ap1, "ap/02.json": ap2},
                      expect_exit=65,
                      expect_stderr=["error: multisig: proposal for a different contract"]))

    # --- rotation / e2e (2) ---
    rot_files = {"msig.json": msig_json, "prop.json": rot_json,
                 "ap/00.json": rot0, "ap/01.json": rot1, "ap/02.json": rot2}
    cases.append(case("rotate_3_of_5_passed",
                      ["msig-collect", "--proposal", "{sandbox}/prop.json",
                       "--multisig", "{sandbox}/msig.json", "--approvals", "{sandbox}/ap",
                       "--chain", CHAIN, "--height", "100"],
                      files=rot_files,
                      expect_stdout=["status=passed", f"chain={CHAIN}", f"multisig_id={CID}",
                                     "proposal_id=7", "signatures=3", "threshold=3"]))
    cases.append(case("e2e_full_flow",
                      ["msig-collect", "--proposal", "{sandbox}/prop.json",
                       "--multisig", "{sandbox}/msig.json", "--approvals", "{sandbox}/ap",
                       "--chain", CHAIN, "--height", "100"],
                      files={**base_files, "ap/placeholder.txt": ""},
                      setup=[
                          ["new", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
                           "--chain", CHAIN, "--entropy-file", "{sandbox}/entropy", "--account", "0",
                           "--kdf", "testing"],
                          ["new", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
                           "--chain", CHAIN, "--entropy-file", "{sandbox}/entropy", "--account", "1",
                           "--kdf", "testing"],
                          ["new", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
                           "--chain", CHAIN, "--entropy-file", "{sandbox}/entropy", "--account", "2",
                           "--kdf", "testing"],
                          ["msig-init", "--chain", CHAIN, "--threshold", "3",
                           "--members", msig_members_arg, "--timeout", "1000",
                           "--out", "{sandbox}/msig.json"],
                          ["msig-propose", "--multisig", "{sandbox}/msig.json", "--chain", CHAIN,
                           "--proposal-id", "1", "--expiry", "500", "--action", "transfer",
                           "--to", accts[0]["addr"], "--amount", "1000",
                           "--out", "{sandbox}/prop.json"],
                          ["msig-approve", "--proposal", "{sandbox}/prop.json",
                           "--multisig", "{sandbox}/msig.json", "--keystore", "{sandbox}/ks",
                           "--password-file", "{sandbox}/pw", "--account", "0",
                           "--chain", CHAIN, "--height", "100", "--out", "{sandbox}/ap/00.json"],
                          ["msig-approve", "--proposal", "{sandbox}/prop.json",
                           "--multisig", "{sandbox}/msig.json", "--keystore", "{sandbox}/ks",
                           "--password-file", "{sandbox}/pw", "--account", "1",
                           "--chain", CHAIN, "--height", "100", "--out", "{sandbox}/ap/01.json"],
                          ["msig-approve", "--proposal", "{sandbox}/prop.json",
                           "--multisig", "{sandbox}/msig.json", "--keystore", "{sandbox}/ks",
                           "--password-file", "{sandbox}/pw", "--account", "2",
                           "--chain", CHAIN, "--height", "100", "--out", "{sandbox}/ap/02.json"],
                      ],
                      expect_stdout=passed_out))

    names = [c["name"] for c in cases]
    assert len(cases) == 22, f"gate requires 22 vectors, got {len(cases)}"
    assert len(set(names)) == 22, names
    return cases


def main() -> None:
    out = {
        "gate": "3-of-5: 3 collected -> success, 2 -> reject; NC: threshold bypass -> red",
        "notice": NOTICE,
        "cases": build_cases(),
    }
    path = ROOT / "crates/aura-wallet/tests/fixtures/w4_multisig_vectors.json"
    path.write_text(json.dumps(out, indent=1) + "\n", encoding="utf-8")
    print(f"wrote {path.relative_to(ROOT)} ({len(out['cases'])} vectors)")


if __name__ == "__main__":
    main()
