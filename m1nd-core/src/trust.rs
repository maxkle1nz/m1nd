// === m1nd-core/src/trust.rs ===
//
// Per-module trust scores from defect history.
// Actuarial risk assessment: more confirmed bugs = lower trust = higher risk.

use crate::error::{M1ndError, M1ndResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

// ── Constants ──

/// Default trust score for nodes with no defect history (cold start).
pub const TRUST_COLD_START_DEFAULT: f32 = 0.5;
/// Default half-life for recency weighting in hours (720h = 30 days).
pub const RECENCY_HALF_LIFE_HOURS: f32 = 720.0;
/// Minimum contribution of old defects to weighted density (prevents decay to zero).
pub const RECENCY_FLOOR: f32 = 0.3;
/// Maximum risk multiplier returned (caps extreme values).
pub const RISK_MULTIPLIER_CAP: f32 = 3.0;
/// Maximum adjusted Bayesian prior (prevents certainty).
pub const PRIOR_CAP: f32 = 0.95;

// ── Core Types ──

/// Raw defect event counters for a single node, stored in the ledger.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustEntry {
    /// Number of confirmed defects (from `learn("correct")`).
    pub defect_count: u32,
    /// Number of false alarms (from `learn("wrong")`).
    pub false_alarm_count: u32,
    /// Number of partial matches (from `learn("partial")`).
    pub partial_count: u32,
    /// Unix timestamp (seconds) of the most recent confirmed defect.
    pub last_defect_timestamp: f64,
    /// Unix timestamp (seconds) of the first confirmed defect.
    pub first_defect_timestamp: f64,
    /// Total learn events (defect + false_alarm + partial).
    pub total_learn_events: u32,
}

/// Computed trust score for a node at a given point in time.
#[derive(Clone, Debug, Serialize)]
pub struct TrustScore {
    /// Trust score in [0.05, 1.0] — lower means riskier.
    pub trust_score: f32,
    /// Raw defect density: defects / total_learn_events.
    pub defect_density: f32,
    /// Risk multiplier in [1.0, `RISK_MULTIPLIER_CAP`].
    pub risk_multiplier: f32,
    /// Recency factor in [0.0, 1.0] — exponential decay since last defect.
    pub recency_factor: f32,
    /// Risk tier classification.
    pub tier: TrustTier,
}

/// Risk tier for a computed trust score.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TrustTier {
    /// trust_score < 0.4 — high defect density, recently active.
    HighRisk,
    /// trust_score in [0.4, 0.7) — moderate history.
    MediumRisk,
    /// trust_score >= 0.7 — few or old defects.
    LowRisk,
    /// No defect history (cold start).
    Unknown,
}

/// Map a trust tier to its agent-facing, action-routable band token.
///
/// The `Unknown` (cold-start, no-evidence) tier maps to `"insufficient_evidence"`
/// rather than a bare neutral number, so an agent reading the band is routed to
/// ingest/learn/cross_verify instead of mistaking a meaningless 0.5 prior for
/// "50% confidence". The risk vocabulary (`high`/`medium`/`low`) mirrors the
/// mission verdict tokens. Every presenter must classify through this helper.
pub fn trust_band(tier: TrustTier) -> &'static str {
    match tier {
        TrustTier::HighRisk => "high",
        TrustTier::MediumRisk => "medium",
        TrustTier::LowRisk => "low",
        TrustTier::Unknown => "insufficient_evidence",
    }
}

/// Trust score output for a single node in a trust report.
#[derive(Clone, Debug, Serialize)]
pub struct TrustNodeOutput {
    /// External ID of the node.
    pub node_id: String,
    /// Human-readable label (last `::` segment of external_id).
    pub label: String,
    /// Trust score in [0.05, 1.0].
    pub trust_score: f32,
    /// Raw defect density.
    pub defect_density: f32,
    /// Risk multiplier.
    pub risk_multiplier: f32,
    /// Recency factor.
    pub recency_factor: f32,
    /// Number of confirmed defects.
    pub defect_count: u32,
    /// Number of false alarms.
    pub false_alarm_count: u32,
    /// Number of partial matches.
    pub partial_count: u32,
    /// Total learn events.
    pub total_learn_events: u32,
    /// Hours since last defect (-1.0 if no defects recorded).
    pub last_defect_age_hours: f64,
    /// Risk tier.
    pub tier: TrustTier,
}

