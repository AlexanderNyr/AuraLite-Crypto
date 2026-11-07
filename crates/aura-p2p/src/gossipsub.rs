//! N2 gossipsub — topics (`D-N2`), domain-separated Ed25519 message signatures,
//! deduplication (`SeenCache`), peer scoring with flood/invalid-message penalties,
//! automatic ban-list escalation, and mesh broadcast (`D-N3`).

use crate::ban::{BanList, BanReason};
use crate::error::P2pError;
use crate::identity::{PeerId, PeerIdentity};
use aura_crypto::{blake3_hash, verify_raw};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

/// Domain tag for deterministic gossip `MessageId` derivation.
pub const GOSSIP_ID_DOMAIN_TAG: &[u8] = b"AURA_P2P_GOSSIP_ID_V1\0";

/// Domain tag for Ed25519 signatures over gossipsub messages (`D-N2`, `D-N3`).
pub const GOSSIP_SIG_DOMAIN_TAG: &[u8] = b"AURA_P2P_GOSSIP_SIG_V1\0";

/// Default maximum gossip payload size (256 KiB).
pub const MAX_GOSSIP_PAYLOAD_BYTES: usize = 256 * 1024;

/// Default maximum `SeenCache` capacity.
pub const DEFAULT_SEEN_CACHE_CAPACITY: usize = 16_384;

/// Canonical gossipsub topics defined by `NET_PLAN` decision `D-N2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GossipTopic {
    /// Signed user transactions (`tx`).
    Tx,
    /// Consensus block proposals (`proposal`).
    Proposal,
    /// Consensus votes (`vote`).
    Vote,
    /// Slashable Byzantine evidence (`evidence`).
    Evidence,
    /// State sync snapshot manifests/chunks (`snapshot`).
    Snapshot,
}

impl GossipTopic {
    /// All five canonical `D-N2` gossipsub topics.
    pub const ALL: [Self; 5] = [
        Self::Tx,
        Self::Proposal,
        Self::Vote,
        Self::Evidence,
        Self::Snapshot,
    ];

    /// Canonical string name of the topic.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tx => "tx",
            Self::Proposal => "proposal",
            Self::Vote => "vote",
            Self::Evidence => "evidence",
            Self::Snapshot => "snapshot",
        }
    }

    /// Parse a topic name (`tx`, `proposal`, `vote`, `evidence`, `snapshot`).
    pub fn from_name(name: &str) -> Result<Self, P2pError> {
        match name {
            "tx" => Ok(Self::Tx),
            "proposal" => Ok(Self::Proposal),
            "vote" => Ok(Self::Vote),
            "evidence" => Ok(Self::Evidence),
            "snapshot" => Ok(Self::Snapshot),
            other => Err(P2pError::UnknownGossipTopic(other.to_owned())),
        }
    }

    /// Wire discriminant (`1..=5`).
    pub const fn wire_tag(self) -> u8 {
        match self {
            Self::Tx => 1,
            Self::Proposal => 2,
            Self::Vote => 3,
            Self::Evidence => 4,
            Self::Snapshot => 5,
        }
    }

    /// Decode a wire discriminant (`1..=5`).
    pub fn from_wire_tag(tag: u8) -> Result<Self, P2pError> {
        match tag {
            1 => Ok(Self::Tx),
            2 => Ok(Self::Proposal),
            3 => Ok(Self::Vote),
            4 => Ok(Self::Evidence),
            5 => Ok(Self::Snapshot),
            other => Err(P2pError::UnknownGossipTopic(format!("tag:{other}"))),
        }
    }
}

impl fmt::Display for GossipTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Deterministic 32-byte gossip message identifier used for deduplication.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageId([u8; 32]);

