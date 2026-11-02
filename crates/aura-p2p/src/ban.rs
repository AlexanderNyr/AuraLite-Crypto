//! Peer and IP ban list for the N1 transport swarm.

use std::collections::BTreeMap;
use std::fmt;
use std::net::IpAddr;

use crate::error::P2pError;
use crate::identity::PeerId;

/// Structured reason why a peer or IP address was placed on the ban list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BanReason {
    /// Claimed `PeerId` did not match `BLAKE3(public_key)`.
    IdentityMismatch,
    /// Presented a valid identity different from the expected dial target.
    IdentitySubstitution,
    /// Failed handshake Ed25519 signature verification.
    InvalidHandshakeSignature,
    /// Sent an oversized or unauthenticated frame.
    ProtocolViolation(String),
    /// Manual or operator-configured ban.
    Manual(String),
}

impl fmt::Display for BanReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdentityMismatch => f.write_str("identity_mismatch"),
            Self::IdentitySubstitution => f.write_str("identity_substitution"),
            Self::InvalidHandshakeSignature => f.write_str("invalid_handshake_signature"),
            Self::ProtocolViolation(detail) => write!(f, "protocol_violation:{detail}"),
            Self::Manual(detail) => write!(f, "manual:{detail}"),
        }
    }
}

/// One active ban record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BanEntry {
    /// Reason the peer or IP was banned.
    pub reason: BanReason,
    /// Optional expiration timestamp in Unix milliseconds (`None` = permanent).
    pub expires_at_ms: Option<u64>,
}

impl BanEntry {
    /// True when the ban is still active at `now_ms`.
    pub const fn is_active(&self, now_ms: u64) -> bool {
        match self.expires_at_ms {
            Some(expiry) => now_ms < expiry,
            None => true,
        }
    }
}

/// Deterministic ban list keyed by [`PeerId`] and [`IpAddr`].
#[derive(Debug, Clone, Default)]
pub struct BanList {
    peers: BTreeMap<PeerId, BanEntry>,
    ips: BTreeMap<IpAddr, BanEntry>,
}

impl BanList {
    /// Create an empty [`BanList`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Ban a [`PeerId`] permanently (`expires_at_ms = None`) or until `expires_at_ms`.
    pub fn ban_peer(&mut self, peer_id: PeerId, reason: BanReason, expires_at_ms: Option<u64>) {
        self.peers.insert(
            peer_id,
            BanEntry {
                reason,
                expires_at_ms,
            },
        );
    }

    /// Ban an [`IpAddr`] permanently (`expires_at_ms = None`) or until `expires_at_ms`.
    pub fn ban_ip(&mut self, ip: IpAddr, reason: BanReason, expires_at_ms: Option<u64>) {
        self.ips.insert(
            ip,
            BanEntry {
                reason,
                expires_at_ms,
            },
        );
    }

    /// Remove a [`PeerId`] from the ban list.
    pub fn unban_peer(&mut self, peer_id: &PeerId) -> bool {
        self.peers.remove(peer_id).is_some()
    }

    /// Remove an [`IpAddr`] from the ban list.
    pub fn unban_ip(&mut self, ip: &IpAddr) -> bool {
        self.ips.remove(ip).is_some()
    }

    /// Check whether `peer_id` is currently banned at `now_ms`.
    pub fn check_peer(&self, peer_id: &PeerId, now_ms: u64) -> Result<(), P2pError> {
        if let Some(entry) = self.peers.get(peer_id)
            && entry.is_active(now_ms)
        {
            return Err(P2pError::PeerBanned {
                peer_id: *peer_id,
                reason: entry.reason.to_string(),
            });
        }
        Ok(())
    }

    /// Check whether `ip` is currently banned at `now_ms`.
    pub fn check_ip(&self, ip: &IpAddr, now_ms: u64) -> Result<(), P2pError> {
        if let Some(entry) = self.ips.get(ip)
            && entry.is_active(now_ms)
        {
            return Err(P2pError::IpBanned {
                ip: *ip,
                reason: entry.reason.to_string(),
            });
        }
        Ok(())
    }

    /// True if `peer_id` has an active ban at `now_ms`.
    pub fn is_peer_banned(&self, peer_id: &PeerId, now_ms: u64) -> bool {
        self.check_peer(peer_id, now_ms).is_err()
    }

    /// Remove all expired peer and IP ban entries as of `now_ms`.
    pub fn prune_expired(&mut self, now_ms: u64) {
        self.peers.retain(|_, entry| entry.is_active(now_ms));
        self.ips.retain(|_, entry| entry.is_active(now_ms));
    }

    /// Number of peer entries currently stored.
    pub fn banned_peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Number of IP entries currently stored.
    pub fn banned_ip_count(&self) -> usize {
        self.ips.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::PeerIdentity;

    #[test]
    fn banned_peer_and_ip_are_rejected_until_expiry() {
        let mut bans = BanList::new();
        let peer = PeerIdentity::from_seed([0x77; 32]).peer_id();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        bans.ban_peer(peer, BanReason::IdentitySubstitution, Some(1_000));
        bans.ban_ip(ip, BanReason::Manual("rate_limit".into()), None);

        assert!(bans.check_peer(&peer, 500).is_err());
        assert!(bans.check_peer(&peer, 1_000).is_ok());
        assert!(bans.check_ip(&ip, 5_000).is_err());

        bans.prune_expired(1_000);
        assert_eq!(bans.banned_peer_count(), 0);
        assert_eq!(bans.banned_ip_count(), 1);
        assert!(bans.unban_ip(&ip));
        assert!(bans.check_ip(&ip, 5_000).is_ok());
    }
}
