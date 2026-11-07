//! N8 Chaos harness: deterministic latency, packet-loss, network-partition,
//! eclipse-attack, and Byzantine-peer simulation over a 100-node stand, plus
//! light-client verification of the converged finalized `state_root` (`D-N3`,
//! `D-N4`).

use aura_crypto::blake3_hash;

use crate::block_sync::{SyncQc, SyncValidatorSet, SyncedHeader};
use crate::dos_defence::{DosDefenceEngine, DosDefenceLimits, IngressEnvelope, IngressKind};
use crate::error::P2pError;
use crate::identity::{PeerId, PeerIdentity};
use crate::light_client::{
    FinalizedCheckpoint, LightClientBounds, LightClientProof, LightClientVerifier, build_das_tree,
    sample_das_chunks,
};

/// Default number of simulated nodes in the N8 stand.
pub const DEFAULT_STAND_NODES: usize = 100;

/// Default number of active consensus validators in the 100-node stand.
pub const DEFAULT_STAND_VALIDATORS: usize = 7;

/// Domain tag for deterministic chaos PRNG draws.
pub const CHAOS_PRNG_DOMAIN: &[u8] = b"AuraLite-Crypto/p2p-chaos-prng-v1\0";

/// Configuration for one chaos scenario on the N8 multi-node stand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChaosScenarioConfig {
    /// Stable scenario identifier (e.g. `"S01_100nodes_20pct_loss_1_byzantine"`).
    pub name: String,
    /// Total nodes in the stand (`100` in the standard N8 stand).
    pub node_count: usize,
    /// Number of consensus heights to finalize during the scenario.
    pub target_heights: u64,
    /// Base one-way link latency in milliseconds.
    pub base_latency_ms: u64,
    /// Maximum additional deterministic jitter in milliseconds.
    pub jitter_ms: u64,
    /// Packet-loss rate in basis points (`0..=9_000`; `2_000` = 20% packet loss).
    pub packet_loss_bps: u16,
    /// Optional network partition: `(minority_group_size, partition_duration_heights)`.
    pub partition: Option<(usize, u64)>,
    /// Indices of nodes targeted by a Sybil eclipse attempt during the first half of the run.
    pub eclipsed_nodes: Vec<usize>,
    /// Optional index of a Byzantine validator/peer that equivocates and sends forged packets.
    pub byzantine_node: Option<usize>,
}

/// Per-node finalized state tracked inside [`ChaosStand`].
#[derive(Debug, Clone)]
pub struct ChaosNodeState {
    /// Node index (`0..node_count`).
    pub index: usize,
    /// Node's authenticated `PeerId`.
    pub peer_id: PeerId,
    /// Whether this node is configured to act as a Byzantine equivocator.
    pub is_byzantine: bool,
    /// Latest verified finalized height on this node.
    pub finalized_height: u64,
    /// Latest verified finalized `block_hash` on this node.
    pub finalized_block_hash: [u8; 32],
    /// Latest verified finalized `state_root` on this node.
    pub finalized_state_root: [u8; 32],
    /// Per-node N6 DoS and equivocation defence engine.
    pub dos_engine: DosDefenceEngine,
    /// Count of packets dropped on ingress due to simulated packet loss or partition.
    pub packets_dropped: usize,
    /// Count of heights recovered via header-first catch-up sync after packet loss/partition/eclipse.
    pub heights_synced_on_heal: usize,
    /// Count of uncertified eclipse headers rejected by this node.
    pub eclipse_headers_rejected: usize,
}

fn node_identity(idx: usize) -> PeerIdentity {
    let mut seed = [0u8; 32];
    seed[0] = 0xC8;
    seed[24..32].copy_from_slice(&(idx as u64 + 1).to_be_bytes());
    PeerIdentity::from_seed(seed)
}

/// Result receipt for one execution `(scenario, run_index)` on the N8 stand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChaosRunReport {
    /// Scenario name.
    pub scenario: String,
    /// Run index (`0..runs_per_scenario`).
    pub run_index: usize,
    /// Number of nodes in the stand.
    pub node_count: usize,
    /// Number of honest nodes that converged.
    pub honest_nodes_converged: usize,
    /// Finalized height reached by all honest nodes.
    pub finalized_height: u64,
    /// Converged finalized `block_hash` across all honest nodes.
    pub finalized_block_hash: [u8; 32],
    /// Converged finalized `state_root` across all honest nodes.
    pub finalized_state_root: [u8; 32],
    /// Whether the independent N7 light client verified the finalized head + QC + DAS proof.
    pub light_client_verified: bool,
    /// Total packets dropped by loss/partition faults across the run.
    pub total_packets_dropped: usize,
    /// Total heights recovered via header-first sync after packet drops/partition heal.
    pub total_catchup_syncs: usize,
    /// Total uncertified eclipse headers rejected by targeted nodes.
    pub total_eclipse_rejections: usize,
    /// Number of honest nodes that detected and banned the Byzantine equivocator.
    pub byzantine_bans_recorded: usize,
    /// Maximum simulated delivery latency observed in milliseconds.
    pub max_observed_latency_ms: u64,
}