impl MessageId {
    /// Construct a `MessageId` from raw 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying 32-byte digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MessageId({self})")
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0[..8] {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Signed gossipsub envelope broadcast across topic meshes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GossipMessage {
    /// Target topic (`tx`, `proposal`, `vote`, `evidence`, `snapshot`).
    pub topic: GossipTopic,
    /// Chain identifier binding (`auralite-devnet-1`, etc.).
    pub chain_id: String,
    /// Originating `PeerId` (`BLAKE3(sender_public_key)`).
    pub sender_peer_id: PeerId,
    /// Originating Ed25519 public key.
    pub sender_public_key: [u8; 32],
    /// Monotonic per-publisher sequence number.
    pub seq_no: u64,
    /// Opaque topic payload bytes.
    pub payload: Vec<u8>,
    /// Ed25519 signature over the domain-separated gossip digest.
    pub signature: [u8; 64],
}

impl GossipMessage {
    /// Construct and sign a new [`GossipMessage`] with `identity`.
    pub fn sign(
        identity: &PeerIdentity,
        chain_id: impl Into<String>,
        topic: GossipTopic,
        seq_no: u64,
        payload: Vec<u8>,
    ) -> Result<Self, P2pError> {
        if payload.is_empty() {
            return Err(P2pError::GossipPayloadEmpty);
        }
        if payload.len() > MAX_GOSSIP_PAYLOAD_BYTES {
            return Err(P2pError::GossipPayloadTooLarge {
                size: payload.len(),
                max: MAX_GOSSIP_PAYLOAD_BYTES,
            });
        }
        let chain_id = chain_id.into();
        let sender_peer_id = identity.peer_id();
        let sender_public_key = *identity.public_key();
        let digest = gossip_signing_digest(topic, &chain_id, &sender_peer_id, seq_no, &payload);
        let signature = identity.sign_digest(&digest);
        Ok(Self {
            topic,
            chain_id,
            sender_peer_id,
            sender_public_key,
            seq_no,
            payload,
            signature,
        })
    }

    /// Compute the deterministic [`MessageId`] of this message.
    pub fn message_id(&self) -> MessageId {
        let payload_hash = blake3_hash(&self.payload);
        let mut buf = Vec::with_capacity(GOSSIP_ID_DOMAIN_TAG.len() + self.chain_id.len() + 80);
        buf.extend_from_slice(GOSSIP_ID_DOMAIN_TAG);
        buf.push(self.topic.wire_tag());
        buf.extend_from_slice(&(self.chain_id.len() as u32).to_le_bytes());
        buf.extend_from_slice(self.chain_id.as_bytes());
        buf.extend_from_slice(self.sender_peer_id.as_bytes());
        buf.extend_from_slice(&self.seq_no.to_le_bytes());
        buf.extend_from_slice(payload_hash.as_bytes());
        MessageId(*blake3_hash(&buf).as_bytes())
    }

    /// Validate payload bounds, `chain_id`, `PeerId == BLAKE3(pubkey)`, and
    /// the Ed25519 signature (`D-N3`: no peer is trusted).
    pub fn verify(
        &self,
        expected_chain_id: &str,
        max_payload_bytes: usize,
    ) -> Result<MessageId, P2pError> {
        if self.payload.is_empty() {
            return Err(P2pError::GossipPayloadEmpty);
        }
        if self.payload.len() > max_payload_bytes {
            return Err(P2pError::GossipPayloadTooLarge {
                size: self.payload.len(),
                max: max_payload_bytes,
            });
        }
        if self.chain_id != expected_chain_id {
            return Err(P2pError::ChainIdMismatch {
                expected: expected_chain_id.to_owned(),
                actual: self.chain_id.clone(),
            });
        }
        self.sender_peer_id
            .verify_public_key(&self.sender_public_key)?;
        let digest = gossip_signing_digest(
            self.topic,
            &self.chain_id,
            &self.sender_peer_id,
            self.seq_no,
            &self.payload,
        );
        verify_raw(&self.sender_public_key, &digest, &self.signature)
            .map_err(|_| P2pError::InvalidGossipSignature)?;
        Ok(self.message_id())
    }

    /// Encode this gossip message to canonical wire bytes.
    pub fn encode(&self) -> Vec<u8> {
        let chain_bytes = self.chain_id.as_bytes();
        let mut buf = Vec::with_capacity(
            1 + 4 + chain_bytes.len() + 32 + 32 + 8 + 4 + self.payload.len() + 64,
        );
        buf.push(self.topic.wire_tag());
        buf.extend_from_slice(&(chain_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(chain_bytes);
        buf.extend_from_slice(self.sender_peer_id.as_bytes());
        buf.extend_from_slice(&self.sender_public_key);
        buf.extend_from_slice(&self.seq_no.to_le_bytes());
        buf.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.payload);
        buf.extend_from_slice(&self.signature);
        buf
    }

    /// Decode a [`GossipMessage`] from canonical wire bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, P2pError> {
        let mut cursor = 0usize;
        if bytes.is_empty() {
            return Err(P2pError::Codec("empty gossip wire frame".into()));
        }
        let topic_tag = bytes[cursor];
        cursor += 1;
        let topic = GossipTopic::from_wire_tag(topic_tag)?;

        let chain_len = read_u32_le(bytes, &mut cursor)? as usize;
        let chain_slice = read_slice(bytes, &mut cursor, chain_len)?;
        let chain_id = std::str::from_utf8(chain_slice)
            .map_err(|_| P2pError::Codec("non-utf8 gossip chain_id".into()))?
            .to_owned();

        let peer_bytes = read_array::<32>(bytes, &mut cursor)?;
        let sender_public_key = read_array::<32>(bytes, &mut cursor)?;
        let seq_no = u64::from_le_bytes(read_array::<8>(bytes, &mut cursor)?);
        let payload_len = read_u32_le(bytes, &mut cursor)? as usize;
        let payload = read_slice(bytes, &mut cursor, payload_len)?.to_vec();
        let signature = read_array::<64>(bytes, &mut cursor)?;
        if cursor != bytes.len() {
            return Err(P2pError::Codec("trailing bytes in gossip frame".into()));
        }
        Ok(Self {
            topic,
            chain_id,
            sender_peer_id: PeerId::from_bytes(peer_bytes),
            sender_public_key,
            seq_no,
            payload,
            signature,
        })
    }
}

fn read_slice<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], P2pError> {
    let end = cursor
        .checked_add(len)
        .ok_or_else(|| P2pError::Codec("gossip cursor overflow".into()))?;
    if end > bytes.len() {
        return Err(P2pError::Codec("unexpected EOF in gossip frame".into()));
    }
    let slice = &bytes[*cursor..end];
    *cursor = end;
    Ok(slice)
}

fn read_array<const N: usize>(bytes: &[u8], cursor: &mut usize) -> Result<[u8; N], P2pError> {
    let slice = read_slice(bytes, cursor, N)?;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    Ok(out)
}

fn read_u32_le(bytes: &[u8], cursor: &mut usize) -> Result<u32, P2pError> {
    Ok(u32::from_le_bytes(read_array::<4>(bytes, cursor)?))
}

fn gossip_signing_digest(
    topic: GossipTopic,
    chain_id: &str,
    sender_peer_id: &PeerId,
    seq_no: u64,
    payload: &[u8],
) -> [u8; 32] {
    let payload_hash = blake3_hash(payload);
    let mut buf = Vec::with_capacity(GOSSIP_SIG_DOMAIN_TAG.len() + chain_id.len() + 80);
    buf.extend_from_slice(GOSSIP_SIG_DOMAIN_TAG);
    buf.push(topic.wire_tag());
    buf.extend_from_slice(&(chain_id.len() as u32).to_le_bytes());
    buf.extend_from_slice(chain_id.as_bytes());
    buf.extend_from_slice(sender_peer_id.as_bytes());
    buf.extend_from_slice(&seq_no.to_le_bytes());
    buf.extend_from_slice(payload_hash.as_bytes());
    *blake3_hash(&buf).as_bytes()
}

/// Bounded FIFO deduplication cache of seen [`MessageId`]s.
#[derive(Debug, Clone)]
pub struct SeenCache {
    capacity: usize,
    order: VecDeque<MessageId>,
    set: BTreeSet<MessageId>,
}

impl SeenCache {
    /// Create a [`SeenCache`] with a maximum capacity of `capacity` message IDs.
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            capacity: cap,
            order: VecDeque::with_capacity(cap.min(1024)),
            set: BTreeSet::new(),
        }
    }

    /// Number of currently cached message IDs.
    pub fn len(&self) -> usize {
        self.set.len()
    }

    /// True when the cache holds no message IDs.
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// True if `id` is already present in the cache.
    pub fn contains(&self, id: &MessageId) -> bool {
        self.set.contains(id)
    }

    /// Record `id` in the cache. Returns `true` if `id` was newly inserted, or
    /// `false` if `id` was already present (duplicate).
    pub fn insert(&mut self, id: MessageId) -> bool {
        if !self.set.insert(id) {
            return false;
        }
        if self.order.len() == self.capacity
            && let Some(oldest) = self.order.pop_front()
        {
            self.set.remove(&oldest);
        }
        self.order.push_back(id);
        true
    }
}

