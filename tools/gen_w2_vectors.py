#!/usr/bin/env python3
"""Independent PUBLIC TEST-ONLY generator of the 40 W2 CLI vectors.

Writes `crates/aura-wallet/tests/fixtures/w2_cli_vectors.json`, asserted by
`crates/aura-wallet/tests/w2_cli.rs` (gate: "40 CLI vectors, including
rejection of a tx with a foreign chain-id").

No Rust production code is invoked: BIP-39 (PBKDF2-HMAC-SHA512), SLIP-0010
Ed25519 derivation, bech32 addresses, the C4 wire encoder, BLAKE3 and Ed25519
all come from the independent Python reference stack (`tools/gen_c4_vectors`,
`tools/w1_interop_reference`), cross-checked below against published vectors
and against W1's pinned smoke values. All fixtures are PUBLIC TEST DATA.
"""
from __future__ import annotations

import hashlib
import hmac
import json
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, "/home/user/aura-python-deps")

from tools import gen_c4_vectors as c4  # noqa: E402
from tools.w1_interop_reference import bech32_address  # noqa: E402
from mnemonic import Mnemonic  # noqa: E402

FIXTURE = ROOT / "crates/aura-wallet/tests/fixtures/w2_cli_vectors.json"
MNEMO = Mnemonic("english")

CHAIN = "auralite-test-1"
FOREIGN = "evil-net-9"
ALTERNATE = "other-net"
NOTICE = "PUBLIC TEST DATA ONLY - DO NOT USE FOR FUNDS"
PASSWORD = "correct horse battery staple"
WRONG_PASSWORD = "incorrect horse battery staple"
STAKE_PREFIX = b"AuraLite-Crypto/wallet-stake-v1\x00"
BALLOT_PREFIX = b"AuraLite-Crypto/wallet-ballot-v1\x00"
HASH_CONTEXT = b"AuraLite-Crypto/crypto-v1\x00"

# W1 smoke values, pinned in docs (Key Results) — the generator must agree.
KNOWN_ADDR_0 = "aura1qz7tt7lhvtptdwyj7ukegllk0et4th0l9cd5f2m9"
KNOWN_PK_0 = "811b322f9c28877bf9f543a8e8db1f3c4fd45a6ccc6cadf315499893d49b8299"


def self_check() -> None:
    for length, expected in c4.OFFICIAL_B3.items():
        if c4.blake3_short(bytes(i % 251 for i in range(length))).hex() != expected:
            raise ValueError("pure-Python BLAKE3 failed official vector")
    for secret, public, message, signature in c4.RFC8032:
        key = bytes.fromhex(secret)
        if (
            c4.ed25519_public_key(key).hex() != public
            or c4.ed25519_sign(bytes.fromhex(message), key).hex() != signature
        ):
            raise ValueError("pure-Python Ed25519 failed RFC 8032")
    # domain_message must be HASH_CONTEXT || u64le(len(prefix)) || prefix ||
    # u64le(len(payload)) || payload with prefix = chain || 0 || tag || 0.
    prefix = CHAIN.encode() + b"\x00tx\x00"
    manual = (
        HASH_CONTEXT
        + len(prefix).to_bytes(8, "little")
        + prefix
        + (3).to_bytes(8, "little")
        + b"abc"
    )
    if c4.domain_message(CHAIN, "tx", b"abc") != manual:
        raise ValueError("domain_message does not match the Rust construction")


def seed_from_mnemonic(phrase: str, passphrase: str = "") -> bytes:
    phrase_n = unicodedata.normalize("NFKD", phrase).encode("utf-8")
    salt = ("mnemonic" + unicodedata.normalize("NFKD", passphrase)).encode("utf-8")
    return hashlib.pbkdf2_hmac("sha512", phrase_n, salt, 2048)


def account(seed: bytes, index: int) -> dict:
    ext = hmac.new(b"ed25519 seed", seed, hashlib.sha512).digest()
    for i in (44, 1, 0, 0, index):
        ext = hmac.new(
            ext[32:],
            b"\x00" + ext[:32] + (i | 0x80000000).to_bytes(4, "big"),
            hashlib.sha512,
        ).digest()
    priv = ext[:32]
    pub = c4.ed25519_public_key(priv)
    raw = c4.signer_address(pub)
    return {
        "priv": priv,
        "pub": pub,
        "pub_hex": pub.hex(),
        "raw": raw,
        "raw_hex": raw.hex(),
        "addr": bech32_address(raw),
        "path": f"m/44'/1'/0'/0'/{index}'",
    }


