// === m1nd-mcp/src/trust_envelope.rs ===
//
// OMEGA Move 1 — the TRUST-GATED ANSWER ENVELOPE (first slice, on `seek`).
//
// Wraps every `seek` answer in a per-answer trust RECEIPT so an agent can
// mechanically decide to ACT on it, re-verify it, abstain, or treat it as
// unprovable — in one round-trip, without re-deriving the evidence by hand.
//
// The verdict is a CALIBRATED WEIGHTING over the available trust factors, NOT an
// any-red AND-fold. The critic's hard rule (§O.3 #1 / §O.10 Move 1): an AND over
// noisy probes spuriously abstains ~23% of the time, so agents route around the
// gate and the moat dies. Here a single red factor never forces abstention if
// the weighted majority is clean.
//
// REUSE-FIRST: the final decision reuses `calibration::verdict_for` (the same
// conformal τ/τ_low binning `predict` uses) over the `envelope` calibration row.
// The only net-new code is the two band→reliability maps and the ~one pure
// weighting function below — no new dependency, no new engine.
//
// HONESTY INVARIANTS (all enforced + unit-tested):
//   * `known:false` factors drop from BOTH numerator and denominator — never
//     counted as a pass OR a fail.
//   * A single red factor must NOT force `abstain` when the weighted majority is
//     clean (the anti-AND property).
//   * No `envelope` calibration row ⇒ verdict capped at `reverify`, `act`
//     UNREACHABLE, `calibrated:false`.
//   * ALL factors unknown (or a zero/degenerate weighted denominator, or a
//     non-finite score) ⇒ `unprovable` — never a fake number, never `act`.

use crate::protocol::layers::{TrustEnvelope, TrustFactor};
use m1nd_core::calibration::{self, CalibrationRow};

/// Verdict string for the honest "no provable signal" state. Sibling of
/// calibration's `VERDICT_ACT`/`VERDICT_REVERIFY`/`VERDICT_ABSTAIN`; the envelope
/// adds this fourth state for "nothing here is even measurable".
pub const VERDICT_UNPROVABLE: &str = "unprovable";

/// Named factors, so the composition site and the maps agree on stable keys.
pub const FACTOR_TRUST_BAND: &str = "trust_band";
pub const FACTOR_BINDING: &str = "binding";

// Default per-factor weights for slice 1. Stored as constants here but consumed
// through the weighting fn; a later move can lift these into the calibration
// table without touching the weighting math.
pub const WEIGHT_TRUST_BAND: f32 = 1.0;
pub const WEIGHT_BINDING: f32 = 1.0;

/// One composed factor fed to the pure weighting function.
///
/// `reliability` is the band already mapped to a [0,1] reliability value (higher
/// = more trustworthy). It is only consulted when `known` is true.
#[derive(Clone, Debug)]
pub struct FactorInput {
    pub name: String,
    pub band: String,
    pub weight: f32,
    pub known: bool,
    /// Reliability in [0,1]; ignored when `known == false`.
    pub reliability: f32,
    /// Optional repair call to surface if this factor drags the verdict off act.
    pub repair_hint: Option<&'static str>,
}

impl FactorInput {
    fn to_factor(&self) -> TrustFactor {
        TrustFactor {
            name: self.name.clone(),
            band: self.band.clone(),
            weight: self.weight,
            known: self.known,
        }
    }
}

/// Map a `trust_band` string (from `m1nd_core::trust::trust_band`) to a
/// reliability in [0,1].
///
/// NOTE the band vocabulary is RISK-named: "high" means HIGH RISK ⇒ LOW
/// reliability. `insufficient_evidence` is the cold-start band — there is no
/// evidence to weigh, so it is reported to the caller as `known:false` (handled
/// at the composition site), never mapped to a middle number.
pub fn trust_band_reliability(band: &str) -> Option<f32> {
    match band {
        "high" => Some(0.2),             // high risk  → low reliability
        "medium" => Some(0.5),           // medium risk
        "low" => Some(0.8),              // low risk    → high reliability
        "insufficient_evidence" => None, // cold start → known:false
        _ => None,                       // unknown vocabulary → drop honestly
    }
}