/// Peer scoring and rate-limiting parameters (`D-N2`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerScoreParams {
    /// Starting score assigned to newly registered peers.
    pub initial_score: i64,
    /// Upper clamp for a peer's score.
    pub max_score: i64,
    /// Score increment awarded for each valid, first-seen gossip message.
    pub valid_message_reward: i64,
    /// Score penalty subtracted for duplicate messages from a peer.
    pub duplicate_penalty: i64,
    /// Score penalty subtracted for invalid messages (bad signature, identity
    /// mismatch, oversized/empty payload, unsubscribed topic, wrong chain).
    pub invalid_message_penalty: i64,
    /// Score penalty subtracted each time a peer exceeds `max_messages_per_window`.
    pub flood_penalty: i64,
    /// Score at or below which incoming messages from the peer are ignored.
    pub graylist_threshold: i64,
    /// Score at or below which the peer is disconnected and added to [`BanList`].
    pub ban_threshold: i64,
    /// Sliding window duration in milliseconds for flood detection.
    pub rate_window_ms: u64,
    /// Maximum messages a single peer may send within `rate_window_ms` before
    /// incurring `flood_penalty`.
    pub max_messages_per_window: usize,
    /// Maximum allowed payload size in bytes.
    pub max_payload_bytes: usize,
}

impl Default for PeerScoreParams {
    fn default() -> Self {
        Self {
            initial_score: 0,
            max_score: 100,
            valid_message_reward: 1,
            duplicate_penalty: 0,
            invalid_message_penalty: 25,
            flood_penalty: 20,
            graylist_threshold: -20,
            ban_threshold: -50,
            rate_window_ms: 1_000,
            max_messages_per_window: 2_000,
            max_payload_bytes: MAX_GOSSIP_PAYLOAD_BYTES,
        }
    }
}

