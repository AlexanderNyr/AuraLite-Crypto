//! `aurad` — the node binary's D0 stub.
//!
//! The binary is a thin shell: argument handling, the refusal text and the survey
//! live in `aura_node::baseline`, where the gate can call them directly and where
//! they are documented. Keeping `main` this small is the point — D-D1 says `aurad`
//! is a thin composition of crates and all logic lives in libraries.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    let code = aura_node::baseline::run(&args, &mut out, &mut err);
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
