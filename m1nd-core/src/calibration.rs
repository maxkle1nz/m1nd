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

use crate::error::{M1ndError, M1ndResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

// ── Constants ──

/// Default operator risk budget (target miscoverage). 0.1 = "I will accept at
/// most a 10% error rate among predictions the gate lets through as `act`".
pub const DEFAULT_TARGET_ALPHA: f32 = 0.1;

/// Signal name for the calibrated `predict`/co-change verdict. Stable key into
/// `calibration_state.json` and the contract the harness + predict gate share.
pub const CALIBRATION_SIGNAL_PREDICT: &str = "predict";

/// Signal name for the calibrated trust-envelope verdict (OMEGA Move 1). Sibling
/// of `CALIBRATION_SIGNAL_PREDICT`: the `envelope` row's τ/τ_low bins the
/// weighted trust score into the final `act`/`reverify`/`abstain` decision. No
/// `envelope` row ⇒ the envelope is honestly uncalibrated (capped at `reverify`,
/// `act` unreachable).
pub const CALIBRATION_SIGNAL_ENVELOPE: &str = "envelope";

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
#[serde(deny_unknown_fields)]
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
    /// Validate this row as calibrated evidence for `signal`.
    ///
    /// A row that cannot be represented as a finite bounded calibration result
    /// is not allowed to arm a runtime decision gate.  This is public so wire
    /// projections can apply the exact same fail-closed rule as persistence.
    pub fn validate_for_signal(&self, signal: &str) -> M1ndResult<()> {
        validate_calibration_row(signal, self)
    }

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
#[serde(deny_unknown_fields)]
struct CalibrationPersistenceFormat {
    version: u32,
    rows: BTreeMap<String, CalibrationRow>,
}

const CALIBRATION_PERSISTENCE_VERSION: u32 = 1;

fn validate_calibration_row(signal: &str, row: &CalibrationRow) -> M1ndResult<()> {
    if signal.trim().is_empty() {
        return Err(M1ndError::CorruptState {
            reason: "calibration table contains an empty signal id".into(),
        });
    }
    if !row.tau.is_finite()
        || !row.target_alpha.is_finite()
        || !row.measured_precision.is_finite()
        || !row.coverage.is_finite()
    {
        return Err(M1ndError::CorruptState {
            reason: format!("calibration row '{signal}' contains non-finite fields"),
        });
    }
    if !(0.0..=1.0).contains(&row.tau)
        || !(0.0..=1.0).contains(&row.target_alpha)
        || !(0.0..=1.0).contains(&row.measured_precision)
        || !(0.0..=1.0).contains(&row.coverage)
    {
        return Err(M1ndError::CorruptState {
            reason: format!("calibration row '{signal}' contains values outside [0,1]"),
        });
    }
    if row.n == 0 {
        return Err(M1ndError::CorruptState {
            reason: format!("calibration row '{signal}' has zero labeled samples"),
        });
    }
    Ok(())
}

fn validate_calibration_rows<'a>(
    rows: impl IntoIterator<Item = (&'a str, &'a CalibrationRow)>,
) -> M1ndResult<()> {
    for (signal, row) in rows {
        validate_calibration_row(signal, row)?;
    }
    Ok(())
}

/// Encode the calibration table as deterministic, versioned pretty JSON bytes.
pub fn encode_calibration_state_json(table: &CalibrationTable) -> M1ndResult<Vec<u8>> {
    validate_calibration_rows(
        table
            .rows
            .iter()
            .map(|(signal, row)| (signal.as_str(), row)),
    )?;
    let format = CalibrationPersistenceFormat {
        version: CALIBRATION_PERSISTENCE_VERSION,
        rows: table
            .rows
            .iter()
            .map(|(signal, row)| (signal.clone(), row.clone()))
            .collect(),
    };
    serde_json::to_vec_pretty(&format).map_err(M1ndError::Serde)
}

/// Decode calibration JSON bytes and reject malformed, unsupported, or
/// non-finite state as one fail-closed unit.
pub fn decode_calibration_state_json(bytes: &[u8]) -> M1ndResult<CalibrationTable> {
    let format: CalibrationPersistenceFormat =
        serde_json::from_slice(bytes).map_err(M1ndError::Serde)?;
    if format.version != CALIBRATION_PERSISTENCE_VERSION {
        return Err(M1ndError::CorruptState {
            reason: format!(
                "unsupported calibration persistence version {}",
                format.version
            ),
        });
    }
    validate_calibration_rows(
        format
            .rows
            .iter()
            .map(|(signal, row)| (signal.as_str(), row)),
    )?;
    let table = CalibrationTable {
        rows: format.rows.into_iter().collect(),
    };
    let projected = encode_calibration_state_json(&table)?;
    if serde_json::from_slice::<serde_json::Value>(bytes)?
        != serde_json::from_slice::<serde_json::Value>(&projected)?
    {
        return Err(M1ndError::CorruptState {
            reason: "calibration checkpoint is not the complete current schema".into(),
        });
    }
    Ok(table)
}