/// Per-peer score and rate-window state tracked by [`GossipEngine`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerScoreState {
    /// Current peer score.
    pub score: i64,
    /// Start timestamp (ms) of the current rate-limiting window.
    pub window_start_ms: u64,
    /// Messages received from this peer in the current rate-limiting window.
    pub messages_in_window: usize,
    /// Total valid unique messages delivered by this peer.
    pub valid_deliveries: u64,
    /// Total duplicate messages received from this peer.
    pub duplicates: u64,
    /// Total invalid or rate-violating messages received from this peer.
    pub violations: u64,
}

impl PeerScoreState {
    fn new(initial_score: i64) -> Self {
        Self {
            score: initial_score,
            window_start_ms: 0,
            messages_in_window: 0,
            valid_deliveries: 0,
            duplicates: 0,
            violations: 0,
        }
    }
}

/// Outcome of ingesting a gossip message from a remote peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GossipOutcome {
    /// Message is valid, unseen, stored locally, and ready to forward to `forward_to`.
    Accepted {
        /// Deterministic ID of the accepted message.
        message_id: MessageId,
        /// Mesh peers subscribed to the topic (excluding sender and local node).
        forward_to: Vec<PeerId>,
    },
    /// Message was already in [`SeenCache`] and was dropped without forwarding.
    Duplicate(MessageId),
    /// Message was rejected due to validation, rate limit, or peer score/ban status.
    Rejected {
        /// Specific rejection error.
        error: P2pError,
        /// Peer's updated score after applying any penalty.
        peer_score: i64,
        /// True if the peer's score reached or fell below `ban_threshold` and was banned.
        banned: bool,
    },
}

/// Deterministic gossipsub router with topic subscriptions, signature verification,
/// deduplication (`SeenCache`), peer scoring, and [`BanList`] integration.
pub struct GossipEngine {
    identity: PeerIdentity,
    chain_id: String,
    params: PeerScoreParams,
    seen: SeenCache,
    bans: BanList,
    local_topics: BTreeSet<GossipTopic>,
    peer_topics: BTreeMap<PeerId, BTreeSet<GossipTopic>>,
    peer_scores: BTreeMap<PeerId, PeerScoreState>,
    inbox: BTreeMap<GossipTopic, Vec<GossipMessage>>,
    next_seq_no: u64,
}

impl GossipEngine {
    /// Construct a new [`GossipEngine`] subscribed to all 5 canonical `D-N2` topics.
    pub fn new(
        identity: PeerIdentity,
        chain_id: impl Into<String>,
        params: PeerScoreParams,
    ) -> Self {
        let mut local_topics = BTreeSet::new();
        for topic in GossipTopic::ALL {
            local_topics.insert(topic);
        }
        Self {
            identity,
            chain_id: chain_id.into(),
            params,
            seen: SeenCache::new(DEFAULT_SEEN_CACHE_CAPACITY),
            bans: BanList::new(),
            local_topics,
            peer_topics: BTreeMap::new(),
            peer_scores: BTreeMap::new(),
            inbox: BTreeMap::new(),
            next_seq_no: 1,
        }
    }

    /// Local node's [`PeerId`].
    pub const fn local_peer_id(&self) -> PeerId {
        self.identity.peer_id()
    }

    /// Local node's `chain_id`.
    pub fn chain_id(&self) -> &str {
        &self.chain_id
    }

    /// Subscribe the local node to `topic`.
    pub fn subscribe(&mut self, topic: GossipTopic) {
        self.local_topics.insert(topic);
    }

    /// Unsubscribe the local node from `topic`.
    pub fn unsubscribe(&mut self, topic: GossipTopic) {
        self.local_topics.remove(&topic);
    }

    /// True if the local node is subscribed to `topic`.
    pub fn is_subscribed(&self, topic: GossipTopic) -> bool {
        self.local_topics.contains(&topic)
    }

    /// Register a connected peer and subscribe it to the given topics.
    pub fn register_peer(
        &mut self,
        peer_id: PeerId,
        topics: impl IntoIterator<Item = GossipTopic>,
    ) {
        let initial = self.params.initial_score;
        self.peer_scores
            .entry(peer_id)
            .or_insert_with(|| PeerScoreState::new(initial));
        let entry = self.peer_topics.entry(peer_id).or_default();
        for topic in topics {
            entry.insert(topic);
        }
    }

    /// Return the current score of `peer_id`, if registered.
    pub fn peer_score(&self, peer_id: &PeerId) -> Option<i64> {
        self.peer_scores.get(peer_id).map(|s| s.score)
    }

    /// Return the full [`PeerScoreState`] of `peer_id`, if registered.
    pub fn peer_score_state(&self, peer_id: &PeerId) -> Option<&PeerScoreState> {
        self.peer_scores.get(peer_id)
    }

    /// True if `peer_id` is currently banned in the engine's [`BanList`].
    pub fn is_peer_banned(&self, peer_id: &PeerId, now_ms: u64) -> bool {
        self.bans.is_peer_banned(peer_id, now_ms)
    }