/// Aggregate trust statistics across a report scope.
#[derive(Clone, Debug, Serialize)]
pub struct TrustSummary {
    /// Number of nodes with at least `min_history` learn events.
    pub total_nodes_with_history: u32,
    /// Count of HighRisk nodes.
    pub high_risk_count: u32,
    /// Count of MediumRisk nodes.
    pub medium_risk_count: u32,
    /// Count of LowRisk nodes.
    pub low_risk_count: u32,
    /// Count of Unknown nodes.
    pub unknown_count: u32,
    /// Mean trust score across all nodes in the report.
    pub mean_trust: f32,
}

/// Complete trust report for a scope.
#[derive(Clone, Debug, Serialize)]
pub struct TrustResult {
    /// Trust scores sorted per `sort_by`.
    pub trust_scores: Vec<TrustNodeOutput>,
    /// Aggregate statistics.
    pub summary: TrustSummary,
    /// Scope string used ("all", "file", "module", "function").
    pub scope: String,
    /// Wall-clock time in milliseconds.
    pub elapsed_ms: f64,
}

/// Sort order for trust report results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustSortBy {
    /// Sort by trust score ascending (riskiest first).
    TrustAsc,
    /// Sort by trust score descending (most trusted first).
    TrustDesc,
    /// Sort by defect count descending.
    DefectsDesc,
    /// Sort by time since last defect ascending (most recent first).
    Recency,
}

impl std::str::FromStr for TrustSortBy {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "trust_desc" => Self::TrustDesc,
            "defects_desc" => Self::DefectsDesc,
            "recency" => Self::Recency,
            _ => Self::TrustAsc,
        })
    }
}

// ── Ledger ──

/// Actuarial defect ledger that maps node external IDs to their defect histories.
///
/// Accumulates `record_defect`, `record_false_alarm`, and `record_partial` events
/// as learn feedback arrives, then computes time-weighted trust scores on demand.
#[derive(Clone, Debug, Default)]
pub struct TrustLedger {
    entries: HashMap<String, TrustEntry>,
}

impl TrustLedger {
    /// Create an empty ledger.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Record a defect (from learn("correct")).
    pub fn record_defect(&mut self, external_id: &str, timestamp: f64) {
        let entry = self
            .entries
            .entry(external_id.to_string())
            .or_insert_with(|| TrustEntry {
                defect_count: 0,
                false_alarm_count: 0,
                partial_count: 0,
                last_defect_timestamp: 0.0,
                first_defect_timestamp: timestamp,
                total_learn_events: 0,
            });
        entry.defect_count += 1;
        entry.total_learn_events += 1;
        entry.last_defect_timestamp = timestamp;
        if entry.defect_count == 1 {
            entry.first_defect_timestamp = timestamp;
        }
    }

    /// Record a false alarm (from learn("wrong")).
    pub fn record_false_alarm(&mut self, external_id: &str, timestamp: f64) {
        let entry = self
            .entries
            .entry(external_id.to_string())
            .or_insert_with(|| TrustEntry {
                defect_count: 0,
                false_alarm_count: 0,
                partial_count: 0,
                last_defect_timestamp: 0.0,
                first_defect_timestamp: 0.0,
                total_learn_events: 0,
            });
        entry.false_alarm_count += 1;
        entry.total_learn_events += 1;
        let _ = timestamp; // false alarms don't update defect timestamps
    }

    /// Record a partial match (from learn("partial")).
    pub fn record_partial(&mut self, external_id: &str, timestamp: f64) {
        let entry = self
            .entries
            .entry(external_id.to_string())
            .or_insert_with(|| TrustEntry {
                defect_count: 0,
                false_alarm_count: 0,
                partial_count: 0,
                last_defect_timestamp: 0.0,
                first_defect_timestamp: 0.0,
                total_learn_events: 0,
            });
        entry.partial_count += 1;
        entry.total_learn_events += 1;
        let _ = timestamp;
    }