/// Map a lightweight trust-mode band (derived from cheap in-memory binding reads)
/// to a reliability in [0,1].
pub fn binding_reliability(band: &str) -> Option<f32> {
    match band {
        "full_trust" => Some(1.0),
        "needs_ingest" | "orientation_only" => Some(0.4),
        "stale_binding_suspected" | "degraded" => Some(0.15),
        _ => None,
    }
}

/// Pure, deterministic weighting: fold the composed factors into a calibrated
/// [`TrustEnvelope`]. No I/O, no clock, no state — fully unit-testable.
///
/// score = Σ_i (w_i · v_i · known_i) / Σ_i (w_i · known_i)
///
/// with `v_i` the per-factor reliability in [0,1]. Unknown factors contribute to
/// NEITHER sum. The verdict then bins `score` through the `envelope` calibration
/// row's τ/τ_low (`calibration::verdict_for`) — a weighted DECISION, not a
/// conjunction. See the module honesty invariants for the degradation rules.
pub fn weigh_factors(factors: &[FactorInput], cal_row: Option<&CalibrationRow>) -> TrustEnvelope {
    let factor_receipts: Vec<TrustFactor> = factors.iter().map(FactorInput::to_factor).collect();

    // Accumulate ONLY the known factors into the weighted score. Unknown factors
    // never touch either sum (the honest UNPROVABLE-per-factor invariant).
    let mut numerator = 0.0f32;
    let mut denominator = 0.0f32;
    let mut known_count = 0usize;
    for f in factors {
        if !f.known {
            continue;
        }
        // A negative / non-finite weight or reliability is not evidence — skip it
        // rather than let it poison the fold into a NaN/negative score.
        if !f.weight.is_finite() || f.weight <= 0.0 || !f.reliability.is_finite() {
            continue;
        }
        let v = f.reliability.clamp(0.0, 1.0);
        numerator += f.weight * v;
        denominator += f.weight;
        known_count += 1;
    }

    // ALL-UNKNOWN or degenerate denominator ⇒ honestly `unprovable` (never a fake
    // number, never `act`). This also catches an all-zero-weight known set.
    if known_count == 0 || denominator <= f32::EPSILON {
        return TrustEnvelope {
            verdict: VERDICT_UNPROVABLE.to_string(),
            score: 0.0,
            calibrated: cal_row.is_some(),
            factors: factor_receipts,
            reasons: vec![
                "no provable trust factor was available on this path — the answer is UNPROVABLE, not trusted; re-verify against local files".to_string(),
            ],
            next_repair_call: first_repair_hint(factors),
        };
    }

    let score = numerator / denominator;
    // NaN/non-finite guard (belt-and-suspenders — denominator is already > EPS,
    // but division could still surprise on subnormal inputs). Honest unprovable.
    if !score.is_finite() {
        return TrustEnvelope {
            verdict: VERDICT_UNPROVABLE.to_string(),
            score: 0.0,
            calibrated: cal_row.is_some(),
            factors: factor_receipts,
            reasons: vec![
                "the weighted trust score was non-finite — reporting UNPROVABLE rather than a fabricated verdict".to_string(),
            ],
            next_repair_call: first_repair_hint(factors),
        };
    }

    // Decide. With a calibration row, bin the score through the SAME conformal
    // τ/τ_low `predict` uses. WITHOUT a row, the envelope is honestly
    // uncalibrated: `act` is UNREACHABLE and the verdict is capped at `reverify`
    // (softened from predict's None→abstain, because some factors ARE known here).
    let (verdict, calibrated) = match cal_row {
        Some(row) => (
            calibration::verdict_for(score, row.tau, row.tau_low()).to_string(),
            true,
        ),
        None => (calibration::VERDICT_REVERIFY.to_string(), false),
    };

    let mut reasons = Vec::new();
    match calibrated {
        true => reasons.push(format!(
            "weighted trust score {score:.2} over {known_count} known factor(s), binned by the calibrated `envelope` threshold → {verdict}"
        )),
        false => reasons.push(format!(
            "weighted trust score {score:.2} over {known_count} known factor(s), but the `envelope` signal is UNCALIBRATED — `act` is unreachable and the verdict is capped at `reverify` until a calibration row is measured"
        )),
    }
    // Name any known factor that fell below a middling reliability so the agent
    // sees WHY the verdict is not `act`, not just THAT it is not.
    for f in factors {
        if f.known && f.reliability.is_finite() && f.reliability < 0.5 {
            reasons.push(format!(
                "factor `{}` is weak (band `{}`, reliability {:.2})",
                f.name, f.band, f.reliability
            ));
        }
    }
    // Name the deferred (unknown) factors honestly.
    for f in factors {
        if !f.known {
            reasons.push(format!("factor `{}` deferred ({})", f.name, f.band));
        }
    }

    // A repair call is meaningful only when the verdict is not `act`. Prefer the
    // hint from the weakest known factor; fall back to the first available hint.
    let next_repair_call = if verdict == calibration::VERDICT_ACT {
        None
    } else {
        weakest_repair_hint(factors).or_else(|| first_repair_hint(factors))
    };

    TrustEnvelope {
        verdict,
        score,
        calibrated,
        factors: factor_receipts,
        reasons,
        next_repair_call,
    }
}

