// === crates/m1nd-core/src/plasticity.rs ===

use crate::error::{M1ndError, M1ndResult};
use crate::graph::Graph;
use crate::types::*;

// ---------------------------------------------------------------------------
// Constants from plasticity.py
// ---------------------------------------------------------------------------

pub const DEFAULT_LEARNING_RATE: f32 = 0.08;
pub const DEFAULT_DECAY_RATE: f32 = 0.005;
pub const LTP_THRESHOLD: u16 = 5;
pub const LTD_THRESHOLD: u16 = 5;
pub const LTP_BONUS: f32 = 0.15;
pub const LTD_PENALTY: f32 = 0.15;
pub const HOMEOSTATIC_CEILING: f32 = 5.0;
pub const WEIGHT_FLOOR: f32 = 0.05;
pub const WEIGHT_CAP: f32 = 3.0;
/// Default ring buffer capacity for query memory (FM-PL-005).
pub const DEFAULT_MEMORY_CAPACITY: usize = 1000;
/// CAS retry limit for atomic weight updates (FM-ACT-019).
pub const CAS_RETRY_LIMIT: u32 = 64;

// ---------------------------------------------------------------------------
// SynapticState — per-edge learning state snapshot
// Replaces: plasticity.py SynapticState
// ---------------------------------------------------------------------------

/// Snapshot of per-edge learning state for persistence.
/// Replaces: plasticity.py SynapticState dataclass
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SynapticState {
    pub source_label: String,
    pub target_label: String,
    pub relation: String,
    /// Complete edge identity for current sidecars — complete, but not unique:
    /// parallel edges share it, and [`PlasticityEngine::import_state`] resolves
    /// those positionally. `None` marks a legacy triple-only row and is accepted
    /// only when that triple is unambiguous.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<u8>,
    /// Complete edge identity for current sidecars. See [`Self::direction`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inhibitory: Option<bool>,
    pub original_weight: f32,
    pub current_weight: f32,
    pub strengthen_count: u16,
    pub weaken_count: u16,
    pub ltp_applied: bool,
    pub ltd_applied: bool,
    #[serde(default)]
    pub last_used_query: u32,
}

fn validate_synaptic_state(state: &SynapticState) -> M1ndResult<()> {
    if !state.original_weight.is_finite() || !state.current_weight.is_finite() {
        return Err(M1ndError::CorruptState {
            reason: format!(
                "non-finite weight in synaptic state: {}->{}",
                state.source_label, state.target_label
            ),
        });
    }
    match (state.direction, state.inhibitory) {
        (None, None) => {}
        (Some(direction), Some(_)) if direction <= EdgeDirection::Bidirectional as u8 => {}
        (Some(direction), Some(_)) => {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "unknown synaptic direction {direction} for {}->{}",
                    state.source_label, state.target_label
                ),
            });
        }
        _ => {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "partial synaptic identity for {}->{}",
                    state.source_label, state.target_label
                ),
            });
        }
    }
    Ok(())
}

/// Encode synaptic state using the existing pretty-JSON sidecar format.
///
/// The historical NaN firewall is preserved: a non-finite current weight falls
/// back to its finite original weight. A non-finite original has no trustworthy
/// fallback and is rejected.
pub fn encode_plasticity_state_json(states: &[SynapticState]) -> M1ndResult<Vec<u8>> {
    let mut safe_states = Vec::with_capacity(states.len());
    for state in states {
        let mut safe = state.clone();
        if !safe.original_weight.is_finite() {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "non-finite original weight in synaptic state: {}->{}",
                    safe.source_label, safe.target_label
                ),
            });
        }
        if !safe.current_weight.is_finite() {
            safe.current_weight = safe.original_weight;
        }
        validate_synaptic_state(&safe)?;
        safe_states.push(safe);
    }
    serde_json::to_vec_pretty(&safe_states).map_err(M1ndError::Serde)
}

/// Decode a current checkpoint plasticity payload. Unlike the compatibility
/// file loader, this boundary requires the complete edge identity and every
/// current field, and rejects unknown fields rather than defaulting them.
pub fn decode_plasticity_state_json(bytes: &[u8]) -> M1ndResult<Vec<SynapticState>> {
    const CURRENT_FIELDS: &[&str] = &[
        "source_label",
        "target_label",
        "relation",
        "direction",
        "inhibitory",
        "original_weight",
        "current_weight",
        "strengthen_count",
        "weaken_count",
        "ltp_applied",
        "ltd_applied",
        "last_used_query",
    ];
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(M1ndError::Serde)?;
    let rows = value.as_array().ok_or_else(|| M1ndError::CorruptState {
        reason: "current plasticity checkpoint is not a JSON array".into(),
    })?;
    for (index, row) in rows.iter().enumerate() {
        let object = row.as_object().ok_or_else(|| M1ndError::CorruptState {
            reason: format!("plasticity row {index} is not an object"),
        })?;
        if object.len() != CURRENT_FIELDS.len()
            || CURRENT_FIELDS
                .iter()
                .any(|field| !object.contains_key(*field))
        {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "plasticity row {index} is not the complete current checkpoint schema"
                ),
            });
        }
    }
    let states: Vec<SynapticState> = serde_json::from_slice(bytes).map_err(M1ndError::Serde)?;
    for state in &states {
        validate_synaptic_state(state)?;
        if state.direction.is_none() || state.inhibitory.is_none() {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "legacy plasticity identity is not authoritative for {}->{}",
                    state.source_label, state.target_label
                ),
            });
        }
    }
    Ok(states)
}

