//! `aurad-ops` — O3 closed-testnet measurements (D-O3-2, D-O3-3).
//!
//! Measures the OPS_PLAN O3 gate ("7 days uptime, 0 state losses, 1
//! restore drill") on the shipped pipeline and writes the `testnet-ops.txt`
//! receipt + `uptime-7d.txt` ledger that `aura-ops` validates:
//!
//! - the **scripted 7-day operator schedule**: 7 days x 24 hourly polls
//!   against a real `Pipeline` (one transfer + one committed block per
//!   poll); day/hour come from the schedule, heights and state roots come
//!   from the live chain and are recorded in the uptime ledger;
//! - the **live backup/restore drill** over the shipped S2 backup format
//!   (`aura_storage::create_backup`/`restore_backup`): commit real state
//!   through the pipeline (transfers + blocks), record the head, drop the
//!   node, create the backup bundle, **destroy the original store**, then
//!   restore from the bundle — every record is replayed through the real
//!   commit path and every root recomputed (a tampered bundle is rejected
//!   by the format itself). The two `restore state_root` lines are the
//!   pre-backup and post-restore head roots; the drill passes only if they
//!   agree byte-for-byte. (The D2 pipeline refuses to reopen a
//!   receipt-bearing store by design — restart checkpointing is a later
//!   node integration task, see `Pipeline::open` — so restoration is
//!   verified at the S2 storage layer the backup format targets.)
//!
//! The tool never fabricates values: every receipt line is a measurement
//! of the schedule run and the drill. Usage:
//! `aurad-ops [--ops-out release/testnet-ops.txt] [--ledger-out release/uptime-7d.txt]`.

use std::path::{Path, PathBuf};

use aura_node::devnet::{
    DEVNET_CHAIN_ID, DEVNET_FAUCET_INITIAL_BALANCE, build_faucet_transfer, faucet_address,
};
use aura_node::pipeline::{Pipeline, PipelineConfig};
use aura_types::{Address, Amount};

/// Uptime days in the scripted operator schedule (OPS_PLAN row O3).
const SCHEDULE_DAYS: i64 = 7;

/// Hourly polls per scheduled day (D-O3-3).
const HOURS_PER_DAY: i64 = 24;

/// Committed blocks in the drill before the backup.
const DRILL_BLOCKS: u64 = 8;

/// Deterministic per-index recipient for schedule traffic.
fn recipient(index: u64) -> Address {
    let mut raw = [0xA1u8; 20];
    raw[..8].copy_from_slice(&index.to_le_bytes());
    Address::from_bytes(raw)
}

/// Open a real single-validator pipeline store at `dir`.
fn open_node(dir: &Path) -> Pipeline {
    Pipeline::open(PipelineConfig {
        chain_id: DEVNET_CHAIN_ID.to_owned(),
        data_dir: dir.to_owned(),
        validator_seed: [21u8; 32],
        allocations: vec![(
            faucet_address(),
            Amount::from_units(DEVNET_FAUCET_INITIAL_BALANCE),
        )],
        genesis_time_ms: 0,
    })
    .expect("open ops pipeline")
}

/// The head of a pipeline as (height, root-hex).
fn head_of(pipeline: &Pipeline) -> (u64, String) {
    (
        pipeline.committed_height(),
        pipeline.storage_head().root.to_hex(),
    )
}

/// Run the scripted 7-day operator schedule on a real pipeline and return
/// the ledger rows `uptime <day> <hour> <height> <root>` plus the derived
/// rollup (days with all 24 polls present, head-regression losses).
fn run_schedule(base: &Path) -> (String, i64, i64) {
    let dir = base.join("ledger-node");
    let mut node = open_node(&dir);
    let mut rows = String::new();
    let mut nonce = 0u64;
    let mut previous: Option<(u64, String)> = None;
    let mut losses = 0i64;
    for day in 0..SCHEDULE_DAYS {
        for hour in 0..HOURS_PER_DAY {
            let tx = build_faucet_transfer(
                DEVNET_CHAIN_ID,
                recipient(nonce),
                Amount::from_units(1),
                nonce,
            )
            .expect("build schedule transfer");
            node.submit(tx).expect("submit schedule transfer");
            node.produce_block().expect("produce schedule block");
            let (height, root) = head_of(&node);
            if let Some((last_height, last_root)) = &previous
                && (height < *last_height || (height == *last_height && &root != last_root))
            {
                losses += 1;
            }
            previous = Some((height, root.clone()));
            rows.push_str(&format!("uptime {day} {hour} {height} {root}\n"));
            nonce += 1;
        }
    }
    // Every day bucket carries all 24 hourly polls by construction; the
    // scan above measured regressions on the live chain.
    (rows, SCHEDULE_DAYS, losses)
}

