// === m1nd-core/src/calibration.rs ===
//
// Conformal calibration table for m1nd retrieval signals.
//
// Turns an uncalibrated model `confidence` into a MEASURED, abstention-gated
// verdict. The first (and currently only) calibrated signal is `predict`
// (co-change): the repo's own git history is a FREE labeled corpus, so a
// date-split harness can measure precision-at-coverage on held-out commits and
// derive a split-conformal abstention threshold τ against an operator risk
// budget α (split-conformal abstention, arXiv 2405.01563).
//
// Design mirrors `trust.rs`: a `HashMap`-backed table with the SAME atomic
// (temp file + rename) versioned-JSON persistence. No new dependency — the
// conformal math is hand-rolled (`conformal_quantile`).
//
// HONESTY INVARIANT: a band is NEVER quoted as a probability anywhere — only
// the binned verdict (`act` | `reverify` | `abstain`) is emitted. A signal with
// no calibration row is honestly `abstain`/uncalibrated, never `act`.

use crate::error::M1ndResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ── Constants ──

/// Default operator risk budget (target miscoverage). 0.1 = "I will accept at
/// most a 10% error rate among predictions the gate lets through as `act`".
pub const DEFAULT_TARGET_ALPHA: f32 = 0.1;

/// Signal name for the calibrated `predict`/co-change verdict. Stable key into
/// `calibration_state.json` and the contract the harness + predict gate share.
pub const CALIBRATION_SIGNAL_PREDICT: &str = "predict";

/// Verdict strings — house style is stringly-typed verdicts (mirrors
/// `Sufficiency::state`, `xray_gate.verdict`, `proof_state`).
pub const VERDICT_ACT: &str = "act";
pub const VERDICT_REVERIFY: &str = "reverify";
pub const VERDICT_ABSTAIN: &str = "abstain";

// ── Row ──

/// A calibrated decision row for ONE signal (e.g. "predict").
///
/// `tau` is the split-conformal threshold: a live confidence at or above `tau`
/// clears the operator's risk budget `target_alpha` on held-out data. The
/// remaining fields are the measured evidence behind that threshold, kept so a
/// recalled row can self-describe its standing instead of being a bare number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalibrationRow {
    /// Conformal threshold τ: live confidence ≥ τ ⇒ `act`.
    pub tau: f32,
    /// Operator risk budget α the threshold was computed against.
    pub target_alpha: f32,
    /// Measured precision among predictions kept at `act` coverage (held-out).
    pub measured_precision: f32,
    /// Fraction of held-out predictions that cleared `tau` (coverage at `act`).
    pub coverage: f32,
    /// Number of labeled held-out (prediction, confidence) pairs measured.
    pub n: usize,
    /// Wall-clock (epoch millis) the row was calibrated.
    pub calibrated_at_ms: u64,
}

impl CalibrationRow {
    /// The lower band edge: below this, the gate `abstain`s. Predictions in
    /// `[tau_low, tau)` are the borderline band → `reverify`. We anchor the band
    /// width to a fraction of the conformal threshold so the `reverify` zone
    /// scales with the model's own confidence units (no second magic constant).
    pub fn tau_low(&self) -> f32 {
        // Reverify band = the lower half of the conformal threshold. A live
        // confidence between half-τ and τ is "close but not clearing the risk
        // budget" → ask the agent to re-verify rather than abstain outright.
        self.tau * 0.5
    }

    /// Bin a live `confidence` into the honest verdict for this row.
    ///
    /// Never returns a probability — only the binned verdict string.
    pub fn verdict(&self, confidence: f32) -> &'static str {
        verdict_for(confidence, self.tau, self.tau_low())
    }
}

/// Bin a live `confidence` against a high (`tau`) and low (`tau_low`) threshold.
///
/// - `act`      when `confidence >= tau` (clears the risk budget),
/// - `reverify` when `tau_low <= confidence < tau` (borderline),
/// - `abstain`  when `confidence < tau_low` (or no/NaN signal).
///
/// A NaN or non-finite confidence is treated as no signal → `abstain` (never a
/// fake-high `act`).
pub fn verdict_for(confidence: f32, tau: f32, tau_low: f32) -> &'static str {
    if !confidence.is_finite() {
        return VERDICT_ABSTAIN;
    }
    if confidence >= tau {
        VERDICT_ACT
    } else if confidence >= tau_low {
        VERDICT_REVERIFY
    } else {
        VERDICT_ABSTAIN
    }
}

// ── Conformal quantile (split-conformal; arXiv 2405.01563) ──

