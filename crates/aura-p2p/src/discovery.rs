//! N3 peer discovery: Kademlia DHT, bootstrap list, NAT/UPnP port mapping,
//! and Circuit Relay v2 for peers behind symmetric NAT.
//!
//! Every peer record arriving over discovery is cryptographically verified
//! (`D-N3`):
//! - `PeerId == BLAKE3(ed25519_public_key)` in constant time,
//! - domain-separated Ed25519 signature over `(DISCOVERY_RECORD_DOMAIN, chain_id, seq, addrs, nat_status)`,
//! - strictly increasing sequence number (`seq`) per `PeerId`,
//! - active `BanList` enforcement before any record enters a Kademlia k-bucket.

use std::collections::{BTreeMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use aura_crypto::{blake3_hash, verify_raw};

use crate::ban::BanList;
use crate::error::P2pError;
use crate::identity::{PEER_ID_LEN, PeerId, PeerIdentity};
use crate::transport::MAX_FRAME_BYTES;

/// Domain tag for N3 signed Kademlia peer records.
pub const DISCOVERY_RECORD_DOMAIN: &[u8] = b"AuraLite-Crypto/p2p-discovery-record-v1\0";

/// Default Kademlia k-bucket capacity (`k = 20`).
pub const KADEMLIA_K: usize = 20;

/// Default Kademlia iterative lookup parallelism (`alpha = 3`).
pub const KADEMLIA_ALPHA: usize = 3;

/// Number of Kademlia routing table buckets (256-bit `PeerId`).
pub const KADEMLIA_BUCKETS: usize = 256;

/// Default Circuit Relay v2 reservation TTL in milliseconds (30 minutes).
pub const DEFAULT_RELAY_TTL_MS: u64 = 30 * 60 * 1_000;

/// 256-bit Kademlia XOR distance metric between two [`PeerId`]s.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct XorDistance([u8; PEER_ID_LEN]);

impl XorDistance {
    /// Compute the 256-bit XOR distance `a ^ b`.
    pub fn between(a: &PeerId, b: &PeerId) -> Self {
        let mut out = [0u8; PEER_ID_LEN];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = a.as_bytes()[i] ^ b.as_bytes()[i];
        }
        Self(out)
    }

    /// Borrow the raw 32-byte big-endian distance array.
    pub const fn as_bytes(&self) -> &[u8; PEER_ID_LEN] {
        &self.0
    }

    /// True when the distance is all zeros (`a == b`).
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }

    /// Kademlia bucket index (`0..256`), where `255` is the furthest bucket
    /// (differing in the most-significant bit) and `0` differs only in the
    /// least-significant bit. Returns `None` when `self.is_zero()`.
    pub fn bucket_index(&self) -> Option<usize> {
        for (byte_idx, &byte) in self.0.iter().enumerate() {
            if byte != 0 {
                let lz = byte.leading_zeros() as usize;
                let leading_bits = byte_idx * 8 + lz;
                return Some(255 - leading_bits);
            }
        }
        None
    }
}

/// NAT traversal status advertised in a peer's signed discovery record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NatStatus {
    /// Directly dialable public endpoint(s).
    Public,
    /// Behind cone NAT with an active UPnP external port mapping.
    UpnpMapped {
        /// Externally mapped `ip:port` opened via UPnP IGD.
        external: SocketAddr,
    },
    /// Behind symmetric/carrier-grade NAT; reachable through Circuit Relay v2 peers.
    BehindSymmetricNat {
        /// Relay [`PeerId`]s holding an active reservation for this peer.
        relay_peers: Vec<PeerId>,
    },
}

/// Deterministic UPnP IGD port mapper for nodes behind cone NAT.
#[derive(Debug, Clone)]
pub struct UpnpMapper {
    gateway_available: bool,
    external_ip: IpAddr,
}

impl UpnpMapper {
    /// Construct a UPnP mapper for a local network gateway.
    pub const fn new(gateway_available: bool, external_ip: IpAddr) -> Self {
        Self {
            gateway_available,
            external_ip,
        }
    }

