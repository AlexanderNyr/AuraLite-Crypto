//! Canonical transaction, signing bytes, txid and validation.

use aura_codec::{
    CODEC_VERSION, CodecError, Decode, Encode, Reader, Writer, decode_canonical, encode_to_bytes,
};
use aura_crypto::{blake3_hash, ed25519_public_key};
use aura_types::{ADDRESS_LEN, Address, Amount, ChainId, Domain, Gas, Hash, Height, Nonce};

use crate::error::TxError;
use crate::kind::TxKind;
use crate::signature::Signature;

/// Prefix of the bytes that are signed for a transaction.
///
/// NC target: `tools/nc_c4.py` changes this constant and requires the C4
/// golden vectors to go red. A change here invalidates every existing
/// signature and requires a protocol version bump.
pub const SIGNING_CONTEXT: &[u8] = b"AuraLite-Crypto/tx-signing-v1\x00";

/// Prefix of the bytes hashed to produce a transaction id.
///
/// NC target: `tools/nc_c4.py` changes this constant and requires the txid
/// golden vectors to go red.
pub const TXID_CONTEXT: &[u8] = b"AuraLite-Crypto/txid-v1\x00";

/// Prefix for deriving a 20-byte account address from a 32-byte public key.
pub const ADDRESS_CONTEXT: &[u8] = b"AuraLite-Crypto/address-v1\x00";

/// Maximum payload size. Aligned with `aura-codec`'s `MAX_ITEMS` so a decoded
/// payload can never exceed what the canonical decoder accepts.
pub const MAX_TX_PAYLOAD: usize = 65_536;

/// Derive the canonical 20-byte address for an Ed25519 public key.
///
/// `address = BLAKE3(ADDRESS_CONTEXT || public_key)[0..20]`. The binding is
/// checked on every verification, so a transaction cannot claim someone
/// else's `signer`.
pub fn signer_address(public_key: &[u8; 32]) -> Address {
    let mut input = Vec::with_capacity(ADDRESS_CONTEXT.len() + 32);
    input.extend_from_slice(ADDRESS_CONTEXT);
    input.extend_from_slice(public_key);
    let digest = blake3_hash(&input);
    let mut bytes = [0u8; ADDRESS_LEN];
    bytes.copy_from_slice(&digest.as_bytes()[..ADDRESS_LEN]);
    Address::from_bytes(bytes)
}

/// Construction parameters for [`UnsignedTx::new`].
///
/// A transaction genuinely has nine unsigned protocol fields. Grouping them
/// keeps the constructor readable and satisfies `clippy::too_many_arguments`
/// without suppressing the lint. `codec_version` is added by the constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxParams {
    /// Chain this transaction is valid on (replay protection).
    pub chain_id: ChainId,
    /// 20-byte account address derived from the signer's public key.
    pub signer: Address,
    /// Per-account sequence number.
    pub nonce: Nonce,
    /// What the transaction does.
    pub kind: TxKind,
    /// Opaque, length-prefixed call data.
    pub payload: Vec<u8>,
    /// Maximum gas the sender will buy.
    pub gas_limit: Gas,
    /// Maximum total fee the sender will pay.
    pub max_fee: Amount,
    /// Priority tip, must not exceed `max_fee`.
    pub tip: Amount,
    /// Last consensus height at which the transaction may be included.
    pub expiry: Height,
}

/// The unsigned part of a transaction. Field order is the wire order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsignedTx {
    /// Wire format version, must equal [`CODEC_VERSION`].
    pub codec_version: u8,
    /// Chain this transaction is valid on (replay protection).
    pub chain_id: ChainId,
    /// 20-byte account address derived from the signer's public key.
    pub signer: Address,
    /// Per-account sequence number.
    pub nonce: Nonce,
    /// What the transaction does.
    pub kind: TxKind,
    /// Opaque, length-prefixed call data.
    pub payload: Vec<u8>,
    /// Maximum gas the sender will buy.
    pub gas_limit: Gas,
    /// Maximum total fee the sender will pay.
    pub max_fee: Amount,
    /// Priority tip, must not exceed `max_fee`.
    pub tip: Amount,
    /// Last consensus height at which the transaction may be included.
    pub expiry: Height,
}

impl UnsignedTx {
    /// Construct and structurally validate an unsigned transaction.
    pub fn new(params: TxParams) -> Result<Self, TxError> {
        if params.payload.len() > MAX_TX_PAYLOAD {
            return Err(TxError::PayloadTooLarge(params.payload.len()));
        }
        if params.gas_limit.value() == 0 {
            return Err(TxError::ZeroGasLimit);
        }
        if params.tip > params.max_fee {
            return Err(TxError::TipExceedsMaxFee);
        }
        Ok(UnsignedTx {
            codec_version: CODEC_VERSION,
            chain_id: params.chain_id,
            signer: params.signer,
            nonce: params.nonce,
            kind: params.kind,
            payload: params.payload,
            gas_limit: params.gas_limit,
            max_fee: params.max_fee,
            tip: params.tip,
            expiry: params.expiry,
        })
    }