/// Summary of a multi-scenario × multi-run N8 chaos campaign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChaosCampaignSummary {
    /// Number of scenarios executed.
    pub scenarios_count: usize,
    /// Number of runs per scenario.
    pub runs_per_scenario: usize,
    /// Total runs executed (`scenarios_count * runs_per_scenario`).
    pub total_runs: usize,
    /// Number of runs where any honest nodes diverged on finalized head (`must be 0`).
    pub divergent_runs: usize,
    /// Individual run reports.
    pub reports: Vec<ChaosRunReport>,
}

/// Deterministic pseudorandom u64 draw for `(scenario, run_index, height, node_idx, salt)`.
fn chaos_draw(scenario: &str, run_index: usize, height: u64, node_idx: usize, salt: u64) -> u64 {
    let mut buf = Vec::with_capacity(CHAOS_PRNG_DOMAIN.len() + scenario.len() + 36);
    buf.extend_from_slice(CHAOS_PRNG_DOMAIN);
    buf.extend_from_slice(&(scenario.len() as u32).to_le_bytes());
    buf.extend_from_slice(scenario.as_bytes());
    buf.extend_from_slice(&(run_index as u64).to_le_bytes());
    buf.extend_from_slice(&height.to_le_bytes());
    buf.extend_from_slice(&(node_idx as u64).to_le_bytes());
    buf.extend_from_slice(&salt.to_le_bytes());
    let digest = blake3_hash(&buf);
    u64::from_le_bytes(digest.as_bytes()[..8].try_into().unwrap())
}

/// Verify that all honest nodes in `nodes` reached the exact same `(height, block_hash, state_root)`.
pub fn verify_cluster_convergence(
    scenario: &str,
    nodes: &[ChaosNodeState],
    expected_height: u64,
) -> Result<([u8; 32], [u8; 32], usize), P2pError> {
    let honest: Vec<&ChaosNodeState> = nodes.iter().filter(|n| !n.is_byzantine).collect();
    if honest.is_empty() {
        return Err(P2pError::ChaosInvalidConfig(
            "stand must contain at least one honest node".to_owned(),
        ));
    }
    let first = honest[0];
    if first.finalized_height != expected_height {
        return Err(P2pError::ChaosHeadDivergence {
            scenario: scenario.to_owned(),
            node_index: first.index,
        });
    }
    for node in &honest[1..] {
        if node.finalized_height != expected_height
            || node.finalized_block_hash != first.finalized_block_hash
            || node.finalized_state_root != first.finalized_state_root
        {
            return Err(P2pError::ChaosHeadDivergence {
                scenario: scenario.to_owned(),
                node_index: node.index,
            });
        }
    }
    Ok((
        first.finalized_block_hash,
        first.finalized_state_root,
        honest.len(),
    ))
}

/// Simulated multi-node P2P & consensus stand for N8 chaos testing.
pub struct ChaosStand {
    chain_id: String,
    config: ChaosScenarioConfig,
    validators: Vec<PeerIdentity>,
    validator_set: SyncValidatorSet,
    nodes: Vec<ChaosNodeState>,
    genesis_header: SyncedHeader,
}