    /// Request an external port mapping on the UPnP gateway.
    pub fn request_mapping(
        &self,
        _internal_addr: SocketAddr,
        external_port: u16,
    ) -> Result<NatStatus, P2pError> {
        if external_port == 0 {
            return Err(P2pError::InvalidUpnpPort(external_port));
        }
        if !self.gateway_available {
            return Err(P2pError::BehindSymmetricNatUnsupportedUpnp.into_p2p());
        }
        Ok(NatStatus::UpnpMapped {
            external: SocketAddr::new(self.external_ip, external_port),
        })
    }
}

trait InternalUpnpErr {
    fn into_p2p(self) -> P2pError;
}

enum InternalUpnp {
    BehindSymmetricNatUnsupportedUpnp,
}

use InternalUpnp as P2pErrorInternal;
#[allow(non_upper_case_globals)]
impl P2pError {
    const BehindSymmetricNatUnsupportedUpnp: P2pErrorInternal =
        P2pErrorInternal::BehindSymmetricNatUnsupportedUpnp;
}

impl InternalUpnpErr for P2pErrorInternal {
    fn into_p2p(self) -> P2pError {
        P2pError::Io("UPnP gateway unavailable on symmetric NAT".to_owned())
    }
}

/// Cryptographically signed Kademlia peer record (`D-N1`, `D-N3`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedPeerRecord {
    /// Canonical `PeerId = BLAKE3(public_key)`.
    pub peer_id: PeerId,
    /// 32-byte Ed25519 verifying key.
    pub public_key: [u8; 32],
    /// Chain identifier binding this record to a specific network.
    pub chain_id: String,
    /// Strictly increasing sequence number for record updates.
    pub seq: u64,
    /// Direct listen socket addresses (may be empty if behind symmetric NAT).
    pub direct_addrs: Vec<SocketAddr>,
    /// NAT traversal / relay reachability descriptor.
    pub nat_status: NatStatus,
    /// 64-byte Ed25519 signature over the canonical record digest.
    pub signature: [u8; 64],
}

impl SignedPeerRecord {
    /// Build and sign a [`SignedPeerRecord`] using `identity`.
    pub fn sign(
        identity: &PeerIdentity,
        chain_id: &str,
        seq: u64,
        direct_addrs: Vec<SocketAddr>,
        nat_status: NatStatus,
    ) -> Self {
        let peer_id = identity.peer_id();
        let public_key = *identity.public_key();
        let digest = record_signing_digest(
            chain_id,
            &peer_id,
            &public_key,
            seq,
            &direct_addrs,
            &nat_status,
        );
        let signature = identity.sign_digest(&digest);
        Self {
            peer_id,
            public_key,
            chain_id: chain_id.to_owned(),
            seq,
            direct_addrs,
            nat_status,
            signature,
        }
    }

    /// True when the record advertises at least one usable dial path (direct,
    /// UPnP external address, or at least one relay peer).
    pub fn is_reachable(&self) -> bool {
        match &self.nat_status {
            NatStatus::Public => !self.direct_addrs.is_empty(),
            NatStatus::UpnpMapped { external } => external.port() != 0,
            NatStatus::BehindSymmetricNat { relay_peers } => !relay_peers.is_empty(),
        }
    }

    /// Verify `PeerId == BLAKE3(public_key)`, `chain_id`, reachability, and the
    /// domain-separated Ed25519 signature.
    pub fn verify(&self, expected_chain_id: &str) -> Result<(), P2pError> {
        self.peer_id.verify_public_key(&self.public_key)?;
        if self.chain_id != expected_chain_id {
            return Err(P2pError::ChainIdMismatch {
                expected: expected_chain_id.to_owned(),
                actual: self.chain_id.clone(),
            });
        }
        if !self.is_reachable() {
            return Err(P2pError::UnreachablePeerRecord(self.peer_id));
        }
        let digest = record_signing_digest(
            &self.chain_id,
            &self.peer_id,
            &self.public_key,
            self.seq,
            &self.direct_addrs,
            &self.nat_status,
        );
        verify_raw(&self.public_key, &digest, &self.signature)
            .map_err(|_| P2pError::InvalidPeerRecordSignature)
    }
}