    /// Iterate the recorded `(external_id, &TrustEntry)` pairs without exposing
    /// the backing map. Mirrors `CalibrationTable::signals()`. Used by the
    /// envelope-calibration harness to derive a labeled corpus from real learn
    /// outcomes (a confirmed defect ⇒ trusting the node would have been WRONG;
    /// a false alarm ⇒ trusting it was RIGHT).
    pub fn entries(&self) -> impl Iterator<Item = (&str, &TrustEntry)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Number of nodes with any recorded learn history.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Compute trust score for a single node at the given time (default params).
    pub fn compute_trust(&self, external_id: &str, now: f64) -> TrustScore {
        self.compute_trust_with_params(
            external_id,
            now,
            RECENCY_HALF_LIFE_HOURS,
            RISK_MULTIPLIER_CAP,
        )
    }

    /// Compute trust score with configurable half-life and risk cap.
    pub fn compute_trust_with_params(
        &self,
        external_id: &str,
        now: f64,
        half_life_hours: f32,
        risk_cap: f32,
    ) -> TrustScore {
        let entry = match self.entries.get(external_id) {
            Some(e) => e,
            None => {
                return TrustScore {
                    trust_score: TRUST_COLD_START_DEFAULT,
                    defect_density: 0.0,
                    risk_multiplier: 1.0,
                    recency_factor: 0.0,
                    tier: TrustTier::Unknown,
                };
            }
        };

        if entry.total_learn_events == 0 {
            return TrustScore {
                trust_score: TRUST_COLD_START_DEFAULT,
                defect_density: 0.0,
                risk_multiplier: 1.0,
                recency_factor: 0.0,
                tier: TrustTier::Unknown,
            };
        }

        let raw_density = entry.defect_count as f32 / entry.total_learn_events as f32;

        // Recency weighting: half-life configurable (default 720 hours = 30 days)
        // recency = exp(-ln2 * hours_since_last_defect / half_life_hours)
        let recency = if entry.defect_count > 0 && entry.last_defect_timestamp > 0.0 {
            let hours_since = ((now - entry.last_defect_timestamp) / 3600.0).max(0.0) as f32;
            (-std::f32::consts::LN_2 * hours_since / half_life_hours.max(1.0)).exp()
        } else {
            0.0
        };

        // Time-weighted density: even old bugs contribute 30% (RECENCY_FLOOR)
        let weighted_density = raw_density * (RECENCY_FLOOR + (1.0 - RECENCY_FLOOR) * recency);

        // Trust score: 1.0 - weighted_density, clamped to [0.05, 1.0]
        let trust_score = (1.0 - weighted_density).max(0.05);

        // Risk multiplier: 1.0 + (weighted_density * 2.0), capped at risk_cap
        let risk_multiplier = (1.0 + weighted_density * 2.0).min(risk_cap);

        // Tier classification
        let tier = if trust_score < 0.4 {
            TrustTier::HighRisk
        } else if trust_score < 0.7 {
            TrustTier::MediumRisk
        } else {
            TrustTier::LowRisk
        };

        TrustScore {
            trust_score,
            defect_density: raw_density,
            risk_multiplier,
            recency_factor: recency,
            tier,
        }
    }

