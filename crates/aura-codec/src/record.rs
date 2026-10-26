//! A reference record that demonstrates the fixed field order contract.

use aura_types::{Address, Amount, CanonicalMap, ChainId, Domain, Hash, Height, Nonce};

use crate::CODEC_VERSION;
use crate::codec::{Decode, Encode, Reader, Writer};
use crate::error::CodecError;

/// A test/reference record covering every primitive kind in the format.
///
/// The field order below **is** the wire order. `Encode` and `Decode` must
/// stay in lockstep; the C2 golden vectors (from the independent Python
/// encoder) pin that order byte for byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalRecord {
    /// Wire format version; must equal [`CODEC_VERSION`].
    pub version: u8,
    /// Signature-domain tag.
    pub domain: Domain,
    /// A strict boolean flag.
    pub flag: bool,
    /// Validated chain identifier.
    pub chain: ChainId,
    /// Block height.
    pub height: Height,
    /// Account sequence number.
    pub nonce: Nonce,
    /// Amount in smallest units.
    pub amount: Amount,
    /// 32-byte hash.
    pub hash: Hash,
    /// 20-byte address.
    pub address: Address,
    /// Optional UTF-8 memo.
    pub memo: Option<String>,
    /// Deterministic ordered tags.
    pub tags: CanonicalMap<String, u64>,
}

impl Encode for CanonicalRecord {
    fn encode(&self, writer: &mut Writer) {
        self.version.encode(writer);
        self.domain.encode(writer);
        self.flag.encode(writer);
        self.chain.encode(writer);
        self.height.encode(writer);
        self.nonce.encode(writer);
        self.amount.encode(writer);
        self.hash.encode(writer);
        self.address.encode(writer);
        self.memo.encode(writer);
        self.tags.encode(writer);
    }
}

impl Decode for CanonicalRecord {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let version = u8::decode(reader)?;
        if version != CODEC_VERSION {
            return Err(CodecError::UnsupportedVersion(CODEC_VERSION, version));
        }
        Ok(CanonicalRecord {
            version,
            domain: Decode::decode(reader)?,
            flag: Decode::decode(reader)?,
            chain: Decode::decode(reader)?,
            height: Decode::decode(reader)?,
            nonce: Decode::decode(reader)?,
            amount: Decode::decode(reader)?,
            hash: Decode::decode(reader)?,
            address: Decode::decode(reader)?,
            memo: Decode::decode(reader)?,
            tags: Decode::decode(reader)?,
        })
    }
}