    /// Borrow the engine's [`BanList`].
    pub const fn ban_list(&self) -> &BanList {
        &self.bans
    }

    /// Borrow the engine's [`SeenCache`].
    pub const fn seen_cache(&self) -> &SeenCache {
        &self.seen
    }

    /// Validated messages received or locally published on `topic`.
    pub fn messages_for_topic(&self, topic: GossipTopic) -> &[GossipMessage] {
        self.inbox.get(&topic).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Sign and publish a payload to `topic`, recording its `MessageId` in
    /// [`SeenCache`] and returning `(message, forward_peers)`.
    pub fn publish(
        &mut self,
        topic: GossipTopic,
        payload: Vec<u8>,
    ) -> Result<(GossipMessage, Vec<PeerId>), P2pError> {
        if !self.local_topics.contains(&topic) {
            return Err(P2pError::UnsubscribedTopic(topic.as_str().to_owned()));
        }
        let seq_no = self.next_seq_no;
        self.next_seq_no = self.next_seq_no.saturating_add(1);
        let msg = GossipMessage::sign(&self.identity, &self.chain_id, topic, seq_no, payload)?;
        let id = msg.message_id();
        self.seen.insert(id);
        self.inbox.entry(topic).or_default().push(msg.clone());
        let forward_to = self.mesh_peers_for(topic, None);
        Ok((msg, forward_to))
    }

    /// Decode and ingest raw wire gossip bytes from `from_peer`.
    ///
    /// `from_peer` must be the authenticated transport source, not the
    /// message's untrusted `sender_peer_id`. Rejections and score-based bans
    /// are attributed to this source; gossip messages may be forwarded.
    pub fn ingest_wire(
        &mut self,
        from_peer: PeerId,
        raw_bytes: &[u8],
        now_ms: u64,
    ) -> GossipOutcome {
        match GossipMessage::decode(raw_bytes) {
            Ok(msg) => self.ingest(from_peer, msg, now_ms),
            Err(err) => self.penalize_and_reject(
                from_peer,
                self.params.invalid_message_penalty,
                err,
                now_ms,
            ),
        }
    }

    /// Ingest a [`GossipMessage`] from `from_peer` at timestamp `now_ms`.
    ///
    /// `from_peer` is the authenticated transport source. The signed message
    /// origin is separately checked, and must not be used as the ban/score
    /// attribution target because the transport peer may be forwarding it.
    pub fn ingest(&mut self, from_peer: PeerId, msg: GossipMessage, now_ms: u64) -> GossipOutcome {
        // 1. Reject banned peers immediately.
        if let Err(err) = self.bans.check_peer(&from_peer, now_ms) {
            let score = self
                .peer_score(&from_peer)
                .unwrap_or(self.params.ban_threshold);
            return GossipOutcome::Rejected {
                error: err,
                peer_score: score,
                banned: true,
            };
        }

        let initial = self.params.initial_score;
        let state = self
            .peer_scores
            .entry(from_peer)
            .or_insert_with(|| PeerScoreState::new(initial));

        // 2. Reject graylisted peers.
        if state.score <= self.params.graylist_threshold {
            let current_score = state.score;
            return GossipOutcome::Rejected {
                error: P2pError::PeerScoreBelowThreshold {
                    peer_id: from_peer,
                    score: current_score,
                },
                peer_score: current_score,
                banned: current_score <= self.params.ban_threshold,
            };
        }

        // 3. Enforce per-window flood rate limit.
        if now_ms.saturating_sub(state.window_start_ms) >= self.params.rate_window_ms {
            state.window_start_ms = now_ms;
            state.messages_in_window = 0;
        }
        state.messages_in_window += 1;
        let count_in_window = state.messages_in_window;
        if count_in_window > self.params.max_messages_per_window {
            let max = self.params.max_messages_per_window;
            return self.penalize_and_reject(
                from_peer,
                self.params.flood_penalty,
                P2pError::GossipRateLimitExceeded {
                    peer_id: from_peer,
                    count: count_in_window,
                    max,
                },
                now_ms,
            );
        }

        // 4. Local topic subscription check.
        if !self.local_topics.contains(&msg.topic) {
            return self.penalize_and_reject(
                from_peer,
                self.params.invalid_message_penalty,
                P2pError::UnsubscribedTopic(msg.topic.as_str().to_owned()),
                now_ms,
            );
        }

        // 5. Deduplication check on deterministic MessageId before expensive crypto
        //    if and only if payload bounds pass; wait — to prevent poisoning SeenCache
        //    with forged signatures, check `seen.contains` first, and insert into
        //    `seen` ONLY after cryptographic verification succeeds!
        let candidate_id = msg.message_id();
        if self.seen.contains(&candidate_id) {
            let dup_penalty = self.params.duplicate_penalty;
            if let Some(peer_state) = self.peer_scores.get_mut(&from_peer) {
                peer_state.duplicates += 1;
                peer_state.score = peer_state.score.saturating_sub(dup_penalty);
            }
            return GossipOutcome::Duplicate(candidate_id);
        }

        // 6. Cryptographic & structural validation (`D-N3`).
        let verified_id = match msg.verify(&self.chain_id, self.params.max_payload_bytes) {
            Ok(id) => id,
            Err(err) => {
                return self.penalize_and_reject(
                    from_peer,
                    self.params.invalid_message_penalty,
                    err,
                    now_ms,
                );
            }
        };

        // 7. Record in SeenCache and reward peer score.
        self.seen.insert(verified_id);
        if let Some(peer_state) = self.peer_scores.get_mut(&from_peer) {
            peer_state.valid_deliveries += 1;
            peer_state.score =
                (peer_state.score + self.params.valid_message_reward).min(self.params.max_score);
        }

        let topic = msg.topic;
        let sender = msg.sender_peer_id;
        self.inbox.entry(topic).or_default().push(msg);

        let forward_to = self.mesh_peers_for(topic, Some((from_peer, sender)));
        GossipOutcome::Accepted {
            message_id: verified_id,
            forward_to,
        }
    }

    fn mesh_peers_for(&self, topic: GossipTopic, exclude: Option<(PeerId, PeerId)>) -> Vec<PeerId> {
        let mut peers = Vec::new();
        for (peer_id, topics) in &self.peer_topics {
            if !topics.contains(&topic) {
                continue;
            }
            if let Some((from_peer, origin_peer)) = exclude
                && (*peer_id == from_peer || *peer_id == origin_peer)
            {
                continue;
            }
            if let Some(state) = self.peer_scores.get(peer_id)
                && state.score <= self.params.graylist_threshold
            {
                continue;
            }
            peers.push(*peer_id);
        }
        peers
    }

    fn penalize_and_reject(
        &mut self,
        peer_id: PeerId,
        penalty: i64,
        error: P2pError,
        now_ms: u64,
    ) -> GossipOutcome {
        let initial = self.params.initial_score;
        let state = self
            .peer_scores
            .entry(peer_id)
            .or_insert_with(|| PeerScoreState::new(initial));
        state.violations += 1;
        state.score = state.score.saturating_sub(penalty);
        let new_score = state.score;
        let banned = if new_score <= self.params.ban_threshold {
            self.bans.ban_peer(
                peer_id,
                BanReason::ProtocolViolation(format!("gossip_score={new_score}: {error}")),
                None,
            );
            self.peer_topics.remove(&peer_id);
            true
        } else {
            self.bans.is_peer_banned(&peer_id, now_ms)
        };
        GossipOutcome::Rejected {
            error,
            peer_score: new_score,
            banned,
        }
    }
}

/// Multi-node gossip mesh coordinator for deterministic and high-throughput
/// gossip propagation across connected [`GossipEngine`] instances.
pub struct GossipMesh {
    nodes: BTreeMap<PeerId, GossipEngine>,
}

impl GossipMesh {
    /// Construct an empty [`GossipMesh`].
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
        }
    }

    /// Add a [`GossipEngine`] node to the mesh.
    pub fn add_node(&mut self, engine: GossipEngine) {
        self.nodes.insert(engine.local_peer_id(), engine);
    }

    /// Connect two peers bidirectionally on all `D-N2` topics.
    pub fn connect(&mut self, a: PeerId, b: PeerId) {
        if let Some(node_a) = self.nodes.get_mut(&a) {
            node_a.register_peer(b, GossipTopic::ALL);
        }
        if let Some(node_b) = self.nodes.get_mut(&b) {
            node_b.register_peer(a, GossipTopic::ALL);
        }
    }

    /// Borrow a node's [`GossipEngine`] by [`PeerId`].
    pub fn node(&self, peer_id: &PeerId) -> Option<&GossipEngine> {
        self.nodes.get(peer_id)
    }

    /// Mutably borrow a node's [`GossipEngine`] by [`PeerId`].
    pub fn node_mut(&mut self, peer_id: &PeerId) -> Option<&mut GossipEngine> {
        self.nodes.get_mut(peer_id)
    }

    /// Publish a payload on `publisher` and propagate wire-encoded gossip frames
    /// across the mesh until quiescence. Returns total deliveries across all peers.
    pub fn broadcast(
        &mut self,
        publisher: PeerId,
        topic: GossipTopic,
        payload: Vec<u8>,
        now_ms: u64,
    ) -> Result<usize, P2pError> {
        let (msg, initial_targets) = self
            .nodes
            .get_mut(&publisher)
            .ok_or_else(|| P2pError::Codec("unknown publisher peer".into()))?
            .publish(topic, payload)?;
        let wire = msg.encode();

        let mut queue: VecDeque<(PeerId, PeerId, Vec<u8>)> = initial_targets
            .into_iter()
            .map(|target| (publisher, target, wire.clone()))
            .collect();

        let mut accepted_deliveries = 0usize;
        while let Some((from_peer, to_peer, raw)) = queue.pop_front() {
            let Some(recipient) = self.nodes.get_mut(&to_peer) else {
                continue;
            };
            if let GossipOutcome::Accepted { forward_to, .. } =
                recipient.ingest_wire(from_peer, &raw, now_ms)
            {
                accepted_deliveries += 1;
                for next_peer in forward_to {
                    queue.push_back((to_peer, next_peer, raw.clone()));
                }
            }
        }
        Ok(accepted_deliveries)
    }
}

