//! W4 — wallet-level M-of-N multisig contract.
//!
//! Design of record: `docs/w4-multisig-plan.md` (D-W4-1..D-W4-4).
//!
//! Deterministic documents (contract descriptor, proposal, approval) are
//! canonical binary blobs wrapped in JSON files; signatures always cover the
//! canonical bytes under the fixed AuraLite `contract` domain. This is the
//! wallet coordination layer over the W1 Ed25519 keys (the chain level is
//! B-series consensus/BLS).

use aura_crypto::{blake3_hash, hash_domain};
use aura_types::{Address, Domain};

/// Canonical prefix of a multisig contract descriptor (D-W4-1).
pub const DESCRIPTOR_PREFIX: &[u8] = b"AuraLite-Crypto/wallet-msig-v1\0";
/// Canonical prefix of a multisig proposal (D-W4-1).
pub const PROPOSAL_PREFIX: &[u8] = b"AuraLite-Crypto/wallet-msig-proposal-v1\0";

/// Fixed refusal messages (byte-for-byte asserted by the W4 vectors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsigError {
    /// Collected fewer approvals than the threshold (the named W4 control).
    ThresholdNotMet {
        /// Approvals that verified.
        collected: usize,
        /// Required threshold.
        threshold: u8,
    },
    /// Two approvals from the same member.
    DuplicateApproval,
    /// Approver is not in the contract member set.
    NotAMember,
    /// Current height is past the proposal expiry.
    ProposalExpired,
    /// Approval was made for a different proposal.
    WrongProposal,
    /// Proposal references a different contract id.
    WrongContract,
    /// Contract file cannot be parsed.
    BadContract,
    /// Proposal file cannot be parsed.
    BadProposal,
    /// Approval file cannot be parsed.
    BadApproval,
    /// Ed25519 signature does not verify under the fixed `contract` domain.
    SignatureInvalid,
}

impl std::fmt::Display for MsigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ThresholdNotMet {
                collected,
                threshold,
            } => {
                write!(f, "multisig: threshold not met ({collected}/{threshold})")
            }
            Self::DuplicateApproval => write!(f, "multisig: duplicate approval"),
            Self::NotAMember => write!(f, "multisig: not a member"),
            Self::ProposalExpired => write!(f, "multisig: proposal expired"),
            Self::WrongProposal => write!(f, "multisig: approval for a different proposal"),
            Self::WrongContract => write!(f, "multisig: proposal for a different contract"),
            Self::BadContract => write!(f, "multisig: bad contract file"),
            Self::BadProposal => write!(f, "multisig: bad proposal file"),
            Self::BadApproval => write!(f, "multisig: bad approval file"),
            Self::SignatureInvalid => write!(f, "signature verification failed"),
        }
    }
}

/// An M-of-N multisig contract descriptor (D-W4-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultisigDescriptor {
    /// Chain the contract operates on.
    pub chain: String,
    /// Required approvals (M).
    pub threshold: u8,
    /// Member public keys, sorted ascending.
    pub members: Vec<[u8; 32]>,
    /// Absolute height after which proposals expire.
    pub timeout: u64,
}

impl MultisigDescriptor {
    /// Canonical bytes: prefix || u8 chain len || chain || M || N || keys.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let chain = self.chain.as_bytes();
        let mut out = Vec::with_capacity(
            DESCRIPTOR_PREFIX.len() + 1 + chain.len() + 2 + self.members.len() * 32,
        );
        out.extend_from_slice(DESCRIPTOR_PREFIX);
        out.push(u8::try_from(chain.len()).expect("chain-id fits u8"));
        out.extend_from_slice(chain);
        out.push(self.threshold);
        out.push(u8::try_from(self.members.len()).expect("N fits u8"));
        for key in &self.members {
            out.extend_from_slice(key);
        }
        out
    }

    /// Contract id: `BLAKE3(canonical bytes)`.
    #[must_use]
    pub fn contract_id(&self) -> [u8; 32] {
        *blake3_hash(&self.canonical_bytes()).as_bytes()
    }

    /// Whether `public` is a contract member.
    #[must_use]
    pub fn is_member(&self, public: &[u8; 32]) -> bool {
        self.members.binary_search(public).is_ok()
    }
}

/// What a multisig proposal does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalAction {
    /// Move `amount` to `to`.
    Transfer {
        /// Recipient.
        to: Address,
        /// Amount in units.
        amount: u64,
    },
    /// Member rotation: the next contract state (D-W4-3).
    Rotate {
        /// The new descriptor.
        descriptor: MultisigDescriptor,
    },
}

/// A proposal bound to one contract (D-W4-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultisigProposal {
    /// Chain the proposal is valid on.
    pub chain: String,
    /// Contract id the proposal belongs to.
    pub contract: [u8; 32],
    /// Per-contract proposal counter.
    pub id: u64,
    /// Height after which approvals are refused.
    pub expiry: u64,
    /// What to do.
    pub action: ProposalAction,
}

impl MultisigProposal {
    /// Canonical proposal bytes (D-W4-1).
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let chain = self.chain.as_bytes();
        let mut out =
            Vec::with_capacity(PROPOSAL_PREFIX.len() + 1 + chain.len() + 32 + 8 + 8 + 1 + 64);
        out.extend_from_slice(PROPOSAL_PREFIX);
        out.push(u8::try_from(chain.len()).expect("chain-id fits u8"));
        out.extend_from_slice(chain);
        out.extend_from_slice(&self.contract);
        out.extend_from_slice(&self.id.to_le_bytes());
        out.extend_from_slice(&self.expiry.to_le_bytes());
        match &self.action {
            ProposalAction::Transfer { to, amount } => {
                out.push(0);
                out.extend_from_slice(to.as_bytes());
                out.extend_from_slice(&amount.to_le_bytes());
            }
            ProposalAction::Rotate { descriptor } => {
                out.push(1);
                out.extend_from_slice(&descriptor.canonical_bytes());
            }
        }
        out
    }

    /// Approval digest: `BLAKE3(domain_message(chain, "contract", bytes))`.
    #[must_use]
    pub fn approval_digest(&self) -> [u8; 32] {
        *hash_domain(&self.chain, Domain::Contract, &self.canonical_bytes()).as_bytes()
    }
}

/// One member's approval: signature over the proposal's canonical bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    /// Member public key.
    pub member: [u8; 32],
    /// The exact proposal bytes this approval was made for.
    pub proposal: Vec<u8>,
    /// Ed25519 signature over [`MultisigProposal::approval_digest`].
    pub sig: [u8; 64],
}

/// Verify one approval against the proposal being collected (D-W4-4 order).
pub fn verify_approval(
    approval: &Approval,
    descriptor: &MultisigDescriptor,
    proposal: &MultisigProposal,
) -> Result<(), MsigError> {
    if !descriptor.is_member(&approval.member) {
        return Err(MsigError::NotAMember);
    }
    let expected = proposal.canonical_bytes();
    if approval.proposal != expected {
        return Err(MsigError::WrongProposal);
    }
    let digest = proposal.approval_digest();
    aura_crypto::verify_raw(&approval.member, &digest, &approval.sig)
        .map_err(|_| MsigError::SignatureInvalid)
}