impl ChaosStand {
    /// Build a new [`ChaosStand`] for `config`.
    pub fn new(chain_id: &str, config: ChaosScenarioConfig) -> Result<Self, P2pError> {
        if config.node_count < 4 {
            return Err(P2pError::ChaosInvalidConfig(
                "node_count must be at least 4".to_owned(),
            ));
        }
        if config.target_heights == 0 {
            return Err(P2pError::ChaosInvalidConfig(
                "target_heights must be >= 1".to_owned(),
            ));
        }
        if config.packet_loss_bps > 9_500 {
            return Err(P2pError::ChaosInvalidConfig(
                "packet_loss_bps must be <= 9500".to_owned(),
            ));
        }

        let val_count = DEFAULT_STAND_VALIDATORS.min(config.node_count);
        let mut nodes = Vec::with_capacity(config.node_count);
        for idx in 0..config.node_count {
            let identity = node_identity(idx);
            let is_byzantine = config.byzantine_node == Some(idx);
            nodes.push(ChaosNodeState {
                index: idx,
                peer_id: identity.peer_id(),
                is_byzantine,
                finalized_height: 0,
                finalized_block_hash: [0u8; 32],
                finalized_state_root: [0u8; 32],
                dos_engine: DosDefenceEngine::new(chain_id, DosDefenceLimits::default()),
                packets_dropped: 0,
                heights_synced_on_heal: 0,
                eclipse_headers_rejected: 0,
            });
        }

        let validators: Vec<PeerIdentity> = (0..val_count).map(node_identity).collect();
        let val_entries = validators
            .iter()
            .map(|id| (id.peer_id(), *id.public_key(), 100u64))
            .collect();
        let validator_set = SyncValidatorSet::from_validators(val_entries)?;

        let genesis_header = SyncedHeader::new(
            chain_id,
            0,
            [0u8; 32],
            [0x10; 32],
            [0x20; 32],
            1_700_008_000_000,
            validators[0].peer_id(),
        );
        for node in &mut nodes {
            node.finalized_height = 0;
            node.finalized_block_hash = genesis_header.block_hash;
            node.finalized_state_root = genesis_header.state_root;
        }

        Ok(Self {
            chain_id: chain_id.to_owned(),
            config,
            validators,
            validator_set,
            nodes,
            genesis_header,
        })
    }

    /// Borrow the nodes in the stand.
    pub fn nodes(&self) -> &[ChaosNodeState] {
        &self.nodes
    }

