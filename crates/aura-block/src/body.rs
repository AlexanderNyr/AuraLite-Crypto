//! Block body: canonically sorted transactions.

use aura_codec::{CodecError, Decode, Encode, Reader, Writer};
use aura_tx::Tx;
use aura_types::Hash;

use crate::error::BlockError;
use crate::merkle::compute_tx_root;

/// Maximum number of transactions in a block. Aligned with `aura-codec`'s
/// `MAX_ITEMS` so the canonical decoder can always read a valid body.
pub const MAX_BLOCK_TXS: usize = 65_536;

/// An ordered list of transactions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Body {
    /// Transactions, canonically sorted by `(signer, nonce)`.
    pub txs: Vec<Tx>,
}

impl Body {
    /// Sort by `(signer, nonce)` and reject duplicates.
    ///
    /// `Address` and `Nonce` both derive `Ord`, so this is a total,
    /// platform-independent order — no `HashMap`, no local time.
    pub fn new(mut txs: Vec<Tx>) -> Result<Self, BlockError> {
        if txs.len() > MAX_BLOCK_TXS {
            return Err(BlockError::TooManyTransactions(txs.len()));
        }
        txs.sort_by(|left, right| {
            (left.unsigned.signer, left.unsigned.nonce)
                .cmp(&(right.unsigned.signer, right.unsigned.nonce))
        });
        for pair in txs.windows(2) {
            let a = (&pair[0].unsigned.signer, &pair[0].unsigned.nonce);
            let b = (&pair[1].unsigned.signer, &pair[1].unsigned.nonce);
            if a == b {
                return Err(BlockError::DuplicateTxKey);
            }
        }
        Ok(Body { txs })
    }

    /// Recompute the Merkle root of this body (order-sensitive).
    pub fn tx_root(&self) -> Hash {
        compute_tx_root(&self.txs)
    }
}

impl Encode for Body {
    fn encode(&self, writer: &mut Writer) {
        self.txs.encode(writer);
    }
}

impl Decode for Body {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Body {
            txs: Vec::<Tx>::decode(reader)?,
        })
    }
}
