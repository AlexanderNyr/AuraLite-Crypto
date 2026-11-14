#!/usr/bin/env python3
"""W3 oracle — 24 CLI vectors for air-gapped offline signing.

Design of record: docs/w3-offline-plan.md (D-W3-1..D-W3-4). Generates
`crates/aura-wallet/tests/fixtures/w3_offline_vectors.json`, asserted
byte-for-byte by `tests/w3_offline.rs`.

Independent implementation: reuses the W2 oracle's self-checked primitives
(BLAKE3, Ed25519 RFC 8032, C2/C4 canonical encoders, bech32, BIP-39/SLIP-0010)
and adds the AURALITE-OFFLINE-V1 envelope and QR framing. PUBLIC TEST DATA
ONLY - DO NOT USE FOR FUNDS.
"""
from __future__ import annotations

import base64
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, "/home/user/aura-python-deps")

from tools import gen_c4_vectors as c4  # noqa: E402
from tools import gen_w2_vectors as g2  # noqa: E402

MAGIC = b"AuraLite-OFFLINE-V1"
KIND_UNSIGNED = 0
KIND_SIGNED = 1
CHAIN = g2.CHAIN
NOTICE = g2.NOTICE

# Smoke pins shared with W1/W2 (Key Results).
KNOWN_ADDR_0 = g2.KNOWN_ADDR_0
ENTROPY_00 = "00" * 32
ENTROPY_FF = "ff" * 32


def b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).decode("ascii").rstrip("=")


def envelope(kind: int, body: bytes) -> str:
    header = MAGIC + bytes([kind]) + len(body).to_bytes(8, "little")
    digest = c4.blake3_short(header + body)
    return b64url(header + body + digest)


def unsigned_body(acct: dict, chain: str, nonce: int, amount: int,
                  gas: int = 21_000, max_fee: int = 1_000_000,
                  tip: int = 0, expiry: int = 2**64 - 1) -> bytes:
    case = dict(chain=chain, nonce=nonce, kind=0,
                payload=amount.to_bytes(8, "little"), gas=gas,
                max_fee=max_fee, tip=tip, expiry=expiry,
                address=acct["raw"], public_key=acct["pub"])
    unsigned, _ = c4.encode_unsigned(case)
    return unsigned


def signed_body(acct: dict, chain: str, nonce: int, amount: int, **kw) -> tuple[bytes, str]:
    r = g2.signed_tx(acct, chain, nonce, 0, amount.to_bytes(8, "little"),
                     kw.get("gas", 21_000), kw.get("max_fee", 1_000_000),
                     kw.get("tip", 0), kw.get("expiry", 2**64 - 1))
    return bytes.fromhex(r["tx"]), r["txid"]


def foreign_domain_body(acct: dict, chain: str, nonce: int, amount: int) -> bytes:
    """Signed under a foreign (domain-less) digest - the named W3 control."""
    unsigned = unsigned_body(acct, chain, nonce, amount)
    signing_bytes = c4.SIGNING_CONTEXT + unsigned
    digest = c4.blake3_short(signing_bytes)
    sig = c4.ed25519_sign(digest, acct["priv"])
    case = dict(chain=chain, nonce=nonce, kind=0,
                payload=amount.to_bytes(8, "little"), gas=21_000,
                max_fee=1_000_000, tip=0, expiry=2**64 - 1,
                address=acct["raw"], public_key=acct["pub"], signature=sig)
    return unsigned + c4.encode_signature(case)


def tampered_sig_body(acct: dict, chain: str, nonce: int, amount: int) -> bytes:
    body, _ = signed_body(acct, chain, nonce, amount)
    flip = bytearray(body)
    flip[-1] ^= 0x01
    return bytes(flip)


def tampered_fields_body(acct: dict, chain: str, nonce: int, amount: int) -> bytes:
    good, _ = signed_body(acct, chain, nonce, amount)
    sig_block = good[len(unsigned_body(acct, chain, nonce, amount)):]
    unsigned = unsigned_body(acct, chain, nonce, amount + 1)
    return unsigned + sig_block


def qr_frame(i: int, n: int, chunk: str) -> str:
    return f"ALO1:{i:02}/{n:02}:{chunk}"