def signed_tx(acct: dict, chain: str, nonce: int, kind: int, payload: bytes,
              gas: int, max_fee: int, tip: int, expiry: int) -> dict:
    case = dict(chain=chain, nonce=nonce, kind=kind, payload=payload, gas=gas,
                max_fee=max_fee, tip=tip, expiry=expiry,
                address=acct["raw"], public_key=acct["pub"])
    unsigned, _ = c4.encode_unsigned(case)
    signing_bytes = c4.SIGNING_CONTEXT + unsigned
    digest = c4.blake3_short(c4.domain_message(chain, "tx", signing_bytes))
    signature = c4.ed25519_sign(digest, acct["priv"])
    case["signature"] = signature
    tx = unsigned + c4.encode_signature(case)
    txid = c4.blake3_short(c4.TXID_CONTEXT + tx)
    return {"txid": txid.hex(), "tx": tx.hex()}


def stake_payload(action_tag: int, validator_raw: bytes, amount: int) -> bytes:
    return STAKE_PREFIX + bytes([action_tag]) + validator_raw + amount.to_bytes(8, "little")


def ballot(acct: dict, chain: str, proposal: int, choice_tag: int, height: int) -> dict:
    body = (
        BALLOT_PREFIX
        + bytes([len(chain)])
        + chain.encode()
        + proposal.to_bytes(8, "little")
        + acct["raw"]
        + bytes([choice_tag])
        + height.to_bytes(8, "little")
    )
    digest = c4.blake3_short(c4.domain_message(chain, "vote", body))
    sig = c4.ed25519_sign(digest, acct["priv"])
    doc = body + acct["pub"] + sig
    return {"ballot_id": hashlib.sha256(doc).hexdigest(), "ballot": doc.hex()}


def ledger(chain: str, height: int, addr: str, balance: int, nonce: int, shuffled: bool = False) -> str:
    account_entry = json.dumps({"balance": balance, "nonce": nonce}, sort_keys=not shuffled)
    if shuffled:
        text = (
            '{\n  "txs": [],\n  "height": %d,\n  "accounts": {\n    "%s": %s\n  },\n  "chain": "%s"\n}\n'
            % (height, addr, account_entry, chain)
        )
    else:
        text = json.dumps(
            {"chain": chain, "height": height,
             "accounts": {addr: {"balance": balance, "nonce": nonce}}, "txs": []},
            indent=2,
        ) + "\n"
    return text


def tx_json(chain: str, signer_hex: str, nonce: int, kind: int, payload_hex: str,
            gas: int, max_fee: int, tip: int, expiry: int,
            drop: str | None = None) -> str:
    doc = {
        "chain": chain, "signer": signer_hex, "nonce": nonce, "kind": kind,
        "payload": payload_hex, "gas": gas, "max_fee": max_fee, "tip": tip,
        "expiry": expiry,
    }
    if drop:
        del doc[drop]
    return json.dumps(doc, indent=2) + "\n"


def case(name: str, argv: list[str], *, files: dict[str, str] | None = None,
         setup: list[list[str]] | None = None, exit_code: int = 0,
         out: list[str] | None = None, err: list[str] | None = None) -> dict:
    return {
        "name": name,
        "argv": argv,
        "files": files or {},
        "setup": setup or [],
        "expect_exit": exit_code,
        "expect_stdout": out or [],
        "expect_stderr": err or [],
    }


