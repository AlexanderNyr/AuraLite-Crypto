#!/usr/bin/env python3
"""W5 hardware vectors: 10 human confirmed/denied scenarios.

Derives every expected byte independently of the Rust implementation and
writes `crates/aura-wallet/tests/fixtures/w5_hardware_vectors.json`. The
fixture pins the rendered device screen, the signature bytes and the exact
exit/stdout/stderr of each scenario. PUBLIC TEST DATA ONLY — never use for
funds.
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, "/home/user/aura-python-deps")

from tools import gen_c4_vectors as c4  # noqa: E402
from tools import gen_w2_vectors as g2  # noqa: E402
from tools import gen_w3_vectors as g3  # noqa: E402
from tools.w1_interop_reference import bech32_address  # noqa: E402
from mnemonic import Mnemonic  # noqa: E402

FIXTURE = ROOT / "crates/aura-wallet/tests/fixtures/w5_hardware_vectors.json"

GATE = "10 \"human confirmed/denied\" scenarios; NC: blind confirmation -> red"
NAMED_CONTROL = "blind_confirmation_rejected"
DEVICE = "auralite-hw-1"

CHAIN = "auralite-test-1"
FOREIGN = "evil-net-9"
PASSWORD = "test password for hardware vectors"
STAKE_PREFIX = b"AuraLite-Crypto/wallet-stake-v1\x00"
KIND_TRANSFER = 0
KIND_DELEGATE = 3

U64MAX = 2**64 - 1


def transfer_payload(recipient_raw: bytes, amount: int) -> bytes:
    return recipient_raw + amount.to_bytes(8, "little")


def stake_payload(action_tag: int, validator_raw: bytes, amount: int) -> bytes:
    return STAKE_PREFIX + bytes([action_tag]) + validator_raw + amount.to_bytes(8, "little")


def unsigned_body(acct: dict, chain: str, nonce: int, kind: int, payload: bytes,
                  gas: int = 21_000, max_fee: int = 1_000_000,
                  tip: int = 0, expiry: int = U64MAX) -> bytes:
    case = dict(chain=chain, nonce=nonce, kind=kind, payload=payload,
                gas=gas, max_fee=max_fee, tip=tip, expiry=expiry,
                address=acct["raw"], public_key=acct["pub"])
    unsigned, _ = c4.encode_unsigned(case)
    return unsigned


def sig_hex(acct: dict, unsigned: bytes, chain: str) -> str:
    signing_bytes = c4.SIGNING_CONTEXT + unsigned
    digest = c4.blake3_short(c4.domain_message(chain, "tx", signing_bytes))
    return c4.ed25519_sign(digest, acct["priv"]).hex()


def display_lines(*, chain: str, kind: str, signer: str, nonce: int,
                  to: str | None = None, validator: str | None = None,
                  amount: int | None = None, gas: int = 21_000,
                  max_fee: int = 1_000_000, tip: int = 0,
                  expiry: int = U64MAX) -> list[str]:
    lines = [
        f"display: chain={chain}",
        f"display: kind={kind}",
        f"display: signer={signer}",
        f"display: nonce={nonce}",
    ]
    if to is not None:
        lines.append(f"display: to={to}")
    if validator is not None:
        lines.append(f"display: validator={validator}")
    if amount is not None:
        lines.append(f"display: amount={amount}")
    lines.append(f"display: gas={gas}")
    lines.append(f"display: max_fee={max_fee}")
    lines.append(f"display: tip={tip}")
    lines.append(f"display: expiry={expiry}")
    return lines


def build_cases() -> list[dict]:
    phrase00 = Mnemonic("english").to_mnemonic(bytes(32))  # abandon ... art
    a0 = g2.account(g2.seed_from_mnemonic(phrase00), 0)
    a1 = g2.account(g2.seed_from_mnemonic(phrase00), 1)
    a2 = g2.account(g2.seed_from_mnemonic(phrase00), 2)

    if a0["addr"] != g2.KNOWN_ADDR_0:
        raise ValueError("derived identity does not match the pinned smoke values")

    pw = PASSWORD + "\n"
    base_files = {"pw": pw, "ent00": ("00" * 32) + "\n"}
    setup = [[
        "new", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
        "--chain", CHAIN, "--entropy-file", "{sandbox}/ent00",
        "--kdf", "testing", "--account", "0",
    ]]

    payload_basic = transfer_payload(a1["raw"], 1_000)
    payload_high = transfer_payload(a1["raw"], 123_456_789)
    body_basic = unsigned_body(a0, CHAIN, 0, KIND_TRANSFER, payload_basic)
    body_high = unsigned_body(a0, CHAIN, 1, KIND_TRANSFER, payload_high)
    body_delegate = unsigned_body(a0, CHAIN, 2, KIND_DELEGATE,
                                  stake_payload(0, a1["raw"], 5_000_000))
    body_legacy = unsigned_body(a0, CHAIN, 3, KIND_TRANSFER,
                                (1_000).to_bytes(8, "little"))
    body_foreign = unsigned_body(a0, CHAIN, 4, KIND_TRANSFER, payload_basic)

    sig_basic = sig_hex(a0, body_basic, CHAIN)
    sig_high = sig_hex(a0, body_high, CHAIN)
    sig_delegate = sig_hex(a0, body_delegate, CHAIN)

    frame_basic = display_lines(
        chain=CHAIN, kind="transfer", signer=a0["addr"], nonce=0,
        to=a1["addr"], amount=1_000)
    frame_high = display_lines(
        chain=CHAIN, kind="transfer", signer=a0["addr"], nonce=1,
        to=a1["addr"], amount=123_456_789)
    frame_delegate = display_lines(
        chain=CHAIN, kind="delegate", signer=a0["addr"], nonce=2,
        validator=a1["addr"], amount=5_000_000)

    def hw_argv(body: bytes, human: str, chain: str = CHAIN,
                extra: list[str] | None = None) -> tuple[list[str], dict[str, str]]:
        files = dict(base_files)
        files["u.env"] = g3.envelope(g3.KIND_UNSIGNED, body) + "\n"
        argv = ["hw-sign", "--in", "{sandbox}/u.env",
                "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
                "--account", "0", "--chain", chain, "--human", human]
        argv += (extra or [])
        argv += ["--out", "{sandbox}/s.env"]
        return argv, files

    def ok_out(frame: list[str], sig: str) -> list[str]:
        return [f"device={DEVICE}"] + frame + ["status=signed", f"sig={sig}"]

    cases: list[dict] = []
    A = cases.append

    def add(name: str, argv: list[str], files: dict[str, str], *,
            exit_code: int = 0, out: list[str] | None = None,
            err: list[str] | None = None) -> None:
        A(g2.case(name, argv, files=files, setup=setup,
                  exit_code=exit_code, out=out, err=err))

    # --- 1..2: basic transfer ------------------------------------------
    argv, files = hw_argv(body_basic, "confirm")
    add("confirm_transfer_basic", argv, files, out=ok_out(frame_basic, sig_basic))
    argv, files = hw_argv(body_basic, "deny")
    add("deny_transfer_basic", argv, files, exit_code=65,
        out=[f"device={DEVICE}"] + frame_basic,
        err=["error: hardware: denied by user"])

    # --- 3..4: high-value transfer -------------------------------------
    argv, files = hw_argv(body_high, "confirm")
    add("confirm_transfer_high_value", argv, files, out=ok_out(frame_high, sig_high))
    argv, files = hw_argv(body_high, "deny")
    add("deny_transfer_high_value", argv, files, exit_code=65,
        out=[f"device={DEVICE}"] + frame_high,
        err=["error: hardware: denied by user"])

    # --- 5: blind confirmation (named control) -------------------------
    argv, files = hw_argv(body_basic, "blind")
    add(NAMED_CONTROL, argv, files, exit_code=65,
        out=[f"device={DEVICE}"],
        err=["error: hardware: blind confirmation rejected"])

    # --- 6: host preview substitutes the recipient ---------------------
    argv, files = hw_argv(body_basic, "confirm",
                          extra=["--show-recipient", a2["addr"],
                                 "--show-amount", "1000"])
    add("display_recipient_substitution", argv, files, exit_code=65,
        out=[f"device={DEVICE}"],
        err=["error: hardware: display mismatch"])

    # --- 7: deny then retry with confirm -------------------------------
    argv, files = hw_argv(body_basic, "deny,confirm")
    add("deny_then_retry_confirm", argv, files,
        out=[f"device={DEVICE}"] + frame_basic + frame_basic
        + ["status=signed", f"sig={sig_basic}"])

    # --- 8: delegate ----------------------------------------------------
    argv, files = hw_argv(body_delegate, "confirm")
    add("confirm_delegate", argv, files, out=ok_out(frame_delegate, sig_delegate))

    # --- 9: undecodable payload ----------------------------------------
    argv, files = hw_argv(body_legacy, "confirm")
    add("undecodable_payload_refused", argv, files, exit_code=65,
        out=[f"device={DEVICE}"],
        err=["error: hardware: undecodable payload"])

    # --- 10: foreign chain ---------------------------------------------
    argv, files = hw_argv(body_foreign, "confirm", chain=FOREIGN)
    add("foreign_chain_refused", argv, files, exit_code=65,
        out=[f"device={DEVICE}"],
        err=[f"error: foreign chain-id: tx={CHAIN}, wallet={FOREIGN}"])

    if len(cases) != 10 or len({c["name"] for c in cases}) != 10:
        raise ValueError("expected exactly 10 unique hardware scenarios")
    return cases


def main() -> None:
    payload = {
        "gate": GATE,
        "notice": g2.NOTICE,
        "named_control": NAMED_CONTROL,
        "cases": build_cases(),
    }
    FIXTURE.write_text(json.dumps(payload, indent=2) + "\n")
    print(f"wrote {FIXTURE.relative_to(ROOT)}: 10 cases, named control {NAMED_CONTROL}")


if __name__ == "__main__":
    main()
