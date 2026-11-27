//! `aura-ops` — the O3 closed-testnet ops receipt checker
//! (`aura_testkit::ops`).
//!
//! Subcommands: `check`, `summary`, `ledger`. Design:
//! `docs/o3-testnet-ops-plan.md`.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut out = stdout.lock();
    let mut err = stderr.lock();
    let code = aura_testkit::ops::run(&args, &mut out, &mut err);
    let _ = out.flush();
    let _ = err.flush();
    std::process::exit(code);
}
