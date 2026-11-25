//! W5 — hardware-key device protocol (HID/CCID-like) with on-screen
//! confirmation.
//!
//! A hardware key must never sign anything the human did not see on its own
//! screen. This module is the deterministic simulation of such a device: a
//! fixed frame protocol ([`DeviceCommand`] in, [`DeviceResponse`] out), a
//! screen derived exclusively from the transaction bytes (D-W5-1) and a
//! confirmation binding (D-W5-3) under which every path except
//! `Armed -> Confirm` is refused as blind signing.
//!
//! Design of record: `docs/w5-hardware-plan.md` (D-W5-1..D-W5-4). Vectors:
//! `tools/gen_w5_vectors.py` -> `tests/fixtures/w5_hardware_vectors.json`,
//! asserted byte-for-byte by `tests/w5_hardware.rs`.
//!
//! NC targets (`tools/nc_w5.py`):
//! 1. `sign_blind` acceptance -> `blind_confirmation_rejected` red;
//! 2. `Deny` ignored -> `deny_transfer_basic` red;
//! 3. display binding dropped -> `display_recipient_substitution` red;
//! 4. chain check dropped -> `foreign_chain_refused` red;
//! 5. amount render altered -> `confirm_transfer_high_value` red;
//! 6. payload decode check dropped -> `undecodable_payload_refused` red.

use std::fmt;

use aura_tx::{Tx, TxKind, UnsignedTx};
use aura_types::Address;

use crate::cli::stake_payload;
use crate::secret::SecretKey32;

/// Device firmware identity reported for `GetVersion`.
pub const DEVICE_VERSION: &str = "auralite-hw-1";

/// What the device screen shows for one signing round (D-W5-1).
///
/// Every field is derived from the canonical unsigned transaction bytes the
/// device is asked to sign — never from host-supplied strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayFrame {
    /// Chain id the transaction is valid on.
    pub chain: String,
    /// Lowercase transaction kind (`transfer`, `delegate`, `undelegate`).
    pub kind: String,
    /// Signer address (bech32 in `render_lines`).
    pub signer: Address,
    /// Per-account sequence number.
    pub nonce: u64,
    /// Recipient shown for `transfer` payloads.
    pub recipient: Option<Address>,
    /// Validator shown for `delegate`/`undelegate` payloads.
    pub validator: Option<Address>,
    /// Amount shown for `transfer`/`delegate`/`undelegate` payloads.
    pub amount: Option<u64>,
    /// Maximum gas the sender buys.
    pub gas: u64,
    /// Maximum total fee the sender pays.
    pub max_fee: u128,
    /// Priority tip.
    pub tip: u128,
    /// Last height at which the transaction may be included.
    pub expiry: u64,
}

impl DisplayFrame {
    /// Render the screen as deterministic `display: key=value` lines.
    ///
    /// NC target: `tools/nc_w5.py` alters the `amount` line and requires
    /// `confirm_transfer_high_value` to go red.
    #[must_use]
    pub fn render_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("display: chain={}", self.chain),
            format!("display: kind={}", self.kind),
            format!("display: signer={}", self.signer.to_bech32()),
            format!("display: nonce={}", self.nonce),
        ];
        if let Some(to) = self.recipient {
            lines.push(format!("display: to={}", to.to_bech32()));
        }
        if let Some(validator) = self.validator {
            lines.push(format!("display: validator={}", validator.to_bech32()));
        }
        if let Some(amount) = self.amount {
            lines.push(format!("display: amount={amount}"));
        }
        lines.push(format!("display: gas={}", self.gas));
        lines.push(format!("display: max_fee={}", self.max_fee));
        lines.push(format!("display: tip={}", self.tip));
        lines.push(format!("display: expiry={}", self.expiry));
        lines
    }

    /// Derive the screen from the transaction the device will sign (D-W5-1).
    ///
    /// # Errors
    ///
    /// [`HardwareError::UndecodablePayload`] when the payload cannot be fully
    /// decoded and displayed (D-W5-2) — the device never signs bytes it
    /// cannot show.
    pub fn derive(unsigned: &UnsignedTx) -> Result<Self, HardwareError> {
        let kind = match unsigned.kind {
            TxKind::Transfer => "transfer",
            TxKind::Delegate => "delegate",
            TxKind::Undelegate => "undelegate",
            TxKind::Deploy | TxKind::Call => return Err(HardwareError::UndecodablePayload),
        };
        let (recipient, validator, amount) = match unsigned.kind {
            TxKind::Transfer => {
                if unsigned.payload.len() != 28 {
                    return Err(HardwareError::UndecodablePayload);
                }
                let to = Address::from_bytes(unsigned.payload[0..20].try_into().expect("20 bytes"));
                let amount =
                    u64::from_le_bytes(unsigned.payload[20..28].try_into().expect("8 bytes"));
                (Some(to), None, Some(amount))
            }
            TxKind::Delegate | TxKind::Undelegate => {
                let (validator, amount) = decode_stake(&unsigned.payload, unsigned.kind)
                    .ok_or(HardwareError::UndecodablePayload)?;
                (None, Some(validator), Some(amount))
            }
            TxKind::Deploy | TxKind::Call => return Err(HardwareError::UndecodablePayload),
        };
        Ok(DisplayFrame {
            chain: unsigned.chain_id.as_str().to_owned(),
            kind: kind.to_owned(),
            signer: unsigned.signer,
            nonce: unsigned.nonce.value(),
            recipient,
            validator,
            amount,
            gas: unsigned.gas_limit.value(),
            max_fee: unsigned.max_fee.units(),
            tip: unsigned.tip.units(),
            expiry: unsigned.expiry.value(),
        })
    }
}

