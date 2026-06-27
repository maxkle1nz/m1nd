use crate::protocol::layers::SeekResultEntry;
use crate::protocol::{ActivatedNodeOutput, SeedOutput};
use std::collections::HashSet;

const CLOSE_SCORE_EPS: f32 = 0.05;

pub trait RankedResult: Clone {
    fn score(&self) -> f32;
    fn specificity(&self) -> f32 {
        0.0
    }
    fn family_key(&self) -> String;
}

pub fn dedupe_ranked<T: RankedResult>(mut items: Vec<T>, top_k: usize) -> Vec<T> {
    // Rank by a single blended key: the score nudged by a bounded specificity
    // bonus. Within a near-tie the more specific hit wins (a specific symbol over
    // a generic crate node). The bonus is clamped to ±CLOSE_SCORE_EPS, so the
    // most specificity can shift a key is one eps; two items can differ by up to
    // 2·eps from specificity alone, and any score gap wider than that always
    // wins on score.
    //
    // This MUST be a TOTAL ORDER. The previous "abs(a-b) <= eps ? by spec : by
    // score" switch was intransitive — for A≈B, B≈C, A≠C it compared the three
    // pairs by different keys and could form a cycle (A>B>C>A). That is
    // undefined behaviour for a comparator, and modern Rust's sort PANICS when
    // it detects the violation ("comparison function does not implement a total
    // order") — which started firing once `embed` (default) clustered scores
    // into near-ties on real graphs. A single scalar key removes the branch and
    // the intransitivity entirely. Non-finite score OR specificity carries no
    // signal and sorts last.
    const SPEC_WEIGHT: f32 = 0.02;
    fn rank_key<T: RankedResult>(item: &T) -> f32 {
        let score = item.score();
        if !score.is_finite() {
            return f32::NEG_INFINITY;
        }
        let spec = item.specificity();
        let bonus = if spec.is_finite() {
            (spec * SPEC_WEIGHT).clamp(-CLOSE_SCORE_EPS, CLOSE_SCORE_EPS)
        } else {
            0.0
        };
        score + bonus
    }
    items.sort_by(|a, b| {
        rank_key(b)
            .total_cmp(&rank_key(a))
            .then_with(|| b.score().total_cmp(&a.score()))
            .then_with(|| a.family_key().cmp(&b.family_key()))
    });

    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let key = item.family_key();
        if seen.insert(key) {
            out.push(item);
            if out.len() >= top_k {
                break;
            }
        }
    }
    out
}

/// Greedily pack a *pre-ranked* list into a token budget, keeping the
/// highest-signal items first.
///
/// `ranked` MUST already be sorted best-first (e.g. the output of
/// [`dedupe_ranked`] and any `top_k` truncation) — this function does NOT
/// re-rank. It walks the list in order, accumulating each item's estimated
/// token cost (via `est`), and stops as soon as the *next* item would push the
/// running total past `budget_tokens`. At least one item is always kept so a
/// tiny budget still returns the single top hit (even if that one item alone
/// exceeds the budget — the "single-item overflow" case).
///
/// Returns `(kept, dropped_count)` where `dropped_count == original_len -
/// kept.len()`.
pub fn pack_to_budget<T>(
    ranked: Vec<T>,
    budget_tokens: usize,
    est: impl Fn(&T) -> usize,
) -> (Vec<T>, usize) {
    let original_len = ranked.len();
    let mut kept: Vec<T> = Vec::with_capacity(original_len);
    let mut used = 0usize;

    for item in ranked {
        let cost = est(&item);
        // Always keep the first (top-ranked) item, even if it alone overflows.
        if kept.is_empty() {
            kept.push(item);
            used = used.saturating_add(cost);
            continue;
        }
        if used.saturating_add(cost) > budget_tokens {
            break;
        }
        used = used.saturating_add(cost);
        kept.push(item);
    }

    let dropped = original_len - kept.len();
    (kept, dropped)
}

/// Rough, deterministic token-count ESTIMATE for a string of serialized result
/// text. This is the widely-used `chars / 4` heuristic — it is NOT real
/// tokenization (no BPE/tiktoken), so the true token count for any given model
/// may differ by a meaningful margin. It exists only to let the budget packer
/// rank/threshold consistently. We round up so a non-empty string never
/// estimates as zero tokens.
pub fn estimate_tokens_from_chars(chars: usize) -> usize {
    if chars == 0 {
        0
    } else {
        chars.div_ceil(4)
    }
}

