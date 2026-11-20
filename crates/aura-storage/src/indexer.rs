//! E1 chain indexer over a local row store (ECOSYSTEM_PLAN E1).
//!
//! Ingest is idempotent: re-reading a known block is a cheap no-op.
//! `reorg_to` rolls the canonical chain back to a common ancestor and
//! adopts a longer fork, keeping every observed block in the row store
//! (the phase gate: 10 000 blocks + artificial reorg -> 0 duplicates,
//! 0 losses).

use std::collections::{BTreeMap, HashMap};

/// Reference to a block as the indexer stores it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BlockRef {
    /// Block hash (32 bytes).
    pub hash: [u8; 32],
    /// Parent block hash (all-zero only for the genesis height).
    pub parent: [u8; 32],
    /// Block height above genesis.
    pub height: u64,
}

/// Result of feeding one block to [`ChainIndexer::ingest`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IngestOutcome {
    /// Accepted into the canonical chain (or a new fork branch).
    Indexed,
    /// Block already known and unchanged; ingest is a no-op.
    Duplicate,
    /// Block already known but no longer canonical (rolled back).
    Orphaned,
    /// Refused: height occupied by another hash or parent gap.
    Rejected,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Row {
    parent: [u8; 32],
    height: u64,
    canonical: bool,
}

/// Idempotent block indexer with reorg rollback.
#[derive(Debug, Default)]
pub struct ChainIndexer {
    rows: HashMap<[u8; 32], Row>,
    chain: BTreeMap<u64, [u8; 32]>,
}

impl ChainIndexer {
    /// Fresh, empty indexer (no genesis required).
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one block. Idempotent: known blocks return
    /// [`IngestOutcome::Duplicate`] or [`IngestOutcome::Orphaned`] and
    /// change nothing. Unknown blocks that neither extend the tip nor
    /// grow a known fork (parent gap, occupied height) are
    /// [`IngestOutcome::Rejected`].
    pub fn ingest(&mut self, block: BlockRef) -> IngestOutcome {
        if let Some(row) = self.rows.get(&block.hash) {
            return if row.canonical {
                IngestOutcome::Duplicate
            } else {
                IngestOutcome::Orphaned
            };
        }
        let slot_free = !self.chain.contains_key(&block.height);
        let extends_tip = slot_free
            && match block.height.checked_sub(1) {
                Some(0) | None => block.parent == [0u8; 32],
                Some(parent_height) => self.chain.get(&parent_height) == Some(&block.parent),
            };
        let grows_fork = self.rows.contains_key(&block.parent)
            && self.rows.get(&block.parent).map(|p| p.height + 1) == Some(block.height);
        if !extends_tip && !grows_fork {
            return IngestOutcome::Rejected;
        }
        let canonical = extends_tip;
        self.rows.insert(
            block.hash,
            Row {
                parent: block.parent,
                height: block.height,
                canonical,
            },
        );
        if canonical {
            self.chain.insert(block.height, block.hash);
        }
        IngestOutcome::Indexed
    }

    /// Roll the canonical chain back to `ancestor_height` and adopt
    /// `fork` (strictly longer than the rolled-back suffix). Every
    /// rolled-back block stays in the row store as non-canonical;
    /// returns `false` (no change) when the ancestor is not canonical
    /// or the fork does not extend it.
    pub fn reorg_to(&mut self, ancestor_height: u64, fork: &[BlockRef]) -> bool {
        let ancestor = match self.chain.get(&ancestor_height).copied() {
            Some(hash) => hash,
            None => return false,
        };
        if fork.last().map(|b| b.height).unwrap_or(0) <= ancestor_height {
            return false;
        }
        // Validate the fork before touching state: contiguous heights
        // from the ancestor, each block pointing at the previous one.
        let mut expected_parent = ancestor;
        for (offset, block) in fork.iter().enumerate() {
            let expected_height = ancestor_height + 1 + offset as u64;
            if block.height != expected_height || block.parent != expected_parent {
                return false;
            }
            expected_parent = block.hash;
        }
        let old_tip = self.tip_height();
        for height in (ancestor_height + 1)..=old_tip {
            if let Some(hash) = self.chain.remove(&height)
                && let Some(row) = self.rows.get_mut(&hash)
            {
                row.canonical = false;
            }
        }
        for block in fork {
            self.rows.insert(
                block.hash,
                Row {
                    parent: block.parent,
                    height: block.height,
                    canonical: true,
                },
            );
            self.chain.insert(block.height, block.hash);
        }
        true
    }

    /// Highest canonical height (0 when empty).
    pub fn tip_height(&self) -> u64 {
        self.chain.last_key_value().map(|(h, _)| *h).unwrap_or(0)
    }

    /// Canonical block hash at `height`, if any.
    pub fn canonical(&self, height: u64) -> Option<[u8; 32]> {
        self.chain.get(&height).copied()
    }

    /// Whether this hash is on the canonical chain.
    pub fn is_canonical(&self, hash: &[u8; 32]) -> bool {
        self.rows
            .get(hash)
            .map(|row| row.canonical)
            .unwrap_or(false)
    }

    /// Whether this block has been observed at all.
    pub fn knows(&self, hash: &[u8; 32]) -> bool {
        self.rows.contains_key(hash)
    }

    /// Block reference for a known block (canonical or orphaned).
    pub fn block(&self, hash: &[u8; 32]) -> Option<BlockRef> {
        self.rows.get(hash).map(|row| BlockRef {
            hash: *hash,
            parent: row.parent,
            height: row.height,
        })
    }

    /// Total observed blocks (canonical + orphaned).
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Canonical chain length.
    pub fn canonical_count(&self) -> usize {
        self.chain.len()
    }
}