/// Decode a canonical stake payload by re-encoding it (D-W5-2).
///
/// The trailing `validator (20) || amount (LE u64)` is re-encoded through the
/// canonical `stake_payload` builder and must reproduce the payload exactly.
fn decode_stake(payload: &[u8], kind: TxKind) -> Option<(Address, u64)> {
    if payload.len() < 29 {
        return None;
    }
    let amount = u64::from_le_bytes(payload[payload.len() - 8..].try_into().ok()?);
    let validator = Address::from_bytes(
        payload[payload.len() - 28..payload.len() - 8]
            .try_into()
            .ok()?,
    );
    let action = match kind {
        TxKind::Delegate => crate::cli::StakeAction::Delegate,
        TxKind::Undelegate => crate::cli::StakeAction::Undelegate,
        _ => return None,
    };
    let canonical = stake_payload(action, &validator, amount);
    if canonical == payload {
        Some((validator, amount))
    } else {
        None
    }
}

/// The host's preview of what the user is about to approve.
///
/// A preview that disagrees with the device-derived display is refused
/// (`display mismatch`) before the human is asked (D-W5-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HostClaim {
    /// Recipient the host claims the user is sending to.
    pub recipient: Option<Address>,
    /// Amount the host claims the user is sending.
    pub amount: Option<u64>,
}

impl HostClaim {
    /// Whether every field the host claims agrees with the derived display.
    #[must_use]
    pub fn matches(&self, frame: &DisplayFrame) -> bool {
        self.recipient.is_none_or(|r| Some(r) == frame.recipient)
            && self.amount.is_none_or(|a| Some(a) == frame.amount)
    }
}

/// The human's answer on the device screen (D-W5-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanDecision {
    /// The human approved exactly the displayed fields.
    Confirm,
    /// The human rejected the request.
    Deny,
}

/// Protocol-level refusals of the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardwareError {
    /// The transaction targets a chain the device is not provisioned for.
    ForeignChainId {
        /// Chain id found in the transaction.
        found: String,
        /// Chain id the device is provisioned for.
        expected: String,
    },
    /// The device refuses to sign a payload it cannot fully display.
    UndecodablePayload,
    /// The host preview disagrees with the device-derived display.
    DisplayMismatch,
    /// The human denied the request.
    DeniedByUser,
    /// A signing request that skipped the display/confirm round trip.
    BlindConfirmationRejected,
    /// The signing primitive failed.
    SigningFailed,
}

impl fmt::Display for HardwareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ForeignChainId { .. } => write!(f, "foreign chain-id"),
            Self::UndecodablePayload => write!(f, "undecodable payload"),
            Self::DisplayMismatch => write!(f, "display mismatch"),
            Self::DeniedByUser => write!(f, "denied by user"),
            Self::BlindConfirmationRejected => write!(f, "blind confirmation rejected"),
            Self::SigningFailed => write!(f, "signing failed"),
        }
    }
}

/// A host -> device request frame (HID/CCID-like command set).
#[derive(Debug)]
pub enum DeviceCommand<'a> {
    /// Ask for the firmware identity.
    GetVersion,
    /// Open a signing round: derive and display the screen (D-W5-1).
    BeginSign {
        /// The canonical unsigned transaction to display and sign.
        unsigned: &'a UnsignedTx,
        /// Optional host preview to bind against the derived display.
        claim: Option<&'a HostClaim>,
    },
    /// Deliver the human's answer for the open round (D-W5-3).
    Decide {
        /// The answer shown on the device screen.
        decision: HumanDecision,
    },
    /// Ask the device to sign unseen bytes — the blind-signing attack.
    BlindSign {
        /// Raw transaction bytes the host wants signed without display.
        body: &'a [u8],
    },
}