    /// Execute one deterministic chaos run (`run_index`) and verify convergence + light client.
    pub fn execute_run(&mut self, run_index: usize) -> Result<ChaosRunReport, P2pError> {
        let mut canonical_headers: Vec<(SyncedHeader, SyncQc)> =
            Vec::with_capacity(self.config.target_heights as usize);
        let mut parent_hash = self.genesis_header.block_hash;
        let mut base_ts = self.genesis_header.timestamp_ms;
        let mut max_observed_latency_ms = self.config.base_latency_ms;

        // Honest validator signers (at least 5 of 7 validators are honest even if 1 is Byzantine).
        let honest_validators: Vec<&PeerIdentity> = self
            .validators
            .iter()
            .enumerate()
            .filter(|(idx, _)| self.config.byzantine_node != Some(*idx))
            .map(|(_, id)| id)
            .collect();
        let quorum_signers: Vec<&PeerIdentity> =
            honest_validators.iter().copied().take(5).collect();

        let eclipse_attacker = PeerIdentity::from_seed([0xFA; 32]);

        for height in 1..=self.config.target_heights {
            base_ts += 1_000;
            let proposer = honest_validators[(height as usize) % honest_validators.len()];
            let mut state_root = [0u8; 32];
            state_root[0] = 0x5A;
            state_root[8..16].copy_from_slice(&height.to_be_bytes());
            state_root[16..24].copy_from_slice(&(run_index as u64).to_be_bytes());

            // Build 4 DAS chunks for the block so `tx_root` is a genuine DAS Merkle root.
            let chunks: Vec<Vec<u8>> = (0..4u8)
                .map(|c| vec![c ^ (height as u8) ^ (run_index as u8); 64])
                .collect();
            let (tx_root, _) = build_das_tree(&chunks)?;

            let header = SyncedHeader::new(
                &self.chain_id,
                height,
                parent_hash,
                tx_root,
                state_root,
                base_ts,
                proposer.peer_id(),
            );
            let qc = SyncQc::sign(
                &self.chain_id,
                height,
                1,
                header.block_hash,
                &quorum_signers,
            );

            // If a Byzantine node is configured, it broadcasts conflicting votes (equivocation)
            // at height 1 so honest nodes detect and ban it via DosDefenceEngine.
            if let Some(byz_idx) = self.config.byzantine_node
                && height == 1
            {
                let byz_id = node_identity(byz_idx);
                let vote_a = IngressEnvelope::sign(
                    &byz_id,
                    &self.chain_id,
                    1,
                    IngressKind::Vote {
                        height: 1,
                        round: 1,
                        block_hash: header.block_hash,
                    },
                );
                let vote_b = IngressEnvelope::sign(
                    &byz_id,
                    &self.chain_id,
                    2,
                    IngressKind::Vote {
                        height: 1,
                        round: 1,
                        block_hash: [0xEE; 32],
                    },
                );
                for node in &mut self.nodes {
                    if !node.is_byzantine {
                        let _ = node
                            .dos_engine
                            .ingest(byz_id.peer_id(), vote_a.clone(), base_ts);
                        let _ = node
                            .dos_engine
                            .ingest(byz_id.peer_id(), vote_b.clone(), base_ts);
                    }
                }
            }

            // Deliver `header + qc` across the 100-node stand subject to latency, packet loss,
            // temporary partition, and eclipse attempts.
            let partition_active = self
                .config
                .partition
                .is_some_and(|(_, dur)| height <= dur.min(self.config.target_heights));
            let partitioned_minority = self.config.partition.map_or(0, |(m, _)| m);
            let eclipse_active = height <= (self.config.target_heights / 2).max(1);

            for node in &mut self.nodes {
                if node.is_byzantine {
                    // Byzantine node tries to hold a forked state_root locally.
                    node.finalized_height = height;
                    node.finalized_block_hash = [0xFF; 32];
                    node.finalized_state_root = [0xFF; 32];
                    continue;
                }

                // Simulated link latency + jitter.
                let jitter = if self.config.jitter_ms > 0 {
                    chaos_draw(&self.config.name, run_index, height, node.index, 1)
                        % (self.config.jitter_ms + 1)
                } else {
                    0
                };
                let latency = self.config.base_latency_ms + jitter;
                if latency > max_observed_latency_ms {
                    max_observed_latency_ms = latency;
                }

                // If this node is targeted by an eclipse attack while `eclipse_active`,
                // the eclipse attacker feeds it a forged/uncertified header first.
                if eclipse_active && self.config.eclipsed_nodes.contains(&node.index) {
                    let fake_header = SyncedHeader::new(
                        &self.chain_id,
                        height,
                        node.finalized_block_hash,
                        [0x99; 32],
                        [0x88; 32],
                        base_ts,
                        eclipse_attacker.peer_id(),
                    );
                    let fake_qc = SyncQc::sign(
                        &self.chain_id,
                        height,
                        1,
                        fake_header.block_hash,
                        &[&eclipse_attacker],
                    );
                    // Node enforces D-N3/D-N4: verifies QC against validator_set and rejects eclipse header.
                    if fake_qc
                        .verify(
                            &self.chain_id,
                            height,
                            &fake_header.block_hash,
                            &self.validator_set,
                        )
                        .is_err()
                    {
                        node.eclipse_headers_rejected += 1;
                    } else {
                        // Should never happen unless QC verification is broken.
                        node.finalized_height = height;
                        node.finalized_block_hash = fake_header.block_hash;
                        node.finalized_state_root = fake_header.state_root;
                    }
                }

                // Check whether gossip delivery is dropped by partition or packet loss.
                let in_minority_partition = partition_active
                    && node.index >= (self.config.node_count - partitioned_minority);
                let loss_roll = (chaos_draw(&self.config.name, run_index, height, node.index, 2)
                    % 10_000) as u16;
                let packet_lost = loss_roll < self.config.packet_loss_bps;

                if in_minority_partition || packet_lost {
                    node.packets_dropped += 1;
                    continue;
                }

                // If the node missed earlier heights due to packet loss, it catches up
                // contiguous verified headers before applying `height`.
                if node.finalized_height + 1 < height {
                    for (prev_h, prev_qc) in
                        &canonical_headers[node.finalized_height as usize..(height - 1) as usize]
                    {
                        prev_h.verify_against_parent(
                            &self.chain_id,
                            node.finalized_height,
                            &node.finalized_block_hash,
                            base_ts.saturating_sub(10_000),
                        )?;
                        prev_qc.verify(
                            &self.chain_id,
                            prev_h.height,
                            &prev_h.block_hash,
                            &self.validator_set,
                        )?;
                        node.finalized_height = prev_h.height;
                        node.finalized_block_hash = prev_h.block_hash;
                        node.finalized_state_root = prev_h.state_root;
                        node.heights_synced_on_heal += 1;
                    }
                }

                header.verify_against_parent(
                    &self.chain_id,
                    node.finalized_height,
                    &node.finalized_block_hash,
                    base_ts.saturating_sub(10_000),
                )?;
                qc.verify(
                    &self.chain_id,
                    header.height,
                    &header.block_hash,
                    &self.validator_set,
                )?;
                node.finalized_height = header.height;
                node.finalized_block_hash = header.block_hash;
                node.finalized_state_root = header.state_root;
            }

            parent_hash = header.block_hash;
            canonical_headers.push((header, qc));
        }

        // Partition healing & anti-entropy header-first sync: any honest node that
        // missed the tail heights during packet loss or partition catches up via verified header+QC sync.
        for node in &mut self.nodes {
            if node.is_byzantine {
                continue;
            }
            if node.finalized_height < self.config.target_heights {
                for (h, qc) in &canonical_headers[node.finalized_height as usize..] {
                    if h.block_hash != h.compute_block_hash() {
                        return Err(P2pError::BlockSyncHeaderHashMismatch { height: h.height });
                    }
                    qc.verify(&self.chain_id, h.height, &h.block_hash, &self.validator_set)?;
                    node.finalized_height = h.height;
                    node.finalized_block_hash = h.block_hash;
                    node.finalized_state_root = h.state_root;
                    node.heights_synced_on_heal += 1;
                }
            }
        }

        // Verify zero finalized-head divergence across all honest nodes.
        let (finalized_block_hash, finalized_state_root, honest_nodes_converged) =
            verify_cluster_convergence(&self.config.name, &self.nodes, self.config.target_heights)?;

        // Verify the converged finalized head on an N7 LightClientVerifier without a full node.
        let (last_header, last_qc) = canonical_headers.last().unwrap().clone();
        let last_chunks: Vec<Vec<u8>> = (0..4u8)
            .map(|c| vec![c ^ (last_header.height as u8) ^ (run_index as u8); 64])
            .collect();
        let (_, all_samples) = build_das_tree(&last_chunks)?;
        let light_peer = PeerIdentity::from_seed([0x7E; 32]).peer_id();
        let das_samples = sample_das_chunks(&all_samples, &last_header.block_hash, &light_peer, 4);
        let mut light_client = LightClientVerifier::new(
            FinalizedCheckpoint::from_header(&self.genesis_header),
            self.validator_set.clone(),
            LightClientBounds::default(),
        );
        let proof = LightClientProof {
            checkpoint_height: self.genesis_header.height,
            checkpoint_hash: self.genesis_header.block_hash,
            header: last_header,
            qc: last_qc,
            das_samples,
        };
        let relayer_peer: PeerId = self.validators[0].peer_id();
        let lc_receipt = light_client.verify_and_advance(relayer_peer, &proof, base_ts)?;
        let light_client_verified = lc_receipt.block_hash == finalized_block_hash
            && lc_receipt.state_root == finalized_state_root;

        let total_packets_dropped = self.nodes.iter().map(|n| n.packets_dropped).sum();
        let total_catchup_syncs = self.nodes.iter().map(|n| n.heights_synced_on_heal).sum();
        let total_eclipse_rejections = self.nodes.iter().map(|n| n.eclipse_headers_rejected).sum();
        let byzantine_bans_recorded = if let Some(byz_idx) = self.config.byzantine_node {
            let byz_peer = self.nodes[byz_idx].peer_id;
            self.nodes
                .iter()
                .filter(|n| {
                    !n.is_byzantine
                        && n.dos_engine
                            .ban_list()
                            .check_peer(&byz_peer, base_ts)
                            .is_err()
                })
                .count()
        } else {
            0
        };

        Ok(ChaosRunReport {
            scenario: self.config.name.clone(),
            run_index,
            node_count: self.config.node_count,
            honest_nodes_converged,
            finalized_height: self.config.target_heights,
            finalized_block_hash,
            finalized_state_root,
            light_client_verified,
            total_packets_dropped,
            total_catchup_syncs,
            total_eclipse_rejections,
            byzantine_bans_recorded,
            max_observed_latency_ms,
        })
    }
}

