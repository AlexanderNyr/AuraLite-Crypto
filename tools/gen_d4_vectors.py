#!/usr/bin/env python3
"""Generate the D4 JSON-RPC contract fixture from independent wire examples."""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates/aura-rpc/tests/fixtures/d4_json_rpc.txt"

vectors: list[tuple[str, bytes, str]] = []

def add(name: str, request: object | bytes, expected: str) -> None:
    raw = request if isinstance(request, bytes) else json.dumps(request, separators=(",", ":")).encode()
    vectors.append((name, raw, expected))

# Each public D4 method receives a well-formed contract request.
for i, method in enumerate(("submit_tx", "get_balance", "get_tx", "get_block", "call", "estimate_gas"), 1):
    add(f"method-{method}", {"jsonrpc":"2.0", "id":i, "method":method, "params":{}}, "error:-32004" if method == "call" else "ok")

# ID, params, notification, and batch behavior.
add("string-id", {"jsonrpc":"2.0", "id":"request-a", "method":"get_block", "params":None}, "ok")
add("null-id", {"jsonrpc":"2.0", "id":None, "method":"get_block"}, "ok")
add("array-params", {"jsonrpc":"2.0", "id":8, "method":"estimate_gas", "params":[]}, "ok")
add("object-params", {"jsonrpc":"2.0", "id":9, "method":"get_balance", "params":{"address":"aura1test"}}, "ok")
add("notification-balance", {"jsonrpc":"2.0", "method":"get_balance", "params":{}}, "none")
add("notification-submit", {"jsonrpc":"2.0", "method":"submit_tx", "params":{}}, "none")
add("notification-error-suppressed", {"jsonrpc":"2.0", "method":"call", "params":{}}, "none")
add("notification-unknown-suppressed", {"jsonrpc":"2.0", "method":"not_a_method"}, "none")
add("batch-mixed", [{"jsonrpc":"2.0","id":1,"method":"get_block"},{"jsonrpc":"2.0","method":"get_block"}], "batch:1")
add("batch-two-responses", [{"jsonrpc":"2.0","id":1,"method":"get_block"},{"jsonrpc":"2.0","id":2,"method":"estimate_gas"}], "batch:2")
add("batch-all-notifications", [{"jsonrpc":"2.0","method":"get_block"}], "none")
add("batch-empty", [], "error:-32600")
add("unknown-method", {"jsonrpc":"2.0","id":10,"method":"unknown"}, "error:-32601")
add("subscribe-http", {"jsonrpc":"2.0","id":11,"method":"subscribe"}, "error:-32005")
add("parse-error-empty", b"", "error:-32700")
add("parse-error-truncated", b"{\"jsonrpc\":", "error:-32700")
add("parse-error-utf8", b"\xff\xfe", "error:-32700")
add("invalid-scalar", "true", "error:-32600")
add("invalid-array-empty-item", [1], "batch:1")
add("missing-version", {"id":1,"method":"get_block"}, "error:-32600")
add("wrong-version", {"jsonrpc":"1.0","id":1,"method":"get_block"}, "error:-32600")
add("missing-method", {"jsonrpc":"2.0","id":1}, "error:-32600")
add("method-not-string", {"jsonrpc":"2.0","id":1,"method":7}, "error:-32600")
add("invalid-id-object", {"jsonrpc":"2.0","id":{},"method":"get_block"}, "error:-32600")
add("invalid-id-array", {"jsonrpc":"2.0","id":[],"method":"get_block"}, "error:-32600")
add("invalid-id-fraction", {"jsonrpc":"2.0","id":1.5,"method":"get_block"}, "error:-32600")
add("params-string", {"jsonrpc":"2.0","id":1,"method":"get_block","params":"x"}, "error:-32600")
add("params-number", {"jsonrpc":"2.0","id":1,"method":"get_block","params":4}, "error:-32600")
add("unexpected-member", {"jsonrpc":"2.0","id":1,"method":"get_block","extra":True}, "error:-32600")
add("batch-too-large", [{"jsonrpc":"2.0","id":i,"method":"get_block"} for i in range(33)], "error:-32600")

# Deterministic repetitions vary IDs and method/params combinations while
# independently pinning successful JSON-RPC envelope handling.
methods = ["get_block", "get_balance", "get_tx", "estimate_gas", "submit_tx"]
for n in range(14):
    method = methods[n % len(methods)]
    params = {"height": n} if method == "get_block" else {"address": f"aura1vector{n}"} if method == "get_balance" else {"txid": f"{n:064x}"} if method == "get_tx" else {"kind":"transfer"} if method == "estimate_gas" else {"tx":"0x00"}
    add(f"repeat-{n:02}", {"jsonrpc":"2.0", "id":100+n, "method":method, "params":params}, "ok")

assert len(vectors) == 50, len(vectors)
lines = ["# D4 JSON-RPC 2.0 wire-contract vectors", "# request_hex is the exact HTTP JSON body; expected is ok, none, batch:N, or error:CODE.", ""]
for name, request, expected in vectors:
    lines.extend([f"vector={name}", f"request_hex={request.hex()}", f"expected={expected}", ""])
OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text("\n".join(lines), encoding="utf-8")
print(f"wrote {OUT.relative_to(ROOT)} ({len(vectors)} vectors)")
