//! O0 CI CLI (`check`/`coverage`/`run`/`sync`) — the shard registry and the
//! target registry check; see `docs/o0-ci-plan.md` (D-O0-1..D-O0-5).
#![deny(missing_docs)]

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut out = stdout.lock();
    let mut err = stderr.lock();
    let code = aura_testkit::ci::run(&args, &mut out, &mut err);
    let _ = out.flush();
    let _ = err.flush();
    std::process::exit(code);
}
