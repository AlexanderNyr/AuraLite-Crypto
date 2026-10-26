//! `aura-fuzz` — reproducible 60-second fuzz smoke gate (C9).

use aura_testkit::{FUZZ_TARGETS, run_portable_fuzz};

const USAGE: &str = "\
aura-fuzz — portable deterministic fuzz harness (stable Rust, no sanitizers)

Usage:
  aura-fuzz --seconds <n> [--min-iterations <n>] [--seed <u64>] [--target <name>|all]

Examples:
  aura-fuzz --seconds 60 --target all
  aura-fuzz --seconds 0 --min-iterations 20000 --target codec
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

fn parse_u64(text: &str, name: &str) -> u64 {
    text.parse().unwrap_or_else(|_| {
        eprintln!("aura-fuzz: {name} must be a u64, got {text:?}");
        std::process::exit(2);
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seconds = match option(&args, "--seconds") {
        Some(value) => parse_u64(&value, "--seconds"),
        None => usage(),
    };
    let min_iterations = option(&args, "--min-iterations")
        .map(|value| parse_u64(&value, "--min-iterations"))
        .unwrap_or(1);
    let seed = option(&args, "--seed")
        .map(|value| parse_u64(&value, "--seed"))
        .unwrap_or(0xA17A_11CE_C0DE_2026);
    let target = option(&args, "--target").unwrap_or_else(|| "all".to_owned());

    let targets: Vec<&str> = if target == "all" {
        FUZZ_TARGETS.to_vec()
    } else if FUZZ_TARGETS.contains(&target.as_str()) {
        vec![target.as_str()]
    } else {
        eprintln!("aura-fuzz: unknown target {target:?}");
        usage();
    };

    // `seconds` is the TOTAL campaign budget, split across the selected
    // targets — `all` for 60 s means 60 s, not 6 x 60 s.
    let per_target = if targets.len() > 1 {
        seconds / targets.len() as u64
    } else {
        seconds
    };
    let mut total_iterations = 0u64;
    let mut total_crashes = 0u64;
    let mut total_elapsed_ms = 0u64;
    for name in targets {
        let report = run_portable_fuzz(name, per_target, min_iterations, seed);
        println!(
            "target={} iterations={} crashes={} elapsed_ms={} seed={}",
            report.target, report.iterations, report.crashes, report.elapsed_ms, report.seed
        );
        total_iterations += report.iterations;
        total_crashes += report.crashes;
        total_elapsed_ms += report.elapsed_ms;
    }
    println!(
        "TOTAL iterations={total_iterations} crashes={total_crashes} elapsed_ms={total_elapsed_ms}"
    );
    if total_crashes != 0 || total_iterations == 0 {
        std::process::exit(1);
    }
}
