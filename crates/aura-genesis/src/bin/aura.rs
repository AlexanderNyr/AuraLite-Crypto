//! `aura` — AuraLite-Crypto operator and client CLI (C8 genesis + D8 node ops,
//! query, tx, keys, snapshot, upgrade).
#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../../../aura-node/src/cli.rs"]
mod cli;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    let code = cli::run(&args, &mut out, &mut err);
    std::process::exit(code);
}
