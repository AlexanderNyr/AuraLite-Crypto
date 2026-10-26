//! Signature envelope: algorithm tag + public key + 64-byte signature.

use aura_codec::{CodecError, Decode, Encode, Reader, Writer};

/// Ed25519 algorithm tag. Other tag values are reserved (secp256k1 = 1 is
/// intentionally **not** accepted in the Stage 1 tx format; PQ tags will live
/// in a separate range).
pub const SIG_ALGO_ED25519: u8 = 0;

/// Signature algorithm. Stage 1 accepts Ed25519 only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignatureAlgo {
    /// Ed25519 (RFC 8032), verified through `aura-crypto`.
    Ed25519,
}

impl SignatureAlgo {
    /// Stable protocol tag.
    pub const fn tag(self) -> u8 {
        SIG_ALGO_ED25519
    }

    /// Parse a protocol tag. Unknown algorithms are rejected.
    pub const fn from_tag(tag: u8) -> Option<SignatureAlgo> {
        match tag {
            SIG_ALGO_ED25519 => Some(SignatureAlgo::Ed25519),
            _ => None,
        }
    }
}

/// The signature attached to a [`crate::Tx`].
///
/// Ed25519 cannot recover the public key from a signature, so the key is part
/// of the wire format. [`crate::signer_address`] derives the 20-byte signer
/// from it and validation requires an exact match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    /// Signature algorithm.
    pub algo: SignatureAlgo,
    /// 32-byte Ed25519 public key.
    pub public_key: [u8; 32],
    /// 64-byte Ed25519 signature.
    pub bytes: [u8; 64],
}

impl Signature {
    /// Construct an Ed25519 signature envelope.
    pub fn ed25519(public_key: [u8; 32], bytes: [u8; 64]) -> Self {
        Signature {
            algo: SignatureAlgo::Ed25519,
            public_key,
            bytes,
        }
    }
}

impl Encode for Signature {
    fn encode(&self, writer: &mut Writer) {
        writer.push_byte(self.algo.tag());
        writer.push_bytes(&self.public_key);
        writer.push_bytes(&self.bytes);
    }
}

impl Decode for Signature {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let tag = u8::decode(reader)?;
        let algo =
            SignatureAlgo::from_tag(tag).ok_or(CodecError::InvalidEnumTag("SignatureAlgo", tag))?;
        let key_bytes = reader.read_exact(32)?;
        let mut public_key = [0u8; 32];
        public_key.copy_from_slice(key_bytes);
        let sig_bytes = reader.read_exact(64)?;
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(sig_bytes);
        Ok(Signature {
            algo,
            public_key,
            bytes,
        })
    }
}