/// Derive a lightweight trust-mode band from the cheap, in-memory binding reads
/// available inside `seek` (no re-hash, no file I/O).
///
/// This is the honest CHEAP SUBSET of `handle_session_handshake`'s trust_mode
/// classification (tools.rs): the handshake also folds host-tool-surface and
/// workspace-mismatch signals, which are NOT observable from inside seek — so we
/// only classify what the in-memory graph state actually PROVES:
///   * empty / unfinalized graph (nothing ingested) → `needs_ingest`
///   * a real, finalized, populated graph            → `full_trust`
///
/// The unobservable degradations (degraded host surface, wrong workspace,
/// content poisoning) are left to the full handshake / `trust_selftest`; this
/// never *fakes* `full_trust` when the cheap reads say the graph is empty.
///
/// DELIBERATELY does NOT use graph-file existence as a staleness signal: a
/// freshly-bound, populated, in-memory graph that has not yet been persisted has
/// no backing file, and treating that as `stale_binding_suspected` would fire a
/// FALSE alarm on the normal path. The `stale_binding_suspected`/`degraded`
/// bands still exist in `binding_reliability` for callers that CAN prove them
/// (e.g. the handshake); the cheap seek subset simply refuses to guess them.
pub fn cheap_trust_mode_band(node_count: u64, edge_count: u64, finalized: bool) -> &'static str {
    if node_count == 0 || edge_count == 0 || !finalized {
        "needs_ingest"
    } else {
        "full_trust"
    }
}

