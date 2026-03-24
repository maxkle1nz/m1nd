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
    items.sort_by(|a, b| {
        let score_delta = (a.score() - b.score()).abs();
        if score_delta <= CLOSE_SCORE_EPS {
            b.specificity()
                .total_cmp(&a.specificity())
                .then_with(|| b.score().total_cmp(&a.score()))
                .then_with(|| a.family_key().cmp(&b.family_key()))
        } else {
            b.score()
                .total_cmp(&a.score())
                .then_with(|| b.specificity().total_cmp(&a.specificity()))
                .then_with(|| a.family_key().cmp(&b.family_key()))
        }
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
}
