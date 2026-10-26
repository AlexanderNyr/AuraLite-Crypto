//! Block header: fields, encoding and `block_hash`.

use aura_codec::{CodecError, Decode, Encode, Reader, Writer, encode_to_bytes};
use aura_crypto::hash_domain;
use aura_types::{Address, Amount, ChainId, Domain, Gas, Hash, Height};

/// Extra context inside the `block_hash` payload, before the header bytes.
///
/// NC target: `tools/nc_c5.py` changes this constant and requires the C5
/// block-hash vectors to go red.
pub const BLOCK_HASH_PAYLOAD_CONTEXT: &[u8] = b"AuraLite-Crypto/block-hash-v1\x00";

/// Maximum length of the opaque QC blob.
pub const MAX_QC_BYTES: usize = 65_536;

/// Everything the caller supplies for a header.
///
/// `tx_root` is deliberately **absent**: it is always derived from the body
/// by [`crate::Block::new`], so it cannot be forged at construction time.
/// `version` is included so a caller can construct a wrong-version header for
/// tests; real blocks use [`aura_types::PROTOCOL_VERSION`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderParams {
    /// Protocol version.
    pub version: u32,
    /// Chain this block belongs to.
    pub chain_id: ChainId,
    /// Block height.
    pub height: Height,
    /// Hash of the parent header.
    pub parent_hash: Hash,
    /// State root, supplied by the state layer (S1; not yet implemented).
    pub state_root: Hash,
    /// Validator-set hash, supplied by consensus (B phases; not yet implemented).
    pub validator_set_hash: Hash,
    /// Consensus timestamp in milliseconds — never `SystemTime::now()`.
    pub timestamp_ms: u64,
    /// Block proposer.
    pub proposer: Address,
    /// Opaque quorum certificate bytes; `None` for genesis-like blocks.
    pub qc: Option<Vec<u8>>,
    /// Gas consumed by the block.
    pub gas_used: Gas,
    /// Base fee for the block.
    pub base_fee: Amount,
}

/// A block header. Field order below **is** the wire order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Protocol version.
    pub version: u32,
    /// Chain this block belongs to (block-level replay separation).
    pub chain_id: ChainId,
    /// Block height.
    pub height: Height,
    /// Hash of the parent header.
    pub parent_hash: Hash,
    /// Binary Merkle root of the body's txids.
    pub tx_root: Hash,
    /// State root.
    pub state_root: Hash,
    /// Validator-set hash.
    pub validator_set_hash: Hash,
    /// Consensus timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Block proposer.
    pub proposer: Address,
    /// Opaque quorum certificate bytes.
    pub qc: Option<Vec<u8>>,
    /// Gas consumed by the block.
    pub gas_used: Gas,
    /// Base fee for the block.
    pub base_fee: Amount,
}

impl Header {
    /// Build a header from caller params and a computed `tx_root`.
    pub fn from_params(params: HeaderParams, tx_root: Hash) -> Self {
        Header {
            version: params.version,
            chain_id: params.chain_id,
            height: params.height,
            parent_hash: params.parent_hash,
            tx_root,
            state_root: params.state_root,
            validator_set_hash: params.validator_set_hash,
            timestamp_ms: params.timestamp_ms,
            proposer: params.proposer,
            qc: params.qc,
            gas_used: params.gas_used,
            base_fee: params.base_fee,
        }
    }

    /// Hash of this header: the block id.
    ///
    /// `hash_domain(chain_id, Domain::Block,
    /// BLOCK_HASH_PAYLOAD_CONTEXT || canonical(header))`. The body is not
    /// included directly; `tx_root` commits to it.
    pub fn block_hash(&self) -> Hash {
        let encoded = encode_to_bytes(self);
        let mut payload = Vec::with_capacity(BLOCK_HASH_PAYLOAD_CONTEXT.len() + encoded.len());
        payload.extend_from_slice(BLOCK_HASH_PAYLOAD_CONTEXT);
        payload.extend_from_slice(&encoded);
        hash_domain(self.chain_id.as_str(), Domain::Block, &payload)
    }
}

impl Encode for Header {
    fn encode(&self, writer: &mut Writer) {
        self.version.encode(writer);
        self.chain_id.encode(writer);
        self.height.encode(writer);
        self.parent_hash.encode(writer);
        self.tx_root.encode(writer);
        self.state_root.encode(writer);
        self.validator_set_hash.encode(writer);
        self.timestamp_ms.encode(writer);
        self.proposer.encode(writer);
        self.qc.encode(writer);
        self.gas_used.encode(writer);
        self.base_fee.encode(writer);
    }
}

impl Decode for Header {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Header {
            version: u32::decode(reader)?,
            chain_id: ChainId::decode(reader)?,
            height: Height::decode(reader)?,
            parent_hash: Hash::decode(reader)?,
            tx_root: Hash::decode(reader)?,
            state_root: Hash::decode(reader)?,
            validator_set_hash: Hash::decode(reader)?,
            timestamp_ms: u64::decode(reader)?,
            proposer: Address::decode(reader)?,
            qc: Option::<Vec<u8>>::decode(reader)?,
            gas_used: Gas::decode(reader)?,
            base_fee: Amount::decode(reader)?,
        })
    }
}
