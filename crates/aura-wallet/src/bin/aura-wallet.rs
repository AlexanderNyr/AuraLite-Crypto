//! `aura-wallet` binary — W2 wallet CLI entry point.
//!
//! Thin shell: the nine wallet commands dispatch to `aura_wallet::cli`
//! (`docs/w2-cli-plan.md`); `report`/`audit-tree` and the W0 survey stay in
//! `aura_wallet::baseline`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    let code = match args.first().map(String::as_str) {
        Some(cmd) if aura_wallet::cli::COMMANDS.contains(&cmd) => {
            aura_wallet::cli::run(&args, &mut out, &mut err)
        }
        _ => aura_wallet::baseline::run(&args, &mut out, &mut err),
    };
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
