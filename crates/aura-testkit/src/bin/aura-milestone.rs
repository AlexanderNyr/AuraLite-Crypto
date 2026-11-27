//! `aura-milestone` — the O2 milestone receipt checker
//! (`aura_testkit::milestone`).
//!
//! Subcommands: `check`, `summary`. Design: `docs/o2-m0-m1-plan.md`.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut out = stdout.lock();
    let mut err = stderr.lock();
    let code = aura_testkit::milestone::run(&args, &mut out, &mut err);
    let _ = out.flush();
    let _ = err.flush();
    std::process::exit(code);
}
