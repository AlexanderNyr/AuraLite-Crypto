//! `aura-wallet` binary — W0 baseline entry point.
//!
//! Thin shell: argument handling, refusal output, and the key-exposure survey
//! live in `aura_wallet::baseline`, where the gate can call them directly.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    let code = aura_wallet::baseline::run(&args, &mut out, &mut err);
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