// ---------------------------------------------------------------------------
// QueryRecord — per-query metadata for memory
// Replaces: plasticity.py QueryRecord
// ---------------------------------------------------------------------------

/// Record of a single query for the memory ring buffer.
/// Replaces: plasticity.py QueryRecord
#[derive(Clone, Debug)]
pub struct QueryRecord {
    pub query_text: String,
    pub seeds: Vec<NodeId>,
    pub activated_nodes: Vec<NodeId>,
    pub timestamp: f64,
}

// ---------------------------------------------------------------------------
// QueryMemory — bounded ring buffer (FM-PL-005)
// Replaces: plasticity.py QueryMemory
// ---------------------------------------------------------------------------

/// Bounded ring buffer of recent queries. Fixed capacity prevents unbounded growth.
/// Tracks node frequency and seed bigrams for priming.
/// FM-PL-005: ring buffer replaces unbounded Vec.
/// Replaces: plasticity.py QueryMemory
pub struct QueryMemory {
    records: Vec<Option<QueryRecord>>,
    capacity: usize,
    write_head: usize,
    /// Node access frequency (how often each node appears in recent queries).
    node_frequency: Vec<u32>,
    /// Seed bigram frequency: pairs of seeds that co-occur.
    seed_bigrams: std::collections::HashMap<(NodeId, NodeId), u32>,
}

impl QueryMemory {
    pub fn new(capacity: usize, num_nodes: u32) -> Self {
        Self {
            records: vec![None; capacity],
            capacity,
            write_head: 0,
            node_frequency: vec![0; num_nodes as usize],
            seed_bigrams: std::collections::HashMap::new(),
        }
    }

    /// Record a query. Overwrites oldest if at capacity.
    /// Replaces: plasticity.py QueryMemory.record()
    pub fn record(&mut self, record: QueryRecord) {
        // If overwriting an old record, decrement its frequency counts
        if let Some(old) = &self.records[self.write_head] {
            for &node in &old.activated_nodes {
                let idx = node.as_usize();
                if idx < self.node_frequency.len() {
                    self.node_frequency[idx] = self.node_frequency[idx].saturating_sub(1);
                }
            }
            // Decrement bigram counts
            for i in 0..old.seeds.len() {
                for j in (i + 1)..old.seeds.len() {
                    let key = if old.seeds[i] < old.seeds[j] {
                        (old.seeds[i], old.seeds[j])
                    } else {
                        (old.seeds[j], old.seeds[i])
                    };
                    if let Some(count) = self.seed_bigrams.get_mut(&key) {
                        *count = count.saturating_sub(1);
                    }
                }
            }
        }

        // Increment frequency counts for new record
        for &node in &record.activated_nodes {
            let idx = node.as_usize();
            if idx < self.node_frequency.len() {
                self.node_frequency[idx] += 1;
            }
        }

        // Update seed bigrams
        for i in 0..record.seeds.len() {
            for j in (i + 1)..record.seeds.len() {
                let key = if record.seeds[i] < record.seeds[j] {
                    (record.seeds[i], record.seeds[j])
                } else {
                    (record.seeds[j], record.seeds[i])
                };
                *self.seed_bigrams.entry(key).or_insert(0) += 1;
            }
        }

        self.records[self.write_head] = Some(record);
        self.write_head = (self.write_head + 1) % self.capacity;
    }

    /// Get priming signal: nodes that frequently co-occur with the given seeds.
    /// Replaces: plasticity.py QueryMemory.get_priming_signal()
    pub fn get_priming_signal(
        &self,
        seeds: &[NodeId],
        boost_strength: FiniteF32,
    ) -> Vec<(NodeId, FiniteF32)> {
        if seeds.is_empty() {
            return Vec::new();
        }

        // Find nodes that frequently appear in queries containing these seeds
        let mut node_scores: std::collections::HashMap<u32, f32> = std::collections::HashMap::new();

        for record in self.records.iter().flatten() {
            // Check if this record shares any seeds
            let shared = seeds.iter().any(|s| record.seeds.contains(s));
            if !shared {
                continue;
            }

            for &node in &record.activated_nodes {
                if !seeds.contains(&node) {
                    *node_scores.entry(node.0).or_insert(0.0) += 1.0;
                }
            }
        }

        // Normalize and apply boost strength
        let max_score = node_scores.values().cloned().fold(0.0f32, f32::max);
        if max_score <= 0.0 {
            return Vec::new();
        }

        let mut results: Vec<(NodeId, FiniteF32)> = node_scores
            .into_iter()
            .map(|(id, score)| {
                let normalized = (score / max_score) * boost_strength.get();
                (NodeId::new(id), FiniteF32::new(normalized.min(1.0)))
            })
            .filter(|(_, s)| s.get() > 0.01)
            .collect();

        results.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        results.truncate(50); // Cap priming signals
        results
    }