/// A device -> host response frame.
#[derive(Debug)]
pub enum DeviceResponse {
    /// Firmware identity for `GetVersion`.
    Version(&'static str),
    /// The rendered screen for an opened round.
    Display(DisplayFrame),
    /// The signed transaction after an explicit `Confirm`.
    Signed(Tx),
    /// The device refused the request.
    Refused(HardwareError),
}

/// Internal round state (D-W5-3: `Idle -> Armed -> Idle`).
enum Phase {
    /// No round is open; any `Decide` is a blind signing attempt.
    Idle,
    /// A round is open and displayed to the human.
    Armed {
        /// The transaction the round will sign on `Confirm`.
        unsigned: UnsignedTx,
    },
}

/// The simulated HID/CCID-like secure element.
///
/// Holds the signing key, the chain it is provisioned for and the round
/// state. Nothing is signed without `Armed -> Confirm` (D-W5-3).
pub struct HardwareDevice {
    /// Ed25519 signing key held by the device (never leaves it).
    secret: SecretKey32,
    /// Chain the device is provisioned for.
    expected_chain: String,
    /// Round state.
    phase: Phase,
}

impl HardwareDevice {
    /// Provision the device with a key and its chain.
    #[must_use]
    pub fn new(secret: SecretKey32, expected_chain: impl Into<String>) -> Self {
        Self {
            secret,
            expected_chain: expected_chain.into(),
            phase: Phase::Idle,
        }
    }

    /// Exchange one protocol frame.
    ///
    /// # Errors
    ///
    /// Never panics; refusals arrive as [`DeviceResponse::Refused`].
    #[must_use]
    pub fn exchange(&mut self, command: DeviceCommand<'_>) -> DeviceResponse {
        match command {
            DeviceCommand::GetVersion => DeviceResponse::Version(DEVICE_VERSION),
            DeviceCommand::BeginSign { unsigned, claim } => {
                match self.begin_sign(unsigned, claim) {
                    Ok(frame) => DeviceResponse::Display(frame),
                    Err(e) => DeviceResponse::Refused(e),
                }
            }
            DeviceCommand::Decide { decision } => match self.decide(decision) {
                Ok(tx) => DeviceResponse::Signed(tx),
                Err(e) => DeviceResponse::Refused(e),
            },
            DeviceCommand::BlindSign { body } => match self.sign_blind(body) {
                Ok(tx) => DeviceResponse::Signed(tx),
                Err(e) => DeviceResponse::Refused(e),
            },
        }
    }

    /// Open a signing round: chain gate, display derivation, claim binding.
    fn begin_sign(
        &mut self,
        unsigned: &UnsignedTx,
        claim: Option<&HostClaim>,
    ) -> Result<DisplayFrame, HardwareError> {
        // The chain gate runs before anything is displayed (NC-4 target).
        if unsigned.chain_id.as_str() != self.expected_chain {
            return Err(HardwareError::ForeignChainId {
                found: unsigned.chain_id.as_str().to_owned(),
                expected: self.expected_chain.clone(),
            });
        }
        let frame = DisplayFrame::derive(unsigned)?;
        if let Some(claim) = claim {
            // A host preview that disagrees with the derived display is
            // refused before the human is asked (NC-3 target).
            if !claim.matches(&frame) {
                return Err(HardwareError::DisplayMismatch);
            }
        }
        self.phase = Phase::Armed {
            unsigned: unsigned.clone(),
        };
        Ok(frame)
    }

    /// Deliver the human's answer for the open round (D-W5-3).
    fn decide(&mut self, decision: HumanDecision) -> Result<Tx, HardwareError> {
        let Phase::Armed { unsigned } = std::mem::replace(&mut self.phase, Phase::Idle) else {
            // A decision without a displayed round is a blind signing attempt.
            return Err(HardwareError::BlindConfirmationRejected);
        };
        match decision {
            HumanDecision::Confirm => self.finish_sign(&unsigned),
            // A denial never signs and never latches the device (NC-2 target).
            HumanDecision::Deny => Err(HardwareError::DeniedByUser),
        }
    }

    /// The blind-signing attack surface: always refused (D-W5-3).
    fn sign_blind(&mut self, body: &[u8]) -> Result<Tx, HardwareError> {
        // Never sign unseen bytes — refused before any key use (NC-1 target).
        let _ = body;
        Err(HardwareError::BlindConfirmationRejected)
    }

    /// Sign the confirmed transaction under the fixed AuraLite `tx` domain
    /// (D-W5-4), producing a [`Tx`] the W3 offline path verifies.
    fn finish_sign(&self, unsigned: &UnsignedTx) -> Result<Tx, HardwareError> {
        unsigned
            .sign(self.secret.as_bytes())
            .map_err(|_| HardwareError::SigningFailed)
    }
}