/// Build the canonical 20 N8 chaos scenarios across the 100-node stand.
pub fn canonical_20_scenarios() -> Vec<ChaosScenarioConfig> {
    vec![
        // S01: NET_PLAN closure criteria — 100 nodes, 20% packet loss, 1 Byzantine peer.
        ChaosScenarioConfig {
            name: "S01_100nodes_20pct_loss_1_byzantine".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 6,
            base_latency_ms: 25,
            jitter_ms: 40,
            packet_loss_bps: 2_000,
            partition: None,
            eclipsed_nodes: vec![],
            byzantine_node: Some(6),
        },
        // S02: Low latency + 5% loss.
        ChaosScenarioConfig {
            name: "S02_low_latency_5pct_loss".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 5,
            base_latency_ms: 10,
            jitter_ms: 15,
            packet_loss_bps: 500,
            partition: None,
            eclipsed_nodes: vec![],
            byzantine_node: None,
        },
        // S03: High WAN latency + jitter.
        ChaosScenarioConfig {
            name: "S03_wan_latency_jitter".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 5,
            base_latency_ms: 120,
            jitter_ms: 180,
            packet_loss_bps: 1_000,
            partition: None,
            eclipsed_nodes: vec![],
            byzantine_node: None,
        },
        // S04: 30% heavy packet loss.
        ChaosScenarioConfig {
            name: "S04_heavy_30pct_packet_loss".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 5,
            base_latency_ms: 30,
            jitter_ms: 50,
            packet_loss_bps: 3_000,
            partition: None,
            eclipsed_nodes: vec![],
            byzantine_node: None,
        },
        // S05: 70/30 network partition for 3 heights then heal.
        ChaosScenarioConfig {
            name: "S05_partition_70_30_and_heal".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 6,
            base_latency_ms: 20,
            jitter_ms: 20,
            packet_loss_bps: 500,
            partition: Some((30, 3)),
            eclipsed_nodes: vec![],
            byzantine_node: None,
        },
        // S06: 50/50 split partition for 2 heights then heal.
        ChaosScenarioConfig {
            name: "S06_partition_50_50_and_heal".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 6,
            base_latency_ms: 35,
            jitter_ms: 45,
            packet_loss_bps: 500,
            partition: Some((50, 2)),
            eclipsed_nodes: vec![],
            byzantine_node: None,
        },
        // S07: Single-node Sybil eclipse attempt on node 50.
        ChaosScenarioConfig {
            name: "S07_single_node_eclipse_attempt".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 5,
            base_latency_ms: 15,
            jitter_ms: 25,
            packet_loss_bps: 500,
            partition: None,
            eclipsed_nodes: vec![50],
            byzantine_node: None,
        },
        // S08: Multi-node Sybil eclipse attempt on 5 nodes.
        ChaosScenarioConfig {
            name: "S08_multi_node_eclipse_attempt".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 5,
            base_latency_ms: 20,
            jitter_ms: 30,
            packet_loss_bps: 1_000,
            partition: None,
            eclipsed_nodes: vec![10, 25, 50, 75, 90],
            byzantine_node: None,
        },
        // S09: Byzantine equivocator on validator #5.
        ChaosScenarioConfig {
            name: "S09_byzantine_equivocator_validator5".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 5,
            base_latency_ms: 15,
            jitter_ms: 20,
            packet_loss_bps: 500,
            partition: None,
            eclipsed_nodes: vec![],
            byzantine_node: Some(5),
        },
        // S10: Partition + Byzantine equivocator.
        ChaosScenarioConfig {
            name: "S10_partition_plus_byzantine".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 6,
            base_latency_ms: 25,
            jitter_ms: 35,
            packet_loss_bps: 1_000,
            partition: Some((25, 3)),
            eclipsed_nodes: vec![],
            byzantine_node: Some(6),
        },
        // S11: Eclipse + Byzantine equivocator.
        ChaosScenarioConfig {
            name: "S11_eclipse_plus_byzantine".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 5,
            base_latency_ms: 25,
            jitter_ms: 30,
            packet_loss_bps: 1_500,
            partition: None,
            eclipsed_nodes: vec![12, 44, 88],
            byzantine_node: Some(6),
        },
        // S12: Partition + Eclipse + 20% packet loss.
        ChaosScenarioConfig {
            name: "S12_partition_eclipse_20pct_loss".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 6,
            base_latency_ms: 40,
            jitter_ms: 60,
            packet_loss_bps: 2_000,
            partition: Some((20, 2)),
            eclipsed_nodes: vec![15, 60],
            byzantine_node: None,
        },
        // S13: Extreme jitter (0..300ms) + 15% loss.
        ChaosScenarioConfig {
            name: "S13_extreme_jitter_15pct_loss".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 5,
            base_latency_ms: 5,
            jitter_ms: 300,
            packet_loss_bps: 1_500,
            partition: None,
            eclipsed_nodes: vec![],
            byzantine_node: None,
        },
        // S14: Asymmetric 85/15 edge partition for 4 heights.
        ChaosScenarioConfig {
            name: "S14_edge_partition_85_15".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 6,
            base_latency_ms: 20,
            jitter_ms: 25,
            packet_loss_bps: 1_000,
            partition: Some((15, 4)),
            eclipsed_nodes: vec![],
            byzantine_node: None,
        },
        // S15: 25% packet loss + 10 eclipsed nodes.
        ChaosScenarioConfig {
            name: "S15_25pct_loss_10_eclipsed_nodes".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 5,
            base_latency_ms: 30,
            jitter_ms: 40,
            packet_loss_bps: 2_500,
            partition: None,
            eclipsed_nodes: vec![7, 17, 27, 37, 47, 57, 67, 77, 87, 97],
            byzantine_node: None,
        },
        // S16: Full-Duration partition of 40 nodes healed at tail.
        ChaosScenarioConfig {
            name: "S16_deep_partition_40_nodes_tail_heal".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 6,
            base_latency_ms: 20,
            jitter_ms: 20,
            packet_loss_bps: 500,
            partition: Some((40, 5)),
            eclipsed_nodes: vec![],
            byzantine_node: None,
        },
        // S17: Validator-adjacent eclipse attempt (nodes 1, 2, 3).
        ChaosScenarioConfig {
            name: "S17_validator_eclipse_attempt".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 5,
            base_latency_ms: 15,
            jitter_ms: 25,
            packet_loss_bps: 1_000,
            partition: None,
            eclipsed_nodes: vec![1, 2, 3],
            byzantine_node: None,
        },
        // S18: Combined 35% packet loss + Byzantine peer.
        ChaosScenarioConfig {
            name: "S18_35pct_loss_plus_byzantine".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 5,
            base_latency_ms: 45,
            jitter_ms: 55,
            packet_loss_bps: 3_500,
            partition: None,
            eclipsed_nodes: vec![],
            byzantine_node: Some(4),
        },
        // S19: Combined partition + eclipse + Byzantine peer.
        ChaosScenarioConfig {
            name: "S19_partition_eclipse_byzantine_combo".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 6,
            base_latency_ms: 30,
            jitter_ms: 50,
            packet_loss_bps: 2_000,
            partition: Some((30, 3)),
            eclipsed_nodes: vec![20, 40, 80],
            byzantine_node: Some(6),
        },
        // S20: Maximum stress — 40% loss + 35-node partition + 8 eclipsed + 1 Byzantine.
        ChaosScenarioConfig {
            name: "S20_max_stress_40pct_loss_partition_eclipse_byzantine".to_owned(),
            node_count: DEFAULT_STAND_NODES,
            target_heights: 6,
            base_latency_ms: 60,
            jitter_ms: 90,
            packet_loss_bps: 4_000,
            partition: Some((35, 3)),
            eclipsed_nodes: vec![8, 18, 28, 38, 48, 58, 68, 78],
            byzantine_node: Some(6),
        },
    ]
}