fn record_signing_digest(
    chain_id: &str,
    peer_id: &PeerId,
    public_key: &[u8; 32],
    seq: u64,
    direct_addrs: &[SocketAddr],
    nat_status: &NatStatus,
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(256);
    buf.extend_from_slice(DISCOVERY_RECORD_DOMAIN);
    buf.extend_from_slice(&(chain_id.len() as u32).to_le_bytes());
    buf.extend_from_slice(chain_id.as_bytes());
    buf.extend_from_slice(peer_id.as_bytes());
    buf.extend_from_slice(public_key);
    buf.extend_from_slice(&seq.to_le_bytes());
    buf.extend_from_slice(&(direct_addrs.len() as u32).to_le_bytes());
    for addr in direct_addrs {
        let s = addr.to_string();
        buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
        buf.extend_from_slice(s.as_bytes());
    }
    match nat_status {
        NatStatus::Public => buf.push(0),
        NatStatus::UpnpMapped { external } => {
            buf.push(1);
            let s = external.to_string();
            buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
            buf.extend_from_slice(s.as_bytes());
        }
        NatStatus::BehindSymmetricNat { relay_peers } => {
            buf.push(2);
            buf.extend_from_slice(&(relay_peers.len() as u32).to_le_bytes());
            for rp in relay_peers {
                buf.extend_from_slice(rp.as_bytes());
            }
        }
    }
    *blake3_hash(&buf).as_bytes()
}

/// Verified bootstrap peer list used to seed Kademlia DHT discovery.
#[derive(Debug, Clone)]
pub struct BootstrapList {
    chain_id: String,
    entries: BTreeMap<PeerId, SignedPeerRecord>,
}

impl BootstrapList {
    /// Construct an empty bootstrap list for `chain_id`.
    pub fn new(chain_id: &str) -> Self {
        Self {
            chain_id: chain_id.to_owned(),
            entries: BTreeMap::new(),
        }
    }

    /// Verify and add a bootstrap peer record.
    pub fn add_bootstrap(&mut self, record: SignedPeerRecord) -> Result<(), P2pError> {
        record.verify(&self.chain_id)?;
        self.entries.insert(record.peer_id, record);
        Ok(())
    }

    /// Borrow all verified bootstrap records in deterministic `PeerId` order.
    pub fn records(&self) -> Vec<SignedPeerRecord> {
        self.entries.values().cloned().collect()
    }

    /// Number of bootstrap peers configured.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if no bootstrap peers are configured.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Active Circuit Relay v2 slot reservation granted to a NATed peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayReservation {
    /// Relay node's `PeerId`.
    pub relay_peer_id: PeerId,
    /// NATed client node's `PeerId`.
    pub client_peer_id: PeerId,
    /// Expiration timestamp in unix milliseconds.
    pub expires_at_ms: u64,
}

/// Circuit Relay v2 server allowing NATed peers to reserve slots and exchange
/// bounded frames when direct traversal is unavailable.
#[derive(Debug, Clone)]
pub struct RelayServer {
    relay_peer_id: PeerId,
    chain_id: String,
    max_reservations: usize,
    reservations: BTreeMap<PeerId, RelayReservation>,
}

impl RelayServer {
    /// Create a new Circuit Relay v2 server for `relay_peer_id`.
    pub fn new(relay_peer_id: PeerId, chain_id: &str, max_reservations: usize) -> Self {
        Self {
            relay_peer_id,
            chain_id: chain_id.to_owned(),
            max_reservations,
            reservations: BTreeMap::new(),
        }
    }

