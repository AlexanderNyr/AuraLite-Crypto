# H3 — Fuzzing plan (campaign, triage, oracles)

Status: 🚧 IN PROGRESS — H3
Owner: core maintainer
Date: 2026-11-14
Gate: `24 h total, 0 new crashes; NC: injected bug → fuzz finds it`

## 1. Scope and DoD

SECURITY_PLAN row H3: cargo-fuzz targets for **codec, tx decode, RPC, p2p
frames, WASM verifier, wallet keystore**; 24 h total fuzzing with 0 new
crashes; the named control injects a bug and the fuzz oracle must find it.
C9 already shipped the portable stable harness (`aura-fuzz`, 6 areas) and
four libFuzzer sources; H3 owes the remaining coverage, the campaign,
sanitizer runs, coverage measurement and crash triage.

Deliverables:

- **D-H3-1 — target matrix complete.** libFuzzer (`fuzz/`) covers 11 targets
  after this phase: the 9 existing sources plus `fuzz_p2p_frame.rs`
  (`HandshakeHello::decode`, `GossipMessage::decode`) and
  `fuzz_wasm_verify.rs` (`Sandbox::gate`, the static verifier half:
  size → census → compile). The portable harness keeps its 6 areas.
- **D-H3-2 — fuzz oracles.** `fuzz/src/h3.rs`: deterministic totality +
  determinism checks per decoder area over a fixed boundary corpus
  (empty, truncated, length-prefix bombs, wasm magic, JSON braces, `0xff`
  runs). A decoder panic fails the oracle with
  `H3: fuzz oracle caught a decoder panic in … (injected bug found)`.
- **D-H3-3 — campaign runner.** `tools/h3_fuzz_campaign.py`: recorded,
  resumable, append-only campaigns over both harnesses. Every run records
  the exact command, seed, source manifest (before/after), `run.log`
  SHA-256 and exit status into `<out>/result.json`. Crash artifacts go to
  `<out>/crashes/`. Schedules: `smoke` (session evidence) and `full`
  (the 24-hour budget below). Seeds derive from `20261180 + block index`.
- **D-H3-4 — 24-hour budget.** `full` distributes 86,400 s evenly over the
  17 runs (11 libFuzzer targets + 6 portable areas ≈ 5,082 s each). The
  schedule is a pure parameter of the runner: `--schedule full`.
- **D-H3-5 — sanitizers.** libFuzzer runs build with AddressSanitizer
  (cargo-fuzz default). The portable harness is stable-Rust without
  sanitizers (honest limitation, recorded per run).
- **D-H3-6 — coverage measurement.** libFuzzer runs record
  `-print_coverage=1` output in `run.log`; the summary counts executed
  functions per target.
- **D-H3-7 — crash triage.** §4.
- **D-H3-8 — mutation controls** (`tools/nc_h3.py`, 6): §5.

## 2. Target matrix

| area | libFuzzer target | portable area | decoder under test |
| --- | --- | --- | --- |
| codec | `fuzz_codec` | `codec` | `aura_codec::decode_canonical` |
| tx decode | `fuzz_tx` | `tx` | `Tx::from_canonical_bytes` |
| RPC | `fuzz_rpc` | — | `RpcService::process_{http,websocket}_message` |
| p2p frames | `fuzz_p2p_frame` **(new)** | — | `HandshakeHello::decode`, `GossipMessage::decode` |
| WASM verifier | `fuzz_wasm_verify` **(new)** | — | `Sandbox::gate` |
| wallet keystore | `fuzz_keystore_parse`, `fuzz_keystore_decrypt` | — | keystore parse/decrypt |
| state key | `fuzz_state_key` | `state-key` | `StateKey` parse |
| genesis | `fuzz_genesis_json` | `genesis-json` | genesis JSON parse |
| block / merkle | — | `block`, `merkle-proof` | block decode, proof verify |
| scrypt diff | `fuzz_scrypt_differential` | — | fork-vs-upstream differential |
| recovery | `fuzz_recovery_metadata` | — | recovery metadata parse |

## 3. Campaigns and deviation record

- `make fuzz-h3` runs the `smoke` schedule (session evidence: 120 s per
  portable area, 120 s per libFuzzer target when cargo-fuzz is installed,
  fixed seeds). `make fuzz-h3-full` runs the 24-hour `full` schedule.
- **Deviation (2026-11-14, session environment).** The closeout evidence
  records the `smoke` campaign actually executed in this workspace (exact
  seconds per run are in `docs/evidence/h3/inventory.txt` and the campaign
  `result.json`); libFuzzer/ASan runs require `nightly-2026-11-03` +
  `cargo-fuzz` and are recorded as `skipped` when the toolchain is absent,
  never as passes. The 24-hour `full` claim is **not** made here; the
  runner and seeds make it a single parameter change on reference
  infrastructure. Seeds do not make libFuzzer evolution deterministic;
  only the portable harness replays exactly.

## 4. Crash triage

1. Any non-zero exit or crash artifact FAILS the run; the runner never
   overwrites an existing crash directory.
2. Triage order: reproduce with the recorded seed (`aura-fuzz --seed S`) or
   the saved artifact (`cargo fuzz run <target> <artifact>`); minimise;
   classify decoder panic vs OOM vs harness fault; file against the owning
   crate; a fix lands with a regression vector in the crate's golden
   fixture and a new `smoke` run.
3. "0 new crashes" means: no crash artifacts and no non-zero exits in the
   recorded runs — nothing more.

## 5. Gates and controls

- `tools/nc_h3.py` injects decoder bugs (source mutations, restored after
  each control) and requires the fuzz oracles (`fuzz/src/h3.rs`, run via
  `cargo test --locked --manifest-path fuzz/Cargo.toml --lib h3`) to go red
  with `H3: fuzz oracle caught a decoder panic`:
  **NC-1 (named) injected bug → fuzz finds it** (`HandshakeHello::decode`
  panics on empty input), NC-2 codec panic on `0xff`, NC-3 tx panic on
  `0xff` prefix, NC-4 gossip frame panic on 4-byte input, NC-5 wasm gate
  panic on `\0asm` magic, NC-6 RPC panic on `{`.
- Campaign gate: `make fuzz-h3` (smoke, recorded) and `make fuzz-h3-full`
  (24 h). Evidence `docs/evidence/h3/` is regenerated by
  `make evidence-h3` (`report.md`, `gates.txt`, `check.log`, `nc.log`,
  `inventory.txt` cumulative TOTAL: 448, `SHA256SUMS`).
- The fuzz oracles and campaign are deliberately NOT part of `make test`
  (they need a C/C++ toolchain and wall time; W1 precedent): the cheap
  gates stay in `make test`, the campaign is a recorded closeout artifact.
