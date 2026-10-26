//! Transaction kind tags.

use aura_codec::{CodecError, Decode, Encode, Reader, Writer};

/// What the transaction asks the chain to do.
///
/// Tags are protocol constants. Unknown tags are rejected at decode time;
/// adding a kind requires a new phase, a `CODEC_VERSION` bump and a migration
/// test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TxKind {
    /// Native asset transfer.
    Transfer,
    /// Publish WASM bytecode.
    Deploy,
    /// Invoke a contract.
    Call,
    /// Delegate stake to a validator.
    Delegate,
    /// Withdraw delegated stake.
    Undelegate,
}

impl TxKind {
    /// All kinds, in tag order.
    pub const ALL: [TxKind; 5] = [
        TxKind::Transfer,
        TxKind::Deploy,
        TxKind::Call,
        TxKind::Delegate,
        TxKind::Undelegate,
    ];

    /// Stable protocol tag.
    pub const fn tag(self) -> u8 {
        match self {
            TxKind::Transfer => 0,
            TxKind::Deploy => 1,
            TxKind::Call => 2,
            TxKind::Delegate => 3,
            TxKind::Undelegate => 4,
        }
    }

    /// Parse a protocol tag, rejecting unknown values.
    pub const fn from_tag(tag: u8) -> Option<TxKind> {
        match tag {
            0 => Some(TxKind::Transfer),
            1 => Some(TxKind::Deploy),
            2 => Some(TxKind::Call),
            3 => Some(TxKind::Delegate),
            4 => Some(TxKind::Undelegate),
            _ => None,
        }
    }
}

impl Encode for TxKind {
    fn encode(&self, writer: &mut Writer) {
        writer.push_byte(self.tag());
    }
}

impl Decode for TxKind {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let tag = u8::decode(reader)?;
        TxKind::from_tag(tag).ok_or(CodecError::InvalidEnumTag("TxKind", tag))
    }
}