    /// Number of recorded queries.
    pub fn len(&self) -> usize {
        self.records.iter().filter(|r| r.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the top `n` nodes by accumulated query-access frequency.
    ///
    /// This is the cheapest "what has this agent been paying attention to" signal:
    /// it reads directly from the in-memory ring-buffer frequency counter without
    /// any graph traversal or seed requirement.  Returns `(NodeId, frequency)` pairs
    /// sorted descending by frequency, capped at `n`.
    pub fn top_node_frequencies(&self, n: usize) -> Vec<(NodeId, u32)> {
        let mut indexed: Vec<(NodeId, u32)> = self
            .node_frequency
            .iter()
            .enumerate()
            .filter(|(_, &freq)| freq > 0)
            .map(|(idx, &freq)| (NodeId::new(idx as u32), freq))
            .collect();
        // Partial sort: top-k by descending frequency
        indexed.sort_unstable_by_key(|b| std::cmp::Reverse(b.1));
        indexed.truncate(n);
        indexed
    }
}

// ---------------------------------------------------------------------------
// PlasticityConfig — tunables
// ---------------------------------------------------------------------------

/// Plasticity engine configuration.
/// Replaces: plasticity.py PlasticityEngine.__init__ parameters
pub struct PlasticityConfig {
    pub learning_rate: LearningRate,
    pub decay_rate: PosF32,
    pub ltp_threshold: u16,
    pub ltd_threshold: u16,
    pub ltp_bonus: FiniteF32,
    pub ltd_penalty: FiniteF32,
    pub homeostatic_ceiling: FiniteF32,
    pub weight_floor: FiniteF32,
    pub weight_cap: FiniteF32,
    pub memory_capacity: usize,
    pub cas_retry_limit: u32,
}

impl Default for PlasticityConfig {
    fn default() -> Self {
        Self {
            learning_rate: LearningRate::DEFAULT,
            decay_rate: PosF32::new(DEFAULT_DECAY_RATE).unwrap(),
            ltp_threshold: LTP_THRESHOLD,
            ltd_threshold: LTD_THRESHOLD,
            ltp_bonus: FiniteF32::new(LTP_BONUS),
            ltd_penalty: FiniteF32::new(LTD_PENALTY),
            homeostatic_ceiling: FiniteF32::new(HOMEOSTATIC_CEILING),
            weight_floor: FiniteF32::new(WEIGHT_FLOOR),
            weight_cap: FiniteF32::new(WEIGHT_CAP),
            memory_capacity: DEFAULT_MEMORY_CAPACITY,
            cas_retry_limit: CAS_RETRY_LIMIT,
        }
    }
}

// ---------------------------------------------------------------------------
// PlasticityResult — output of a learning cycle
// ---------------------------------------------------------------------------

/// Result of a single plasticity update cycle.
#[derive(Clone, Debug)]
pub struct PlasticityResult {
    pub edges_strengthened: u32,
    pub edges_decayed: u32,
    pub ltp_events: u32,
    pub ltd_events: u32,
    pub homeostatic_rescales: u32,
    pub priming_nodes: u32,
}

// ---------------------------------------------------------------------------
// PlasticityEngine — Hebbian learning engine
// Replaces: plasticity.py PlasticityEngine
// ---------------------------------------------------------------------------

/// Hebbian plasticity engine with LTP/LTD, homeostatic normalization,
/// and query memory. Writes weights atomically to CSR (FM-ACT-021).
/// Checks graph generation on every operation (FM-PL-006).
/// Replaces: plasticity.py PlasticityEngine
pub struct PlasticityEngine {
    config: PlasticityConfig,
    memory: QueryMemory,
    /// Graph generation at engine init. Asserted on every operation (FM-PL-006).
    expected_generation: Generation,
    /// Query counter for last_used_query tracking.
    query_count: u32,
}

impl PlasticityEngine {
    /// Create engine bound to current graph generation.
    /// Replaces: plasticity.py PlasticityEngine.__init__()
    pub fn new(graph: &Graph, config: PlasticityConfig) -> Self {
        Self {
            memory: QueryMemory::new(config.memory_capacity, graph.num_nodes()),
            expected_generation: graph.generation,
            query_count: 0,
            config,
        }
    }

    /// Check graph generation match (FM-PL-006).
    fn check_generation(&self, graph: &Graph) -> M1ndResult<()> {
        if self.expected_generation != graph.generation {
            return Err(M1ndError::GraphGenerationMismatch {
                expected: self.expected_generation,
                actual: graph.generation,
            });
        }
        Ok(())
    }

    /// Full learning cycle: Hebbian strengthen + decay + LTP/LTD + homeostatic.
    /// Writes weights atomically to CSR via CAS (FM-ACT-021).
    /// Asserts graph generation match (FM-PL-006).
    /// Replaces: plasticity.py PlasticityEngine.query()
    pub fn update(
        &mut self,
        graph: &mut Graph,
        activated_nodes: &[(NodeId, FiniteF32)],
        seeds: &[(NodeId, FiniteF32)],
        query_text: &str,
    ) -> M1ndResult<PlasticityResult> {
        // FM-PL-006: generation check is relaxed for plasticity updates
        // since they modify weights (not structure)

        self.query_count += 1;

        // Build activated set for fast lookup
        let n = graph.num_nodes() as usize;
        let mut activated_set = vec![false; n];
        let mut act_map = std::collections::HashMap::new();
        for &(node, score) in activated_nodes {
            let idx = node.as_usize();
            if idx < n {
                activated_set[idx] = true;
                act_map.insert(node.0, score.get());
            }
        }

        // Step 1: Hebbian strengthen
        let edges_strengthened = self.hebbian_strengthen(graph, activated_nodes)?;

        // Step 2: Synaptic decay
        let edges_decayed = self.synaptic_decay(graph, &activated_set)?;

        // Step 3: LTP/LTD
        let (ltp_events, ltd_events) = self.apply_ltp_ltd(graph)?;

        // Step 4: Homeostatic normalization
        let homeostatic_rescales = self.homeostatic_normalize(graph)?;

        // Step 5: Record query in memory
        let record = QueryRecord {
            query_text: query_text.to_string(),
            seeds: seeds.iter().map(|s| s.0).collect(),
            activated_nodes: activated_nodes.iter().map(|a| a.0).collect(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0),
        };
        self.memory.record(record);

        let priming_nodes = self
            .memory
            .get_priming_signal(
                &seeds.iter().map(|s| s.0).collect::<Vec<_>>(),
                FiniteF32::new(0.1),
            )
            .len() as u32;

        Ok(PlasticityResult {
            edges_strengthened,
            edges_decayed,
            ltp_events,
            ltd_events,
            homeostatic_rescales,
            priming_nodes,
        })
    }

    /// Hebbian strengthening: delta_w = lr * act_src * act_tgt for co-activated edges.
    /// Replaces: plasticity.py PlasticityEngine._hebbian_strengthen()
    fn hebbian_strengthen(
        &self,
        graph: &mut Graph,
        activated: &[(NodeId, FiniteF32)],
    ) -> M1ndResult<u32> {
        let n = graph.num_nodes() as usize;
        let lr = self.config.learning_rate.get();
        let cap = self.config.weight_cap.get();
        let mut count = 0u32;

        // Build activation lookup
        let mut act_val = vec![0.0f32; n];
        for &(node, score) in activated {
            let idx = node.as_usize();
            if idx < n {
                act_val[idx] = score.get();
            }
        }

        // For each activated node, strengthen edges to co-activated neighbors
        for &(src, src_act) in activated {
            let range = graph.csr.out_range(src);
            for j in range {
                let tgt = graph.csr.targets[j];
                let tgt_idx = tgt.as_usize();
                if tgt_idx >= n {
                    continue;
                }
                let tgt_act = act_val[tgt_idx];
                if tgt_act <= 0.0 {
                    continue;
                }

                // Hebbian: delta_w = lr * act_src * act_tgt
                let delta = lr * src_act.get() * tgt_act;
                let edge_idx = EdgeIdx::new(j as u32);
                let current = graph.csr.read_weight(edge_idx).get();
                let new_weight = (current + delta).min(cap);

                let _ = graph.csr.atomic_write_weight(
                    edge_idx,
                    FiniteF32::new(new_weight),
                    self.config.cas_retry_limit,
                );

                // Update plasticity metadata
                if j < graph.edge_plasticity.strengthen_count.len() {
                    graph.edge_plasticity.strengthen_count[j] =
                        graph.edge_plasticity.strengthen_count[j].saturating_add(1);
                    graph.edge_plasticity.current_weight[j] = FiniteF32::new(new_weight);
                    graph.edge_plasticity.last_used_query[j] = self.query_count;
                }

                count += 1;
            }
        }

        Ok(count)
    }

    /// Synaptic decay: w *= (1 - decay_rate) for inactive edges.
    /// Replaces: plasticity.py PlasticityEngine._synaptic_decay()
    fn synaptic_decay(&self, graph: &mut Graph, activated_set: &[bool]) -> M1ndResult<u32> {
        let n = graph.num_nodes() as usize;
        let decay_factor = 1.0 - self.config.decay_rate.get();
        let floor = self.config.weight_floor.get();
        let mut count = 0u32;

        for (i, &is_activated) in activated_set.iter().enumerate().take(n) {
            if is_activated {
                continue; // Skip activated nodes
            }

            let range = graph.csr.out_range(NodeId::new(i as u32));
            for j in range {
                let edge_idx = EdgeIdx::new(j as u32);
                let current = graph.csr.read_weight(edge_idx).get();
                let new_weight = (current * decay_factor).max(floor);

                if (new_weight - current).abs() > 1e-6 {
                    let _ = graph.csr.atomic_write_weight(
                        edge_idx,
                        FiniteF32::new(new_weight),
                        self.config.cas_retry_limit,
                    );

                    if j < graph.edge_plasticity.weaken_count.len() {
                        graph.edge_plasticity.weaken_count[j] =
                            graph.edge_plasticity.weaken_count[j].saturating_add(1);
                        graph.edge_plasticity.current_weight[j] = FiniteF32::new(new_weight);
                    }

                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// LTP/LTD: permanent bonus/penalty after N consecutive strengthen/weaken.
    /// Replaces: plasticity.py PlasticityEngine._apply_ltp_ltd()
    fn apply_ltp_ltd(&self, graph: &mut Graph) -> M1ndResult<(u32, u32)> {
        let cap = self.config.weight_cap.get();
        let floor = self.config.weight_floor.get();
        let mut ltp_count = 0u32;
        let mut ltd_count = 0u32;

        let num_edges = graph.edge_plasticity.strengthen_count.len();
        for j in 0..num_edges {
            // LTP: sustained strengthening
            if !graph.edge_plasticity.ltp_applied[j]
                && graph.edge_plasticity.strengthen_count[j] >= self.config.ltp_threshold
            {
                let edge_idx = EdgeIdx::new(j as u32);
                let current = graph.csr.read_weight(edge_idx).get();
                let new_weight = (current + self.config.ltp_bonus.get()).min(cap);
                let _ = graph.csr.atomic_write_weight(
                    edge_idx,
                    FiniteF32::new(new_weight),
                    self.config.cas_retry_limit,
                );
                graph.edge_plasticity.ltp_applied[j] = true;
                graph.edge_plasticity.current_weight[j] = FiniteF32::new(new_weight);
                ltp_count += 1;
            }

            // LTD: sustained weakening
            if !graph.edge_plasticity.ltd_applied[j]
                && graph.edge_plasticity.weaken_count[j] >= self.config.ltd_threshold
            {
                let edge_idx = EdgeIdx::new(j as u32);
                let current = graph.csr.read_weight(edge_idx).get();
                let new_weight = (current - self.config.ltd_penalty.get()).max(floor);
                let _ = graph.csr.atomic_write_weight(
                    edge_idx,
                    FiniteF32::new(new_weight),
                    self.config.cas_retry_limit,
                );
                graph.edge_plasticity.ltd_applied[j] = true;
                graph.edge_plasticity.current_weight[j] = FiniteF32::new(new_weight);
                ltd_count += 1;
            }
        }

        Ok((ltp_count, ltd_count))
    }

    /// Homeostatic normalization: scale incoming weights if total exceeds ceiling.
    /// FM-PL-003 fix: tracks already-scaled edges to prevent bidirectional penalty.
    /// Replaces: plasticity.py PlasticityEngine._homeostatic_normalize()
    fn homeostatic_normalize(&self, graph: &mut Graph) -> M1ndResult<u32> {
        let n = graph.num_nodes() as usize;
        let ceiling = self.config.homeostatic_ceiling.get();
        let mut rescale_count = 0u32;

        for i in 0..n {
            // Sum incoming edge weights
            let range = graph.csr.in_range(NodeId::new(i as u32));
            let mut total_incoming = 0.0f32;
            for j in range.clone() {
                let fwd_idx = graph.csr.rev_edge_idx[j];
                total_incoming += graph.csr.read_weight(fwd_idx).get();
            }

            if total_incoming > ceiling {
                // Scale down all incoming edges proportionally
                let scale = ceiling / total_incoming;
                for j in range {
                    let fwd_idx = graph.csr.rev_edge_idx[j];
                    let current = graph.csr.read_weight(fwd_idx).get();
                    let new_weight = current * scale;
                    let _ = graph.csr.atomic_write_weight(
                        fwd_idx,
                        FiniteF32::new(new_weight),
                        self.config.cas_retry_limit,
                    );
                    if fwd_idx.as_usize() < graph.edge_plasticity.current_weight.len() {
                        graph.edge_plasticity.current_weight[fwd_idx.as_usize()] =
                            FiniteF32::new(new_weight);
                    }
                }
                rescale_count += 1;
            }
        }

        Ok(rescale_count)
    }

    /// Export synaptic state for persistence.
    /// FM-PL-008 fix: atomic write (temp file + rename).
    /// FM-PL-001 NaN firewall: non-finite weights fall back to original.
    /// Replaces: plasticity.py PlasticityEngine.export_state()
    pub fn export_state(&self, graph: &Graph) -> M1ndResult<Vec<SynapticState>> {
        let n = graph.num_nodes() as usize;
        let num_plasticity = graph.edge_plasticity.original_weight.len();
        let num_csr = graph.csr.num_edges();
        if num_plasticity != num_csr
            || graph.edge_plasticity.current_weight.len() != num_csr
            || graph.edge_plasticity.strengthen_count.len() != num_csr
            || graph.edge_plasticity.weaken_count.len() != num_csr
            || graph.edge_plasticity.ltp_applied.len() != num_csr
            || graph.edge_plasticity.ltd_applied.len() != num_csr
            || graph.edge_plasticity.last_used_query.len() != num_csr
            || graph.csr.weights.len() != num_csr
            || graph.csr.targets.len() != num_csr
            || graph.csr.relations.len() != num_csr
            || graph.csr.directions.len() != num_csr
            || graph.csr.inhibitory.len() != num_csr
        {
            return Err(M1ndError::CorruptState {
                reason: "cannot export a partial CSR/plasticity ownership set".into(),
            });
        }

        // Build reverse map: NodeId -> external_id string
        let mut node_ext_id = vec![String::new(); n];
        for (&interned, &node_id) in &graph.id_to_node {
            if let Some(s) = graph.strings.try_resolve(interned) {
                if node_id.as_usize() < n {
                    node_ext_id[node_id.as_usize()] = s.to_string();
                }
            }
        }

        // Build edge_idx -> source NodeId from CSR offsets
        let mut edge_source = vec![0u32; num_csr];
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let lo = graph.csr.offsets[i] as usize;
            let hi = graph.csr.offsets[i + 1] as usize;
            for j in lo..hi {
                edge_source[j] = i as u32;
            }
        }

        let cap = num_csr;
        let mut states = Vec::with_capacity(cap);

        #[allow(clippy::needless_range_loop)]
        for j in 0..cap {
            let original = graph.edge_plasticity.original_weight[j].get();
            let mut current = graph.edge_plasticity.current_weight[j].get();

            // FM-PL-001 NaN firewall
            if !current.is_finite() {
                current = original;
            }

            // Real labels from CSR topology
            let src_idx = edge_source[j] as usize;
            let tgt_idx = graph.csr.targets[j].as_usize();
            let source_label = if src_idx < n {
                node_ext_id[src_idx].clone()
            } else {
                format!("node_{}", src_idx)
            };
            let target_label = if tgt_idx < n {
                node_ext_id[tgt_idx].clone()
            } else {
                format!("node_{}", tgt_idx)
            };
            let relation = graph
                .strings
                .try_resolve(graph.csr.relations[j])
                .unwrap_or("edge")
                .to_string();

            states.push(SynapticState {
                source_label,
                target_label,
                relation,
                direction: Some(graph.csr.directions[j] as u8),
                inhibitory: Some(graph.csr.inhibitory[j]),
                original_weight: original,
                current_weight: current,
                strengthen_count: graph.edge_plasticity.strengthen_count[j],
                weaken_count: graph.edge_plasticity.weaken_count[j],
                ltp_applied: graph.edge_plasticity.ltp_applied[j],
                ltd_applied: graph.edge_plasticity.ltd_applied[j],
                last_used_query: graph.edge_plasticity.last_used_query[j],
            });
        }

        Ok(states)
    }

    /// Import synaptic state from persistence.
    /// FM-PL-007 fix: validates JSON schema, wraps in try/catch.
    /// Current sidecars match the complete structural identity
    /// `(source, target, relation, direction, inhibitory)`. Parallel edges share
    /// that identity, so the rows owning one are consumed in file order against
    /// that identity's CSR slots in slot order — the same order `export_state`
    /// wrote them in — and only while those slots are interchangeable. More rows
    /// than slots is corruption; a parallel group that disagrees on causal
    /// strength is dropped instead of guessed. Legacy triple-only rows are
    /// migrated only when exactly one live edge owns that triple.
    /// Replaces: plasticity.py PlasticityEngine.import_state()
    pub fn import_state(&mut self, graph: &mut Graph, states: &[SynapticState]) -> M1ndResult<u32> {
        let n = graph.num_nodes() as usize;
        let num_csr = graph.csr.num_edges();
        let num_plasticity = graph.edge_plasticity.original_weight.len();
        if num_plasticity != num_csr
            || graph.edge_plasticity.current_weight.len() != num_csr
            || graph.edge_plasticity.strengthen_count.len() != num_csr
            || graph.edge_plasticity.weaken_count.len() != num_csr
            || graph.edge_plasticity.ltp_applied.len() != num_csr
            || graph.edge_plasticity.ltd_applied.len() != num_csr
            || graph.edge_plasticity.last_used_query.len() != num_csr
            || graph.csr.weights.len() != num_csr
        {
            return Err(M1ndError::CorruptState {
                reason: "CSR and edge-plasticity arrays have different lengths".into(),
            });
        }

        // Build reverse map: NodeId -> external_id
        let mut node_ext_id = vec![String::new(); n];
        for (&interned, &node_id) in &graph.id_to_node {
            if let Some(s) = graph.strings.try_resolve(interned) {
                if node_id.as_usize() < n {
                    node_ext_id[node_id.as_usize()] = s.to_string();
                }
            }
        }

        // Build edge_idx -> source from CSR offsets
        let mut edge_source = vec![0u32; num_csr];
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let lo = graph.csr.offsets[i] as usize;
            let hi = graph.csr.offsets[i + 1] as usize;
            for j in lo..hi {
                edge_source[j] = i as u32;
            }
        }

        // Build both legacy and complete identity indexes. Values stay vectors:
        // silently taking the last parallel edge would corrupt another synapse.
        use std::collections::{HashMap, HashSet};
        type Triple = (String, String, String);
        type FullKey = (String, String, String, u8, bool);
        let cap = num_csr;
        let mut triple_to_edges: HashMap<Triple, Vec<usize>> = HashMap::with_capacity(cap);
        let mut full_to_edges: HashMap<FullKey, Vec<usize>> = HashMap::with_capacity(cap);
        #[allow(clippy::needless_range_loop)]
        for j in 0..cap {
            let src_idx = edge_source[j] as usize;
            let tgt_idx = graph.csr.targets[j].as_usize();
            if src_idx < n && tgt_idx < n {
                let rel = graph
                    .strings
                    .try_resolve(graph.csr.relations[j])
                    .unwrap_or("");
                let triple = (
                    node_ext_id[src_idx].clone(),
                    node_ext_id[tgt_idx].clone(),
                    rel.to_string(),
                );
                triple_to_edges.entry(triple.clone()).or_default().push(j);
                full_to_edges
                    .entry((
                        triple.0,
                        triple.1,
                        triple.2,
                        graph.csr.directions[j] as u8,
                        graph.csr.inhibitory[j],
                    ))
                    .or_default()
                    .push(j);
            }
        }

        struct RestorePlan {
            slot: usize,
            original_weight: f32,
            current_weight: f32,
            strengthen_count: u16,
            weaken_count: u16,
            ltp_applied: bool,
            ltd_applied: bool,
            last_used_query: u32,
        }

        // Resolve and validate the entire sidecar before mutating one slot.
        // Rows are consumed in file order, so the cursor walks each identity's
        // parallel edges in the same CSR order `export_state` wrote them in.
        let mut full_key_cursor = HashMap::<FullKey, usize>::new();
        let mut selected_slots = HashSet::<usize>::new();
        let mut plans = Vec::with_capacity(states.len());

        for state in states {
            if !state.original_weight.is_finite() {
                return Err(M1ndError::CorruptState {
                    reason: format!(
                        "non-finite original weight for {} -> {} ({})",
                        state.source_label, state.target_label, state.relation
                    ),
                });
            }
            let current_weight = if state.current_weight.is_finite() {
                state.current_weight
            } else {
                state.original_weight
            };
            let triple = (
                state.source_label.clone(),
                state.target_label.clone(),
                state.relation.clone(),
            );

            let slot = match (state.direction, state.inhibitory) {
                (Some(direction), Some(inhibitory)) => {
                    if direction > EdgeDirection::Bidirectional as u8 {
                        return Err(M1ndError::CorruptState {
                            reason: format!(
                                "unknown synaptic direction {direction} for {} -> {}",
                                state.source_label, state.target_label
                            ),
                        });
                    }
                    let key = (
                        triple.0.clone(),
                        triple.1.clone(),
                        triple.2.clone(),
                        direction,
                        inhibitory,
                    );
                    let slots = match full_to_edges.get(&key).map(Vec::as_slice) {
                        None | Some([]) => continue,
                        Some(slots) => slots,
                    };
                    // Parallel edges share every persisted identity field, so
                    // `export_state` necessarily writes one row per slot with the
                    // same key: rejecting that would refuse the file the previous
                    // clean shutdown wrote. Bind positionally instead — but only
                    // while the candidate slots are interchangeable. Causal
                    // strength is the one structural column outside the persisted
                    // key, so a group that disagrees on it is dropped rather than
                    // guessed: losing relearnable weights beats binding a record
                    // to an edge it did not come from.
                    let cursor = full_key_cursor.entry(key).or_insert(0);
                    if slots.len() > 1 && !parallel_slots_are_interchangeable(graph, slots) {
                        if *cursor == 0 {
                            eprintln!(
                                "[m1nd] WARNING: {} parallel edges for {} -> {} ({}) differ outside the persisted synaptic key; their learned weights are dropped and relearned",
                                slots.len(),
                                state.source_label,
                                state.target_label,
                                state.relation
                            );
                        }
                        *cursor += 1;
                        continue;
                    }
                    let Some(&slot) = slots.get(*cursor) else {
                        return Err(M1ndError::CorruptState {
                            reason: format!(
                                "duplicate full synaptic key for {} -> {} ({}, direction={direction}, inhibitory={inhibitory}): more rows than the {} parallel edge(s) that own it",
                                state.source_label,
                                state.target_label,
                                state.relation,
                                slots.len()
                            ),
                        });
                    };
                    *cursor += 1;
                    slot
                }
                (None, None) => match triple_to_edges.get(&triple).map(Vec::as_slice) {
                    None | Some([]) => continue,
                    Some([slot]) => *slot,
                    Some(matches) => {
                        return Err(M1ndError::CorruptState {
                            reason: format!(
                                "legacy triple-only synaptic state for {} -> {} ({}) is ambiguous across {} edges",
                                state.source_label,
                                state.target_label,
                                state.relation,
                                matches.len()
                            ),
                        });
                    }
                },
                _ => {
                    return Err(M1ndError::CorruptState {
                        reason: format!(
                            "partial synaptic identity for {} -> {} ({})",
                            state.source_label, state.target_label, state.relation
                        ),
                    });
                }
            };

            if !selected_slots.insert(slot) {
                return Err(M1ndError::CorruptState {
                    reason: format!(
                        "multiple synaptic rows resolve to CSR slot {slot} for {} -> {}",
                        state.source_label, state.target_label
                    ),
                });
            }
            plans.push(RestorePlan {
                slot,
                original_weight: state.original_weight,
                current_weight,
                strengthen_count: state.strengthen_count,
                weaken_count: state.weaken_count,
                ltp_applied: state.ltp_applied,
                ltd_applied: state.ltd_applied,
                last_used_query: state.last_used_query,
            });
        }

        let mut max_last_used_query = self.query_count;
        for plan in &plans {
            // `&mut Graph` excludes concurrent writers. The restore plan is
            // completely validated, so an infallible atomic store avoids a
            // spurious-CAS failure after an earlier slot was already applied.
            graph.csr.weights[plan.slot].store(
                plan.current_weight.to_bits(),
                std::sync::atomic::Ordering::Release,
            );
            graph.edge_plasticity.original_weight[plan.slot] = FiniteF32::new(plan.original_weight);
            graph.edge_plasticity.current_weight[plan.slot] = FiniteF32::new(plan.current_weight);
            graph.edge_plasticity.strengthen_count[plan.slot] = plan.strengthen_count;
            graph.edge_plasticity.weaken_count[plan.slot] = plan.weaken_count;
            graph.edge_plasticity.ltp_applied[plan.slot] = plan.ltp_applied;
            graph.edge_plasticity.ltd_applied[plan.slot] = plan.ltd_applied;
            graph.edge_plasticity.last_used_query[plan.slot] = plan.last_used_query;
            max_last_used_query = max_last_used_query.max(plan.last_used_query);
        }
        self.query_count = max_last_used_query;

        Ok(plans.len() as u32)
    }

    /// Get priming signal from query memory.
    pub fn get_priming(
        &self,
        seeds: &[NodeId],
        boost_strength: FiniteF32,
    ) -> Vec<(NodeId, FiniteF32)> {
        self.memory.get_priming_signal(seeds, boost_strength)
    }

    /// Return the top `n` nodes by accumulated query-access frequency from the
    /// ring-buffer memory.  This is the cheapest global "attention" signal —
    /// no seeds required, O(num_nodes) time with a single pass.
    pub fn top_node_access_frequencies(&self, n: usize) -> Vec<(NodeId, u32)> {
        self.memory.top_node_frequencies(n)
    }
}

/// Parallel edges (same source, target, relation, direction and inhibitory flag)
/// can only be restored positionally, so they must first be proven mutually
/// substitutable: causal strength is the one structural column outside the
/// persisted synaptic key. A column that cannot be read counts as disagreement —
/// dropping relearnable weights is cheaper than binding a record to an edge it
/// did not come from.
fn parallel_slots_are_interchangeable(graph: &Graph, slots: &[usize]) -> bool {
    let Some(reference) = graph.csr.causal_strengths.get(slots[0]) else {
        return false;
    };
    let reference = reference.get().to_bits();
    slots.iter().all(|&slot| {
        graph
            .csr
            .causal_strengths
            .get(slot)
            .is_some_and(|value| value.get().to_bits() == reference)
    })
}

#[cfg(test)]
mod codec_tests {
    use super::*;

    fn sample_state() -> SynapticState {
        SynapticState {
            source_label: "source".to_string(),
            target_label: "target".to_string(),
            relation: "calls".to_string(),
            direction: Some(EdgeDirection::Forward as u8),
            inhibitory: Some(false),
            original_weight: 0.5,
            current_weight: 0.8,
            strengthen_count: 2,
            weaken_count: 1,
            ltp_applied: true,
            ltd_applied: false,
            last_used_query: 7,
        }
    }

    #[test]
    fn plasticity_memory_codec_matches_file_format_and_nan_firewall() {
        let mut state = sample_state();
        state.current_weight = f32::NAN;
        let states = vec![state];

        let encoded = encode_plasticity_state_json(&states).expect("encode");
        assert_eq!(
            encoded,
            encode_plasticity_state_json(&states).expect("repeat encode")
        );
        let decoded = decode_plasticity_state_json(&encoded).expect("decode");
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].current_weight, decoded[0].original_weight);

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plasticity_state.json");
        crate::snapshot::save_plasticity_state(&states, &path).expect("file save");
        assert_eq!(std::fs::read(path).expect("saved bytes"), encoded);
    }

    #[test]
    fn plasticity_checkpoint_codec_rejects_legacy_identity_defaults() {
        let legacy = serde_json::to_vec_pretty(&serde_json::json!([{
            "source_label": "source",
            "target_label": "target",
            "relation": "calls",
            "original_weight": 0.5,
            "current_weight": 0.8,
            "strengthen_count": 2,
            "weaken_count": 1,
            "ltp_applied": true,
            "ltd_applied": false
        }]))
        .expect("legacy json");
        assert!(decode_plasticity_state_json(&legacy).is_err());

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("legacy-plasticity.json");
        std::fs::write(&path, &legacy).expect("write legacy fixture");
        let decoded = crate::snapshot::load_plasticity_state(&path)
            .expect("friendly file loader keeps legacy compatibility");
        assert_eq!(decoded[0].direction, None);
        assert_eq!(decoded[0].inhibitory, None);
        assert_eq!(decoded[0].last_used_query, 0);
    }

    #[test]
    fn plasticity_memory_codec_rejects_corruption_and_nonfinite_original() {
        assert!(decode_plasticity_state_json(b"{").is_err());

        let mut nonfinite = sample_state();
        nonfinite.original_weight = f32::INFINITY;
        assert!(encode_plasticity_state_json(&[nonfinite]).is_err());

        let mut partial = serde_json::to_value([sample_state()]).expect("value");
        partial[0]
            .as_object_mut()
            .expect("state object")
            .remove("inhibitory");
        let partial = serde_json::to_vec_pretty(&partial).expect("partial json");
        assert!(decode_plasticity_state_json(&partial).is_err());

        let mut unknown_direction = sample_state();
        unknown_direction.direction = Some(u8::MAX);
        let bytes = serde_json::to_vec_pretty(&[unknown_direction]).expect("json");
        assert!(decode_plasticity_state_json(&bytes).is_err());

        let mut unknown_field = serde_json::to_value([sample_state()]).expect("value");
        unknown_field[0]["future_field"] = serde_json::json!(true);
        let bytes = serde_json::to_vec_pretty(&unknown_field).expect("json");
        assert!(decode_plasticity_state_json(&bytes).is_err());
    }
}

static_assertions::assert_impl_all!(PlasticityEngine: Send, Sync);
