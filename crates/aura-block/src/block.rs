//! Complete block: header plus body, hashing and validation.

use aura_codec::{CodecError, Decode, Encode, Reader, Writer, decode_canonical, encode_to_bytes};
use aura_types::{Hash, PROTOCOL_VERSION};

use crate::body::{Body, MAX_BLOCK_TXS};
use crate::error::BlockError;
use crate::header::{Header, HeaderParams, MAX_QC_BYTES};

/// A complete block. Header order, then body order, is the wire order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Header (commits to the body via `tx_root`).
    pub header: Header,
    /// Canonically sorted transactions.
    pub body: Body,
}

impl Block {
    /// Build a block: sort the body, compute `tx_root`, then assemble.
    ///
    /// The caller cannot supply `tx_root`, so a freshly built block always
    /// has a correct root. Tests and NCs mutate the public fields directly
    /// to construct invalid blocks.
    pub fn new(params: HeaderParams, txs: Vec<aura_tx::Tx>) -> Result<Self, BlockError> {
        let body = Body::new(txs)?;
        let tx_root = body.tx_root();
        Ok(Block {
            header: Header::from_params(params, tx_root),
            body,
        })
    }

    /// The block id: hash of the header (see [`Header::block_hash`]).
    pub fn block_hash(&self) -> Hash {
        self.header.block_hash()
    }

    /// Canonical wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        encode_to_bytes(self)
    }

    /// Decode with full C2 canonicality (no trailing bytes, no non-minimal
    /// varints, re-encode must reproduce the input).
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, BlockError> {
        Ok(decode_canonical::<Block>(bytes)?)
    }

    /// Validate block-level invariants for `expected_chain`.
    ///
    /// Deliberately does **not** execute or charge gas; transaction signature
    /// checks are available separately in [`Block::validate_transactions`].
    ///
    /// NC targets: `tools/nc_c5.py` disables the canonical-order check and
    /// the `tx_root` check here, requiring the reorder vectors to go red.
    pub fn validate(&self, expected_chain: &str) -> Result<(), BlockError> {
        if self.header.version != PROTOCOL_VERSION {
            return Err(BlockError::UnsupportedProtocolVersion(
                PROTOCOL_VERSION,
                self.header.version,
            ));
        }
        if self.header.chain_id.as_str() != expected_chain {
            return Err(BlockError::WrongChain(
                expected_chain.to_owned(),
                self.header.chain_id.as_str().to_owned(),
            ));
        }
        if self.body.txs.len() > MAX_BLOCK_TXS {
            return Err(BlockError::TooManyTransactions(self.body.txs.len()));
        }
        if let Some(qc) = &self.header.qc
            && qc.len() > MAX_QC_BYTES
        {
            return Err(BlockError::QcTooLarge(qc.len()));
        }
        for pair in self.body.txs.windows(2) {
            let a = (&pair[0].unsigned.signer, &pair[0].unsigned.nonce);
            let b = (&pair[1].unsigned.signer, &pair[1].unsigned.nonce);
            if a == b {
                return Err(BlockError::DuplicateTxKey);
            }
            if a > b {
                return Err(BlockError::UnsortedTransactions);
            }
        }
        let computed_root = self.body.tx_root();
        if computed_root != self.header.tx_root {
            return Err(BlockError::TxRootMismatch(
                computed_root,
                self.header.tx_root,
            ));
        }
        Ok(())
    }

    /// Validate every transaction in the body at this block's height.
    pub fn validate_transactions(&self, expected_chain: &str) -> Result<(), BlockError> {
        for tx in &self.body.txs {
            tx.validate(expected_chain, self.header.height)?;
        }
        Ok(())
    }
}

impl Encode for Block {
    fn encode(&self, writer: &mut Writer) {
        self.header.encode(writer);
        self.body.encode(writer);
    }
}

impl Decode for Block {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Block {
            header: Header::decode(reader)?,
            body: Body::decode(reader)?,
        })
    }
}