    /// The exact bytes that are signed: context plus the canonical encoding
    /// of every unsigned field. The signature is **not** included.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.push_bytes(SIGNING_CONTEXT);
        self.encode(&mut writer);
        writer.into_bytes()
    }

    /// Sign with a seed. The seed's public key must derive to `self.signer`.
    pub fn sign(&self, seed: &[u8; 32]) -> Result<Tx, TxError> {
        let public_key = ed25519_public_key(seed);
        if signer_address(&public_key) != self.signer {
            return Err(TxError::SignerMismatch);
        }
        let signature = aura_crypto::sign(
            seed,
            self.chain_id.as_str(),
            Domain::Tx,
            &self.signing_bytes(),
        );
        Ok(Tx {
            unsigned: self.clone(),
            sig: Signature::ed25519(public_key, signature),
        })
    }
}

impl Encode for UnsignedTx {
    fn encode(&self, writer: &mut Writer) {
        self.codec_version.encode(writer);
        self.chain_id.encode(writer);
        self.signer.encode(writer);
        self.nonce.encode(writer);
        self.kind.encode(writer);
        self.payload.encode(writer);
        self.gas_limit.encode(writer);
        self.max_fee.encode(writer);
        self.tip.encode(writer);
        self.expiry.encode(writer);
    }
}

impl Decode for UnsignedTx {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let version = u8::decode(reader)?;
        if version != CODEC_VERSION {
            return Err(CodecError::UnsupportedVersion(CODEC_VERSION, version));
        }
        Ok(UnsignedTx {
            codec_version: version,
            chain_id: ChainId::decode(reader)?,
            signer: Address::decode(reader)?,
            nonce: Nonce::decode(reader)?,
            kind: TxKind::decode(reader)?,
            payload: Vec::<u8>::decode(reader)?,
            gas_limit: Gas::decode(reader)?,
            max_fee: Amount::decode(reader)?,
            tip: Amount::decode(reader)?,
            expiry: Height::decode(reader)?,
        })
    }
}

/// A complete transaction: unsigned fields plus the signature envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tx {
    /// All unsigned fields, in canonical order.
    pub unsigned: UnsignedTx,
    /// Algorithm tag, public key and 64-byte signature.
    pub sig: Signature,
}

fn verify_ed25519(pk: &[u8; 32], chain: &str, msg: &[u8], sig: &[u8; 64]) -> Result<(), TxError> {
    aura_crypto::verify(pk, chain, Domain::Tx, msg, sig).map_err(|_| TxError::InvalidSignature)
}

impl Tx {
    /// Bytes that are signed (delegates to [`UnsignedTx::signing_bytes`]).
    pub fn signing_bytes(&self) -> Vec<u8> {
        self.unsigned.signing_bytes()
    }

    /// Canonical wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        encode_to_bytes(self)
    }

    /// Transaction id: `BLAKE3(TXID_CONTEXT || canonical_bytes)`.
    ///
    /// Includes the signature, so two different signatures over the same
    /// unsigned payload have different ids.
    pub fn txid(&self) -> Hash {
        let encoded = self.to_bytes();
        let mut input = Vec::with_capacity(TXID_CONTEXT.len() + encoded.len());
        input.extend_from_slice(TXID_CONTEXT);
        input.extend_from_slice(&encoded);
        blake3_hash(&input)
    }

    /// Decode with full C2 canonicality: no trailing bytes, no non-minimal
    /// varints, field order fixed, re-encode must reproduce the input.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, TxError> {
        Ok(decode_canonical::<Tx>(bytes)?)
    }

    /// Verify the Ed25519 signature and the signer↔public-key binding.
    ///
    /// NC target: `tools/nc_c4.py` disables the binding check and requires
    /// the signer-mismatch test to go red.
    pub fn verify_signature(&self) -> Result<(), TxError> {
        if signer_address(&self.sig.public_key) != self.unsigned.signer {
            return Err(TxError::SignerMismatch);
        }
        let message = self.signing_bytes();
        let chain = self.unsigned.chain_id.as_str();
        verify_ed25519(&self.sig.public_key, chain, &message, &self.sig.bytes)
    }

    /// Full pre-execution validation for `expected_chain` at `height`.
    ///
    /// NC target: `tools/nc_c4.py` disables the chain check and requires the
    /// replay test to go red.
    pub fn validate(&self, expected_chain: &str, height: Height) -> Result<(), TxError> {
        if self.unsigned.codec_version != CODEC_VERSION {
            return Err(TxError::UnsupportedCodecVersion(
                CODEC_VERSION,
                self.unsigned.codec_version,
            ));
        }
        if self.unsigned.chain_id.as_str() != expected_chain {
            return Err(TxError::WrongChain(
                expected_chain.to_owned(),
                self.unsigned.chain_id.as_str().to_owned(),
            ));
        }
        if self.unsigned.payload.len() > MAX_TX_PAYLOAD {
            return Err(TxError::PayloadTooLarge(self.unsigned.payload.len()));
        }
        if self.unsigned.gas_limit.value() == 0 {
            return Err(TxError::ZeroGasLimit);
        }
        if self.unsigned.tip > self.unsigned.max_fee {
            return Err(TxError::TipExceedsMaxFee);
        }
        if height.value() > self.unsigned.expiry.value() {
            return Err(TxError::Expired(
                self.unsigned.expiry.value(),
                height.value(),
            ));
        }
        self.verify_signature()
    }
}

impl Encode for Tx {
    fn encode(&self, writer: &mut Writer) {
        self.unsigned.encode(writer);
        self.sig.encode(writer);
    }
}

impl Decode for Tx {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let unsigned = UnsignedTx::decode(reader)?;
        let sig = Signature::decode(reader)?;
        Ok(Tx { unsigned, sig })
    }
}
