//! `aura-consensus` binary — B0 baseline entry point.
//!
//! Thin shell: argument handling and the refusal/report text live in
//! `aura_consensus::baseline`, where the gate can call them directly.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    let code = aura_consensus::baseline::run(&args, &mut out, &mut err);
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