    /// Grant or renew a Circuit Relay v2 slot reservation for `client_record`.
    pub fn reserve_slot(
        &mut self,
        client_record: &SignedPeerRecord,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<RelayReservation, P2pError> {
        client_record.verify(&self.chain_id)?;
        self.reservations
            .retain(|_, res| res.expires_at_ms > now_ms);
        if !self.reservations.contains_key(&client_record.peer_id)
            && self.reservations.len() >= self.max_reservations
        {
            return Err(P2pError::RelayCapacityReached {
                max: self.max_reservations,
            });
        }
        let reservation = RelayReservation {
            relay_peer_id: self.relay_peer_id,
            client_peer_id: client_record.peer_id,
            expires_at_ms: now_ms.saturating_add(ttl_ms),
        };
        self.reservations
            .insert(client_record.peer_id, reservation.clone());
        Ok(reservation)
    }

    /// Route a bounded frame from `from` to `target` over an active relay reservation.
    pub fn route_via_relay(
        &self,
        from: PeerId,
        target: PeerId,
        payload: &[u8],
        now_ms: u64,
    ) -> Result<Vec<u8>, P2pError> {
        if from == target {
            return Err(P2pError::SelfConnection(from));
        }
        if payload.len() > MAX_FRAME_BYTES {
            return Err(P2pError::FrameTooLarge {
                size: payload.len(),
                max: MAX_FRAME_BYTES,
            });
        }
        let Some(res) = self.reservations.get(&target) else {
            return Err(P2pError::RelayReservationMissingOrExpired(target));
        };
        if res.expires_at_ms <= now_ms {
            return Err(P2pError::RelayReservationMissingOrExpired(target));
        }
        Ok(payload.to_vec())
    }

    /// Count active reservations at `now_ms`.
    pub fn active_reservations(&self, now_ms: u64) -> usize {
        self.reservations
            .values()
            .filter(|r| r.expires_at_ms > now_ms)
            .count()
    }
}

/// Kademlia DHT routing table with 256 k-buckets (`k = 20`) and verified
/// [`SignedPeerRecord`] storage.
#[derive(Debug, Clone)]
pub struct KademliaTable {
    local_peer: PeerId,
    chain_id: String,
    k_bucket_cap: usize,
    buckets: Vec<VecDeque<PeerId>>,
    records: BTreeMap<PeerId, SignedPeerRecord>,
}

impl KademliaTable {
    /// Construct an empty Kademlia routing table for `local_peer`.
    pub fn new(local_peer: PeerId, chain_id: &str, k_bucket_cap: usize) -> Self {
        Self {
            local_peer,
            chain_id: chain_id.to_owned(),
            k_bucket_cap: k_bucket_cap.max(1),
            buckets: vec![VecDeque::new(); KADEMLIA_BUCKETS],
            records: BTreeMap::new(),
        }
    }

    /// Verify and insert/update a [`SignedPeerRecord`] in the routing table.
    ///
    /// Returns `Ok(true)` if the record was inserted or updated, or `Ok(false)`
    /// if the target k-bucket is full.
    pub fn upsert_record(
        &mut self,
        record: SignedPeerRecord,
        ban_list: &BanList,
        now_ms: u64,
    ) -> Result<bool, P2pError> {
        if record.peer_id == self.local_peer {
            return Err(P2pError::SelfConnection(self.local_peer));
        }
        ban_list.check_peer(&record.peer_id, now_ms)?;
        record.verify(&self.chain_id)?;

        if let Some(existing) = self.records.get(&record.peer_id)
            && record.seq <= existing.seq
        {
            return Err(P2pError::StalePeerRecord {
                peer_id: record.peer_id,
                existing_seq: existing.seq,
                got_seq: record.seq,
            });
        }

        let dist = XorDistance::between(&self.local_peer, &record.peer_id);
        let Some(bucket_idx) = dist.bucket_index() else {
            return Err(P2pError::SelfConnection(self.local_peer));
        };

        let bucket = &mut self.buckets[bucket_idx];
        if let Some(pos) = bucket.iter().position(|p| *p == record.peer_id) {
            bucket.remove(pos);
            bucket.push_back(record.peer_id);
            self.records.insert(record.peer_id, record);
            return Ok(true);
        }

        if bucket.len() >= self.k_bucket_cap {
            return Ok(false);
        }
        bucket.push_back(record.peer_id);
        self.records.insert(record.peer_id, record);
        Ok(true)
    }

