#!/usr/bin/env python3
"""Six D5 REST/gRPC compatibility controls, each through one exact library unit test."""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
LIB = REPO / "crates/aura-grpc-rest/src/lib.rs"
OPENAPI = REPO / "docs/api/openapi-d5.yaml"
PROTO = REPO / "crates/aura-grpc-rest/proto/aura/api/v1/service.proto"


def exact_test(name: str) -> list[str]:
    return ["cargo", "test", "-p", "aura-grpc-rest", "--lib", name, "--", "--exact"]


def run(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=REPO, text=True, capture_output=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str, test_name: str) -> bool:
    original = path.read_text(encoding="utf-8")
    if old not in original:
        print(f"{label}: FAIL — mutation target not found in {path.relative_to(REPO)}")
        return False
    changed = original.replace(old, new, 1)
    try:
        path.write_text(changed, encoding="utf-8")
        result = run(exact_test(test_name))
        red = result.returncode != 0
        tail = "\n".join((result.stdout + result.stderr).strip().splitlines()[-6:])
        print(f"{label}: {'PASS' if red else 'FAIL'}")
        print(f"      exact unit test {test_name} -> rc={result.returncode}")
        if tail:
            print("      " + tail.replace("\n", "\n      "))
        return red
    finally:
        path.write_text(original, encoding="utf-8")


def main() -> int:
    controls = [
        ("NC-1 REST rejects unknown JSON fields", LIB,
         'if !allowed.contains(&key.as_str()) {', 'if false {',
         "tests::rest_rejects_unknown_json_fields"),
        ("NC-2 v1.1 remains additive version 1.1", LIB,
         'const API_VERSION: &str = "1.1";', 'const API_VERSION: &str = "2.0";',
         "tests::rest_v11_is_additive_and_accepts_v1_requests"),
        ("NC-3 unsupported API versions are rejected", LIB,
         '_ => return Some(error_response(404, -32041, "unsupported API version")),',
         '_ => return Some(error_response(200, -32041, "unsupported API version")),',
         "tests::rest_rejects_unsupported_version"),
        ("NC-4 OpenAPI marks unknown fields closed", OPENAPI,
         "additionalProperties: false", "additionalProperties: true",
         "tests::openapi_documents_both_versions_and_closed_request_schemas"),
        ("NC-5 v1 protobuf field numbers remain stable", PROTO,
         "  uint64 nonce = 3;", "  uint64 nonce = 4;",
         "tests::protobuf_v11_response_decodes_with_the_v1_field_set"),
        ("NC-6 unsupported calls map to gRPC Unimplemented", LIB,
         "-32004 | -32005 => Status::unimplemented(message),",
         "-32004 | -32005 => Status::internal(message),",
         "tests::unsupported_contract_calls_remain_explicit_grpc_errors"),
    ]
    results = [expect_red(*control) for control in controls]
    passed = sum(results)
    print(f"RESULT: {passed}/{len(controls)} negative controls PASS")
    restored = run(["cargo", "test", "-p", "aura-grpc-rest", "--lib"])
    if restored.returncode != 0:
        print("restore check: aura-grpc-rest unit tests remain red")
        print((restored.stdout + restored.stderr)[-4000:])
        return 1
    print("restore check: cargo test -p aura-grpc-rest --lib -> GREEN")
    return 0 if passed == len(controls) else 1


if __name__ == "__main__":
    sys.exit(main())
