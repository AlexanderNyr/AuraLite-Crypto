//! W7 ceremony CLI (`open`/`generate`/`split`/`shred`/`seal`/
//! `reconstruct`/`verify`); the offline genesis ceremony runner —
//! see `docs/w7-genesis-ceremony-plan.md` (D-W7-1..D-W7-5).
#![deny(missing_docs)]

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut out = stdout.lock();
    let mut err = stderr.lock();
    let code = aura_genesis::ceremony::run(&args, &mut out, &mut err);
    let _ = out.flush();
    let _ = err.flush();
    std::process::exit(code);
}
