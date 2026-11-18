#!/usr/bin/env python3
"""H3 negative controls (docs/h3-fuzz-plan.md §5).

Each control injects a decoder bug (one source mutation at a unique anchor,
restored afterwards) and requires the H3 fuzz oracles (`fuzz/src/h3.rs`) to
go red with `H3: fuzz oracle caught a decoder panic` — the injected bug must
be found by the fuzz oracle. NC-1 is the named control
(`injected bug -> fuzz finds it`).
"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FUZZ_TEST = ["cargo", "test", "--locked", "--manifest-path", "fuzz/Cargo.toml", "--lib"]
EXPECT = "H3: fuzz oracle caught a decoder panic"

CONTROLS = [
    dict(
        id="H3-NC1",
        label="injected bug -> fuzz finds it",
        path="crates/aura-p2p/src/transport.rs",
        anchor="from wire bytes.\n    pub fn decode(bytes: &[u8]) -> Result<Self, P2pError> {",
        inject='\n        if bytes.is_empty() { panic!("h3-injected"); }',
        test="h3::p2p_frame_decoders_are_total",
    ),
    dict(
        id="H3-NC2",
        label="codec decoder panic must be found",
        path="crates/aura-codec/src/codec.rs",
        anchor=(
            "pub fn decode_canonical<T>(bytes: &[u8]) -> Result<T, CodecError>\n"
            "where\n    T: Decode + Encode,\n{"
        ),
        inject='\n    if bytes.len() == 1 && bytes[0] == 0xff { panic!("h3-injected"); }',
        test="h3::codec_decoder_is_total",
    ),
    dict(
        id="H3-NC3",
        label="tx decoder panic must be found",
        path="crates/aura-tx/src/tx.rs",
        anchor=(
            "varints, field order fixed, re-encode must reproduce the input.\n"
            "    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, TxError> {"
        ),
        inject='\n        if bytes.first() == Some(&0xff) { panic!("h3-injected"); }',
        test="h3::tx_decoder_is_total",
    ),
    dict(
        id="H3-NC4",
        label="gossip frame decoder panic must be found",
        path="crates/aura-p2p/src/gossipsub.rs",
        anchor="from canonical wire bytes.\n    pub fn decode(bytes: &[u8]) -> Result<Self, P2pError> {",
        inject='\n        if bytes.len() == 4 { panic!("h3-injected"); }',
        test="h3::p2p_frame_decoders_are_total",
    ),
    dict(
        id="H3-NC5",
        label="wasm verifier panic must be found",
        path="crates/aura-exec/src/sandbox.rs",
        anchor="pub fn gate(&self, wasm: &[u8]) -> Result<(), SandboxError> {",
        inject='\n        if wasm.starts_with(b"\\0asm") { panic!("h3-injected"); }',
        test="h3::wasm_verifier_is_total",
    ),
    dict(
        id="H3-NC6",
        label="rpc parser panic must be found",
        path="crates/aura-rpc/src/lib.rs",
        anchor="pub fn process_http_message(&self, message: &[u8]) -> Option<Vec<u8>> {",
        inject='\n        if message.starts_with(b"{") { panic!("h3-injected"); }',
        test="h3::rpc_parsers_are_total",
    ),
]


def main() -> int:
    failures = 0
    for control in CONTROLS:
        target = ROOT / control["path"]
        original = target.read_text()
        anchor = control["anchor"]
        count = original.count(anchor)
        if count != 1:
            print(f"{control['id']} SETUP-ERROR anchor-count={count} in {control['path']}")
            return 2
        target.write_text(original.replace(anchor, anchor + control["inject"], 1))
        try:
            result = subprocess.run(
                FUZZ_TEST + [control["test"], "--", "--exact"],
                cwd=ROOT,
                capture_output=True,
                text=True,
            )
        finally:
            target.write_text(original)
        output = result.stdout + result.stderr
        red = result.returncode != 0 and EXPECT in output
        restored = target.read_text() == original
        ok = red and restored
        if not ok:
            failures += 1
        status = "PASS" if ok else "FAIL"
        detail = f"red={red} restored={restored}"
        print(f"{control['id']} {status} ({detail}) [{control['label']}]")
    print(f"H3 controls: {len(CONTROLS) - failures}/{len(CONTROLS)} PASS")
    if failures == 0:
        print(f"RESULT: {len(CONTROLS)}/{len(CONTROLS)} negative controls PASS")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