impl Default for GossipMesh {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAIN: &str = "auralite-devnet-1";

    fn make_engine(seed_byte: u8) -> GossipEngine {
        let id = PeerIdentity::from_seed([seed_byte; 32]);
        GossipEngine::new(id, CHAIN, PeerScoreParams::default())
    }

    #[test]
    fn tampered_gossip_signature_is_rejected_and_penalized() {
        let mut receiver = make_engine(1);
        let sender = PeerIdentity::from_seed([2u8; 32]);
        receiver.register_peer(sender.peer_id(), GossipTopic::ALL);

        let mut msg = GossipMessage::sign(
            &sender,
            CHAIN,
            GossipTopic::Tx,
            1,
            b"signed-transfer-bytes".to_vec(),
        )
        .unwrap();
        msg.signature[0] ^= 0xFF;

        let outcome = receiver.ingest(sender.peer_id(), msg, 100);
        assert_eq!(
            outcome,
            GossipOutcome::Rejected {
                error: P2pError::InvalidGossipSignature,
                peer_score: -25,
                banned: false,
            }
        );
        assert!(receiver.messages_for_topic(GossipTopic::Tx).is_empty());
    }

    #[test]
    fn duplicate_gossip_message_is_dropped_by_seen_cache() {
        let mut receiver = make_engine(1);
        let sender = PeerIdentity::from_seed([2u8; 32]);
        let relay = PeerIdentity::from_seed([3u8; 32]);
        receiver.register_peer(sender.peer_id(), GossipTopic::ALL);
        receiver.register_peer(relay.peer_id(), GossipTopic::ALL);

        let msg =
            GossipMessage::sign(&sender, CHAIN, GossipTopic::Vote, 7, b"vote-1".to_vec()).unwrap();
        let expected_id = msg.message_id();

        let first = receiver.ingest(sender.peer_id(), msg.clone(), 100);
        assert_eq!(
            first,
            GossipOutcome::Accepted {
                message_id: expected_id,
                forward_to: vec![relay.peer_id()],
            }
        );

        let second = receiver.ingest(relay.peer_id(), msg, 101);
        assert_eq!(second, GossipOutcome::Duplicate(expected_id));
        assert_eq!(receiver.messages_for_topic(GossipTopic::Vote).len(), 1);
    }