def split_frames(text: str, size: int) -> list[str]:
    chunks = [text[i:i + size] for i in range(0, len(text), size)]
    n = len(chunks)
    return [qr_frame(i, n, chunk) for i, chunk in enumerate(chunks)]


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
    acct0 = g2.account(g2.seed_from_mnemonic(g2.MNEMO.to_mnemonic(bytes(32))), 0)
    assert acct0["addr"] == KNOWN_ADDR_0, acct0["addr"]
    acct1 = g2.account(g2.seed_from_mnemonic(g2.MNEMO.to_mnemonic(bytes(32))), 1)

    addr = acct0["addr"]
    addr1 = acct1["addr"]
    to = addr
    pass_file = "correct horse battery staple\n"
    wrong_pass = "incorrect horse battery staple\n"

    setup_new = [["new", "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
                  "--chain", CHAIN, "--entropy-file", "{sandbox}/entropy", "--kdf", "testing"]]
    files_new = {"pw": pass_file, "entropy": ENTROPY_00}

    u_env = envelope(KIND_UNSIGNED, unsigned_body(acct0, CHAIN, 0, 1000))
    s_body, s_txid = signed_body(acct0, CHAIN, 0, 1000)
    s_env = envelope(KIND_SIGNED, s_body)

    cases: list[dict] = []

    # --- tx-prepare (4) ---
    cases.append(case(
        "prepare_basic",
        ["tx-prepare", "--chain", CHAIN, "--from", addr, "--to", to,
         "--amount", "1000", "--nonce", "0", "--out", "{sandbox}/u.env"],
        expect_stdout=["status=prepared", f"chain={CHAIN}", f"signer={addr}", "frames=0"],
    ))
    qr_text_len = len(u_env)
    frames_64 = (qr_text_len + 63) // 64
    cases.append(case(
        "prepare_qr_chunks",
        ["tx-prepare", "--chain", CHAIN, "--from", addr, "--to", to,
         "--amount", "1000", "--nonce", "0", "--out", "{sandbox}/u.env",
         "--qr-dir", "{sandbox}/qr", "--qr-size", "64"],
        expect_stdout=["status=prepared", f"chain={CHAIN}", f"signer={addr}",
                       f"frames={frames_64}"],
    ))
    cases.append(case(
        "prepare_tip_gt_fee",
        ["tx-prepare", "--chain", CHAIN, "--from", addr, "--to", to,
         "--amount", "1000", "--nonce", "0", "--tip", "5", "--max-fee", "4",
         "--out", "{sandbox}/u.env"],
        expect_exit=64,
        expect_stderr=["error: usage: --tip must not exceed --max-fee"],
    ))
    cases.append(case(
        "prepare_bad_from",
        ["tx-prepare", "--chain", CHAIN, "--from", "not-bech32", "--to", to,
         "--amount", "1000", "--nonce", "0", "--out", "{sandbox}/u.env"],
        expect_exit=64,
        expect_stderr=["error: usage: --from must be bech32"],
    ))

    # --- tx-sign (5) ---
    cases.append(case(
        "sign_offline_basic",
        ["tx-sign", "--in", "{sandbox}/u.env", "--out", "{sandbox}/s.env",
         "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
         "--chain", CHAIN],
        files={"u.env": u_env, **files_new},
        setup=setup_new,
        expect_stdout=["status=signed", f"chain={CHAIN}", f"txid={s_txid}", "frames=0"],
    ))
    cases.append(case(
        "sign_wrong_password",
        ["tx-sign", "--in", "{sandbox}/u.env", "--out", "{sandbox}/s.env",
         "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw-bad",
         "--chain", CHAIN],
        files={"u.env": u_env, "pw": pass_file, "pw-bad": wrong_pass,
               "entropy": ENTROPY_00},
        setup=setup_new,
        expect_exit=65,
        expect_stderr=["error: bad password"],
    ))
    u_env_acct1 = envelope(KIND_UNSIGNED, unsigned_body(acct1, CHAIN, 0, 1000))
    cases.append(case(
        "sign_signer_mismatch",
        ["tx-sign", "--in", "{sandbox}/u.env", "--out", "{sandbox}/s.env",
         "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
         "--chain", CHAIN],
        files={"u.env": u_env_acct1, **files_new},
        setup=setup_new,
        expect_exit=65,
        expect_stderr=[f"error: refused: signer mismatch: tx={acct1['raw_hex']}, key={acct0['raw_hex']}"],
    ))
    bad_checksum = u_env[:-6] + ("AAAAAA" if u_env[-6:] != "AAAAAA" else "BBBBBB")
    cases.append(case(
        "sign_bad_checksum",
        ["tx-sign", "--in", "{sandbox}/u.env", "--out", "{sandbox}/s.env",
         "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
         "--chain", CHAIN],
        files={"u.env": bad_checksum, **files_new},
        setup=setup_new,
        expect_exit=65,
        expect_stderr=["error: envelope: checksum mismatch"],
    ))
    u_env_foreign = envelope(KIND_UNSIGNED, unsigned_body(acct0, "other-net", 0, 1000))
    cases.append(case(
        "sign_chain_mismatch",
        ["tx-sign", "--in", "{sandbox}/u.env", "--out", "{sandbox}/s.env",
         "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
         "--chain", CHAIN],
        files={"u.env": u_env_foreign, **files_new},
        setup=setup_new,
        expect_exit=65,
        expect_stderr=[f"error: foreign chain-id: tx=other-net, wallet={CHAIN}"],
    ))

    # --- tx-verify (6) ---
    cases.append(case(
        "verify_happy",
        ["tx-verify", "--in", "{sandbox}/s.env", "--chain", CHAIN],
        files={"s.env": s_env},
        expect_stdout=["status=verified", f"chain={CHAIN}", f"txid={s_txid}", f"signer={addr}"],
    ))
    fd_env = envelope(KIND_SIGNED, foreign_domain_body(acct0, CHAIN, 0, 1000))
    cases.append(case(
        "verify_foreign_domain",
        ["tx-verify", "--in", "{sandbox}/s.env", "--chain", CHAIN],
        files={"s.env": fd_env},
        expect_exit=65,
        expect_stderr=["error: signature verification failed"],
    ))
    ts_env = envelope(KIND_SIGNED, tampered_sig_body(acct0, CHAIN, 0, 1000))
    cases.append(case(
        "verify_tampered_sig",
        ["tx-verify", "--in", "{sandbox}/s.env", "--chain", CHAIN],
        files={"s.env": ts_env},
        expect_exit=65,
        expect_stderr=["error: signature verification failed"],
    ))
    tf_env = envelope(KIND_SIGNED, tampered_fields_body(acct0, CHAIN, 0, 1000))
    cases.append(case(
        "verify_tampered_fields",
        ["tx-verify", "--in", "{sandbox}/s.env", "--chain", CHAIN],
        files={"s.env": tf_env},
        expect_exit=65,
        expect_stderr=["error: signature verification failed"],
    ))
    u_env_2000 = envelope(KIND_UNSIGNED, unsigned_body(acct0, CHAIN, 0, 2000))
    cases.append(case(
        "verify_against_mismatch",
        ["tx-verify", "--in", "{sandbox}/s.env", "--chain", CHAIN,
         "--against", "{sandbox}/u2.env"],
        files={"s.env": s_env, "u2.env": u_env_2000},
        expect_exit=65,
        expect_stderr=["error: unsigned mismatch"],
    ))
    cases.append(case(
        "verify_foreign_chain",
        ["tx-verify", "--in", "{sandbox}/s.env", "--chain", "other-net"],
        files={"s.env": s_env},
        expect_exit=65,
        expect_stderr=[f"error: foreign chain-id: tx={CHAIN}, wallet=other-net"],
    ))

    # --- qr (5) ---
    s_frames = split_frames(s_env, 64)
    n_s = len(s_frames)
    qr_files = {f"{i:03}.txt": frame + "\n" for i, frame in enumerate(s_frames)}
    cases.append(case(
        "qr_roundtrip",
        ["qr-join", "--in-dir", "{sandbox}/qr", "--out", "{sandbox}/joined.env"],
        files={f"qr/{k}": v for k, v in qr_files.items()},
        expect_stdout=["status=joined", f"frames={n_s}"],
    ))
    cases.append(case(
        "qr_missing_chunk",
        ["qr-join", "--in-dir", "{sandbox}/qr", "--out", "{sandbox}/joined.env"],
        files={"qr/000.txt": s_frames[0] + "\n",
               "qr/001.txt": s_frames[2] + "\n"},
        expect_exit=65,
        expect_stderr=["error: qr: missing chunk"],
    ))
    cases.append(case(
        "qr_duplicate_chunk",
        ["qr-join", "--in-dir", "{sandbox}/qr", "--out", "{sandbox}/joined.env"],
        files={f"qr/{k}": v for k, v in qr_files.items()} | {"qr/003.txt": s_frames[0] + "\n"},
        expect_exit=65,
        expect_stderr=["error: qr: duplicate chunk"],
    ))
    # foreign chunk: frame 1 replaced by frame 1 of a different envelope of
    # the same length (same chunk sizes) - joins to a corrupt envelope.
    s_body_b, _ = signed_body(acct0, CHAIN, 0, 2000)
    s_env_b = envelope(KIND_SIGNED, s_body_b)
    assert len(s_env_b) == len(s_env), (len(s_env_b), len(s_env))
    b_frames = split_frames(s_env_b, 64)
    assert len(b_frames) == n_s
    foreign_files = {f"qr/{k}": v for k, v in qr_files.items()}
    foreign_files["qr/001.txt"] = b_frames[1] + "\n"
    cases.append(case(
        "qr_foreign_chunk",
        ["qr-join", "--in-dir", "{sandbox}/qr", "--out", "{sandbox}/joined.env"],
        files=foreign_files,
        expect_exit=65,
        expect_stderr=["error: envelope: checksum mismatch"],
    ))
    cases.append(case(
        "qr_bad_frame",
        ["qr-join", "--in-dir", "{sandbox}/qr", "--out", "{sandbox}/joined.env"],
        files={"qr/000.txt": "ALO1:xx garbage\n"},
        expect_exit=65,
        expect_stderr=["error: qr: bad frame"],
    ))

    # --- air-gapped end-to-end (4) ---
    cases.append(case(
        "e2e_usb",
        ["tx-verify", "--in", "{sandbox}/s.env", "--chain", CHAIN,
         "--against", "{sandbox}/u.env"],
        files={**files_new},
        setup=setup_new + [
            ["tx-prepare", "--chain", CHAIN, "--from", addr, "--to", to,
             "--amount", "1000", "--nonce", "0", "--out", "{sandbox}/u.env"],
            ["tx-sign", "--in", "{sandbox}/u.env", "--out", "{sandbox}/s.env",
             "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
             "--chain", CHAIN],
        ],
        expect_stdout=["status=verified", f"chain={CHAIN}", f"txid={s_txid}", f"signer={addr}"],
    ))
    cases.append(case(
        "e2e_qr",
        ["tx-verify", "--in", "{sandbox}/s.env", "--chain", CHAIN,
         "--against", "{sandbox}/u.env"],
        files={**files_new},
        setup=setup_new + [
            ["tx-prepare", "--chain", CHAIN, "--from", addr, "--to", to,
             "--amount", "1000", "--nonce", "0", "--out", "{sandbox}/drop.env",
             "--qr-dir", "{sandbox}/qr", "--qr-size", "64"],
            ["qr-join", "--in-dir", "{sandbox}/qr", "--out", "{sandbox}/u.env"],
            ["tx-sign", "--in", "{sandbox}/u.env", "--out", "{sandbox}/drop2.env",
             "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
             "--chain", CHAIN, "--qr-dir", "{sandbox}/qr2", "--qr-size", "64"],
            ["qr-join", "--in-dir", "{sandbox}/qr2", "--out", "{sandbox}/s.env"],
        ],
        expect_stdout=["status=verified", f"chain={CHAIN}", f"txid={s_txid}", f"signer={addr}"],
    ))
    ledger_json = g2.ledger(CHAIN, 10, addr, 10_000_000, 0)
    cases.append(case(
        "e2e_broadcast",
        ["broadcast", "--tx-file", "{sandbox}/s.env", "--ledger", "{sandbox}/l.json",
         "--chain", CHAIN],
        files={"l.json": ledger_json, **files_new},
        setup=setup_new + [
            ["tx-prepare", "--chain", CHAIN, "--from", addr, "--to", to,
             "--amount", "1000", "--nonce", "0", "--out", "{sandbox}/u.env"],
            ["tx-sign", "--in", "{sandbox}/u.env", "--out", "{sandbox}/s.env",
             "--keystore", "{sandbox}/ks", "--password-file", "{sandbox}/pw",
             "--chain", CHAIN],
        ],
        expect_stdout=["status=broadcast", f"chain={CHAIN}", f"txid={s_txid}", "accepted=true"],
    ))
    cases.append(case(
        "verify_unsigned_rejected",
        ["tx-verify", "--in", "{sandbox}/u.env", "--chain", CHAIN],
        files={"u.env": u_env},
        expect_exit=65,
        expect_stderr=["error: envelope: expected signed"],
    ))

    names = [c["name"] for c in cases]
    assert len(cases) == 24, f"gate requires 24 vectors, got {len(cases)}"
    assert len(set(names)) == 24, names
    return cases


def main() -> None:
    out = {
        "gate": "air-gapped scenario end-to-end; NC: signature with a foreign domain -> reject",
        "notice": NOTICE,
        "cases": build_cases(),
    }
    path = ROOT / "crates/aura-wallet/tests/fixtures/w3_offline_vectors.json"
    path.write_text(json.dumps(out, indent=1) + "\n", encoding="utf-8")
    print(f"wrote {path.relative_to(ROOT)} ({len(out['cases'])} vectors)")


if __name__ == "__main__":
    main()