/// Build the honest `budget` accounting block attached to budgeted retrieval
/// results. `requested` is the agent's declared token budget, `used` is the
/// summed per-item ESTIMATE of the kept items, `kept`/`dropped` are the
/// post-packing counts. The note is phrased for an agent reader.
pub fn budget_block(
    requested: usize,
    used: usize,
    kept: usize,
    dropped: usize,
) -> serde_json::Value {
    let note = if dropped == 0 {
        format!(
            "kept all {kept} hits; estimated ~{used} tokens, within the ~{requested} token budget"
        )
    } else {
        format!(
            "kept the {kept} highest-signal hits; dropped {dropped} lower-ranked to fit ~{requested} tokens"
        )
    };
    serde_json::json!({
        "requested_tokens": requested,
        "estimated_used_tokens": used,
        "kept": kept,
        "dropped": dropped,
        "note": note,
    })
}

fn normalize_label(label: &str) -> String {
    label.trim().to_lowercase()
}

fn is_crate_like(source_path: Option<&str>) -> bool {
    source_path
        .map(|path| path.to_lowercase().contains("cargo.toml"))
        .unwrap_or(false)
}

fn label_specificity(label: &str, node_type: &str, source_path: Option<&str>) -> f32 {
    let mut score = 0.0f32;
    let label_lower = label.trim().to_lowercase();
    let node_type_lower = node_type.to_lowercase();
    let source_path_lower = source_path.unwrap_or("").to_lowercase();

    if label_lower.starts_with("impl ") {
        score += 3.0;
    }

    score += match node_type_lower.as_str() {
        "function" => 2.0,
        "struct" | "type" | "enum" => 1.9,
        "module" => 1.1,
        "file" => 0.6,
        "directory" => 0.1,
        _ => 0.4,
    };

    if source_path_lower.contains("/src/") || source_path_lower.contains("/tests/") {
        score += 0.5;
    }
    if source_path_lower.contains("/examples/") || source_path_lower.contains("/benches/") {
        score += 0.2;
    }
    if source_path_lower.contains("/docs/")
        || source_path_lower.contains("/wiki/")
        || source_path_lower.contains("readme")
        || source_path_lower.contains("changelog")
        || source_path_lower.contains("tutorial")
    {
        score -= 0.8;
    }
    if is_crate_like(source_path) {
        score -= 1.2;
    }

    score
}

fn impl_family_key(label: &str) -> Option<String> {
    let trimmed = label.trim();
    let rest = trimmed.strip_prefix("impl ")?;
    if let Some((trait_part, _self_part)) = rest.split_once(" for ") {
        Some(format!("impl:{}", trait_part.trim().to_lowercase()))
    } else {
        Some(format!("impl:{}", rest.trim().to_lowercase()))
    }
}

impl RankedResult for SeedOutput {
    fn score(&self) -> f32 {
        self.relevance
    }

    fn family_key(&self) -> String {
        normalize_label(&self.label)
    }
}

impl RankedResult for ActivatedNodeOutput {
    fn score(&self) -> f32 {
        self.activation
    }

    fn specificity(&self) -> f32 {
        label_specificity(
            &self.label,
            &self.node_type,
            self.provenance
                .as_ref()
                .and_then(|p| p.source_path.as_deref()),
        )
    }

    fn family_key(&self) -> String {
        if let Some(key) = impl_family_key(&self.label) {
            key
        } else if is_crate_like(
            self.provenance
                .as_ref()
                .and_then(|p| p.source_path.as_deref()),
        ) {
            format!("crate:{}", normalize_label(&self.label))
        } else {
            normalize_label(&self.label)
        }
    }
}

impl RankedResult for SeekResultEntry {
    fn score(&self) -> f32 {
        self.score
    }

    fn specificity(&self) -> f32 {
        label_specificity(&self.label, &self.node_type, self.file_path.as_deref())
    }

