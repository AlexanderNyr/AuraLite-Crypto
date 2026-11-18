#!/usr/bin/env python3
"""H2 oracle: 50 crypto vectors + side-channel checklist extraction.

Independent implementations (nothing is imported from the Rust code under
review):

* Ed25519 (RFC 8032) from `tools/gen_c4_vectors.py` — pure Python,
  C4-validated against RFC 8032 test vectors;
* BLAKE3 via the python `blake3` binding (install `blake3`, or run with
  `PYTHONPATH=~/aura-python-deps`);
* `domain_message` assembled from the written spec (D-H2-2):
  `HASH_CONTEXT || u64le(prefix_len) || prefix || u64le(payload_len) || payload`,
  `prefix = chain_id || 0x00 || domain_tag || 0x00`.

Outputs (deterministic, do not edit by hand):
  crates/aura-crypto/tests/fixtures/h2_vectors.txt   50 vector blocks
  tools/fixtures/h2_review_checklist.json            pinned checklist snapshot

Usage:
  python3 tools/gen_h2_vectors.py            (re)write both fixtures
  python3 tools/gen_h2_vectors.py --check    exit 0 = no drift, 64 = drift
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from tools import gen_c4_vectors as c4  # noqa: E402

try:
    import blake3  # noqa: E402
except ImportError:  # pragma: no cover - environment problem
    sys.exit("error: oracle needs the python blake3 binding (pip install blake3)")

VEC_FIXTURE = ROOT / "crates" / "aura-crypto" / "tests" / "fixtures" / "h2_vectors.txt"
CHECKLIST_FIXTURE = ROOT / "tools" / "fixtures" / "h2_review_checklist.json"
PLAN_DOC = ROOT / "docs" / "h2-crypto-review-plan.md"

HASH_CONTEXT = b"AuraLite-Crypto/crypto-v1\x00"
SEED_PREFIX = b"AuraLite-Crypto/h2-crypto-vectors-v1/seed/"
DOMAINS = ("tx", "vote", "block", "contract", "genesis")
CHAINS = ("aura-test-1", "aura-test-2")
# Ed25519 group order L = 2^252 + 27742317777372353535851937790883648493.
GROUP_ORDER = 2**252 + 27742317777372353535851937790883648493


def seed_for(index: int) -> bytes:
    return hashlib.sha256(SEED_PREFIX + index.to_bytes(4, "little")).digest()


def domain_message(chain: str, tag: str, payload: bytes) -> bytes:
    prefix = chain.encode() + b"\x00" + tag.encode() + b"\x00"
    out = bytearray(HASH_CONTEXT)
    out += len(prefix).to_bytes(8, "little")
    out += prefix
    out += len(payload).to_bytes(8, "little")
    out += payload
    return bytes(out)


def hash_domain(chain: str, tag: str, payload: bytes) -> bytes:
    return blake3.blake3(domain_message(chain, tag, payload)).digest()


def payload_for(index: int) -> bytes:
    return f"h2-crypto-vector-{index:02d}".encode() + bytes([index & 0xFF])


def add_sign_verify(vecs: list[dict], label: str, index: int, chain: str,
                    tag: str, payload: bytes, kind: str = "sign_verify") -> dict:
    seed = seed_for(index)
    digest = hash_domain(chain, tag, payload)
    rec = {
        "vector": label,
        "kind": kind,
        "seed": seed.hex(),
        "chain": chain,
        "domain": tag,
        "payload_hex": payload.hex(),
        "pubkey": c4.ed25519_public_key(seed).hex(),
        "digest": digest.hex(),
        "sig": c4.ed25519_sign(digest, seed).hex(),
    }
    vecs.append(rec)
    return rec


def build_vectors() -> list[dict]:
    vecs: list[dict] = []

    # v01-v10: sign/verify, five domains on each chain.
    for n in range(1, 11):
        i = n - 1
        chain = CHAINS[i // 5]
        tag = DOMAINS[i % 5]
        add_sign_verify(vecs, f"v{n:02d}", n - 1, chain, tag, payload_for(n))

    # v11-v15: keygen only.
    for n in range(11, 16):
        seed = seed_for(n - 1)
        vecs.append({
            "vector": f"v{n:02d}", "kind": "keygen",
            "seed": seed.hex(),
            "pubkey": c4.ed25519_public_key(seed).hex(),
        })

    # v16-v20: wrong domain -> reject.
    for n in range(16, 21):
        i = n - 1
        tag = DOMAINS[(n - 16) % 5]
        other = DOMAINS[(n - 16 + 1) % 5]
        rec = add_sign_verify(vecs, f"v{n:02d}", i, CHAINS[0], tag,
                              payload_for(n), kind="wrong_domain")
        rec["other_domain"] = other

    # v21-v25: cross chain -> reject.
    for n in range(21, 26):
        i = n - 1
        tag = DOMAINS[(n - 21) % 5]
        rec = add_sign_verify(vecs, f"v{n:02d}", i, CHAINS[0], tag,
                              payload_for(n), kind="cross_chain")
        rec["other_chain"] = CHAINS[1]

    # v26-v30: tampered payload -> reject.
    for n in range(26, 31):
        i = n - 1
        tag = DOMAINS[(n - 26) % 5]
        payload = payload_for(n)
        rec = add_sign_verify(vecs, f"v{n:02d}", i, CHAINS[1], tag, payload,
                              kind="tampered_payload")
        tampered = bytearray(payload)
        tampered[len(tampered) // 2] ^= 0x40
        rec["other_payload_hex"] = bytes(tampered).hex()

    # v31-v35: wrong key -> reject.
    for n in range(31, 36):
        i = n - 1
        tag = DOMAINS[(n - 31) % 5]
        rec = add_sign_verify(vecs, f"v{n:02d}", i, CHAINS[0], tag,
                              payload_for(n), kind="wrong_key")
        rec["other_pubkey"] = c4.ed25519_public_key(seed_for(n + 40)).hex()

    # v36-v40: high-S malleation (R, S+L) -> NonCanonicalSignature.
    for n in range(36, 41):
        i = n - 1
        tag = DOMAINS[(n - 36) % 5]
        rec = add_sign_verify(vecs, f"v{n:02d}", i, CHAINS[1], tag,
                              payload_for(n), kind="high_s")
        sig = bytes.fromhex(rec["sig"])
        s = int.from_bytes(sig[32:], "little")
        assert s < GROUP_ORDER, "oracle bug: produced non-canonical S"
        twin = s + GROUP_ORDER
        assert twin < 2**256, "oracle bug: S+L overflow"
        rec["sig_high_s"] = (sig[:32] + twin.to_bytes(32, "little")).hex()

    # v41-v45: digest/layout pin.
    for n in range(41, 46):
        i = n - 1
        tag = DOMAINS[(n - 41) % 5]
        chain = CHAINS[(n - 41) % 2]
        payload = payload_for(n)
        vecs.append({
            "vector": f"v{n:02d}", "kind": "digest",
            "chain": chain, "domain": tag,
            "payload_hex": payload.hex(),
            "digest": hash_domain(chain, tag, payload).hex(),
        })

    # v46-v50: edge payloads (empty, 0x00, 0xff, mixed/NUL, 1 KiB pattern).
    edges = [
        b"",
        b"\x00",
        b"\xff",
        b"\x00\x00\x00\xff\x61\x00\x62\xff",
        bytes((i * 7 + 3) & 0xFF for i in range(1024)),
    ]
    for n, payload in zip(range(46, 51), edges):
        i = n - 1
        tag = DOMAINS[(n - 46) % 5]
        chain = CHAINS[(n - 46) % 2]
        add_sign_verify(vecs, f"v{n:02d}", i, chain, tag, payload, kind="edge")

    assert len(vecs) == 50, f"oracle bug: {len(vecs)} vectors"
    return vecs


def render_vectors(vecs: list[dict]) -> str:
    lines = [
        "# H2 golden vectors — crypto review (exactly 50).",
        "#",
        "# Expectation block: tools/gen_h2_vectors.py (independent oracle:",
        "# pure-Python RFC 8032 (gen_c4_vectors) + python blake3 + a from-spec",
        "# domain_message assembler). Seeds: seed_i = SHA-256(",
        '# "AuraLite-Crypto/h2-crypto-vectors-v1/seed/" || u32le(i)).',
        "# Do not edit by hand.",
        "",
        f"hash_context={HASH_CONTEXT.hex()}",
        "vector_count=50",
        "",
    ]
    order = ["vector", "kind", "seed", "chain", "domain", "other_domain",
             "other_chain", "payload_hex", "other_payload_hex", "pubkey",
             "other_pubkey", "digest", "sig", "sig_high_s"]
    for rec in vecs:
        for key in order:
            if key in rec:
                lines.append(f"{key}={rec[key]}")
        lines.append("")
    return "\n".join(lines)


def parse_checklist(text: str) -> list[dict]:
    section = re.search(r"^## 3\. Side-channel checklist\n(.*?)(?=^## |\Z)",
                        text, re.M | re.S)
    if not section:
        sys.exit("error: oracle: checklist section missing from plan doc")
    items = []
    for m in re.finditer(
            r"^### (H2-SC\d\d) — ([^\n]*)\n(.*?)(?=^### |^## |\Z)",
            section.group(1), re.M | re.S):
        rec = {"id": m.group(1), "title": " ".join(m.group(2).split())}
        for line in m.group(3).splitlines():
            if line.startswith("- "):
                key, _, val = line[2:].partition(": ")
                rec[key.strip().lower()] = " ".join(val.split())
        items.append(rec)
    return items


def render_checklist(items: list[dict]) -> str:
    return json.dumps({"phase": "H2", "items": items},
                      indent=2, sort_keys=True) + "\n"


def main(argv: list[str]) -> int:
    check = len(argv) == 2 and argv[1] == "--check"
    vecs = build_vectors()
    items = parse_checklist(PLAN_DOC.read_text(encoding="utf-8"))
    if len(items) != 10:
        sys.exit(f"error: oracle: expected 10 checklist items, got {len(items)}")
    outputs = {
        VEC_FIXTURE: render_vectors(vecs),
        CHECKLIST_FIXTURE: render_checklist(items),
    }
    drift = []
    for path, content in outputs.items():
        current = path.read_text(encoding="utf-8") if path.exists() else None
        if current != content:
            drift.append(path.name)
            if not check:
                path.write_text(content, encoding="utf-8")
                print(f"wrote {path.relative_to(ROOT)}")
    if check:
        if drift:
            print(f"ORACLE: DRIFT in {', '.join(drift)}")
            return 64
        print("ORACLE: OK — 50 vectors + 10 checklist items, no drift")
        return 0
    print(f"ORACLE: OK — {len(vecs)} vectors, {len(items)} checklist items")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
