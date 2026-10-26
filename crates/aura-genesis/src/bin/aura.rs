//! `aura genesis` — the C8 slice of the future `aura` CLI (D8 owns the rest).

use std::path::Path;

use aura_genesis::from_json_file;

const USAGE: &str = "\
aura — AuraLite-Crypto CLI (C8 implements only the genesis subcommand)

Usage:
  aura genesis hash   --file <genesis.json>
  aura genesis verify --file <genesis.json> --expect <64-hex>
  aura genesis show   --file <genesis.json>
";

fn usage() -> ! {
    eprint!("{USAGE}");
    std::process::exit(2);
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn load(args: &[String]) -> aura_genesis::Genesis {
    let path = match option(args, "--file") {
        Some(path) => path,
        None => usage(),
    };
    match from_json_file(Path::new(&path)) {
        Ok(genesis) => genesis,
        Err(error) => {
            eprintln!("genesis: {error}");
            std::process::exit(1);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 || args[0] != "genesis" {
        usage();
    }
    match args[1].as_str() {
        "hash" => {
            let genesis = load(&args);
            println!("{}", genesis.hash().to_hex());
        }
        "show" => {
            let genesis = load(&args);
            for (key, value) in genesis.summary() {
                println!("{key}={value}");
            }
        }
        "verify" => {
            let expected = match option(&args, "--expect") {
                Some(value) => value,
                None => usage(),
            };
            let genesis = load(&args);
            let actual = genesis.hash().to_hex();
            if actual == expected.to_ascii_lowercase() {
                println!("OK {actual}");
            } else {
                eprintln!("MISMATCH expected {expected} got {actual}");
                std::process::exit(1);
            }
        }
        _ => usage(),
    }
}