/// Compose the slice-1 trust factors for a `seek` answer from the cheap/available
/// signals, then fold them into a [`TrustEnvelope`] via [`weigh_factors`].
///
/// KNOWN (composed here):
///   * `trust_band` — the worst trust band across the top results (already
///     computed per-result). `insufficient_evidence`/absent ⇒ `known:false`.
///   * `binding`    — the cheap in-memory trust-mode band (see above).
///
/// DEFERRED (honest `known:false`, each with a reason naming the probe that
/// would produce it — NOT faked): `cross_verify` evidence-freshness (ingest-only,
/// structurally unavailable in seek), `am_i_stale` (per-file I/O), `closure`
/// (built only in why/impact), `mission_verify` evidence-class (needs an open
/// mission).
pub fn compose_seek_trust_envelope(
    top_trust_bands: &[String],
    binding_band: &str,
    cal_row: Option<&CalibrationRow>,
) -> TrustEnvelope {
    let mut factors: Vec<FactorInput> = Vec::new();

    // Factor: trust_band — worst-of-top band (most conservative). A "high" band
    // is HIGH RISK ⇒ low reliability, so "worst" = the band with the LOWEST
    // reliability. `insufficient_evidence`/unmappable ⇒ known:false.
    let worst = worst_trust_band(top_trust_bands);
    match worst.as_deref().and_then(trust_band_reliability) {
        Some(reliability) => factors.push(FactorInput {
            name: FACTOR_TRUST_BAND.to_string(),
            band: worst.unwrap_or_default(),
            weight: WEIGHT_TRUST_BAND,
            known: true,
            reliability,
            repair_hint: Some("cross_verify"),
        }),
        None => factors.push(FactorInput {
            name: FACTOR_TRUST_BAND.to_string(),
            band: worst
                .map(|b| format!("deferred: {b}"))
                .unwrap_or_else(|| "deferred: no results to band".to_string()),
            weight: WEIGHT_TRUST_BAND,
            known: false,
            reliability: 0.0,
            repair_hint: Some("cross_verify"),
        }),
    }

    // Factor: binding — the cheap in-memory trust-mode band.
    match binding_reliability(binding_band) {
        Some(reliability) => factors.push(FactorInput {
            name: FACTOR_BINDING.to_string(),
            band: binding_band.to_string(),
            weight: WEIGHT_BINDING,
            known: true,
            reliability,
            repair_hint: Some(binding_repair(binding_band)),
        }),
        None => factors.push(FactorInput {
            name: FACTOR_BINDING.to_string(),
            band: format!("deferred: {binding_band}"),
            weight: WEIGHT_BINDING,
            known: false,
            reliability: 0.0,
            repair_hint: Some("trust_selftest"),
        }),
    }

    // Deferred factors — structurally unavailable inside seek. Marked known:false
    // with a reason naming the probe; they touch neither sum.
    for (name, probe) in DEFERRED_FACTORS {
        factors.push(FactorInput {
            name: (*name).to_string(),
            band: format!("deferred: {probe}"),
            weight: 1.0,
            known: false,
            reliability: 0.0,
            repair_hint: None,
        });
    }

    weigh_factors(&factors, cal_row)
}

/// Deferred (structurally-unavailable-in-seek) factors and the probe that would
/// make each provable. Kept honest: named, not faked.
const DEFERRED_FACTORS: &[(&str, &str)] = &[
    (
        "evidence_freshness",
        "cross_verify (ingest-only, unavailable in seek)",
    ),
    ("am_i_stale", "am_i_stale (per-file I/O)"),
    ("closure", "why/impact closure (not built in seek)"),
    ("evidence_class", "mission_verify (needs an open mission)"),
];

/// The most conservative (lowest-reliability) trust band among the top results.
/// `None` when there are no bands. "high" is HIGH RISK, so it is the worst.
fn worst_trust_band(bands: &[String]) -> Option<String> {
    // Rank by risk: higher rank = worse (lower reliability). Unknown vocab and
    // `insufficient_evidence` rank between medium and high so an absent signal
    // does not falsely look safe; but if EVERY band is unmappable the factor ends
    // up known:false at the composition site anyway.
    fn risk_rank(band: &str) -> u8 {
        match band {
            "low" => 0,
            "medium" => 1,
            "insufficient_evidence" => 2,
            "high" => 3,
            _ => 2,
        }
    }
    bands.iter().max_by_key(|b| risk_rank(b)).cloned()
}

/// The repair call that best addresses a degraded binding band.
fn binding_repair(band: &str) -> &'static str {
    match band {
        "stale_binding_suspected" | "degraded" => "recovery_playbook",
        "needs_ingest" => "ingest",
        "orientation_only" => "trust_selftest",
        _ => "trust_selftest",
    }
}

