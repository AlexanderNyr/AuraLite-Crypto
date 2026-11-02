//! Peer identity (`peer_id = hash(pubkey)`) and domain-separated handshake signing.

use std::fmt;

use aura_crypto::{
    SecretBytes, blake3_hash, constant_time_eq, ed25519_public_key, sign_raw, verify_raw,
};

use crate::error::P2pError;

/// Byte length of a canonical [`PeerId`] (`BLAKE3(ed25519_pubkey)`).
pub const PEER_ID_LEN: usize = 32;

/// Domain tag for N1 Noise-style handshake transcript signatures.
pub const HANDSHAKE_DOMAIN_TAG: &[u8] = b"AuraLite-Crypto/p2p-noise-handshake-v1\0";

/// Role in the two-party N1 transport handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeRole {
    /// Outbound connection initiator.
    Initiator,
    /// Inbound listener responder.
    Responder,
}

impl HandshakeRole {
    /// Canonical byte tag bound into the signed handshake digest.
    pub const fn tag(self) -> &'static [u8] {
        match self {
            Self::Initiator => b"initiator",
            Self::Responder => b"responder",
        }
    }
}

/// Deterministic peer identifier: `peer_id = BLAKE3(ed25519_public_key)` (`NET_PLAN` D-N1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerId([u8; PEER_ID_LEN]);

impl PeerId {
    /// Construct a [`PeerId`] directly from raw 32-byte digest bytes.
    pub const fn from_bytes(bytes: [u8; PEER_ID_LEN]) -> Self {
        Self(bytes)
    }

    /// Derive the canonical [`PeerId`] for an Ed25519 public key (`BLAKE3(pubkey)`).
    pub fn from_public_key(public_key: &[u8; 32]) -> Self {
        Self(*blake3_hash(public_key).as_bytes())
    }

    /// Borrow the underlying 32-byte digest.
    pub const fn as_bytes(&self) -> &[u8; PEER_ID_LEN] {
        &self.0
    }

    /// Verify in constant time that `self` equals `BLAKE3(public_key)`.
    pub fn verify_public_key(&self, public_key: &[u8; 32]) -> Result<(), P2pError> {
        let derived = Self::from_public_key(public_key);
        if !constant_time_eq(&self.0, &derived.0) {
            return Err(P2pError::IdentityMismatch {
                claimed: *self,
                derived,
            });
        }
        Ok(())
    }

    /// Lowercase hexadecimal representation.
    pub fn to_hex(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(PEER_ID_LEN * 2);
        for byte in self.0 {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        out
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PeerId({})", &self.to_hex()[..16])
    }
}

/// Local node cryptographic identity for P2P transport and gossip signing.
pub struct PeerIdentity {
    seed: SecretBytes,
    public_key: [u8; 32],
    peer_id: PeerId,
}

impl PeerIdentity {
    /// Deterministically derive a [`PeerIdentity`] from a 32-byte Ed25519 secret seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let public_key = ed25519_public_key(&seed);
        let peer_id = PeerId::from_public_key(&public_key);
        Self {
            seed: SecretBytes::from_slice(&seed).expect("32-byte seed"),
            public_key,
            peer_id,
        }
    }

    /// The 32-byte Ed25519 public key.
    pub const fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }

    /// The derived `PeerId = BLAKE3(public_key)`.
    pub const fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// Sign a handshake transcript hash under `(chain_id, role)`.
    pub fn sign_transcript(
        &self,
        chain_id: &str,
        role: HandshakeRole,
        transcript_hash: &[u8; 32],
    ) -> [u8; 64] {
        let digest = handshake_signing_digest(chain_id, role, transcript_hash);
        sign_raw(self.seed.as_bytes(), &digest)
    }

    /// Sign a domain-separated 32-byte digest with the local Ed25519 secret key.
    pub fn sign_digest(&self, digest: &[u8; 32]) -> [u8; 64] {
        sign_raw(self.seed.as_bytes(), digest)
    }
}

/// Verify a peer's Ed25519 signature over the handshake transcript digest.
pub fn verify_transcript_signature(
    public_key: &[u8; 32],
    chain_id: &str,
    role: HandshakeRole,
    transcript_hash: &[u8; 32],
    signature: &[u8; 64],
) -> Result<(), P2pError> {
    let digest = handshake_signing_digest(chain_id, role, transcript_hash);
    verify_raw(public_key, &digest, signature).map_err(|_| P2pError::InvalidHandshakeSignature)
}

fn handshake_signing_digest(
    chain_id: &str,
    role: HandshakeRole,
    transcript_hash: &[u8; 32],
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(HANDSHAKE_DOMAIN_TAG.len() + chain_id.len() + 64);
    buf.extend_from_slice(HANDSHAKE_DOMAIN_TAG);
    buf.extend_from_slice(&(chain_id.len() as u32).to_le_bytes());
    buf.extend_from_slice(chain_id.as_bytes());
    buf.extend_from_slice(&(role.tag().len() as u32).to_le_bytes());
    buf.extend_from_slice(role.tag());
    buf.extend_from_slice(transcript_hash);
    *blake3_hash(&buf).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_id_must_match_public_key_hash() {
        let id_a = PeerIdentity::from_seed([0x11; 32]);
        let id_b = PeerIdentity::from_seed([0x22; 32]);
        assert_eq!(
            id_a.peer_id(),
            PeerId::from_public_key(id_a.public_key()),
            "peer_id is BLAKE3(public_key)"
        );
        assert!(id_a.peer_id().verify_public_key(id_a.public_key()).is_ok());

        let err = id_a
            .peer_id()
            .verify_public_key(id_b.public_key())
            .expect_err("mismatched public key must be rejected");
        assert_eq!(
            err,
            P2pError::IdentityMismatch {
                claimed: id_a.peer_id(),
                derived: id_b.peer_id(),
            }
        );
    }

    #[test]
    fn transcript_signature_binds_chain_role_and_pubkey() {
        let id_a = PeerIdentity::from_seed([0x33; 32]);
        let id_b = PeerIdentity::from_seed([0x44; 32]);
        let transcript = [0xAA; 32];
        let sig = id_a.sign_transcript("auralite-test-1", HandshakeRole::Initiator, &transcript);

        assert!(
            verify_transcript_signature(
                id_a.public_key(),
                "auralite-test-1",
                HandshakeRole::Initiator,
                &transcript,
                &sig,
            )
            .is_ok()
        );
        assert_eq!(
            verify_transcript_signature(
                id_a.public_key(),
                "auralite-other-1",
                HandshakeRole::Initiator,
                &transcript,
                &sig,
            ),
            Err(P2pError::InvalidHandshakeSignature)
        );
        assert_eq!(
            verify_transcript_signature(
                id_a.public_key(),
                "auralite-test-1",
                HandshakeRole::Responder,
                &transcript,
                &sig,
            ),
            Err(P2pError::InvalidHandshakeSignature)
        );
        assert_eq!(
            verify_transcript_signature(
                id_b.public_key(),
                "auralite-test-1",
                HandshakeRole::Initiator,
                &transcript,
                &sig,
            ),
            Err(P2pError::InvalidHandshakeSignature)
        );
    }
}