/// Run a matrix of `scenarios × runs_per_scenario` and verify zero finalized-head divergence.
pub fn run_chaos_matrix(
    chain_id: &str,
    scenarios: &[ChaosScenarioConfig],
    runs_per_scenario: usize,
) -> Result<ChaosCampaignSummary, P2pError> {
    if scenarios.is_empty() || runs_per_scenario == 0 {
        return Err(P2pError::ChaosInvalidConfig(
            "scenarios and runs_per_scenario must be non-zero".to_owned(),
        ));
    }
    let mut reports = Vec::with_capacity(scenarios.len() * runs_per_scenario);
    let mut divergent_runs = 0usize;

    for scenario in scenarios {
        for run_idx in 0..runs_per_scenario {
            let mut stand = ChaosStand::new(chain_id, scenario.clone())?;
            match stand.execute_run(run_idx) {
                Ok(report) => {
                    if !report.light_client_verified {
                        divergent_runs += 1;
                    }
                    reports.push(report);
                }
                Err(P2pError::ChaosHeadDivergence { .. }) => {
                    divergent_runs += 1;
                }
                Err(err) => return Err(err),
            }
        }
    }

    Ok(ChaosCampaignSummary {
        scenarios_count: scenarios.len(),
        runs_per_scenario,
        total_runs: reports.len() + divergent_runs,
        divergent_runs,
        reports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAIN: &str = "auralite-n8-unit";

    #[test]
    fn hundred_nodes_with_20pct_loss_and_byzantine_peer_converge_and_light_client_verifies() {
        let scenario = canonical_20_scenarios().into_iter().next().unwrap();
        assert_eq!(scenario.node_count, 100);
        assert_eq!(scenario.packet_loss_bps, 2_000);
        assert_eq!(scenario.byzantine_node, Some(6));

        let mut stand = ChaosStand::new(CHAIN, scenario).unwrap();
        let report = stand.execute_run(0).unwrap();
        assert_eq!(report.node_count, 100);
        assert_eq!(report.honest_nodes_converged, 99);
        assert_eq!(report.finalized_height, 6);
        assert!(report.light_client_verified);
        assert!(report.total_packets_dropped > 0);
        assert!(report.total_catchup_syncs > 0);
        assert_eq!(report.byzantine_bans_recorded, 99);
    }

    #[test]
    fn network_partition_and_heal_preserves_safety_and_converges_without_divergence() {
        let scenario = ChaosScenarioConfig {
            name: "unit_partition_heal".to_owned(),
            node_count: 20,
            target_heights: 5,
            base_latency_ms: 10,
            jitter_ms: 10,
            packet_loss_bps: 0,
            partition: Some((8, 5)),
            eclipsed_nodes: vec![],
            byzantine_node: None,
        };
        let mut stand = ChaosStand::new(CHAIN, scenario).unwrap();
        let report = stand.execute_run(0).unwrap();
        assert_eq!(report.honest_nodes_converged, 20);
        assert_eq!(report.finalized_height, 5);
        assert!(
            report.total_catchup_syncs >= 8 * 5,
            "partitioned minority must catch up via verified header+QC sync on heal"
        );
    }

    #[test]
    fn eclipse_attempt_is_defeated_by_qc_verification_and_outbound_diversity() {
        let scenario = ChaosScenarioConfig {
            name: "unit_eclipse_defence".to_owned(),
            node_count: 16,
            target_heights: 4,
            base_latency_ms: 10,
            jitter_ms: 5,
            packet_loss_bps: 0,
            partition: None,
            eclipsed_nodes: vec![4, 8, 12],
            byzantine_node: None,
        };
        let mut stand = ChaosStand::new(CHAIN, scenario).unwrap();
        let report = stand.execute_run(0).unwrap();
        assert_eq!(report.honest_nodes_converged, 16);
        assert!(
            report.total_eclipse_rejections >= 6,
            "eclipsed nodes must reject uncertified eclipse headers"
        );
    }

    #[test]
    fn byzantine_equivocator_is_detected_and_banned_across_honest_nodes() {
        let scenario = ChaosScenarioConfig {
            name: "unit_byzantine_ban".to_owned(),
            node_count: 12,
            target_heights: 4,
            base_latency_ms: 5,
            jitter_ms: 5,
            packet_loss_bps: 0,
            partition: None,
            eclipsed_nodes: vec![],
            byzantine_node: Some(6),
        };
        let mut stand = ChaosStand::new(CHAIN, scenario).unwrap();
        let report = stand.execute_run(0).unwrap();
        assert_eq!(report.honest_nodes_converged, 11);
        assert_eq!(
            report.byzantine_bans_recorded, 11,
            "every honest node must detect the equivocation and ban the Byzantine peer"
        );
    }

    #[test]
    fn head_divergence_detector_flags_any_mismatched_finalized_state_root() {
        let scenario = ChaosScenarioConfig {
            name: "unit_divergence_detector".to_owned(),
            node_count: 8,
            target_heights: 2,
            base_latency_ms: 5,
            jitter_ms: 0,
            packet_loss_bps: 0,
            partition: None,
            eclipsed_nodes: vec![],
            byzantine_node: None,
        };
        let mut stand = ChaosStand::new(CHAIN, scenario).unwrap();
        stand.execute_run(0).unwrap();
        // Corrupt one honest node's finalized_state_root and verify the detector catches it.
        stand.nodes[3].finalized_state_root[0] ^= 0xFF;
        assert_eq!(
            verify_cluster_convergence("unit_divergence_detector", stand.nodes(), 2),
            Err(P2pError::ChaosHeadDivergence {
                scenario: "unit_divergence_detector".to_owned(),
                node_index: 3,
            })
        );
    }

    #[test]
    fn twenty_scenarios_times_three_runs_have_zero_finalized_head_divergence() {
        let mut scenarios = canonical_20_scenarios();
        assert_eq!(scenarios.len(), 20);
        // Use 16-node stands in the fast unit test; the integration test runs the full 100-node stand.
        for s in &mut scenarios {
            s.node_count = 16;
            s.target_heights = 3;
            if let Some((ref mut m, ref mut dur)) = s.partition {
                *m = (*m).min(6);
                *dur = (*dur).min(2);
            }
            s.eclipsed_nodes.retain(|&idx| idx < 16);
        }
        let summary = run_chaos_matrix(CHAIN, &scenarios, 3).unwrap();
        assert_eq!(summary.scenarios_count, 20);
        assert_eq!(summary.runs_per_scenario, 3);
        assert_eq!(summary.total_runs, 60);
        assert_eq!(summary.divergent_runs, 0);
    }
}
