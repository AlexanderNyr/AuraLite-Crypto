#!/usr/bin/env python3
"""D4 mutation controls; every mutation is killed by one exact Rust lib unit test."""
from __future__ import annotations

import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RPC = ROOT / "crates/aura-rpc/src/lib.rs"
SERVER = ROOT / "crates/aura-rpc/src/server.rs"
CASES = [
    ("NC-1 JSON-RPC version must be exactly 2.0", RPC,
     'version != Some(Value::String("2.0".to_owned()))', "false",
     "tests::malformed_and_invalid_requests_are_distinct"),
    ("NC-2 notifications must not receive response bodies", RPC,
     "if !id_present {", "if false {",
     "tests::notifications_and_batch_follow_protocol_semantics"),
    ("NC-3 JSON-RPC batches are bounded to 32 items", RPC,
     "requests.len() > 32", "requests.len() > 64",
     "tests::batch_limit_is_enforced"),
    ("NC-4 HTTP request body limit is enforced", SERVER,
     "if length > MAX_HTTP_BODY {", "if false {",
     "server::tests::http_rejects_oversize_body_before_reading_payload"),
    ("NC-5 WebSocket handshake uses the RFC6455 GUID", SERVER,
     '"258EAFA5-E914-47DA-95CA-C5AB0DC85B11"', '"00000000-0000-0000-0000-000000000000"',
     "server::tests::websocket_accept_matches_rfc6455_example"),
    ("NC-6 WebSocket client frames must be masked", SERVER,
     "|| !masked", "|| false",
     "server::tests::client_websocket_frames_must_be_masked"),
]


def run(test: str) -> tuple[int, str]:
    command = ["cargo", "test", "-p", "aura-rpc", "--lib", "--", "--exact", test]
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)
    return result.returncode, result.stdout + result.stderr


def matches_test(output: str, test: str, status: str) -> bool:
    return re.search(rf"test\s+{re.escape(test)}\s+\.\.\.\s+{status}", output) is not None


def main() -> int:
    originals = {path: path.read_text(encoding="utf-8") for _, path, *_ in CASES}
    passed = 0
    print("D4 NEGATIVE CONTROLS (exact Rust library unit tests)")
    for label, path, old, new, test in CASES:
        current = path.read_text(encoding="utf-8")
        if old not in current:
            print(f"{label}: FAIL (mutation target missing)")
            return 1
        code, output = run(test)
        if code or not matches_test(output, test, "ok"):
            print(f"{label}: FAIL (exact unit test not green before mutation)")
            print(output[-3000:])
            return 1
        path.write_text(current.replace(old, new, 1), encoding="utf-8")
        try:
            code, output = run(test)
        finally:
            path.write_text(originals[path], encoding="utf-8")
        if code and matches_test(output, test, "FAILED"):
            passed += 1
            print(f"{label}: PASS")
        else:
            print(f"{label}: FAIL (expected {test} to fail)")
            print(output[-3000:])
    print(f"RESULT: {passed}/{len(CASES)} negative controls PASS")
    return 0 if passed == len(CASES) else 1


if __name__ == "__main__":
    raise SystemExit(main())
