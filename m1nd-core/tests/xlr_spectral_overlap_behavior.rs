//! Locks `AdaptiveXlrEngine::spectral_overlap` bucketed-overlap math (surfaced by the X-RAY coverage sweep):
//! empty-signal immunity, self-overlap == 1.0, disjoint == 0.0, partial in (0,1), result always finite in [0,1].
//!
//! `spectral_overlap` is the spectral-immunity helper inside the adaptive
//! differential pass. It buckets hot/cold frequency sets into `SPECTRAL_BUCKETS`
//! bins of width `10.0 / SPECTRAL_BUCKETS` and returns
//! `sum(min(hot_bucket, cold_bucket)) / sum(hot_bucket)`. These tests pin the
//! arithmetic so a future refactor of the bucketing cannot silently shift the
//! overlap score that downstream gating depends on.
//!
//! Notes on the buckets used below (`bucket_width = 10.0 / 20 = 0.5`):
//! - `0.10` and `0.20` both land in bucket 0 (`floor(f / 0.5) == 0`).
//! - `1.10` lands in bucket 2 (`floor(1.10 / 0.5) == 2`).
//! - `2.60` lands in bucket 5 (`floor(2.60 / 0.5) == 5`).
//!
//! The bucket width mirrors the source (`10.0 / SPECTRAL_BUCKETS`); these
//! midpoint frequencies are chosen to sit safely inside their buckets at the
//! current bucket count, avoiding rounding ambiguity at boundaries.

use m1nd_core::types::FiniteF32;
use m1nd_core::xlr::{AdaptiveXlrEngine, SPECTRAL_BUCKETS};

/// Width of a single spectral bucket, mirroring the helper's own derivation.
const BUCKET_WIDTH: f32 = 10.0 / SPECTRAL_BUCKETS as f32;

/// Build a frequency whose bucket index is exactly `bucket`, placed at the
/// bucket's midpoint so float rounding cannot drift it into a neighbor.
fn freq_in_bucket(bucket: usize) -> FiniteF32 {
    let midpoint = (bucket as f32 + 0.5) * BUCKET_WIDTH;
    FiniteF32::new(midpoint)
}

#[test]
fn empty_hot_returns_zero() {
    let cold = vec![freq_in_bucket(0), freq_in_bucket(2)];
    let overlap = AdaptiveXlrEngine::spectral_overlap(&[], &cold);
    assert_eq!(
        overlap,
        FiniteF32::ZERO,
        "empty hot signal must yield zero overlap (spectral immunity)"
    );
}

#[test]
fn empty_cold_returns_zero() {
    let hot = vec![freq_in_bucket(0), freq_in_bucket(2)];
    let overlap = AdaptiveXlrEngine::spectral_overlap(&hot, &[]);
    assert_eq!(
        overlap,
        FiniteF32::ZERO,
        "empty cold signal must yield zero overlap (spectral immunity)"
    );
}

#[test]
fn both_empty_returns_zero() {
    let overlap = AdaptiveXlrEngine::spectral_overlap(&[], &[]);
    assert_eq!(
        overlap,
        FiniteF32::ZERO,
        "two empty signals must yield zero overlap"
    );
}

#[test]
fn identical_frequency_sets_overlap_is_one() {
    let freqs = vec![
        freq_in_bucket(0),
        freq_in_bucket(2),
        freq_in_bucket(5),
        freq_in_bucket(2),
    ];
    let overlap = AdaptiveXlrEngine::spectral_overlap(&freqs, &freqs);
    assert_eq!(
        overlap,
        FiniteF32::ONE,
        "identical hot/cold frequency sets must overlap fully (== 1.0)"
    );
}

#[test]
fn fully_disjoint_buckets_overlap_is_zero() {
    // Hot occupies buckets {0}, cold occupies buckets {2, 5}: no shared bucket.
    let hot = vec![freq_in_bucket(0)];
    let cold = vec![freq_in_bucket(2), freq_in_bucket(5)];
    let overlap = AdaptiveXlrEngine::spectral_overlap(&hot, &cold);
    assert_eq!(
        overlap,
        FiniteF32::ZERO,
        "disjoint frequency buckets must produce zero overlap"
    );
}

#[test]
fn partial_overlap_is_strictly_between_zero_and_one() {
    // Hot occupies buckets {0, 2}; cold occupies bucket {0}.
    // min per bucket: bucket0 -> min(1,1)=1, bucket2 -> min(1,0)=0.
    // hot_total = 2 -> overlap = 1 / 2 = 0.5.
    let hot = vec![freq_in_bucket(0), freq_in_bucket(2)];
    let cold = vec![freq_in_bucket(0)];
    let overlap = AdaptiveXlrEngine::spectral_overlap(&hot, &cold).get();
    assert!(
        overlap > 0.0 && overlap < 1.0,
        "partial overlap must lie strictly in (0,1), got {overlap}"
    );
    assert!(
        (overlap - 0.5).abs() < 1e-6,
        "expected exactly 0.5 for one-of-two shared buckets, got {overlap}"
    );
}

#[test]
fn overlap_is_normalized_by_hot_total_not_cold() {
    // Hot has a single frequency in bucket 0; cold floods bucket 0 with many.
    // overlap = min(1, 3) / 1 = 1.0 -> denominator is hot_total, capped at 1.0.
    let hot = vec![freq_in_bucket(0)];
    let cold = vec![freq_in_bucket(0), freq_in_bucket(0), freq_in_bucket(0)];
    let overlap = AdaptiveXlrEngine::spectral_overlap(&hot, &cold);
    assert_eq!(
        overlap,
        FiniteF32::ONE,
        "overlap normalizes by hot_total; a hot bucket fully covered by cold scores 1.0"
    );
}

#[test]
fn result_is_always_finite_and_in_unit_interval() {
    // A spread of cases, including frequencies above the 10.0 bucketing ceiling
    // which must clamp into the last bucket rather than escape the range.
    let cases: Vec<(Vec<FiniteF32>, Vec<FiniteF32>)> = vec![
        (vec![freq_in_bucket(0)], vec![freq_in_bucket(0)]),
        (
            vec![freq_in_bucket(0), freq_in_bucket(5)],
            vec![freq_in_bucket(5)],
        ),
        (
            vec![FiniteF32::new(50.0), FiniteF32::new(99.0)],
            vec![FiniteF32::new(123.0)],
        ),
        (vec![FiniteF32::ZERO], vec![FiniteF32::ZERO]),
    ];
    for (hot, cold) in &cases {
        let value = AdaptiveXlrEngine::spectral_overlap(hot, cold).get();
        assert!(
            value.is_finite(),
            "overlap must always be finite, got {value} for hot={hot:?} cold={cold:?}"
        );
        assert!(
            (0.0..=1.0).contains(&value),
            "overlap must stay in [0,1], got {value} for hot={hot:?} cold={cold:?}"
        );
    }
}

#[test]
fn frequencies_above_ceiling_clamp_into_last_bucket() {
    // Both far above the 10.0 ceiling -> both clamp into bucket SPECTRAL_BUCKETS-1,
    // making them identical from the bucketing's point of view -> full overlap.
    let hot = vec![FiniteF32::new(42.0)];
    let cold = vec![FiniteF32::new(999.0)];
    let overlap = AdaptiveXlrEngine::spectral_overlap(&hot, &cold);
    assert_eq!(
        overlap,
        FiniteF32::ONE,
        "over-ceiling frequencies clamp into the final bucket and overlap fully"
    );
}
