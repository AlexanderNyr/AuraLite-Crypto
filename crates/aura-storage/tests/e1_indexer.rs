//! E1 indexer gate (ECOSYSTEM_PLAN E1): "10 000 blocks + artificial
//! reorg -> 0 duplicates/losses" with named negative-control messages.

use aura_storage::indexer::{BlockRef, ChainIndexer, IngestOutcome};

const BLOCKS: u64 = 10_000;
const ANCESTOR: u64 = 9_900;
const FORK_LEN: u64 = 205;

/// Deterministic block on branch `fork` at `height`.
fn blk(fork: u8, height: u64) -> BlockRef {
    let mut hash = [0u8; 32];
    hash[0] = fork;
    hash[1..9].copy_from_slice(&height.to_le_bytes());
    let parent = if height <= 1 {
        [0u8; 32]
    } else if fork == 1 && height == ANCESTOR + 1 {
        // the fork's first block attaches to the common ancestor
        blk(0, ANCESTOR).hash
    } else {
        let mut p = [0u8; 32];
        p[0] = fork;
        p[1..9].copy_from_slice(&(height - 1).to_le_bytes());
        p
    };
    BlockRef {
        hash,
        parent,
        height,
    }
}

fn main_chain() -> Vec<BlockRef> {
    (1..=BLOCKS).map(|h| blk(0, h)).collect()
}

fn fork_chain() -> Vec<BlockRef> {
    ((ANCESTOR + 1)..=(ANCESTOR + FORK_LEN))
        .map(|h| blk(1, h))
        .collect()
}

fn ingest_all(index: &mut ChainIndexer, blocks: &[BlockRef]) -> Vec<IngestOutcome> {
    blocks.iter().map(|b| index.ingest(*b)).collect()
}

#[test]
fn e1_ten_thousand_blocks_and_reorg_leave_zero_duplicates_and_losses() {
    let mut index = ChainIndexer::new();
    let chain = main_chain();

    let first = ingest_all(&mut index, &chain);
    assert!(
        first.iter().all(|o| *o == IngestOutcome::Indexed),
        "E1: blocks must be ingested in canonical order"
    );

    // Canonical height map verified before any other property.
    assert_eq!(
        index.canonical_count(),
        BLOCKS as usize,
        "E1: blocks must be indexed by canonical height"
    );
    for height in 1..=index.tip_height() {
        let hash = index.canonical(height).expect("canonical height present");
        assert!(
            index.is_canonical(&hash),
            "E1: blocks must be indexed by canonical height"
        );
    }
    assert_eq!(index.row_count(), BLOCKS as usize);

    // Idempotency: re-reading the whole chain changes nothing.
    let second = ingest_all(&mut index, &chain);
    assert!(
        second.iter().all(|o| *o == IngestOutcome::Duplicate),
        "E1: indexer ingest must be idempotent"
    );

    // Artificial reorg: a longer fork replaces the suffix.
    let fork = fork_chain();
    assert!(
        index.reorg_to(ANCESTOR, &fork),
        "E1: indexer must roll back on reorg"
    );

    // Named gate, literally: the network of rows rolled back and adopted.
    assert_eq!(
        index.tip_height(),
        ANCESTOR + FORK_LEN,
        "E1: indexer must roll back on reorg"
    );
    assert!(
        !index.is_canonical(&blk(0, BLOCKS).hash),
        "E1: indexer must roll back on reorg"
    );
    assert!(
        index.is_canonical(&blk(1, ANCESTOR + FORK_LEN).hash),
        "E1: indexer must roll back on reorg"
    );

    // Zero duplicates / zero losses, counted independently.
    let mut seen = std::collections::HashSet::new();
    let mut duplicates = 0usize;
    for b in chain.iter().chain(fork.iter()) {
        if !seen.insert(b.hash) {
            duplicates += 1;
        }
    }
    assert_eq!(
        duplicates, 0,
        "E1: reorg must leave 0 duplicates and 0 losses"
    );
    for b in chain.iter().chain(fork.iter()) {
        assert!(
            index.knows(&b.hash),
            "E1: reorg must leave 0 duplicates and 0 losses"
        );
    }
    assert_eq!(
        index.row_count(),
        (BLOCKS + FORK_LEN) as usize,
        "E1: reorg must leave 0 duplicates and 0 losses"
    );
    assert_eq!(
        index.canonical_count(),
        (ANCESTOR + FORK_LEN) as usize,
        "E1: reorg must leave 0 duplicates and 0 losses"
    );

    // Post-reorg height map: ancestor kept, fork adopted.
    assert_eq!(index.canonical(0), None);
    assert_eq!(
        index.canonical(ANCESTOR),
        Some(blk(0, ANCESTOR).hash),
        "E1: blocks must be indexed by canonical height"
    );

    // Out-of-order and gapped reads are refused.
    let mut probe = index;
    let gap = BlockRef {
        hash: [9u8; 32],
        parent: [8u8; 32],
        height: 5000,
    };
    assert_eq!(
        probe.ingest(gap),
        IngestOutcome::Rejected,
        "E1: blocks must be ingested in canonical order"
    );
    let early = BlockRef {
        hash: [7u8; 32],
        parent: [0u8; 32],
        height: 42,
    };
    assert_eq!(
        probe.ingest(early),
        IngestOutcome::Rejected,
        "E1: blocks must be ingested in canonical order"
    );

    // Orphaned blocks stay orphaned when re-read.
    assert_eq!(
        probe.ingest(blk(0, BLOCKS)),
        IngestOutcome::Orphaned,
        "E1: orphaned blocks must not count as canonical"
    );
    assert!(
        !probe.is_canonical(&blk(0, BLOCKS).hash),
        "E1: orphaned blocks must not count as canonical"
    );
}