/// Persist a `CalibrationTable` to disk using an atomic write (temp + rename).
///
/// # Errors
/// Returns `M1ndError::Serde` on serialisation failure or `M1ndError::Io` on
/// filesystem errors.
pub fn save_calibration_state(table: &CalibrationTable, path: &Path) -> M1ndResult<()> {
    let json = encode_calibration_state_json(table)?;

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

/// Load a `CalibrationTable` from disk, returning an empty table if absent.
///
/// Corrupt rows (non-finite τ) are silently dropped with a diagnostic to stderr.
///
/// # Errors
/// Returns `M1ndError::Io` on read failure, `M1ndError::Serde` on malformed
/// JSON, or `M1ndError::CorruptState` when the persisted schema version is not
/// the exact version this runtime understands.
pub fn load_calibration_state(path: &Path) -> M1ndResult<CalibrationTable> {
    if !path.exists() {
        return Ok(CalibrationTable::new());
    }

    let data = std::fs::read_to_string(path)?;
    let format: CalibrationPersistenceFormat =
        serde_json::from_str(&data).map_err(M1ndError::Serde)?;
    if format.version != CALIBRATION_PERSISTENCE_VERSION {
        return Err(M1ndError::CorruptState {
            reason: format!(
                "unsupported calibration persistence version {}",
                format.version
            ),
        });
    }

    // Preserve the historical disk-loader behavior. Checkpoint restoration
    // uses the strict `decode_calibration_state_json` API instead.
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

    #[test]
    fn disk_loader_refuses_unknown_schema_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("calibration_state.json");
        std::fs::write(&path, br#"{"version":2,"rows":{}}"#).expect("fixture");

        let error = load_calibration_state(&path).expect_err("future schema must fail closed");
        assert!(matches!(error, M1ndError::CorruptState { .. }));
    }

    #[test]
    fn calibration_memory_codec_is_deterministic_and_matches_save() {
        let mut table = CalibrationTable::new();
        table.set("zeta", sample_row());
        table.set("alpha", sample_row());

        let first = encode_calibration_state_json(&table).expect("encode");
        assert_eq!(
            first,
            encode_calibration_state_json(&table).expect("repeat encode")
        );
        let text = std::str::from_utf8(&first).expect("utf8");
        assert!(text.find("alpha").unwrap() < text.find("zeta").unwrap());

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("calibration_state.json");
        save_calibration_state(&table, &path).expect("save");
        assert_eq!(std::fs::read(path).expect("saved bytes"), first);

        let decoded = decode_calibration_state_json(&first).expect("decode");
        assert_eq!(decoded.len(), table.len());
        assert_eq!(decoded.get("alpha"), Some(&sample_row()));
    }

    #[test]
    fn calibration_memory_codec_rejects_corruption_version_and_nonfinite() {
        assert!(decode_calibration_state_json(b"{").is_err());
        assert!(decode_calibration_state_json(br#"{"version":2,"rows":{}}"#).is_err());

        let mut row = sample_row();
        row.coverage = f32::INFINITY;
        let mut table = CalibrationTable::new();
        table.set("bad", row);
        assert!(encode_calibration_state_json(&table).is_err());
    }

    #[test]
    fn calibration_checkpoint_codec_rejects_ranges_empty_ids_and_schema_drift() {
        for (signal, mutate) in [
            ("tau", 1.1_f32),
            ("target_alpha", -0.1_f32),
            ("measured_precision", 1.1_f32),
            ("coverage", -0.1_f32),
        ] {
            let mut row = sample_row();
            match signal {
                "tau" => row.tau = mutate,
                "target_alpha" => row.target_alpha = mutate,
                "measured_precision" => row.measured_precision = mutate,
                "coverage" => row.coverage = mutate,
                _ => unreachable!(),
            }
            let mut table = CalibrationTable::new();
            table.set("bad", row);
            assert!(encode_calibration_state_json(&table).is_err(), "{signal}");
        }

        let mut empty_signal = CalibrationTable::new();
        empty_signal.set(" ", sample_row());
        assert!(encode_calibration_state_json(&empty_signal).is_err());

        let mut zero_samples = CalibrationTable::new();
        let mut zero_sample_row = sample_row();
        zero_sample_row.n = 0;
        zero_samples.set("envelope", zero_sample_row);
        assert!(encode_calibration_state_json(&zero_samples).is_err());

        let mut valid = CalibrationTable::new();
        valid.set("predict", sample_row());
        let mut value: serde_json::Value = serde_json::from_slice(
            &encode_calibration_state_json(&valid).expect("valid checkpoint"),
        )
        .expect("value");
        value["rows"]["predict"]["future_field"] = serde_json::json!(true);
        assert!(
            decode_calibration_state_json(&serde_json::to_vec_pretty(&value).expect("json"))
                .is_err()
        );
    }
}