    /// Generate full trust report.
    #[allow(clippy::too_many_arguments)]
    pub fn report(
        &self,
        scope: &str,
        min_history: u32,
        top_k: usize,
        node_filter: Option<&str>,
        sort_by: TrustSortBy,
        now: f64,
        half_life_hours: f32,
        risk_cap: f32,
    ) -> TrustResult {
        let start = std::time::Instant::now();

        let mut outputs: Vec<TrustNodeOutput> = Vec::new();
        let mut high_risk_count = 0u32;
        let mut medium_risk_count = 0u32;
        let mut low_risk_count = 0u32;
        let mut unknown_count = 0u32;
        let mut trust_sum = 0.0f32;
        let mut total_nodes_with_history = 0u32;

        for (external_id, entry) in &self.entries {
            // Scope filter: match node type prefix
            if scope != "all" {
                let matches_scope = match scope {
                    "file" => external_id.starts_with("file::"),
                    "module" => {
                        external_id.starts_with("module::") || external_id.starts_with("dir::")
                    }
                    "function" => {
                        external_id.starts_with("func::") || external_id.starts_with("function::")
                    }
                    _ => true,
                };
                if !matches_scope {
                    continue;
                }
            }

            // Node filter
            if let Some(filter) = node_filter {
                if !external_id.contains(filter) {
                    continue;
                }
            }

            // Min history filter
            if entry.total_learn_events < min_history {
                continue;
            }

            total_nodes_with_history += 1;

            let score = self.compute_trust_with_params(external_id, now, half_life_hours, risk_cap);

            match score.tier {
                TrustTier::HighRisk => high_risk_count += 1,
                TrustTier::MediumRisk => medium_risk_count += 1,
                TrustTier::LowRisk => low_risk_count += 1,
                TrustTier::Unknown => unknown_count += 1,
            }
            trust_sum += score.trust_score;

            // Label: extract filename from external_id
            let label = external_id
                .rsplit("::")
                .next()
                .unwrap_or(external_id)
                .to_string();

            let last_defect_age_hours =
                if entry.defect_count > 0 && entry.last_defect_timestamp > 0.0 {
                    ((now - entry.last_defect_timestamp) / 3600.0).max(0.0)
                } else {
                    -1.0 // no defects
                };

            outputs.push(TrustNodeOutput {
                node_id: external_id.clone(),
                label,
                trust_score: score.trust_score,
                defect_density: score.defect_density,
                risk_multiplier: score.risk_multiplier,
                recency_factor: score.recency_factor,
                defect_count: entry.defect_count,
                false_alarm_count: entry.false_alarm_count,
                partial_count: entry.partial_count,
                total_learn_events: entry.total_learn_events,
                last_defect_age_hours,
                tier: score.tier,
            });
        }

        // Sort
        match sort_by {
            TrustSortBy::TrustAsc => {
                outputs.sort_by(|a, b| {
                    a.trust_score
                        .partial_cmp(&b.trust_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            TrustSortBy::TrustDesc => {
                outputs.sort_by(|a, b| {
                    b.trust_score
                        .partial_cmp(&a.trust_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            TrustSortBy::DefectsDesc => {
                outputs.sort_by_key(|entry| std::cmp::Reverse(entry.defect_count));
            }
            TrustSortBy::Recency => {
                outputs.sort_by(|a, b| {
                    a.last_defect_age_hours
                        .partial_cmp(&b.last_defect_age_hours)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        outputs.truncate(top_k);

        let mean_trust = if total_nodes_with_history > 0 {
            trust_sum / total_nodes_with_history as f32
        } else {
            TRUST_COLD_START_DEFAULT
        };

        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        TrustResult {
            trust_scores: outputs,
            summary: TrustSummary {
                total_nodes_with_history,
                high_risk_count,
                medium_risk_count,
                low_risk_count,
                unknown_count,
                mean_trust,
            },
            scope: scope.to_string(),
            elapsed_ms,
        }
    }

    /// Adjust a Bayesian prior based on trust data for mentioned nodes.
    /// Returns adjusted prior clamped to [0.0, PRIOR_CAP].
    ///
    /// For positive claims ("no bug" claims like NEVER_CALLS, NO_DEPENDENCY, ISOLATED):
    ///   adjusted_prior = base_prior * trust_score (trustworthy module -> more likely true)
    /// For negative claims ("has bug" claims):
    ///   adjusted_prior = base_prior * risk_multiplier (buggy module -> more likely to have this bug)
    pub fn adjust_prior(
        &self,
        base_prior: f32,
        external_ids: &[String],
        is_positive_claim: bool,
        now: f64,
    ) -> f32 {
        if external_ids.is_empty() {
            return base_prior;
        }

        // Compute average trust factor across mentioned nodes
        let mut factor_sum = 0.0f32;
        let mut count = 0u32;

        for ext_id in external_ids {
            let score = self.compute_trust(ext_id, now);
            let factor = if is_positive_claim {
                // "No bug" claim: trust increases confidence
                score.trust_score
            } else {
                // "Has bug" claim: risk multiplier increases confidence
                score.risk_multiplier
            };
            factor_sum += factor;
            count += 1;
        }

        if count == 0 {
            return base_prior;
        }

        let avg_factor = factor_sum / count as f32;
        let adjusted = base_prior * avg_factor;

        // Clamp to [0.0, PRIOR_CAP]
        adjusted.clamp(0.0, PRIOR_CAP)
    }

    /// Number of entries in the ledger.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the ledger contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over recorded external IDs without exposing ledger internals.
    pub fn external_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

// ── Persistence ──

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustPersistenceFormat {
    version: u32,
    entries: BTreeMap<String, TrustEntry>,
}

const TRUST_PERSISTENCE_VERSION: u32 = 1;

fn validate_trust_entries<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a TrustEntry)>,
) -> M1ndResult<()> {
    for (external_id, entry) in entries {
        if external_id.trim().is_empty() {
            return Err(M1ndError::CorruptState {
                reason: "trust ledger contains an empty external id".into(),
            });
        }
        if !entry.last_defect_timestamp.is_finite() || !entry.first_defect_timestamp.is_finite() {
            return Err(M1ndError::CorruptState {
                reason: format!("trust entry '{external_id}' contains non-finite timestamps"),
            });
        }
        let counted_events = entry
            .defect_count
            .checked_add(entry.false_alarm_count)
            .and_then(|count| count.checked_add(entry.partial_count))
            .ok_or_else(|| M1ndError::CorruptState {
                reason: format!("trust entry '{external_id}' event counters overflow"),
            })?;
        if counted_events == 0 || counted_events != entry.total_learn_events {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "trust entry '{external_id}' declares {} events but counters total {counted_events}",
                    entry.total_learn_events
                ),
            });
        }
        let timestamps_are_coherent = if entry.defect_count == 0 {
            entry.first_defect_timestamp == 0.0 && entry.last_defect_timestamp == 0.0
        } else {
            entry.first_defect_timestamp >= 0.0 && entry.last_defect_timestamp >= 0.0
        };
        if !timestamps_are_coherent {
            return Err(M1ndError::CorruptState {
                reason: format!("trust entry '{external_id}' has incoherent defect timestamps"),
            });
        }
    }
    Ok(())
}

/// Encode a trust ledger as deterministic, versioned pretty JSON bytes.
pub fn encode_trust_state_json(ledger: &TrustLedger) -> M1ndResult<Vec<u8>> {
    validate_trust_entries(
        ledger
            .entries
            .iter()
            .map(|(external_id, entry)| (external_id.as_str(), entry)),
    )?;
    let format = TrustPersistenceFormat {
        version: TRUST_PERSISTENCE_VERSION,
        entries: ledger
            .entries
            .iter()
            .map(|(external_id, entry)| (external_id.clone(), entry.clone()))
            .collect(),
    };
    serde_json::to_vec_pretty(&format).map_err(M1ndError::Serde)
}

/// Decode trust-state JSON bytes and reject malformed, unsupported, or
/// non-finite state as one fail-closed unit.
pub fn decode_trust_state_json(bytes: &[u8]) -> M1ndResult<TrustLedger> {
    let format: TrustPersistenceFormat = serde_json::from_slice(bytes).map_err(M1ndError::Serde)?;
    if format.version != TRUST_PERSISTENCE_VERSION {
        return Err(M1ndError::CorruptState {
            reason: format!("unsupported trust persistence version {}", format.version),
        });
    }
    validate_trust_entries(
        format
            .entries
            .iter()
            .map(|(external_id, entry)| (external_id.as_str(), entry)),
    )?;
    let ledger = TrustLedger {
        entries: format.entries.into_iter().collect(),
    };
    let projected = encode_trust_state_json(&ledger)?;
    if serde_json::from_slice::<serde_json::Value>(bytes)?
        != serde_json::from_slice::<serde_json::Value>(&projected)?
    {
        return Err(M1ndError::CorruptState {
            reason: "trust checkpoint is not the complete current schema".into(),
        });
    }
    Ok(ledger)
}

/// Persist a `TrustLedger` to disk using an atomic write (temp file + rename).
///
/// # Parameters
/// - `ledger`: ledger to serialise.
/// - `path`: destination file path (JSON).
///
/// # Errors
/// Returns `M1ndError::Serde` if JSON serialisation fails, or `M1ndError::Io` on
/// filesystem errors.
pub fn save_trust_state(ledger: &TrustLedger, path: &Path) -> M1ndResult<()> {
    let json = encode_trust_state_json(ledger)?;

    // Atomic write: temp file + rename
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("tmp");
    {
        use std::io::Write;
        let file = std::fs::File::create(&temp_path)?;
        let mut writer = std::io::BufWriter::new(file);
        writer.write_all(&json)?;
        writer.flush()?;
    }
    std::fs::rename(&temp_path, path)?;

    Ok(())
}

/// Load a `TrustLedger` from disk, returning an empty ledger if the file does not exist.
///
/// Corrupt entries (non-finite timestamps) are silently dropped with a diagnostic to stderr.
///
/// # Parameters
/// - `path`: source file path (JSON produced by `save_trust_state`).
///
/// # Errors
/// Returns `M1ndError::Io` on read failures or `M1ndError::Serde` if the JSON is malformed.
pub fn load_trust_state(path: &Path) -> M1ndResult<TrustLedger> {
    if !path.exists() {
        return Ok(TrustLedger::new());
    }

    let data = std::fs::read_to_string(path)?;
    let format: TrustPersistenceFormat = serde_json::from_str(&data).map_err(M1ndError::Serde)?;

    // Preserve the historical disk-loader behavior: tolerate a partially
    // corrupt legacy file by dropping only invalid entries. The in-memory
    // `decode_trust_state_json` API is deliberately strict for checkpoints.
    let mut valid_entries = HashMap::new();
    for (key, entry) in format.entries {
        if !entry.last_defect_timestamp.is_finite() || !entry.first_defect_timestamp.is_finite() {
            eprintln!(
                "m1nd trust: rejecting corrupt entry for {}: non-finite timestamps",
                key
            );
            continue;
        }
        valid_entries.insert(key, entry);
    }

    Ok(TrustLedger {
        entries: valid_entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_ledger() -> TrustLedger {
        TrustLedger::new()
    }

    const NOW: f64 = 10_000.0 * 3600.0; // 10 000 hours epoch

    // 1. record_defect: count and total_learn_events increment
    #[test]
    fn record_defect_increments_counts() {
        let mut ledger = make_ledger();
        ledger.record_defect("file::foo.py", NOW);
        let entry = ledger.entries.get("file::foo.py").unwrap();
        assert_eq!(entry.defect_count, 1);
        assert_eq!(entry.total_learn_events, 1);
    }

    // 2. trust_decreases: more defects → lower trust score
    #[test]
    fn trust_decreases_with_defects() {
        let mut ledger = make_ledger();
        // Cold start
        let cold = ledger.compute_trust("file::new.py", NOW);
        assert_eq!(cold.trust_score, TRUST_COLD_START_DEFAULT);

        // Record several defects (recent)
        for i in 0..5 {
            ledger.record_defect("file::buggy.py", NOW - i as f64);
        }
        let buggy = ledger.compute_trust("file::buggy.py", NOW);
        assert!(
            buggy.trust_score < TRUST_COLD_START_DEFAULT,
            "trust_score {} should be below cold start {}",
            buggy.trust_score,
            TRUST_COLD_START_DEFAULT
        );
    }

    // 3. recency_decay: defects from long ago contribute less than recent ones
    #[test]
    fn recency_decay_reduces_old_defects_weight() {
        let mut old_ledger = make_ledger();
        let mut new_ledger = make_ledger();

        // Old defect: 180 days ago
        let old_ts = NOW - 180.0 * 24.0 * 3600.0;
        old_ledger.record_defect("file::module.py", old_ts);

        // Recent defect: just now
        new_ledger.record_defect("file::module.py", NOW);

        let old_score = old_ledger.compute_trust("file::module.py", NOW);
        let new_score = new_ledger.compute_trust("file::module.py", NOW);

        // Old defect → recency near floor → higher trust than very recent defect
        assert!(
            old_score.trust_score > new_score.trust_score,
            "Old defect should decay: old={} new={}",
            old_score.trust_score,
            new_score.trust_score
        );
    }

    // 4. risk_cap: risk_multiplier is bounded by RISK_MULTIPLIER_CAP
    #[test]
    fn risk_multiplier_capped() {
        let mut ledger = make_ledger();
        // Flood with defects (all recent) to push risk up
        for i in 0..50 {
            ledger.record_defect("file::broken.py", NOW - i as f64 * 0.1);
        }
        let score = ledger.compute_trust("file::broken.py", NOW);
        assert!(
            score.risk_multiplier <= RISK_MULTIPLIER_CAP,
            "risk_multiplier {} exceeds cap {}",
            score.risk_multiplier,
            RISK_MULTIPLIER_CAP
        );
    }

    // 5. report_scope: scope="file" only returns file:: nodes
    #[test]
    fn report_scope_filters_by_prefix() {
        let mut ledger = make_ledger();
        ledger.record_defect("file::routes.py", NOW);
        ledger.record_defect("module::services", NOW);

        let result = ledger.report(
            "file",
            1,
            100,
            None,
            TrustSortBy::TrustAsc,
            NOW,
            RECENCY_HALF_LIFE_HOURS,
            RISK_MULTIPLIER_CAP,
        );

        for out in &result.trust_scores {
            assert!(
                out.node_id.starts_with("file::"),
                "Expected file:: prefix, got {}",
                out.node_id
            );
        }
        assert!(
            !result.trust_scores.is_empty(),
            "Should have at least one file:: result"
        );
    }

    // 6. sort_trust_asc: results are in ascending trust order
    #[test]
    fn sort_trust_asc_is_ordered() {
        let mut ledger = make_ledger();
        // file::a: no defects but 1 false alarm (so it has an entry)
        ledger.record_false_alarm("file::clean.py", NOW);
        // file::b: many recent defects → low trust
        for i in 0..5 {
            ledger.record_defect("file::dirty.py", NOW - i as f64);
        }

        let result = ledger.report(
            "all",
            1,
            100,
            None,
            TrustSortBy::TrustAsc,
            NOW,
            RECENCY_HALF_LIFE_HOURS,
            RISK_MULTIPLIER_CAP,
        );

        let scores: Vec<f32> = result.trust_scores.iter().map(|o| o.trust_score).collect();
        for w in scores.windows(2) {
            assert!(w[0] <= w[1], "Not sorted ascending: {} > {}", w[0], w[1]);
        }
    }

    // 7. adjust_prior: positive claim scaled by trust; negative claim scaled by risk
    #[test]
    fn adjust_prior_positive_and_negative_claims() {
        let mut ledger = make_ledger();
        // Give module a recent defect to get a non-trivial score
        for i in 0..3 {
            ledger.record_defect("file::risky.py", NOW - i as f64 * 60.0);
        }

        let base = 0.6f32;
        let ids = vec!["file::risky.py".to_string()];

        let adj_positive = ledger.adjust_prior(base, &ids, true, NOW);
        let adj_negative = ledger.adjust_prior(base, &ids, false, NOW);

        // Positive claim: adjusted ≤ base (trust < 1.0 scales down)
        assert!(
            adj_positive <= base,
            "Positive claim prior {} should be ≤ base {}",
            adj_positive,
            base
        );
        // Negative claim: adjusted may be > or ≈ base (risk_multiplier ≥ 1.0)
        assert!(
            adj_negative >= adj_positive,
            "Negative claim {} should be ≥ positive {}",
            adj_negative,
            adj_positive
        );
        // Both clamped to [0, PRIOR_CAP]
        assert!(adj_positive <= PRIOR_CAP);
        assert!(adj_negative <= PRIOR_CAP);
    }

    // 8. save_load: round-trip preserves defect counts
    #[test]
    fn save_load_round_trip() {
        let mut ledger = make_ledger();
        ledger.record_defect("file::persist.py", NOW);
        ledger.record_defect("file::persist.py", NOW - 3600.0);
        ledger.record_false_alarm("file::persist.py", NOW - 7200.0);

        let dir = std::env::temp_dir();
        let path: PathBuf = dir.join(format!("trust_test_{}.json", std::process::id()));

        save_trust_state(&ledger, &path).expect("save failed");
        let loaded = load_trust_state(&path).expect("load failed");

        let orig_entry = ledger.entries.get("file::persist.py").unwrap();
        let load_entry = loaded.entries.get("file::persist.py").unwrap();

        assert_eq!(load_entry.defect_count, orig_entry.defect_count);
        assert_eq!(load_entry.false_alarm_count, orig_entry.false_alarm_count);
        assert_eq!(load_entry.total_learn_events, orig_entry.total_learn_events);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_load_round_trip_creates_parent_directories() {
        let mut ledger = make_ledger();
        ledger.record_defect("file::persist.py", NOW);

        let dir = std::env::temp_dir().join(format!("trust_nested_{}", std::process::id()));
        let path: PathBuf = dir.join("deep").join("trust_state.json");

        save_trust_state(&ledger, &path).expect("save failed");
        let loaded = load_trust_state(&path).expect("load failed");

        assert!(loaded.entries.contains_key("file::persist.py"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trust_memory_codec_is_deterministic_and_matches_save() {
        let mut ledger = make_ledger();
        ledger.record_defect("file::z.py", NOW);
        ledger.record_defect("file::a.py", NOW - 1.0);

        let first = encode_trust_state_json(&ledger).expect("encode");
        assert_eq!(first, encode_trust_state_json(&ledger).expect("repeat"));
        let text = std::str::from_utf8(&first).expect("utf8");
        assert!(text.find("file::a.py").unwrap() < text.find("file::z.py").unwrap());

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("trust_state.json");
        save_trust_state(&ledger, &path).expect("save");
        assert_eq!(std::fs::read(path).expect("saved bytes"), first);

        let decoded = decode_trust_state_json(&first).expect("decode");
        assert_eq!(decoded.len(), ledger.len());
    }

    #[test]
    fn trust_memory_codec_rejects_corruption_version_and_nonfinite() {
        assert!(decode_trust_state_json(b"{").is_err());
        assert!(decode_trust_state_json(br#"{"version":2,"entries":{}}"#).is_err());

        let mut ledger = make_ledger();
        ledger.record_defect("file::bad.py", NOW);
        ledger
            .entries
            .get_mut("file::bad.py")
            .expect("entry")
            .last_defect_timestamp = f64::NAN;
        assert!(encode_trust_state_json(&ledger).is_err());
    }

    #[test]
    fn trust_checkpoint_codec_rejects_counter_timestamp_and_schema_drift() {
        let mut ledger = make_ledger();
        ledger.record_defect("file::bad.py", NOW);
        ledger
            .entries
            .get_mut("file::bad.py")
            .expect("entry")
            .total_learn_events = 2;
        assert!(encode_trust_state_json(&ledger).is_err());

        let mut no_defect = make_ledger();
        no_defect.record_false_alarm("file::bad.py", NOW);
        no_defect
            .entries
            .get_mut("file::bad.py")
            .expect("entry")
            .last_defect_timestamp = NOW;
        assert!(encode_trust_state_json(&no_defect).is_err());

        let mut valid = make_ledger();
        valid.record_defect("file::valid.py", NOW);
        let mut value: serde_json::Value =
            serde_json::from_slice(&encode_trust_state_json(&valid).expect("valid checkpoint"))
                .expect("value");
        value["entries"]["file::valid.py"]["future_field"] = serde_json::json!(true);
        assert!(
            decode_trust_state_json(&serde_json::to_vec_pretty(&value).expect("json")).is_err()
        );
    }

    // Extra: cold start returns Unknown tier and 0.5 score
    #[test]
    fn cold_start_returns_unknown_tier() {
        let ledger = make_ledger();
        let score = ledger.compute_trust("file::never_seen.py", NOW);
        assert_eq!(score.trust_score, TRUST_COLD_START_DEFAULT);
        assert_eq!(score.tier, TrustTier::Unknown);
        assert_eq!(score.risk_multiplier, 1.0);
        // The cold-start tier must surface as the action-routable band, never
        // as the bare 0.5 number, so an agent is routed to learn/cross_verify.
        assert_eq!(trust_band(score.tier), "insufficient_evidence");
    }

    // band: every tier maps to its agent-facing token (Unknown is NOT a number)
    #[test]
    fn trust_band_maps_every_tier() {
        assert_eq!(trust_band(TrustTier::HighRisk), "high");
        assert_eq!(trust_band(TrustTier::MediumRisk), "medium");
        assert_eq!(trust_band(TrustTier::LowRisk), "low");
        assert_eq!(trust_band(TrustTier::Unknown), "insufficient_evidence");
    }

    // band: an evidenced node lands in a risk band, never "insufficient_evidence"
    #[test]
    fn evidenced_node_gets_risk_band_not_insufficient() {
        let mut ledger = make_ledger();
        for i in 0..5 {
            ledger.record_defect("file::buggy.py", NOW - i as f64);
        }
        let score = ledger.compute_trust("file::buggy.py", NOW);
        let band = trust_band(score.tier);
        assert!(
            matches!(band, "high" | "medium" | "low"),
            "evidenced node band {band} must be a risk band"
        );
        assert_ne!(band, "insufficient_evidence");
    }
}