/// Split-conformal threshold τ for a slice of nonconformity `scores` at risk
/// budget `alpha`.
///
/// `scores` are the model confidences assigned to predictions that did NOT hold
/// (the misses) — feeding the misses' confidence distribution yields the
/// confidence level above which the held-out error rate is bounded by `alpha`.
/// Returns the empirical `ceil((n+1)*(1-alpha))/n` quantile (the finite-sample
/// split-conformal correction). For an empty slice there is no evidence, so the
/// honest threshold is `1.0` (nothing clears it → the gate abstains by default).
pub fn conformal_quantile(scores: &[f32], alpha: f32) -> f32 {
    let n = scores.len();
    if n == 0 {
        // No miss evidence → maximally conservative: nothing can clear τ.
        return 1.0;
    }
    let mut sorted: Vec<f32> = scores.iter().copied().filter(|s| s.is_finite()).collect();
    let m = sorted.len();
    if m == 0 {
        return 1.0;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Finite-sample split-conformal rank: ceil((m+1)*(1-alpha)).
    let alpha = alpha.clamp(0.0, 1.0);
    let rank = (((m + 1) as f32) * (1.0 - alpha)).ceil() as usize;
    if rank >= m {
        // Quantile lands past the largest observed miss-confidence: no finite
        // threshold is justified → 1.0 (abstain-by-default, honest under-data).
        return 1.0;
    }
    // `rank` is 1-based; index into the sorted misses.
    sorted[rank.saturating_sub(1).min(m - 1)]
}

// ── Table ──

/// Calibration table: per-signal `CalibrationRow`s.
///
/// Cloned in shape from `TrustLedger` (a `HashMap` wrapper) so it inherits the
/// same persistence and round-trip discipline.
#[derive(Clone, Debug, Default)]
pub struct CalibrationTable {
    rows: HashMap<String, CalibrationRow>,
}

impl CalibrationTable {
    /// Create an empty table.
    pub fn new() -> Self {
        Self {
            rows: HashMap::new(),
        }
    }

    /// Insert or replace the calibration row for a signal.
    pub fn set(&mut self, signal: &str, row: CalibrationRow) {
        self.rows.insert(signal.to_string(), row);
    }

    /// Look up the calibration row for a signal, if one has been measured.
    pub fn get(&self, signal: &str) -> Option<&CalibrationRow> {
        self.rows.get(signal)
    }

    /// Iterate over calibrated signal names without exposing internals.
    pub fn signals(&self) -> impl Iterator<Item = &str> {
        self.rows.keys().map(String::as_str)
    }

    /// Number of calibrated signals.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the table has no calibrated signals.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

// ── Persistence (clone of trust.rs save/load: atomic temp + rename, versioned) ──

#[derive(Serialize, Deserialize)]
struct CalibrationPersistenceFormat {
    version: u32,
    rows: HashMap<String, CalibrationRow>,
}

/// Persist a `CalibrationTable` to disk using an atomic write (temp + rename).
///
/// # Errors
/// Returns `M1ndError::Serde` on serialisation failure or `M1ndError::Io` on
/// filesystem errors.
pub fn save_calibration_state(table: &CalibrationTable, path: &Path) -> M1ndResult<()> {
    let format = CalibrationPersistenceFormat {
        version: 1,
        rows: table.rows.clone(),
    };

    let json = serde_json::to_string_pretty(&format).map_err(crate::error::M1ndError::Serde)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("tmp");
    {
        use std::io::Write;
        let file = std::fs::File::create(&temp_path)?;
        let mut writer = std::io::BufWriter::new(file);
        writer.write_all(json.as_bytes())?;
        writer.flush()?;
    }
    std::fs::rename(&temp_path, path)?;

    Ok(())
}

/// Load a `CalibrationTable` from disk, returning an empty table if absent.
///
/// Corrupt rows (non-finite τ) are silently dropped with a diagnostic to stderr.
///
/// # Errors
/// Returns `M1ndError::Io` on read failure or `M1ndError::Serde` on malformed JSON.
pub fn load_calibration_state(path: &Path) -> M1ndResult<CalibrationTable> {
    if !path.exists() {
        return Ok(CalibrationTable::new());
    }

    let data = std::fs::read_to_string(path)?;
    let format: CalibrationPersistenceFormat =
        serde_json::from_str(&data).map_err(crate::error::M1ndError::Serde)?;

    let mut valid_rows = HashMap::new();
    for (signal, row) in format.rows {
        if !row.tau.is_finite() || !row.measured_precision.is_finite() {
            eprintln!(
                "m1nd calibration: rejecting corrupt row for {}: non-finite fields",
                signal
            );
            continue;
        }
        valid_rows.insert(signal, row);
    }

    Ok(CalibrationTable { rows: valid_rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_row() -> CalibrationRow {
        CalibrationRow {
            tau: 0.6,
            target_alpha: DEFAULT_TARGET_ALPHA,
            measured_precision: 0.85,
            coverage: 0.4,
            n: 100,
            calibrated_at_ms: 1_700_000_000_000,
        }
    }

    // ── Conformal quantile: deterministic oracle (written FAILING first) ──
    //
    // Mirrors the closure_verdict / compute_sufficiency branch tests: a fixed
    // labeled score array → an EXACT expected index value, computed by hand.
    //
    // Scores (10 miss-confidences), already chosen so sorting is obvious:
    //   [0.05, 0.10, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90]
    // n = m = 10, alpha = 0.1 → rank = ceil((10+1)*(1-0.1)) = ceil(9.9) = 10.
    // rank (10) >= m (10) → the quantile lands past the largest observed miss,
    // so no finite threshold is justified → τ = 1.0 (abstain-by-default).
    #[test]
    fn conformal_quantile_exact_index_high_alpha_saturates_to_one() {
        let scores = [
            0.05, 0.10, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90f32,
        ];
        let tau = conformal_quantile(&scores, 0.1);
        assert_eq!(
            tau, 1.0,
            "rank ceil(11*0.9)=10 >= m=10 ⇒ τ saturates to 1.0"
        );
    }

    // alpha = 0.5 → rank = ceil((10+1)*0.5) = ceil(5.5) = 6.
    // 1-based rank 6 → sorted index 5 → value 0.50. EXACT.
    #[test]
    fn conformal_quantile_exact_index_mid_alpha() {
        let scores = [
            0.05, 0.10, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90f32,
        ];
        let tau = conformal_quantile(&scores, 0.5);
        assert_eq!(tau, 0.50, "rank ceil(11*0.5)=6 ⇒ 1-based[6]=index[5]=0.50");
    }

    // Empty scores → no evidence → maximally conservative τ = 1.0.
    #[test]
    fn conformal_quantile_empty_is_one() {
        let tau = conformal_quantile(&[], 0.1);
        assert_eq!(tau, 1.0);
    }

    // ── Verdict binning ──
    #[test]
    fn verdict_bins_act_reverify_abstain() {
        let row = sample_row(); // tau = 0.6, tau_low = 0.3
        assert_eq!(row.verdict(0.7), VERDICT_ACT);
        assert_eq!(row.verdict(0.6), VERDICT_ACT); // boundary inclusive
        assert_eq!(row.verdict(0.45), VERDICT_REVERIFY);
        assert_eq!(row.verdict(0.3), VERDICT_REVERIFY); // boundary inclusive
        assert_eq!(row.verdict(0.1), VERDICT_ABSTAIN);
    }

    #[test]
    fn verdict_nan_confidence_abstains() {
        let row = sample_row();
        assert_eq!(row.verdict(f32::NAN), VERDICT_ABSTAIN);
        assert_eq!(row.verdict(f32::INFINITY), VERDICT_ABSTAIN);
    }

    // ── Persistence round-trip (mirrors trust.rs save_load_round_trip) ──
    #[test]
    fn save_load_round_trip() {
        let mut table = CalibrationTable::new();
        table.set("predict", sample_row());

        let dir = std::env::temp_dir();
        let path: PathBuf = dir.join(format!("calibration_test_{}.json", std::process::id()));

        save_calibration_state(&table, &path).expect("save failed");
        let loaded = load_calibration_state(&path).expect("load failed");

        assert_eq!(loaded.get("predict"), Some(&sample_row()));
        assert_eq!(loaded.len(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_load_round_trip_creates_parent_directories() {
        let mut table = CalibrationTable::new();
        table.set("predict", sample_row());

        let dir = std::env::temp_dir().join(format!("calibration_nested_{}", std::process::id()));
        let path: PathBuf = dir.join("deep").join("calibration_state.json");

        save_calibration_state(&table, &path).expect("save failed");
        let loaded = load_calibration_state(&path).expect("load failed");
        assert_eq!(loaded.get("predict"), Some(&sample_row()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_is_empty_table() {
        let path = std::env::temp_dir().join("calibration_does_not_exist_xyz.json");
        let _ = std::fs::remove_file(&path);
        let table = load_calibration_state(&path).expect("load failed");
        assert!(table.is_empty());
    }
}
