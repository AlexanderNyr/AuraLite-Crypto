//! `kill -9` harness for the S7 chaos gate.
//!
//! Usage:
//!   s7-chaos --dir <path> --commits <n> [--crash-after-wal]
//!
//! Opens the store, commits `n` deterministic S2 batches, optionally appends
//! one more WAL record WITHOUT publishing it, then kills itself with SIGKILL.
//! The parent test reopens the store, requires head + root to match the
//! independent Python fixture, backs the store up, restores it into a fresh
//! directory and requires the same head + root — 50 times, zero mismatches.

use std::path::PathBuf;
use std::process::Command;

use aura_storage::{Storage, vector_changes};

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(arg_value(&args, "--dir").expect("--dir is required"));
    let commits: u64 = arg_value(&args, "--commits")
        .expect("--commits is required")
        .parse()
        .expect("commit count must be u64");
    let crash_after_wal = args.iter().any(|arg| arg == "--crash-after-wal");

    let mut storage = Storage::open(&dir).expect("open store");
    for index in 1..=commits {
        let receipt = storage
            .commit(vector_changes(index))
            .unwrap_or_else(|error| panic!("commit {index}: {error}"));
        assert_eq!(receipt.version, index, "commit version");
    }
    if crash_after_wal {
        storage
            .append_wal_only(vector_changes(commits + 1))
            .expect("append WAL only");
    }

    let head = storage.head();
    println!("HEAD {} {}", head.version, head.root.to_hex());
    let pid = std::process::id();
    println!("KILL {pid}");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let _ = Command::new("sh")
        .args(["-c", &format!("kill -9 {pid}")])
        .status();
    // The signal should have terminated us; if not, fail loudly.
    std::process::exit(70);
}