    /// Return up to `count` closest verified peer records to `target` ordered by
    /// ascending [`XorDistance`].
    pub fn closest_peers(&self, target: &PeerId, count: usize) -> Vec<SignedPeerRecord> {
        let mut list: Vec<(XorDistance, &SignedPeerRecord)> = self
            .records
            .values()
            .map(|r| (XorDistance::between(&r.peer_id, target), r))
            .collect();
        list.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.peer_id.cmp(&b.1.peer_id)));
        list.into_iter()
            .take(count)
            .map(|(_, r)| r.clone())
            .collect()
    }

    /// Look up a peer's verified record by `PeerId`.
    pub fn get_record(&self, peer_id: &PeerId) -> Option<&SignedPeerRecord> {
        self.records.get(peer_id)
    }

    /// Total number of verified peer records across all 256 k-buckets.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// True when the routing table contains no peer records.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Number of entries in bucket `index` (`0..256`).
    pub fn bucket_len(&self, index: usize) -> usize {
        self.buckets.get(index).map_or(0, VecDeque::len)
    }
}

/// One participating node in a decentralized N3 Kademlia + NAT/UPnP + Relay swarm.
pub struct DiscoveryNode {
    /// Cryptographic peer identity.
    pub identity: PeerIdentity,
    /// Signed self record advertised in the DHT.
    pub self_record: SignedPeerRecord,
    /// Local Kademlia routing table.
    pub table: KademliaTable,
    /// Local ban list.
    pub ban_list: BanList,
    /// Optional Circuit Relay v2 server (enabled on UPnP/public relay-capable nodes).
    pub relay_server: Option<RelayServer>,
}

/// Decentralized N3 discovery network of `N` NATed nodes with no central server.
pub struct DiscoveryNetwork {
    /// Chain identifier shared by the swarm.
    pub chain_id: String,
    /// Participating nodes (`0..N`).
    pub nodes: Vec<DiscoveryNode>,
}