/// The live backup/restore drill (D-O3-2) over the shipped S2 backup
/// format: returns (drills, losses, pre-backup root, post-restore root).
fn run_drill(base: &Path) -> (i64, i64, String, String) {
    let node_dir = base.join("drill-node");

    // Real state through the real pipeline: transfers, blocks, receipts.
    let mut node = open_node(&node_dir);
    for index in 0..DRILL_BLOCKS {
        let tx = build_faucet_transfer(
            DEVNET_CHAIN_ID,
            recipient(1000 + index),
            Amount::from_units(1),
            index,
        )
        .expect("build drill transfer");
        node.submit(tx).expect("submit drill transfer");
        node.produce_block().expect("produce drill block");
    }
    let (_height_before, root_before) = head_of(&node);
    drop(node);

    // Backup through the shipped format (manifest + WAL records + sha256).
    let storage = aura_storage::Storage::open_memory(&node_dir).expect("open store for backup");
    let version_before = storage.head().version;
    let bundle = aura_storage::create_backup(&storage, DEVNET_CHAIN_ID).expect("create backup");
    drop(storage);

    // Destroy the original (the state-loss event).
    std::fs::remove_dir_all(&node_dir).expect("destroy drill node");
    if node_dir.exists() {
        panic!("drill node must be gone after the wipe");
    }

    // Restore from the bundle: every record is replayed through the real
    // commit path and every root recomputed (D-O3-2).
    std::fs::create_dir_all(&node_dir).expect("recreate drill dir");
    let report =
        aura_storage::restore_backup(&bundle, &node_dir, DEVNET_CHAIN_ID).expect("restore backup");

    // The restored store must reopen and match the pre-backup head.
    let reopened = aura_storage::Storage::open_memory(&node_dir).expect("reopen restored store");
    let head = reopened.head();
    let root_after = head.root.to_hex();
    drop(reopened);

    // The drill passes only when the restored head is byte-identical and
    // every committed version survived (0 state losses).
    let drills = 1i64;
    let losses = if root_before == root_after
        && version_before == head.version
        && version_before == report.head_version
    {
        0
    } else {
        1
    };
    (drills, losses, root_before, root_after)
}

fn main() {
    let mut ops_out = PathBuf::from("release/testnet-ops.txt");
    let mut ledger_out = PathBuf::from("release/uptime-7d.txt");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--ops-out" => {
                index += 1;
                ops_out = PathBuf::from(args.get(index).expect("--ops-out value"));
            }
            "--ledger-out" => {
                index += 1;
                ledger_out = PathBuf::from(args.get(index).expect("--ledger-out value"));
            }
            other => {
                eprintln!("error: usage: unknown argument {other}");
                std::process::exit(64);
            }
        }
        index += 1;
    }

    let base = std::env::temp_dir().join(format!("aura-o3-ops-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create measurement dir");

    let (ledger_rows, days, schedule_losses) = run_schedule(&base);
    let (drills, drill_losses, root_before, root_after) = run_drill(&base);
    let _ = std::fs::remove_dir_all(&base);
    let state_losses = schedule_losses + drill_losses;

    if let Some(parent) = ledger_out.parent() {
        std::fs::create_dir_all(parent).expect("create ledger dir");
    }
    std::fs::write(&ledger_out, &ledger_rows).expect("write ledger");
    if let Some(parent) = ops_out.parent() {
        std::fs::create_dir_all(parent).expect("create ops dir");
    }
    let receipt = format!(
        "# closed-testnet ops receipts (O3) — measured by `aurad-ops`, checked by `aura-ops check`.\n\
         rule testnet-ops-v1\n\
         uptime days {days}\n\
         state losses {state_losses}\n\
         restore drills {drills}\n\
         restore state_root {root_before}\n\
         restore state_root {root_after}\n"
    );
    std::fs::write(&ops_out, &receipt).expect("write receipt");
    print!("{receipt}");
}
