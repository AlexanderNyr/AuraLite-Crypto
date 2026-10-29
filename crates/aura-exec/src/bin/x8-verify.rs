//! `x8-verify` — run the X8 static verifier over one or more WASM files.
//!
//! The gate and the evidence script both call this binary, so the report a reader
//! sees in `docs/evidence/x8/` is the same report the gate asserts on.
//!
//! Usage:
//! ```text
//! x8-verify examples/kv/wasm/kv.wasm
//! x8-verify --fixture crates/aura-exec/tests/fixtures/x8_mutants.txt
//! ```

use aura_exec::verify::{VerifyLimits, verify};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        println!("usage: x8-verify [--fixture MANIFEST] <file>...");
        return;
    }
    let limits = VerifyLimits::hosted();
    if args[0] == "--fixture" {
        let Some(manifest) = args.get(1) else {
            eprintln!("--fixture needs a path");
            std::process::exit(2);
        };
        let text = std::fs::read_to_string(manifest).expect("manifest is readable");
        let base = std::path::Path::new(manifest)
            .parent()
            .expect("manifest has a parent");
        for line in text.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let fields: Vec<&str> = line.split_whitespace().collect();
            let path = fields
                .iter()
                .find_map(|field| field.strip_prefix("file="))
                .expect("every mutant line names a file");
            let expect = fields
                .iter()
                .find_map(|field| field.strip_prefix("expect="))
                .unwrap_or("accept");
            let bytes = std::fs::read(base.join(path)).expect("mutant is readable");
            let report = verify(&bytes, &limits);
            let verdict = match report.first() {
                Some(problem) => format!("reject:{}", problem.code),
                None => "accept".to_string(),
            };
            println!(
                "{path} expect={expect} got={verdict} functions={} instructions={} max_stack={} max_ops={}",
                report.functions,
                report.instructions,
                report.max_stack_depth,
                report.max_function_ops
            );
        }
        return;
    }
    for path in args {
        let bytes = std::fs::read(&path).expect("module is readable");
        let report = verify(&bytes, &limits);
        println!(
            "{path}: {} functions={} instructions={} max_stack={} max_ops={}",
            if report.ok { "accept" } else { "reject" },
            report.functions,
            report.instructions,
            report.max_stack_depth,
            report.max_function_ops
        );
        for problem in &report.errors {
            println!("  {problem}");
        }
    }
}
