//! `aura-release` — the O1 release pipeline driver (`aura_testkit::release`).
//!
//! Subcommands: `check`, `sbom`, `checksums`, `digest`, `sync`.
//! Design: `docs/o1-releases-plan.md`.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut out = stdout.lock();
    let mut err = stderr.lock();
    let code = aura_testkit::release::run(&args, &mut out, &mut err);
    let _ = out.flush();
    let _ = err.flush();
    std::process::exit(code);
}
