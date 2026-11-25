#!/usr/bin/env python3
"""W6 web wallet vectors: 10 signing scenarios over deep links and envelopes.

Derives every expected byte independently of the Rust implementation and
writes `crates/aura-wallet/tests/fixtures/w6_web_wallet_vectors.json`. The
fixture pins the rendered `ui:` confirmation view (real fields from the
transaction bytes), the signature bytes and the exact exit/stdout/stderr of
each scenario. PUBLIC TEST DATA ONLY — never use for funds.
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

FIXTURE = ROOT / "crates/aura-wallet/tests/fixtures/w6_web_wallet_vectors.json"

GATE = "signing shows the human the real tx fields; NC: recipient substitution in UI -> red"
NAMED_CONTROL = "ui_recipient_substitution"
UI = "wallet=aura-keys-1"

CHAIN = "auralite-test-1"
PASSWORD = "test password for web wallet vectors"
TRUSTED_APP = "https://app.auralite.invalid"
TRUSTED_WALLET = "https://wallet.auralite.invalid"
# Homoglyph lookalike: two Cyrillic 'а' (U+0430) inside wallet.auralite.invalid.
LOOKALIKE = "https://wallet.\u0430ur\u0430lite.invalid"

STAKE_PREFIX = b"AuraLite-Crypto/wallet-stake-v1\x00"
KIND_TRANSFER = 0
KIND_DELEGATE = 3
U64MAX = 2**64 - 1


def transfer_payload(recipient_raw: bytes, amount: int) -> bytes:
    return recipient_raw + amount.to_bytes(8, "little")


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


def ui_lines(*, chain: str, signer: str, nonce: int, to: str, amount: int,
             gas: int = 21_000, max_fee: int = 1_000_000, tip: int = 0,
             expiry: int = U64MAX) -> list[str]:
    return [
        f"ui: chain={chain}",
        "ui: kind=transfer",
        f"ui: signer={signer}",
        f"ui: nonce={nonce}",
        f"ui: to={to}",
        f"ui: amount={amount}",
        f"ui: gas={gas}",
        f"ui: max_fee={max_fee}",
        f"ui: tip={tip}",
        f"ui: expiry={expiry}",
    ]


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

    payload = transfer_payload(a1["raw"], 1_000)
    body = unsigned_body(a0, CHAIN, 0, KIND_TRANSFER, payload)
    body_delegate = unsigned_body(a0, CHAIN, 1, KIND_DELEGATE,
                                  STAKE_PREFIX + b"\x00" + a1["raw"] + (5_000_000).to_bytes(8, "little"))
    envelope = g3.envelope(g3.KIND_UNSIGNED, body)
    sig = sig_hex(a0, body, CHAIN)

    view = ui_lines(chain=CHAIN, signer=a0["addr"], nonce=0,
                    to=a1["addr"], amount=1_000)

    def link(*params: str) -> str:
        return "auralite://sign?" + "&".join(params)

    def argv(human: str, origin: str, *, env: bool = True, uri: str | None = None,
             extra: list[str] | None = None) -> tuple[list[str], dict[str, str]]:
        files = dict(base_files)
        args = ["web-sign", "--keystore", "{sandbox}/ks",
                "--password-file", "{sandbox}/pw", "--account", "0",
                "--chain", CHAIN, "--human", human, "--origin", origin]
        if uri is not None:
            args += ["--deep-link", uri]
        else:
            if env:
                files["u.env"] = envelope + "\n"
            args += ["--in", "{sandbox}/u.env"]
        args += (extra or [])
        args += ["--out", "{sandbox}/s.env"]
        return args, files

    def add(name: str, args: list[str], files: dict[str, str], *,
            exit_code: int = 0, out: list[str] | None = None,
            err: list[str] | None = None) -> None:
        cases.append(g2.case(name, args, files=files, setup=setup,
                             exit_code=exit_code, out=out, err=err))

    ok = [UI] + view + ["status=signed", f"sig={sig}"]
    refused_after_view = [UI] + view

    cases: list[dict] = []

    # --- 1: direct envelope, real fields only ---------------------------
    args, files = argv("confirm", TRUSTED_APP)
    add("sign_transfer_real_fields", args, files, out=ok)

    # --- 2: deep link whose claim matches the bytes ---------------------
    uri = link(f"tx={envelope}", f"to={a1['addr']}", "amount=1000")
    args, files = argv("confirm", TRUSTED_APP, env=False, uri=uri)
    add("sign_transfer_matching_claim", args, files, out=ok)

    # --- 3: the claim substitutes the recipient (named control) ---------
    uri = link(f"tx={envelope}", f"to={a2['addr']}", "amount=1000")
    args, files = argv("confirm", TRUSTED_APP, env=False, uri=uri)
    add(NAMED_CONTROL, args, files, exit_code=65, out=refused_after_view,
        err=["error: web: recipient substitution"])

    # --- 4: the claim substitutes the amount ----------------------------
    uri = link(f"tx={envelope}", f"to={a1['addr']}", "amount=999")
    args, files = argv("confirm", TRUSTED_APP, env=False, uri=uri)
    add("ui_amount_substitution", args, files, exit_code=65, out=refused_after_view,
        err=["error: web: amount substitution"])

    # --- 5: malformed deep link ----------------------------------------
    args, files = argv("confirm", TRUSTED_APP, env=False, uri="auralite://sign?tx=")
    add("deep_link_malformed", args, files, exit_code=65, out=[UI],
        err=["error: web: malformed deep link"])

    # --- 6: unknown deep-link parameter ---------------------------------
    uri = link(f"tx={envelope}", "approve=1")
    args, files = argv("confirm", TRUSTED_APP, env=False, uri=uri)
    add("deep_link_unknown_param", args, files, exit_code=65, out=[UI],
        err=["error: web: unknown deep-link parameter"])

    # --- 7: homoglyph lookalike origin ----------------------------------
    uri = link(f"tx={envelope}")
    args, files = argv("confirm", LOOKALIKE, env=False, uri=uri)
    add("origin_lookalike_refused", args, files, exit_code=65, out=[UI],
        err=["error: web: suspicious origin"])

    # --- 8: the other trusted host signs --------------------------------
    args, files = argv("confirm", TRUSTED_WALLET)
    add("origin_trusted_signs", args, files, out=ok)

    # --- 9: blind signing without a view --------------------------------
    args, files = argv("blind", TRUSTED_APP)
    add("blind_ui_sign_refused", args, files, exit_code=65, out=[UI],
        err=["error: web: ui confirmation skipped"])

    # --- 10: delegate is out of scope for v1 ----------------------------
    files = dict(base_files)
    files["u.env"] = g3.envelope(g3.KIND_UNSIGNED, body_delegate) + "\n"
    args = ["web-sign", "--keystore", "{sandbox}/ks",
            "--password-file", "{sandbox}/pw", "--account", "0",
            "--chain", CHAIN, "--human", "confirm", "--origin", TRUSTED_APP,
            "--in", "{sandbox}/u.env", "--out", "{sandbox}/s.env"]
    add("unsupported_kind_refused", args, files, exit_code=65, out=[UI],
        err=["error: web: unsupported transaction kind"])

    if len(cases) != 10 or len({c["name"] for c in cases}) != 10:
        raise ValueError("expected exactly 10 unique web wallet scenarios")
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