    #[test]
    fn flooding_peer_loses_score_and_is_banned() {
        let params = PeerScoreParams {
            max_messages_per_window: 5,
            rate_window_ms: 1_000,
            flood_penalty: 20,
            graylist_threshold: -60,
            ban_threshold: -50,
            ..PeerScoreParams::default()
        };
        let mut receiver = GossipEngine::new(PeerIdentity::from_seed([1u8; 32]), CHAIN, params);
        let flooder = PeerIdentity::from_seed([9u8; 32]);
        receiver.register_peer(flooder.peer_id(), GossipTopic::ALL);

        // First 5 unique messages in the 1000 ms window are accepted (+5 score).
        for seq in 1..=5 {
            let msg = GossipMessage::sign(
                &flooder,
                CHAIN,
                GossipTopic::Tx,
                seq,
                format!("tx-{seq}").into_bytes(),
            )
            .unwrap();
            assert!(matches!(
                receiver.ingest(flooder.peer_id(), msg, 500),
                GossipOutcome::Accepted { .. }
            ));
        }
        assert_eq!(receiver.peer_score(&flooder.peer_id()), Some(5));

        // Subsequent messages in the same window exceed max_messages_per_window (5)
        // and incur -20 flood_penalty each: 5 -> -15 -> -35 -> -55 (<= -50 -> BANNED).
        for seq in 6..=8 {
            let msg = GossipMessage::sign(
                &flooder,
                CHAIN,
                GossipTopic::Tx,
                seq,
                format!("tx-{seq}").into_bytes(),
            )
            .unwrap();
            let outcome = receiver.ingest(flooder.peer_id(), msg, 500);
            if seq < 8 {
                assert!(matches!(
                    outcome,
                    GossipOutcome::Rejected {
                        error: P2pError::GossipRateLimitExceeded { .. },
                        banned: false,
                        ..
                    }
                ));
            } else {
                assert!(matches!(
                    outcome,
                    GossipOutcome::Rejected {
                        error: P2pError::GossipRateLimitExceeded { .. },
                        peer_score: -55,
                        banned: true,
                    }
                ));
            }
        }
        assert!(receiver.is_peer_banned(&flooder.peer_id(), 500));
    }