    fn family_key(&self) -> String {
        if let Some(key) = impl_family_key(&self.label) {
            key
        } else if is_crate_like(self.file_path.as_deref()) {
            format!("crate:{}", normalize_label(&self.label))
        } else {
            normalize_label(&self.label)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::layers::{SeekConnection, SeekResultEntry, SeekScoreBreakdown};
    use crate::protocol::{ActivatedNodeOutput, DimensionsOutput, ProvenanceOutput, SeedOutput};

    /// Minimal `RankedResult` with directly-settable score/specificity so we can
    /// reproduce the adversarial ordering that broke the comparator.
    #[derive(Clone)]
    struct Ranked {
        s: f32,
        spec: f32,
        key: String,
    }
    impl RankedResult for Ranked {
        fn score(&self) -> f32 {
            self.s
        }
        fn specificity(&self) -> f32 {
            self.spec
        }
        fn family_key(&self) -> String {
            self.key.clone()
        }
    }

    /// Regression: `dedupe_ranked`'s comparator MUST be a total order. The old
    /// "abs(a-b) <= eps ? by specificity : by score" switch was intransitive —
    /// for a tight score ramp with alternating specificity it forms cycles
    /// (A>B>C>A), which modern Rust's sort detects and PANICS on. This input
    /// reproduces that exact shape; the test passing (no panic) is the guard.
    #[test]
    fn dedupe_ranked_is_a_total_order_under_near_tie_ramp() {
        // The minimal cycle the old comparator produced: scores 1.00/1.03/1.06
        // (adjacent within eps=0.05, ends 0.06 apart) with decreasing
        // specificity. Old: A≈B,B≈C by spec but A≠C by score => A>B>C>A.
        let triple = vec![
            Ranked {
                s: 1.00,
                spec: 0.9,
                key: "a".into(),
            },
            Ranked {
                s: 1.03,
                spec: 0.5,
                key: "b".into(),
            },
            Ranked {
                s: 1.06,
                spec: 0.1,
                key: "c".into(),
            },
        ];
        let out = dedupe_ranked(triple, 10);
        assert_eq!(out.len(), 3, "all distinct families survive");

        // A dense ramp (step << eps) with alternating specificity packs many
        // intransitive triples into the same sort — the strongest trigger.
        let ramp: Vec<Ranked> = (0..64)
            .map(|i| Ranked {
                s: 0.40 + i as f32 * 0.012,
                spec: if i % 2 == 0 { 0.9 } else { 0.1 },
                key: format!("k{i}"),
            })
            .collect();
        let sorted = dedupe_ranked(ramp, 64);
        assert_eq!(sorted.len(), 64, "no family collisions, nothing dropped");
        // Completing without panic IS the regression guard — the old comparator
        // aborts on this exact shape.

        // A clearly-higher score (gap > 2*eps) is never overturned by specificity,
        // even when the low scorer is maximally specific and the high one is not.
        let clear = vec![
            Ranked {
                s: 0.50,
                spec: 9.9,
                key: "low_but_specific".into(),
            },
            Ranked {
                s: 0.70,
                spec: -9.9,
                key: "high_but_generic".into(),
            },
        ];
        let out = dedupe_ranked(clear, 10);
        assert_eq!(
            out[0].family_key(),
            "high_but_generic",
            "a clearly-higher score must win regardless of specificity"
        );
    }

    #[test]
    fn dedupe_ranked_prefers_impl_family_over_duplicate_label_hits() {
        let items = vec![
            ActivatedNodeOutput {
                node_id: "a".into(),
                label: "impl Extractor for RustExtractor".into(),
                node_type: "Module".into(),
                activation: 0.80,
                dimensions: DimensionsOutput {
                    structural: 0.0,
                    semantic: 0.0,
                    temporal: 0.0,
                    causal: 0.0,
                },
                pagerank: 0.2,
                tags: vec![],
                provenance: None,
            },
            ActivatedNodeOutput {
                node_id: "b".into(),
                label: "impl Extractor for PythonExtractor".into(),
                node_type: "Module".into(),
                activation: 0.79,
                dimensions: DimensionsOutput {
                    structural: 0.0,
                    semantic: 0.0,
                    temporal: 0.0,
                    causal: 0.0,
                },
                pagerank: 0.2,
                tags: vec![],
                provenance: None,
            },
            ActivatedNodeOutput {
                node_id: "c".into(),
                label: "m1nd-core".into(),
                node_type: "Module".into(),
                activation: 0.78,
                dimensions: DimensionsOutput {
                    structural: 0.0,
                    semantic: 0.0,
                    temporal: 0.0,
                    causal: 0.0,
                },
                pagerank: 0.2,
                tags: vec![],
                provenance: Some(ProvenanceOutput {
                    source_path: Some(
                        "/Users/cosmophonix/SISTEMA/m1nd/m1nd-core/Cargo.toml".into(),
                    ),
                    line_start: None,
                    line_end: None,
                    excerpt: None,
                    namespace: Some("rust:cargo".into()),
                    canonical: true,
                }),
            },
        ];

        let shaped = dedupe_ranked(items, 10);
        assert_eq!(shaped.len(), 2);
        assert_eq!(shaped[0].label, "impl Extractor for RustExtractor");
    }

    #[test]
    fn dedupe_ranked_keeps_unique_seed_labels() {
        let items = vec![
            SeedOutput {
                node_id: "a".into(),
                label: "resolve".into(),
                relevance: 0.9,
            },
            SeedOutput {
                node_id: "b".into(),
                label: "resolve".into(),
                relevance: 0.8,
            },
        ];

        let shaped = dedupe_ranked(items, 10);
        assert_eq!(shaped.len(), 1);
        assert_eq!(shaped[0].label, "resolve");
    }

    #[test]
    fn dedupe_ranked_prefers_specific_results_over_crate_nodes() {
        let items = vec![
            SeekResultEntry {
                node_id: "crate".into(),
                label: "m1nd-core".into(),
                node_type: "module".into(),
                score: 0.78,
                score_breakdown: SeekScoreBreakdown {
                    embedding_similarity: 0.7,
                    graph_activation: 0.1,
                    temporal_recency: 0.0,
                },
                heuristic_signals: None,
                intent_summary: "crate".into(),
                file_path: Some("/Users/cosmophonix/SISTEMA/m1nd/m1nd-core/Cargo.toml".into()),
                line_start: None,
                line_end: None,
                excerpt: None,
                connections: vec![SeekConnection {
                    node_id: "x".into(),
                    label: "x".into(),
                    relation: "imports".into(),
                }],
            },
            SeekResultEntry {
                node_id: "sym".into(),
                label: "impl Extractor for RustExtractor".into(),
                node_type: "module".into(),
                score: 0.77,
                score_breakdown: SeekScoreBreakdown {
                    embedding_similarity: 0.69,
                    graph_activation: 0.11,
                    temporal_recency: 0.0,
                },
                heuristic_signals: None,
                intent_summary: "impl".into(),
                file_path: Some(
                    "/Users/cosmophonix/SISTEMA/m1nd/m1nd-ingest/src/extract/rust_lang.rs".into(),
                ),
                line_start: Some(1),
                line_end: Some(4),
                excerpt: None,
                connections: vec![],
            },
        ];

        let shaped = dedupe_ranked(items, 10);
        assert_eq!(shaped[0].label, "impl Extractor for RustExtractor");
    }

    // --- pack_to_budget / estimate_tokens_from_chars ----------------------

    /// Each test item costs a flat 10 tokens, so budgets map cleanly to counts.
    fn flat_cost(_item: &usize) -> usize {
        10
    }

    #[test]
    fn pack_to_budget_tiny_budget_keeps_at_least_one() {
        let ranked = vec![1usize, 2, 3, 4, 5];
        // Budget smaller than a single item's cost -> still keep the top hit.
        let (kept, dropped) = pack_to_budget(ranked, 3, flat_cost);
        assert_eq!(kept, vec![1]);
        assert_eq!(dropped, 4);
        assert_eq!(kept.len() + dropped, 5);
    }

    #[test]
    fn pack_to_budget_zero_budget_still_keeps_one() {
        let ranked = vec![1usize, 2, 3];
        let (kept, dropped) = pack_to_budget(ranked, 0, flat_cost);
        assert_eq!(kept, vec![1]);
        assert_eq!(dropped, 2);
    }

    #[test]
    fn pack_to_budget_generous_budget_keeps_all() {
        let ranked = vec![1usize, 2, 3, 4, 5];
        let (kept, dropped) = pack_to_budget(ranked, 10_000, flat_cost);
        assert_eq!(kept, vec![1, 2, 3, 4, 5]);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn pack_to_budget_mid_budget_keeps_ranked_prefix() {
        let ranked = vec![1usize, 2, 3, 4, 5];
        // 35 fits exactly 3 items (30); the 4th (40) would overflow.
        let (kept, dropped) = pack_to_budget(ranked, 35, flat_cost);
        assert_eq!(kept, vec![1, 2, 3], "keeps the top-ranked prefix in order");
        assert_eq!(dropped, 2);
        assert_eq!(kept.len() + dropped, 5);
    }

    #[test]
    fn pack_to_budget_exact_budget_boundary_inclusive() {
        let ranked = vec![1usize, 2, 3, 4];
        // 20 == exactly two items; boundary is inclusive (<= budget).
        let (kept, dropped) = pack_to_budget(ranked, 20, flat_cost);
        assert_eq!(kept, vec![1, 2]);
        assert_eq!(dropped, 2);
    }

    #[test]
    fn pack_to_budget_empty_input() {
        let ranked: Vec<usize> = vec![];
        let (kept, dropped) = pack_to_budget(ranked, 100, flat_cost);
        assert!(kept.is_empty());
        assert_eq!(dropped, 0);
    }

    #[test]
    fn estimate_tokens_monotonic_and_chars_over_four() {
        assert_eq!(estimate_tokens_from_chars(0), 0);
        // chars/4 rounded up: 1..=4 -> 1 token.
        assert_eq!(estimate_tokens_from_chars(1), 1);
        assert_eq!(estimate_tokens_from_chars(4), 1);
        assert_eq!(estimate_tokens_from_chars(5), 2);
        assert_eq!(estimate_tokens_from_chars(8), 2);
        assert_eq!(estimate_tokens_from_chars(400), 100);
        // Monotonic non-decreasing as char count grows.
        let mut prev = 0;
        for chars in [0usize, 1, 4, 5, 9, 16, 40, 41, 100, 1000] {
            let est = estimate_tokens_from_chars(chars);
            assert!(est >= prev, "estimate must be monotonic in char count");
            prev = est;
        }
    }
}