/// The repair hint of the weakest known factor (lowest reliability), if any.
fn weakest_repair_hint(factors: &[FactorInput]) -> Option<String> {
    factors
        .iter()
        .filter(|f| f.known && f.reliability.is_finite() && f.repair_hint.is_some())
        .min_by(|a, b| {
            a.reliability
                .partial_cmp(&b.reliability)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .and_then(|f| f.repair_hint.map(str::to_string))
}

/// The first available repair hint across all factors (known or not), if any.
fn first_repair_hint(factors: &[FactorInput]) -> Option<String> {
    factors
        .iter()
        .find_map(|f| f.repair_hint.map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A calibration row with tau=0.6 ⇒ tau_low=0.3. Mirrors calibration.rs's
    // `sample_row`: act ≥ 0.6, reverify in [0.3, 0.6), abstain < 0.3.
    fn cal_row() -> CalibrationRow {
        CalibrationRow {
            tau: 0.6,
            target_alpha: calibration::DEFAULT_TARGET_ALPHA,
            measured_precision: 0.85,
            coverage: 0.4,
            n: 100,
            calibrated_at_ms: 1_700_000_000_000,
        }
    }

    fn known(name: &str, reliability: f32, weight: f32) -> FactorInput {
        FactorInput {
            name: name.to_string(),
            band: "band".to_string(),
            weight,
            known: true,
            reliability,
            repair_hint: Some("cross_verify"),
        }
    }

    fn unknown(name: &str) -> FactorInput {
        FactorInput {
            name: name.to_string(),
            band: "deferred: probe".to_string(),
            weight: WEIGHT_TRUST_BAND,
            known: false,
            reliability: 0.0,
            repair_hint: None,
        }
    }

    // ── Exact weighting oracle: fixed factors → EXACT score → EXACT verdict ──
    // Two equal-weight known factors at reliability 0.8 and 1.0:
    // score = (1·0.8 + 1·1.0) / (1 + 1) = 1.8 / 2 = 0.90. 0.90 ≥ τ(0.6) ⇒ act.
    #[test]
    fn exact_score_and_act_verdict() {
        let factors = [known("binding", 1.0, 1.0), known("trust_band", 0.8, 1.0)];
        let env = weigh_factors(&factors, Some(&cal_row()));
        assert_eq!(env.score, 0.90, "exact weighted score");
        assert_eq!(env.verdict, "act");
        assert!(env.calibrated);
        assert!(env.next_repair_call.is_none(), "act ⇒ no repair call");
    }

    // ── Unknown factors drop out of BOTH sums ──
    // known-clean(0.8) + one-unknown MUST equal known-clean(0.8) alone: same
    // score, same verdict. The unknown touches neither numerator nor denominator.
    #[test]
    fn unknown_factor_drops_out_of_both_sums() {
        let clean_only = [known("trust_band", 0.8, 1.0)];
        let clean_plus_unknown = [known("trust_band", 0.8, 1.0), unknown("closure")];

        let a = weigh_factors(&clean_only, Some(&cal_row()));
        let b = weigh_factors(&clean_plus_unknown, Some(&cal_row()));

        assert_eq!(a.score, b.score, "unknown factor must not move the score");
        assert_eq!(a.verdict, b.verdict);
        assert_eq!(a.score, 0.80);
    }

    // ── ANTI-AND: a single red factor must NOT force abstain when the weighted
    //    majority is clean. THREE clean factors (1.0) + ONE red (0.05):
    // score = (1.0+1.0+1.0+0.05)/4 = 3.05/4 = 0.7625 ≥ τ(0.6) ⇒ act (NOT abstain).
    // An any-red AND-fold would have abstained here; the calibrated weighting acts.
    #[test]
    fn single_red_factor_does_not_force_abstain() {
        let factors = [
            known("a", 1.0, 1.0),
            known("b", 1.0, 1.0),
            known("c", 1.0, 1.0),
            known("red", 0.05, 1.0),
        ];
        let env = weigh_factors(&factors, Some(&cal_row()));
        assert!(
            (env.score - 0.7625).abs() < 1e-6,
            "weighted score should be 0.7625, got {}",
            env.score
        );
        assert_eq!(
            env.verdict, "act",
            "one red factor must NOT force abstain when the weighted majority is clean"
        );
    }

    // ── No calibration row ⇒ NEVER act; capped at reverify; calibrated:false. ──
    // Even an all-clean 1.0 score cannot reach `act` without a measured row.
    #[test]
    fn no_calibration_row_caps_at_reverify_never_act() {
        let factors = [known("binding", 1.0, 1.0), known("trust_band", 1.0, 1.0)];
        let env = weigh_factors(&factors, None);
        assert_eq!(env.score, 1.0, "score still computed");
        assert_eq!(env.verdict, "reverify", "uncalibrated ⇒ capped at reverify");
        assert!(!env.calibrated);
        assert_ne!(env.verdict, "act", "act is UNREACHABLE without calibration");
        assert!(env.next_repair_call.is_some());
    }

    // ── All factors unknown ⇒ unprovable (never a fake number, never act). ──
    #[test]
    fn all_unknown_is_unprovable() {
        let factors = [unknown("trust_band"), unknown("binding")];
        let env = weigh_factors(&factors, Some(&cal_row()));
        assert_eq!(env.verdict, "unprovable");
        assert_eq!(env.score, 0.0);
        assert_ne!(env.verdict, "act");
    }

    // Empty factor set ⇒ unprovable, no divide-by-zero.
    #[test]
    fn empty_factor_set_is_unprovable() {
        let env = weigh_factors(&[], Some(&cal_row()));
        assert_eq!(env.verdict, "unprovable");
        assert_eq!(env.score, 0.0);
    }

    // NaN / non-finite reliability on the only factor ⇒ that factor is skipped ⇒
    // no known factor remains ⇒ unprovable, never a NaN score.
    #[test]
    fn nan_reliability_yields_unprovable_not_nan() {
        let mut f = known("trust_band", f32::NAN, 1.0);
        f.known = true;
        let env = weigh_factors(&[f], Some(&cal_row()));
        assert_eq!(env.verdict, "unprovable");
        assert!(env.score.is_finite(), "score must never be NaN");
    }

    // Zero / non-finite weight on the only known factor ⇒ skipped ⇒ unprovable.
    #[test]
    fn zero_weight_factor_is_dropped() {
        let env = weigh_factors(&[known("trust_band", 0.9, 0.0)], Some(&cal_row()));
        assert_eq!(env.verdict, "unprovable");
    }

    // A genuinely weak weighted score bins to abstain (not act) — the honest
    // low end still works. score = 0.10 < τ_low(0.3) ⇒ abstain, with a repair.
    #[test]
    fn weak_score_abstains_with_repair() {
        let env = weigh_factors(&[known("binding", 0.10, 1.0)], Some(&cal_row()));
        assert_eq!(env.verdict, "abstain");
        assert!(env.next_repair_call.is_some());
    }

    // Borderline score bins to reverify. score = 0.45 ∈ [0.3, 0.6) ⇒ reverify.
    #[test]
    fn borderline_score_reverifies() {
        let env = weigh_factors(&[known("binding", 0.45, 1.0)], Some(&cal_row()));
        assert_eq!(env.verdict, "reverify");
        assert!(env.next_repair_call.is_some());
    }

    // Band → reliability maps are exact and risk-aware.
    #[test]
    fn band_maps_are_exact() {
        assert_eq!(trust_band_reliability("high"), Some(0.2));
        assert_eq!(trust_band_reliability("medium"), Some(0.5));
        assert_eq!(trust_band_reliability("low"), Some(0.8));
        assert_eq!(trust_band_reliability("insufficient_evidence"), None);
        assert_eq!(trust_band_reliability("garbage"), None);

        assert_eq!(binding_reliability("full_trust"), Some(1.0));
        assert_eq!(binding_reliability("needs_ingest"), Some(0.4));
        assert_eq!(binding_reliability("orientation_only"), Some(0.4));
        assert_eq!(binding_reliability("stale_binding_suspected"), Some(0.15));
        assert_eq!(binding_reliability("degraded"), Some(0.15));
        assert_eq!(binding_reliability("garbage"), None);
    }
}