    #[test]
    fn invalid_messages_drive_peer_score_to_ban() {
        let mut receiver = make_engine(1);
        let attacker = PeerIdentity::from_seed([4u8; 32]);
        receiver.register_peer(attacker.peer_id(), GossipTopic::ALL);

        // Two invalid signatures drop score from 0 -> -25 -> -50 (ban_threshold).
        for seq in 1..=2 {
            let mut bad = GossipMessage::sign(
                &attacker,
                CHAIN,
                GossipTopic::Proposal,
                seq,
                b"forged-proposal".to_vec(),
            )
            .unwrap();
            bad.signature[10] ^= 0x55;
            let outcome = receiver.ingest(attacker.peer_id(), bad, 200);
            if seq == 1 {
                assert_eq!(
                    outcome,
                    GossipOutcome::Rejected {
                        error: P2pError::InvalidGossipSignature,
                        peer_score: -25,
                        banned: false,
                    }
                );
            } else {
                assert!(matches!(
                    outcome,
                    GossipOutcome::Rejected {
                        error: P2pError::PeerScoreBelowThreshold { .. }
                            | P2pError::InvalidGossipSignature,
                        ..
                    }
                ));
            }
        }

        // With graylist_threshold set below ban_threshold, verify direct ban escalation:
        let params = PeerScoreParams {
            invalid_message_penalty: 30,
            graylist_threshold: -60,
            ban_threshold: -50,
            ..PeerScoreParams::default()
        };
        let mut engine = GossipEngine::new(PeerIdentity::from_seed([10u8; 32]), CHAIN, params);
        engine.register_peer(attacker.peer_id(), GossipTopic::ALL);
        for seq in 1..=2 {
            let mut bad = GossipMessage::sign(
                &attacker,
                CHAIN,
                GossipTopic::Proposal,
                seq,
                b"forged-proposal".to_vec(),
            )
            .unwrap();
            bad.signature[0] ^= 0x01;
            let _ = engine.ingest(attacker.peer_id(), bad, 300);
        }
        assert!(engine.is_peer_banned(&attacker.peer_id(), 300));
        assert_eq!(engine.peer_score(&attacker.peer_id()), Some(-60));
    }

    #[test]
    fn oversized_and_empty_gossip_payloads_are_rejected() {
        let sender = PeerIdentity::from_seed([5u8; 32]);
        assert_eq!(
            GossipMessage::sign(&sender, CHAIN, GossipTopic::Tx, 1, Vec::new()).unwrap_err(),
            P2pError::GossipPayloadEmpty
        );
        assert_eq!(
            GossipMessage::sign(
                &sender,
                CHAIN,
                GossipTopic::Snapshot,
                2,
                vec![0xAA; MAX_GOSSIP_PAYLOAD_BYTES + 1],
            )
            .unwrap_err(),
            P2pError::GossipPayloadTooLarge {
                size: MAX_GOSSIP_PAYLOAD_BYTES + 1,
                max: MAX_GOSSIP_PAYLOAD_BYTES,
            }
        );

        // Also verify receiver-side enforcement against tighter max_payload_bytes.
        let params = PeerScoreParams {
            max_payload_bytes: 64,
            ..PeerScoreParams::default()
        };
        let mut receiver = GossipEngine::new(PeerIdentity::from_seed([6u8; 32]), CHAIN, params);
        receiver.register_peer(sender.peer_id(), GossipTopic::ALL);
        let msg = GossipMessage::sign(&sender, CHAIN, GossipTopic::Tx, 3, vec![0x11; 128]).unwrap();
        let outcome = receiver.ingest(sender.peer_id(), msg, 100);
        assert_eq!(
            outcome,
            GossipOutcome::Rejected {
                error: P2pError::GossipPayloadTooLarge { size: 128, max: 64 },
                peer_score: -25,
                banned: false,
            }
        );
    }

    #[test]
    fn unsubscribed_or_unknown_topic_is_rejected() {
        assert_eq!(
            GossipTopic::from_name("unknown-topic").unwrap_err(),
            P2pError::UnknownGossipTopic("unknown-topic".into())
        );
        assert_eq!(
            GossipTopic::from_wire_tag(99).unwrap_err(),
            P2pError::UnknownGossipTopic("tag:99".into())
        );

        let mut receiver = make_engine(7);
        receiver.unsubscribe(GossipTopic::Snapshot);
        let sender = PeerIdentity::from_seed([8u8; 32]);
        receiver.register_peer(sender.peer_id(), GossipTopic::ALL);

        let msg = GossipMessage::sign(
            &sender,
            CHAIN,
            GossipTopic::Snapshot,
            1,
            b"snapshot-chunk".to_vec(),
        )
        .unwrap();
        let outcome = receiver.ingest(sender.peer_id(), msg, 100);
        assert_eq!(
            outcome,
            GossipOutcome::Rejected {
                error: P2pError::UnsubscribedTopic("snapshot".into()),
                peer_score: -25,
                banned: false,
            }
        );
    }
}