impl DiscoveryNetwork {
    /// Construct a decentralized network of `node_count` nodes behind NAT
    /// (no central server):
    /// - Nodes `0..relay_capable_count` sit behind cone NAT with UPnP port
    ///   mappings (`NatStatus::UpnpMapped`) and also act as decentralized
    ///   Circuit Relay v2 hops.
    /// - Nodes `relay_capable_count..node_count` sit behind strict symmetric
    ///   NAT (`NatStatus::BehindSymmetricNat`), reserve a slot on two local
    ///   relay peers, and seed their `KademliaTable` with only 2 neighbor peers.
    pub fn new_natted_mesh(
        chain_id: &str,
        node_count: usize,
        relay_capable_count: usize,
        now_ms: u64,
    ) -> Result<Self, P2pError> {
        assert!(node_count >= 4 && relay_capable_count >= 2 && relay_capable_count <= node_count);

        let mut identities = Vec::with_capacity(node_count);
        for idx in 0..node_count {
            let mut seed = [0u8; 32];
            seed[0] = 0xD3;
            seed[1..9].copy_from_slice(&(idx as u64 + 1).to_le_bytes());
            identities.push(PeerIdentity::from_seed(seed));
        }

        // 1. Build UPnP-mapped records and RelayServers for the cone-NAT nodes.
        let mut records: Vec<SignedPeerRecord> = Vec::with_capacity(node_count);
        let mut relay_servers: Vec<Option<RelayServer>> = Vec::with_capacity(node_count);

        for (idx, id) in identities.iter().enumerate() {
            if idx < relay_capable_count {
                let internal = SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(192, 168, 1, (idx + 10) as u8)),
                    30_000 + idx as u16,
                );
                let mapper = UpnpMapper::new(
                    true,
                    IpAddr::V4(Ipv4Addr::new(203, 0, 113, (idx + 1) as u8)),
                );
                let nat_status = mapper.request_mapping(internal, 40_000 + idx as u16)?;
                let rec = SignedPeerRecord::sign(id, chain_id, 1, vec![internal], nat_status);
                records.push(rec);
                relay_servers.push(Some(RelayServer::new(
                    id.peer_id(),
                    chain_id,
                    node_count * 2,
                )));
            } else {
                let r1 = identities[idx % relay_capable_count].peer_id();
                let r2 = identities[(idx + 1) % relay_capable_count].peer_id();
                let nat_status = NatStatus::BehindSymmetricNat {
                    relay_peers: vec![r1, r2],
                };
                let rec = SignedPeerRecord::sign(id, chain_id, 1, Vec::new(), nat_status);
                // Register Circuit Relay v2 reservations on both chosen relay peers.
                for r_idx in [idx % relay_capable_count, (idx + 1) % relay_capable_count] {
                    if let Some(ref mut srv) = relay_servers[r_idx] {
                        let _ = srv.reserve_slot(&rec, now_ms, DEFAULT_RELAY_TTL_MS)?;
                    }
                }
                records.push(rec);
                relay_servers.push(None);
            }
        }

        // 2. Initialize each node's KademliaTable with only 2 adjacent peers
        //    (a sparse decentralized ring + relay link, with NO central server).
        let mut nodes = Vec::with_capacity(node_count);
        for idx in 0..node_count {
            let id = PeerIdentity::from_seed({
                let mut seed = [0u8; 32];
                seed[0] = 0xD3;
                seed[1..9].copy_from_slice(&(idx as u64 + 1).to_le_bytes());
                seed
            });
            let mut table = KademliaTable::new(id.peer_id(), chain_id, KADEMLIA_K.max(node_count));
            let ban_list = BanList::new();

            let next_peer = (idx + 1) % node_count;
            let relay_peer = idx % relay_capable_count;
            let _ = table.upsert_record(records[next_peer].clone(), &ban_list, now_ms)?;
            if relay_peer != idx && relay_peer != next_peer {
                let _ = table.upsert_record(records[relay_peer].clone(), &ban_list, now_ms)?;
            }

            nodes.push(DiscoveryNode {
                identity: id,
                self_record: records[idx].clone(),
                table,
                ban_list,
                relay_server: relay_servers[idx].take(),
            });
        }

        Ok(Self {
            chain_id: chain_id.to_owned(),
            nodes,
        })
    }

    /// Execute iterative Kademlia `FIND_NODE` discovery rounds across the swarm
    /// until convergence or `max_rounds`.
    ///
    /// In each round, every node queries its `KADEMLIA_ALPHA` closest known peers
    /// for each target bucket/peer, and also pushes its own `SignedPeerRecord` to
    /// newly discovered peers (Kademlia `PING`/`STORE` reciprocity).
    pub fn run_iterative_discovery(
        &mut self,
        max_rounds: usize,
        now_ms: u64,
    ) -> Result<usize, P2pError> {
        let n = self.nodes.len();
        let all_peer_ids: Vec<PeerId> = self
            .nodes
            .iter()
            .map(|node| node.identity.peer_id())
            .collect();
        let peer_index: BTreeMap<PeerId, usize> = all_peer_ids
            .iter()
            .enumerate()
            .map(|(i, p)| (*p, i))
            .collect();

        for round in 1..=max_rounds {
            let mut changed = false;
            for i in 0..n {
                let caller_record = self.nodes[i].self_record.clone();
                for target in &all_peer_ids {
                    if *target == caller_record.peer_id {
                        continue;
                    }
                    let candidates = self.nodes[i].table.closest_peers(target, KADEMLIA_ALPHA);
                    for cand in candidates {
                        let Some(&cand_idx) = peer_index.get(&cand.peer_id) else {
                            continue;
                        };
                        // Reciprocal Kademlia introduction: candidate learns caller's record.
                        if self.nodes[cand_idx]
                            .table
                            .get_record(&caller_record.peer_id)
                            .is_none()
                        {
                            let ban = self.nodes[cand_idx].ban_list.clone();
                            if self.nodes[cand_idx].table.upsert_record(
                                caller_record.clone(),
                                &ban,
                                now_ms,
                            )? {
                                changed = true;
                            }
                        }
                        // Candidate returns its closest peers to `target` (plus its own record).
                        let mut returned =
                            self.nodes[cand_idx].table.closest_peers(target, KADEMLIA_K);
                        returned.push(self.nodes[cand_idx].self_record.clone());
                        for rec in returned {
                            if rec.peer_id == caller_record.peer_id {
                                continue;
                            }
                            if self.nodes[i].table.get_record(&rec.peer_id).is_none() {
                                let ban = self.nodes[i].ban_list.clone();
                                if self.nodes[i].table.upsert_record(rec, &ban, now_ms)? {
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
            if self.nodes.iter().all(|node| node.table.len() == n - 1) {
                return Ok(round);
            }
            if !changed {
                break;
            }
        }
        Ok(max_rounds)
    }

    /// Route a message from node `from_idx` to node `to_idx` using the target's
    /// discovered [`SignedPeerRecord`] (via UPnP external port or Circuit Relay v2).
    pub fn route_message(
        &self,
        from_idx: usize,
        to_idx: usize,
        payload: &[u8],
        now_ms: u64,
    ) -> Result<Vec<u8>, P2pError> {
        let from_peer = self.nodes[from_idx].identity.peer_id();
        let target_peer = self.nodes[to_idx].identity.peer_id();
        let Some(target_rec) = self.nodes[from_idx].table.get_record(&target_peer) else {
            return Err(P2pError::UnreachablePeerRecord(target_peer));
        };
        match &target_rec.nat_status {
            NatStatus::Public | NatStatus::UpnpMapped { .. } => {
                if payload.len() > MAX_FRAME_BYTES {
                    return Err(P2pError::FrameTooLarge {
                        size: payload.len(),
                        max: MAX_FRAME_BYTES,
                    });
                }
                Ok(payload.to_vec())
            }
            NatStatus::BehindSymmetricNat { relay_peers } => {
                for relay_id in relay_peers {
                    if let Some(relay_node) = self
                        .nodes
                        .iter()
                        .find(|n| n.identity.peer_id() == *relay_id)
                        && let Some(ref srv) = relay_node.relay_server
                        && let Ok(delivered) =
                            srv.route_via_relay(from_peer, target_peer, payload, now_ms)
                    {
                        return Ok(delivered);
                    }
                }
                Err(P2pError::RelayReservationMissingOrExpired(target_peer))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ban::BanReason;

    const CHAIN: &str = "auralite-n3-unit";

    fn sample_addr(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1)), port)
    }

    #[test]
    fn xor_distance_metric_and_bucket_index_are_exact() {
        let a = PeerId::from_bytes([0x00; 32]);
        let mut b_bytes = [0x00; 32];
        b_bytes[0] = 0x80; // MSB differs -> bucket 255
        let b = PeerId::from_bytes(b_bytes);

        let mut c_bytes = [0x00; 32];
        c_bytes[31] = 0x01; // LSB differs -> bucket 0
        let c = PeerId::from_bytes(c_bytes);

        assert!(XorDistance::between(&a, &a).is_zero());
        assert_eq!(XorDistance::between(&a, &a).bucket_index(), None);
        assert_eq!(XorDistance::between(&a, &b).bucket_index(), Some(255));
        assert_eq!(XorDistance::between(&a, &c).bucket_index(), Some(0));
        assert!(XorDistance::between(&a, &c) < XorDistance::between(&a, &b));
    }

    #[test]
    fn rejects_peer_record_with_substituted_peer_id() {
        let honest = PeerIdentity::from_seed([1u8; 32]);
        let impostor = PeerIdentity::from_seed([2u8; 32]);
        let mut rec = SignedPeerRecord::sign(
            &honest,
            CHAIN,
            1,
            vec![sample_addr(9000)],
            NatStatus::Public,
        );
        assert!(rec.verify(CHAIN).is_ok());

        // Substitute public_key without updating PeerId -> must fail IdentityMismatch.
        rec.public_key = *impostor.public_key();
        assert!(matches!(
            rec.verify(CHAIN),
            Err(P2pError::IdentityMismatch { .. })
        ));
    }

    #[test]
    fn rejects_forged_or_tampered_peer_record_signature() {
        let id = PeerIdentity::from_seed([3u8; 32]);
        let mut rec =
            SignedPeerRecord::sign(&id, CHAIN, 1, vec![sample_addr(9001)], NatStatus::Public);
        rec.signature[0] ^= 0xFF;
        assert_eq!(rec.verify(CHAIN), Err(P2pError::InvalidPeerRecordSignature));
    }

    #[test]
    fn kademlia_rejects_stale_sequence_record_replay() {
        let local = PeerIdentity::from_seed([10u8; 32]);
        let remote = PeerIdentity::from_seed([11u8; 32]);
        let ban = BanList::new();
        let mut table = KademliaTable::new(local.peer_id(), CHAIN, KADEMLIA_K);

        let rec_v2 = SignedPeerRecord::sign(
            &remote,
            CHAIN,
            2,
            vec![sample_addr(9002)],
            NatStatus::Public,
        );
        assert!(table.upsert_record(rec_v2, &ban, 1_000).unwrap());

        let rec_v1 = SignedPeerRecord::sign(
            &remote,
            CHAIN,
            1,
            vec![sample_addr(9001)],
            NatStatus::Public,
        );
        assert_eq!(
            table.upsert_record(rec_v1, &ban, 1_001),
            Err(P2pError::StalePeerRecord {
                peer_id: remote.peer_id(),
                existing_seq: 2,
                got_seq: 1,
            })
        );
    }

    #[test]
    fn kademlia_rejects_banned_peer_records() {
        let local = PeerIdentity::from_seed([20u8; 32]);
        let bad_peer = PeerIdentity::from_seed([21u8; 32]);
        let mut ban = BanList::new();
        ban.ban_peer(
            bad_peer.peer_id(),
            BanReason::Manual("malicious peer".to_owned()),
            Some(60_000),
        );

        let mut table = KademliaTable::new(local.peer_id(), CHAIN, KADEMLIA_K);
        let rec = SignedPeerRecord::sign(
            &bad_peer,
            CHAIN,
            1,
            vec![sample_addr(9005)],
            NatStatus::Public,
        );
        assert!(matches!(
            table.upsert_record(rec, &ban, 2_000),
            Err(P2pError::PeerBanned { .. })
        ));
    }

    #[test]
    fn relay_rejects_unreserved_or_expired_target() {
        let relay = PeerIdentity::from_seed([30u8; 32]);
        let sender = PeerIdentity::from_seed([31u8; 32]);
        let target = PeerIdentity::from_seed([32u8; 32]);
        let mut srv = RelayServer::new(relay.peer_id(), CHAIN, 8);

        // Unreserved target must be rejected.
        assert_eq!(
            srv.route_via_relay(sender.peer_id(), target.peer_id(), b"hello", 1_000),
            Err(P2pError::RelayReservationMissingOrExpired(target.peer_id()))
        );

        // Reserve slot with TTL=500ms at t=1000ms -> valid at t=1200ms, expired at t=1500ms.
        let target_rec = SignedPeerRecord::sign(
            &target,
            CHAIN,
            1,
            Vec::new(),
            NatStatus::BehindSymmetricNat {
                relay_peers: vec![relay.peer_id()],
            },
        );
        srv.reserve_slot(&target_rec, 1_000, 500).unwrap();
        assert_eq!(
            srv.route_via_relay(sender.peer_id(), target.peer_id(), b"hello", 1_200)
                .unwrap(),
            b"hello"
        );
        assert_eq!(
            srv.route_via_relay(sender.peer_id(), target.peer_id(), b"hello", 1_500),
            Err(P2pError::RelayReservationMissingOrExpired(target.peer_id()))
        );
    }
}