def build_cases() -> list[dict]:
    # --- deterministic identities -------------------------------------
    phrase00 = MNEMO.to_mnemonic(bytes(32))                       # abandon ... art
    phraseff = MNEMO.to_mnemonic(bytes([0xFF] * 32))
    phrase12 = " ".join(phrase00.split()[:11] + ["about"])        # canonical 12-word
    garbage = " ".join(["zoo"] * 23 + ["wrong"])                  # 24 words, invalid
    a0 = account(seed_from_mnemonic(phrase00), 0)
    a1 = account(seed_from_mnemonic(phrase00), 1)
    aff = account(seed_from_mnemonic(phraseff), 0)
    apass = account(seed_from_mnemonic(phrase00, "TREZOR"), 0)
    a12 = account(seed_from_mnemonic(phrase12), 0)

    # Self-check the pinned smoke identity.
    if a0["addr"] != KNOWN_ADDR_0 or a0["pub_hex"] != KNOWN_PK_0:
        raise ValueError("derived identity does not match the pinned smoke values")

    pw = PASSWORD + "\n"
    wrong_pw = WRONG_PASSWORD + "\n"
    led_main = ledger(CHAIN, 10, a0["addr"], 1_000_000, 0)
    led_seq = ledger(CHAIN, 10, a0["addr"], 1_000_000, 1)
    led_shuffled = ledger(CHAIN, 10, a0["addr"], 1_000_000, 0, shuffled=True)
    led_foreign = ledger(FOREIGN, 10, a0["addr"], 1_000_000, 0)

    MAX = 18_446_744_073_709_551_615

    def out_new(acct: dict, ks_entry: str, mnemonic: str, status: str = "created",
                chain: str = CHAIN) -> list[str]:
        return [
            f"status={status}",
            f"chain={chain}",
            f"address={acct['addr']}",
            f"path={acct['path']}",
            f"public_key={acct['pub_hex']}",
            f"keystore={{sandbox}}/ks/{ks_entry}",
            "notice=write the mnemonic down; it is shown once",
            f"mnemonic={{{mnemonic}}}",
        ]

    def out_import(acct: dict, ks_entry: str, chain: str = CHAIN) -> list[str]:
        return out_new(acct, ks_entry, "", status="imported", chain=chain)[:-2]

    base_files = {"pw": pw, "ent00": ("00" * 32) + "\n", "entff": ("ff" * 32) + "\n",
                  "m00": phrase00 + "\n", "m12": phrase12 + "\n", "mgarbage": garbage + "\n"}

    def fs(*extra: tuple[str, str]) -> dict[str, str]:
        d = dict(base_files)
        d.update(extra)
        return d

    def new_argv(index: str = "0", entropy: str = "ent00", extra: list[str] | None = None) -> list[str]:
        argv = ["new", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
                "--chain", CHAIN, "--entropy-file", "{sandbox}/" + entropy,
                "--kdf", "testing", "--account", index]
        return argv + (extra or [])

    cases: list[dict] = []
    A = cases.append

    # --- new x5 --------------------------------------------------------
    A(case("new_basic", new_argv(), files=fs(), out=out_new(a0, "0.json", "p00")))
    A(case("new_account_1", new_argv("1"), files=fs(), out=out_new(a1, "1.json", "p00")))
    A(case("new_entropy_ff", new_argv("0", "entff"), files=fs(), out=out_new(aff, "0.json", "pff")))
    A(case("new_exists", new_argv(),
           files=fs(), setup=[new_argv()],
           exit_code=65,
           err=[f"error: refused: keystore entry {{sandbox}}/ks/0.json already exists"]))
    A(case("new_passphrase", new_argv("0", "ent00", ["--mnemonic-passphrase", "TREZOR"]),
           files=fs(), out=out_new(apass, "0.json", "p00")))

    # --- import x5 -----------------------------------------------------
    def import_argv(mnemonic_file: str, index: str = "0", extra: list[str] | None = None) -> list[str]:
        argv = ["import", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
                "--mnemonic-file", "{sandbox}/" + mnemonic_file, "--chain", CHAIN,
                "--kdf", "testing", "--account", index]
        return argv + (extra or [])

    A(case("import_basic", import_argv("m00"), files=fs(), out=out_import(a0, "0.json")))
    A(case("import_bad_profile", import_argv("m00", "0", ["--profile", "nope"]),
           files=fs(), exit_code=64,
           err=["error: usage: --profile must be a known normalization profile id, got nope"]))
    A(case("import_reduced_refused", import_argv("m12"), files=fs(), exit_code=65,
           err=["error: refused: reduced mnemonic refused (12 words, expected 24; "
                "pass --accept-reduced-mnemonic to override)"]))
    A(case("import_reduced_accepted", import_argv("m12", "0", ["--accept-reduced-mnemonic"]),
           files=fs(), out=out_import(a12, "0.json")))
    A(case("import_garbage", import_argv("mgarbage"), files=fs(), exit_code=69,
           err=["error: crypto: invalid BIP-39 mnemonic"]))

    # --- list x3 -------------------------------------------------------
    A(case("list_one", ["list", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw"],
           files=fs(), setup=[new_argv()],
           out=[f"entry=0", f"path={a0['path']}", f"address={a0['addr']}",
                f"public_key={a0['pub_hex']}", "entries=1"]))
    A(case("list_two", ["list", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw"],
           files=fs(), setup=[new_argv("0"), new_argv("1")],
           out=[f"entry=0", f"path={a0['path']}", f"address={a0['addr']}",
                f"public_key={a0['pub_hex']}",
                f"entry=1", f"path={a1['path']}", f"address={a1['addr']}",
                f"public_key={a1['pub_hex']}", "entries=2"]))
    A(case("list_wrong_password",
           ["list", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/wrongpw"],
           files=fs(("wrongpw", wrong_pw)), setup=[new_argv()],
           exit_code=65, err=["error: bad password"]))

    # --- balance x4 ----------------------------------------------------
    def balance_argv(addr: str, chain: str = CHAIN, lg: str = "led.json") -> list[str]:
        return ["balance", "--address", addr, "--ledger", "{sandbox}/" + lg, "--chain", chain]

    A(case("balance_known", balance_argv(a0["addr"]), files=fs(("led.json", led_main)),
           out=[f"address={a0['addr']}", f"chain={CHAIN}", "balance=1000000", "nonce=0"]))
    A(case("balance_unknown", balance_argv(aff["addr"]), files=fs(("led.json", led_main)),
           exit_code=69, err=["error: ledger: unknown address"]))
    A(case("balance_ledger_chain_mismatch", balance_argv(a0["addr"], CHAIN, "ledf.json"),
           files=fs(("ledf.json", led_foreign)), exit_code=65,
           err=[f"error: foreign chain-id: tx={FOREIGN}, wallet={CHAIN}"]))
    A(case("balance_json_order", balance_argv(a0["addr"]), files=fs(("led.json", led_shuffled)),
           out=[f"address={a0['addr']}", f"chain={CHAIN}", "balance=1000000", "nonce=0"]))

    # --- transfer x6 ---------------------------------------------------
    def transfer_argv(*extra: str) -> list[str]:
        return ["transfer", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
                "--ledger", "{sandbox}/led.json", "--chain", CHAIN,
                "--to", a1["addr"], "--amount", "5"] + list(extra)

    t18 = signed_tx(a0, CHAIN, 0, 0, (5).to_bytes(8, "little"), 21000, 100, 0, MAX)
    A(case("transfer_happy", transfer_argv("--max-fee", "100"),
           files=fs(("led.json", led_main)), setup=[new_argv()],
           out=["status=signed", f"chain={CHAIN}", f"from={a0['addr']}", f"to={a1['addr']}",
                "amount=5", "nonce=0", f"txid={t18['txid']}", f"tx={t18['tx']}"]))
    t19 = signed_tx(a0, CHAIN, 1, 0, (5).to_bytes(8, "little"), 21000, 100, 0, MAX)
    A(case("transfer_nonce_one", transfer_argv("--max-fee", "100"),
           files=fs(("led.json", led_seq)), setup=[new_argv()],
           out=["status=signed", f"chain={CHAIN}", f"from={a0['addr']}", f"to={a1['addr']}",
                "amount=5", "nonce=1", f"txid={t19['txid']}", f"tx={t19['tx']}"]))
    A(case("transfer_insufficient", transfer_argv("--amount", "2000000", "--max-fee", "100"),
           files=fs(("led.json", led_main)), setup=[new_argv()],
           exit_code=69, err=["error: ledger: insufficient funds"]))
    A(case("transfer_bad_password",
           ["transfer", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/wrongpw",
            "--ledger", "{sandbox}/led.json", "--chain", CHAIN, "--to", a1["addr"],
            "--amount", "5", "--max-fee", "100"],
           files=fs(("led.json", led_main), ("wrongpw", wrong_pw)), setup=[new_argv()],
           exit_code=65, err=["error: bad password"]))
    A(case("transfer_tip_gt_fee", transfer_argv("--max-fee", "100", "--tip", "200"),
           files=fs(("led.json", led_main)), setup=[new_argv()],
           exit_code=64, err=["error: usage: --tip must not exceed --max-fee"]))
    t23 = signed_tx(a0, CHAIN, 0, 0, (5).to_bytes(8, "little"), 50000, 7, 3, 999)
    A(case("transfer_explicit_fees",
           transfer_argv("--max-fee", "7", "--tip", "3", "--gas", "50000", "--expiry", "999"),
           files=fs(("led.json", led_main)), setup=[new_argv()],
           out=["status=signed", f"chain={CHAIN}", f"from={a0['addr']}", f"to={a1['addr']}",
                "amount=5", "nonce=0", f"txid={t23['txid']}", f"tx={t23['tx']}"]))

    # --- sign x6 -------------------------------------------------------
    def sign_argv(txfile: str, chain: str = CHAIN, account: str = "0") -> list[str]:
        return ["sign", "--tx-file", "{sandbox}/" + txfile, "--keystore", "{sandbox}/ks",
                "--password-file", "{sandbox}/pw", "--chain", chain, "--account", account]

    txj = tx_json(CHAIN, a0["raw_hex"], 0, 0, "0500000000000000", 21000, 100, 0, 100)
    txj_foreign = tx_json(FOREIGN, a0["raw_hex"], 0, 0, "0500000000000000", 21000, 100, 0, 100)
    txj_wrong_signer = tx_json(CHAIN, aff["raw_hex"], 0, 0, "0500000000000000", 21000, 100, 0, 100)
    txj_no_nonce = tx_json(CHAIN, a0["raw_hex"], 0, 0, "0500000000000000", 21000, 100, 0, 100, drop="nonce")
    txj_bad_kind = tx_json(CHAIN, a0["raw_hex"], 0, 9, "0500000000000000", 21000, 100, 0, 100)

    s24 = signed_tx(a0, CHAIN, 0, 0, (5).to_bytes(8, "little"), 21000, 100, 0, 100)
    A(case("sign_happy", sign_argv("tx.json"),
           files=fs(("tx.json", txj)), setup=[new_argv()],
           out=["status=signed", f"chain={CHAIN}", f"txid={s24['txid']}", f"tx={s24['tx']}"]))
    A(case("sign_foreign_chain", sign_argv("txf.json"),
           files=fs(("txf.json", txj_foreign)), setup=[new_argv()],
           exit_code=65,
           err=[f"error: foreign chain-id: tx={FOREIGN}, wallet={CHAIN}"]))
    s26 = signed_tx(a0, FOREIGN, 0, 0, (5).to_bytes(8, "little"), 21000, 100, 0, 100)
    A(case("sign_consistent_foreign", sign_argv("txf.json", FOREIGN),
           files=fs(("txf.json", txj_foreign)), setup=[new_argv()],
           out=["status=signed", f"chain={FOREIGN}", f"txid={s26['txid']}", f"tx={s26['tx']}"]))
    A(case("sign_signer_mismatch", sign_argv("txw.json"),
           files=fs(("txw.json", txj_wrong_signer)), setup=[new_argv()],
           exit_code=65,
           err=[f"error: refused: signer mismatch: tx={aff['raw_hex']}, key={a0['raw_hex']}"]))
    A(case("sign_missing_field", sign_argv("txn.json"),
           files=fs(("txn.json", txj_no_nonce)), setup=[new_argv()],
           exit_code=64, err=["error: usage: tx json: missing nonce"]))
    A(case("sign_bad_kind", sign_argv("txk.json"),
           files=fs(("txk.json", txj_bad_kind)), setup=[new_argv()],
           exit_code=64, err=["error: usage: tx json: unknown kind"]))

    # --- broadcast x4 --------------------------------------------------
    b_tx = signed_tx(a0, CHAIN, 0, 0, (5).to_bytes(8, "little"), 21000, 100, 0, 100)
    tampered = bytearray.fromhex(b_tx["tx"])
    tampered[-1] ^= 0x01
    b_expired = signed_tx(a0, CHAIN, 0, 0, (5).to_bytes(8, "little"), 21000, 100, 0, 5)

    def broadcast_argv(txfile: str, chain: str = CHAIN, lg: str = "led.json") -> list[str]:
        return ["broadcast", "--tx-file", "{sandbox}/" + txfile,
                "--ledger", "{sandbox}/" + lg, "--chain", chain]

    A(case("broadcast_happy", broadcast_argv("tr.tx"),
           files=fs(("led.json", led_main), ("tr.tx", b_tx["tx"] + "\n")),
           out=["status=broadcast", f"chain={CHAIN}", f"txid={b_tx['txid']}", "accepted=true"]))
    A(case("broadcast_tampered_sig", broadcast_argv("trbad.tx"),
           files=fs(("led.json", led_main), ("trbad.tx", bytes(tampered).hex() + "\n")),
           exit_code=65, err=["error: refused: tx signature does not verify"]))
    A(case("broadcast_foreign", broadcast_argv("tr.tx", FOREIGN, "ledf.json"),
           files=fs(("ledf.json", led_foreign), ("tr.tx", b_tx["tx"] + "\n")),
           exit_code=65,
           err=[f"error: foreign chain-id: tx={CHAIN}, wallet={FOREIGN}"]))
    A(case("broadcast_expired", broadcast_argv("trex.tx"),
           files=fs(("led.json", led_main), ("trex.tx", b_expired["tx"] + "\n")),
           exit_code=65, err=["error: refused: tx expired"]))

    # --- stake x4 ------------------------------------------------------
    def stake_argv(action: str, amount: str, *extra: str) -> list[str]:
        return ["stake", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
                "--ledger", "{sandbox}/led.json", "--chain", CHAIN, "--action", action,
                "--validator", a1["addr"], "--amount", amount] + list(extra)

    st_d = signed_tx(a0, CHAIN, 0, 3, stake_payload(0, a1["raw"], 777), 21000, 100, 0, MAX)
    A(case("stake_delegate", stake_argv("delegate", "777", "--max-fee", "100"),
           files=fs(("led.json", led_main)), setup=[new_argv()],
           out=["status=signed", f"chain={CHAIN}", "action=delegate",
                f"validator={a1['addr']}", "amount=777",
                f"txid={st_d['txid']}", f"tx={st_d['tx']}"]))
    st_u = signed_tx(a0, CHAIN, 0, 4, stake_payload(1, a1["raw"], 777), 21000, 100, 0, MAX)
    A(case("stake_undelegate", stake_argv("undelegate", "777", "--max-fee", "100"),
           files=fs(("led.json", led_main)), setup=[new_argv()],
           out=["status=signed", f"chain={CHAIN}", "action=undelegate",
                f"validator={a1['addr']}", "amount=777",
                f"txid={st_u['txid']}", f"tx={st_u['tx']}"]))
    A(case("stake_amount_zero", stake_argv("delegate", "0"),
           files=fs(("led.json", led_main)), setup=[new_argv()],
           exit_code=64, err=["error: usage: --amount must be positive"]))
    A(case("stake_bad_action", stake_argv("redelegate", "777"),
           files=fs(("led.json", led_main)), setup=[new_argv()],
           exit_code=64, err=["error: usage: --action must be delegate or undelegate"]))

    # --- vote x3 -------------------------------------------------------
    b38 = ballot(a0, CHAIN, 7, 0, 5)
    A(case("vote_sign",
           ["vote", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
            "--chain", CHAIN, "--proposal", "7", "--choice", "yes", "--cast-height", "5"],
           files=fs(), setup=[new_argv()],
           out=["status=signed", f"chain={CHAIN}", "proposal=7", f"voter={a0['addr']}",
                "choice=yes", "cast_height=5", f"ballot_id={b38['ballot_id']}",
                f"ballot={b38['ballot']}"]))
    A(case("vote_verify_roundtrip",
           ["vote", "--chain", CHAIN, "--verify-ballot", "{sandbox}/ballot.hex"],
           files=fs(("ballot.hex", b38["ballot"] + "\n")),
           out=["verified=true", f"chain={CHAIN}", "proposal=7", f"voter={a0['addr']}",
                "choice=yes", "cast_height=5", f"ballot_id={b38['ballot_id']}"]))
    A(case("vote_verify_foreign",
           ["vote", "--chain", ALTERNATE, "--verify-ballot", "{sandbox}/ballot.hex"],
           files=fs(("ballot.hex", b38["ballot"] + "\n")),
           exit_code=65,
           err=[f"error: foreign chain-id: tx={CHAIN}, wallet={ALTERNATE}"]))

    return cases


def main() -> None:
    self_check()
    cases = build_cases()
    if len(cases) != 40:
        raise ValueError(f"expected 40 vectors, built {len(cases)}")
    names = [c["name"] for c in cases]
    if len(set(names)) != 40:
        raise ValueError("vector names are not unique")
    doc = {
        "notice": NOTICE,
        "generated_by": "tools/gen_w2_vectors.py",
        "gate": "40 CLI vectors, including rejection of a tx with a foreign chain-id",
        "phrases": {
            "p00": MNEMO.to_mnemonic(bytes(32)),
            "pff": MNEMO.to_mnemonic(bytes([0xFF] * 32)),
        },
        "cases": cases,
    }
    FIXTURE.write_text(json.dumps(doc, indent=2) + "\n")
    print(f"wrote {FIXTURE.relative_to(ROOT)} ({len(cases)} vectors)")


if __name__ == "__main__":
    main()
