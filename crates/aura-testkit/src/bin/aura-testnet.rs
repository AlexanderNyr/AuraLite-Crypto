//! `aura-testnet` — the O4 public testnet receipt checker
//! (`aura_testkit::testnet`).
//!
//! Subcommands: `check`, `summary`, `metrics`, `triage`. Design:
//! `docs/o4-testnet-public-plan.md`.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut out = stdout.lock();
    let mut err = stderr.lock();
    let code = aura_testkit::testnet::run(&args, &mut out, &mut err);
    let _ = out.flush();
    let _ = err.flush();
    std::process::exit(code);
}
