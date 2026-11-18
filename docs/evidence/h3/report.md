# H3 evidence report (fuzzing)

Targets: 11 libFuzzer sources (2 new: `fuzz_p2p_frame`, `fuzz_wasm_verify`)
plus the 6 portable areas (docs/h3-fuzz-plan.md §2).

Recorded campaign `smoke`: 6 executed runs,
11 skipped (cargo-fuzz toolchain absent), 0 crash
artifacts, seeds from 20261180. Per-run commands, run.log SHA-256 and
source manifests: `campaign/result.json` + `campaign/logs/`.

Deviation (plan §3): this workspace records the smoke schedule; the
24-hour `full` budget is a parameter of `tools/h3_fuzz_campaign.py` and
is not claimed here. libFuzzer/ASan require nightly-2026-11-03 +
cargo-fuzz; without them those runs are `skipped`, never passes.

Mutation controls: 6/6 (`tools/nc_h3.py`, log in `nc.log`); the named
control injects a decoder panic and the fuzz oracle finds it.
