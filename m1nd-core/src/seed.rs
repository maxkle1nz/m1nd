// === crates/m1nd-core/src/seed.rs ===

use crate::error::M1ndResult;
use crate::graph::Graph;
use crate::types::*;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum seeds returned (FM-ACT-017: cap for broad queries).
const MAX_SEEDS: usize = 200;
/// Minimum relevance to include a seed.
const MIN_RELEVANCE: f32 = 0.1;
/// Relevance scores by match type.
const EXACT_MATCH_RELEVANCE: f32 = 1.0;
const PREFIX_MATCH_RELEVANCE: f32 = 0.9;
const TAG_MATCH_RELEVANCE: f32 = 0.85;
const FUZZY_RELEVANCE_SCALE: f32 = 0.7;
const CODE_PATH_BONUS: f32 = 0.10;
const TEST_PATH_BONUS: f32 = 0.05;
const REPO_PATH_BONUS: f32 = 0.08;
const DOC_PATH_PENALTY: f32 = 0.12;
const QUERY_PATH_TOKEN_BONUS: f32 = 0.03;
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "from", "into", "its", "own", "codebase", "task",
    "validate", "using", "focus", "around",
];

/// Bias nodes whose provenance is code-like instead of docs-like.
/// This keeps self-analysis focused on source files, especially when the
/// query spans broad technical terms that also appear in docs/wiki pages.
pub fn source_path_bias(source_path: Option<&str>, query_tokens: &[String]) -> f32 {
    let Some(source_path) = source_path else {
        return 0.0;
    };

    let source_path = source_path.to_lowercase();
    if source_path.is_empty() {
        return 0.0;
    }

    let mut bias = 0.0f32;
    if source_path.contains("/src/") || source_path.contains("src/") {
        bias += CODE_PATH_BONUS;
    }
    if source_path.contains("/tests/")
        || source_path.contains("/benches/")
        || source_path.contains("/examples/")
    {
        bias += TEST_PATH_BONUS;
    }
    if source_path.contains("m1nd-core")
        || source_path.contains("m1nd-mcp")
        || source_path.contains("m1nd-ingest")
        || source_path.contains("m1nd-ui")
    {
        bias += REPO_PATH_BONUS;
    }
    if source_path.contains("/docs/")
        || source_path.contains("/wiki/")
        || source_path.contains("readme")
        || source_path.contains("changelog")
        || source_path.contains("tutorial")
    {
        bias -= DOC_PATH_PENALTY;
    }

    for token in query_tokens {
        if token.len() <= 2 {
            continue;
        }
        if source_path.contains(token.as_str()) {
            bias += QUERY_PATH_TOKEN_BONUS;
        }
    }

    bias.clamp(-0.25, 0.25)
}

// ---------------------------------------------------------------------------
// Ranking-noise demote — shared by every ranker (askGOD F5 verdict, 2026-07-24)
// ---------------------------------------------------------------------------

/// Tag prefix marking a node as ranking noise. Ingest stamps `noise:minified`
/// on nodes from files whose content looks machine-generated
/// (`m1nd_ingest::path_policy::looks_minified_source`).
pub const NOISE_TAG_PREFIX: &str = "noise:";

/// Labels this short carry no lexical signal a query can match, so their rank is
/// bought entirely with centrality. Minifiers rename every symbol to one or two
/// characters and funnel the whole bundle through a handful of helpers, which is
/// exactly how `…::fn::s` came to out-rank real code on a 103k-node brain.
pub const NOISE_SHORT_LABEL_MAX: usize = 2;

/// Multiplier applied to a `noise:`-tagged node's score.
const NOISE_TAG_DEMOTE: f32 = 0.35;
/// Multiplier applied to a node whose label is at most [`NOISE_SHORT_LABEL_MAX`]
/// characters and that the query did not ask for by name.
const SHORT_LABEL_DEMOTE: f32 = 0.5;

/// True for a tag in the reserved `noise:` namespace.
pub fn is_noise_tag(tag: &str) -> bool {
    tag.starts_with(NOISE_TAG_PREFIX)
}

/// True when `query_lower` (already lowercased) contains `label` as a WHOLE
/// token. This is the escape hatch that keeps short *real* identifiers findable:
/// `id`, `ok`, `go`, `db` are legitimate names, and an agent that types one of
/// them by hand must still get it — only unasked-for short labels are demoted.
pub fn query_names_label(query_lower: &str, label: &str) -> bool {
    if label.is_empty() || query_lower.is_empty() {
        return false;
    }
    query_lower
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|token| token.eq_ignore_ascii_case(label))
}

/// True when a label carries the shape a minifier leaves behind — 1-2 characters
/// the query never named — and therefore nothing a static embedding can mean.
///
/// This is a NARROWER test than "is ranking noise": a `noise:` tag says where a
/// node came from, this says the label itself is unreadable. A caller that wants
/// to drop a hit rather than rank it lower must gate on THIS, never on the tag —
/// a tagged file still holds readable exports, and losing those would contradict
/// what the tag is for.
pub fn is_minifier_shaped_label(label: &str, query_lower: &str) -> bool {
    let label_len = label.chars().count();
    label_len > 0 && label_len <= NOISE_SHORT_LABEL_MAX && !query_names_label(query_lower, label)
}

/// Multiplicative demote in `(0, 1]` for a node that is ranking noise.
///
/// SOFT by construction: the node keeps its score, its edges, and its place in
/// the result set — it just stops burying the code an agent actually asked for.
/// Nothing here deletes or hides a node, because a bundle committed on purpose
/// is still part of the repository.
pub fn ranking_noise_demote(label: &str, noise_tagged: bool, query_lower: &str) -> f32 {
    let mut factor = 1.0f32;
    if noise_tagged {
        factor *= NOISE_TAG_DEMOTE;
    }
    if is_minifier_shaped_label(label, query_lower) {
        factor *= SHORT_LABEL_DEMOTE;
    }
    factor
}

/// [`ranking_noise_demote`] resolved against a graph node. Returns `1.0` (no
/// demote) for an out-of-range index, so a caller can apply it blindly.
pub fn graph_ranking_noise_demote(graph: &Graph, idx: usize, query_lower: &str) -> f32 {
    // Callers pass NodeIds that may predate a rebuild, so bound on the actual
    // column lengths, not only on `count`.
    if idx >= graph.nodes.count as usize
        || idx >= graph.nodes.label.len()
        || idx >= graph.nodes.tags.len()
    {
        return 1.0;
    }
    let label = graph
        .strings
        .try_resolve(graph.nodes.label[idx])
        .unwrap_or("");
    let noise_tagged = graph.nodes.tags[idx]
        .iter()
        .any(|tag| graph.strings.try_resolve(*tag).is_some_and(is_noise_tag));
    ranking_noise_demote(label, noise_tagged, query_lower)
}

// ---------------------------------------------------------------------------
// SeedFinder — fuzzy query -> node matching
// Replaces: engine_v2.py SeedFinder, engine_fast.py FastSeedFinder
// ---------------------------------------------------------------------------

/// Finds seed nodes matching a natural-language query.
/// Uses label substring, tag intersection, type filtering, and synonym expansion.
pub struct SeedFinder;

impl SeedFinder {
    /// Tokenize query: lowercase, split on whitespace/punctuation, filter short tokens.
    fn tokenize(query: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        for raw in query.to_lowercase().split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '?' | '!' | '.' | ',' | ':' | ';' | '(' | ')' | '{' | '}' | '[' | ']'
                )
        }) {
            let trimmed = raw.trim_matches(|c: char| matches!(c, '"' | '\'' | '`'));
            if trimmed.len() <= 2 || STOPWORDS.contains(&trimmed) {
                continue;
            }
            if !tokens.iter().any(|existing| existing == trimmed) {
                tokens.push(trimmed.to_string());
            }
            for part in Self::split_identifier(trimmed) {
                if part.len() > 2
                    && !STOPWORDS.contains(&part.as_str())
                    && !tokens.iter().any(|existing| existing == &part)
                {
                    tokens.push(part);
                }
            }
        }
        tokens
    }

    fn split_identifier(ident: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        for part in ident.split(['_', '-', '/', '\\', ':']) {
            if part.is_empty() {
                continue;
            }
            let mut current = String::new();
            for ch in part.chars() {
                if ch.is_uppercase() && !current.is_empty() {
                    tokens.push(current.to_lowercase());
                    current.clear();
                }
                current.push(ch);
            }
            if !current.is_empty() {
                tokens.push(current.to_lowercase());
            }
        }
        tokens
    }

    fn token_match_score(
        graph: &Graph,
        index: usize,
        label_lower: &str,
        source_path_lower: Option<&str>,
        token: &str,
    ) -> f32 {
        if label_lower == token {
            return EXACT_MATCH_RELEVANCE;
        }

        let label_parts = Self::split_identifier(label_lower);
        if label_lower.starts_with(token) || token.starts_with(label_lower) {
            return PREFIX_MATCH_RELEVANCE;
        }
        if label_parts.iter().any(|part| part == token) {
            return 0.92;
        }
        if label_lower.contains(token) || token.contains(label_lower) {
            return 0.8;
        }

        for &tag_interned in &graph.nodes.tags[index] {
            let tag = graph.strings.resolve(tag_interned).to_lowercase();
            if tag == token {
                return TAG_MATCH_RELEVANCE;
            }
            if tag.contains(token) {
                return 0.8;
            }
        }

        if let Some(source_path_lower) = source_path_lower {
            if source_path_lower
                .split(['/', '_', '-', '.'])
                .any(|part| part == token)
            {
                return 0.82;
            }
            if source_path_lower.contains(token) {
                return 0.72;
            }
        }

        let sim = Self::trigram_similarity(token, label_lower);
        if sim > 0.3 {
            return FUZZY_RELEVANCE_SCALE * sim;
        }
        0.0
    }

    fn node_type_bias(node_type: &NodeType) -> f32 {
        match node_type {
            NodeType::Function | NodeType::Struct | NodeType::Type | NodeType::Module => 0.06,
            NodeType::Class | NodeType::Enum => 0.05,
            NodeType::File => 0.03,
            NodeType::Directory => -0.02,
            NodeType::Concept | NodeType::Material | NodeType::Process | NodeType::Product => -0.04,
            _ => 0.0,
        }
    }

    fn family_key(label: &str, node_type: &NodeType, source_path: Option<&str>) -> String {
        let label_lower = label.trim().to_lowercase();
        if let Some(rest) = label.trim().strip_prefix("impl ") {
            if let Some((trait_part, _self_part)) = rest.split_once(" for ") {
                return format!("impl:{}", trait_part.trim().to_lowercase());
            }
            return format!("impl:{}", rest.trim().to_lowercase());
        }

        if source_path
            .map(|path| path.to_lowercase().contains("cargo.toml"))
            .unwrap_or(false)
            && matches!(node_type, NodeType::Module)
        {
            return format!("crate:{}", label_lower);
        }

        label_lower
    }

    fn node_specificity_bias(label: &str, node_type: &NodeType, source_path: Option<&str>) -> f32 {
        let mut score = Self::node_type_bias(node_type);
        let label_lower = label.trim().to_lowercase();
        let source_path_lower = source_path.unwrap_or("").to_lowercase();

        if label_lower.starts_with("impl ") {
            score += 2.0;
        }
        if source_path_lower.contains("/src/") || source_path_lower.contains("/tests/") {
            score += 0.4;
        }
        if source_path_lower.contains("/docs/")
            || source_path_lower.contains("/wiki/")
            || source_path_lower.contains("readme")
            || source_path_lower.contains("changelog")
            || source_path_lower.contains("tutorial")
        {
            score -= 0.6;
        }
        if source_path_lower.contains("cargo.toml") && matches!(node_type, NodeType::Module) {
            score -= 0.8;
        }

        score
    }

    /// Trigram set for fuzzy matching.
    fn trigrams(s: &str) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        if chars.len() < 3 {
            return vec![s.to_string()];
        }
        chars.windows(3).map(|w| w.iter().collect()).collect()
    }

    /// Trigram cosine similarity between two strings.
    fn trigram_similarity(a: &str, b: &str) -> f32 {
        let ta = Self::trigrams(&a.to_lowercase());
        let tb = Self::trigrams(&b.to_lowercase());
        if ta.is_empty() || tb.is_empty() {
            return 0.0;
        }
        let mut dot = 0usize;
        for t in &ta {
            if tb.contains(t) {
                dot += 1;
            }
        }
        if dot == 0 {
            return 0.0;
        }
        dot as f32 / ((ta.len() as f32).sqrt() * (tb.len() as f32).sqrt())
    }

    /// Find seeds matching `query`. Returns (NodeId, relevance) sorted descending.
    /// Replaces: engine_v2.py SeedFinder.find_seeds()
    pub fn find_seeds(
        graph: &Graph,
        query: &str,
        max_seeds: usize,
    ) -> M1ndResult<Vec<(NodeId, FiniteF32)>> {
        let tokens = Self::tokenize(query);
        if tokens.is_empty() {
            return Ok(Vec::new());
        }

        let n = graph.num_nodes() as usize;
        let cap = max_seeds.min(MAX_SEEDS);

        // Per-node best relevance
        let mut relevance = vec![0.0f32; n];

        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let node_id = NodeId::new(i as u32);
            let label = graph.strings.resolve(graph.nodes.label[i]);
            let label_lower = label.to_lowercase();
            let source_path = graph.nodes.provenance[i]
                .source_path
                .and_then(|s| graph.strings.try_resolve(s));
            let source_path_lower = source_path.map(str::to_lowercase);

            let mut best = 0.0f32;
            let mut total = 0.0f32;
            let mut matched_tokens = 0usize;

            for token in &tokens {
                let score = Self::token_match_score(
                    graph,
                    i,
                    &label_lower,
                    source_path_lower.as_deref(),
                    token,
                );
                if score > 0.0 {
                    matched_tokens += 1;
                    total += score;
                    best = best.max(score);
                }
            }

            if matched_tokens == 0 {
                relevance[i] = (source_path_bias(source_path, &tokens)
                    + Self::node_type_bias(&graph.nodes.node_type[i]))
                .max(0.0);
                continue;
            }

            let coverage = matched_tokens as f32 / tokens.len().max(1) as f32;
            let avg_match = total / matched_tokens as f32;
            if best >= EXACT_MATCH_RELEVANCE && coverage >= 1.0 {
                relevance[i] = EXACT_MATCH_RELEVANCE;
                continue;
            }
            let aggregate = avg_match * 0.5 + coverage * 0.35 + best * 0.15;
            relevance[i] = (aggregate
                + source_path_bias(source_path, &tokens)
                + Self::node_type_bias(&graph.nodes.node_type[i]))
            .clamp(0.0, 1.0);
        }

        let mut best_by_family: HashMap<String, (usize, f32, f32)> = HashMap::new();

        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let r = relevance[i];
            if r < MIN_RELEVANCE {
                continue;
            }

            let label = graph.strings.resolve(graph.nodes.label[i]);
            let source_path = graph.nodes.provenance[i]
                .source_path
                .and_then(|s| graph.strings.try_resolve(s));
            let family_key = Self::family_key(label, &graph.nodes.node_type[i], source_path);
            let specificity =
                Self::node_specificity_bias(label, &graph.nodes.node_type[i], source_path);

            best_by_family
                .entry(family_key)
                .and_modify(|existing| {
                    let (best_idx, best_score, best_specificity) = *existing;
                    let should_replace = r > best_score
                        || (r == best_score && specificity > best_specificity)
                        || (r == best_score && specificity == best_specificity && i < best_idx);
                    if should_replace {
                        *existing = (i, r, specificity);
                    }
                })
                .or_insert((i, r, specificity));
        }

        // Collect, filter, sort, cap
        let mut results: Vec<(NodeId, FiniteF32)> = best_by_family
            .iter()
            .map(|(_, (i, r, _))| (NodeId::new(*i as u32), FiniteF32::new(*r)))
            .collect();

        results.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        results.truncate(cap);
        Ok(results)
    }

    /// Find seeds using semantic engine for enhanced matching.
    /// Two-phase: SeedFinder.find_seeds() + SemanticEngine re-rank.
    /// Replaces: engine_v2.py ConnectomeEngine._find_and_boost_seeds()
    pub fn find_seeds_semantic(
        graph: &Graph,
        semantic: &crate::semantic::SemanticEngine,
        query: &str,
        max_seeds: usize,
    ) -> M1ndResult<Vec<(NodeId, FiniteF32)>> {
        // Phase 1: basic seed finding
        let mut seeds = Self::find_seeds(graph, query, max_seeds * 3)?;

        // Phase 2: re-rank with semantic similarity
        let semantic_scores = semantic.query_fast(graph, query, max_seeds * 3)?;
        let mut sem_map = std::collections::HashMap::new();
        for (node, score) in &semantic_scores {
            sem_map.insert(node.0, score.get());
        }

        // Blend: 0.6 * basic + 0.4 * semantic
        for (node, ref mut score) in &mut seeds {
            let sem = sem_map.get(&node.0).copied().unwrap_or(0.0);
            let blended = score.get() * 0.6 + sem * 0.4;
            *score = FiniteF32::new(blended);
        }

        seeds.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        seeds.truncate(max_seeds.min(MAX_SEEDS));
        Ok(seeds)
    }
}

#[cfg(test)]
mod ranking_noise_tests {
    use super::*;

    #[test]
    fn short_and_noise_tagged_labels_are_demoted() {
        // Minifier-shaped: no lexical signal, rank bought with centrality.
        assert!(ranking_noise_demote("s", false, "handle function call") < 1.0);
        assert!(ranking_noise_demote("ab", false, "handle function call") < 1.0);
        // Machine-generated provenance, whatever the label.
        assert!(ranking_noise_demote("renderRow", true, "render a row") < 1.0);
        // Both signals compound.
        assert!(
            ranking_noise_demote("s", true, "unrelated")
                < ranking_noise_demote("s", false, "unrelated")
        );
        // Ordinary authored code is untouched.
        assert_eq!(
            ranking_noise_demote("handle_function_call", false, "handle_function_call"),
            1.0
        );
        // Empty labels carry no shape to judge.
        assert_eq!(ranking_noise_demote("", false, "anything"), 1.0);
    }

    #[test]
    fn a_short_label_the_query_names_is_never_demoted() {
        // The escape hatch for real short identifiers.
        for query in [
            "what does id do",
            "trace db writes",
            "ok()",
            "the `go` helper",
        ] {
            let label = query
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .find(|t| t.len() <= 2 && !t.is_empty())
                .expect("fixture query names a short token");
            assert_eq!(
                ranking_noise_demote(label, false, query),
                1.0,
                "query {query:?} named {label:?} and must not demote it"
            );
        }
        // ... but only as a WHOLE token: `s` inside `strings` is not a naming.
        assert!(ranking_noise_demote("s", false, "strings resolve") < 1.0);
        // The exemption is lexical only — it never rescues generated provenance.
        assert!(ranking_noise_demote("id", true, "what does id do") < 1.0);
    }

    #[test]
    fn only_an_unreadable_label_is_minifier_shaped_not_mere_provenance() {
        // What a caller may DROP on: a 1-2 char label nobody asked for.
        assert!(is_minifier_shaped_label("h", "render the widget row"));
        assert!(is_minifier_shaped_label("qz", "decode the payload"));

        // What a caller may NOT drop on, even though both are demoted:
        // a readable export that merely lives in a generated file...
        assert!(!is_minifier_shaped_label(
            "decodeTelemetryEnvelope",
            "how is telemetry unpacked"
        ));
        assert!(
            ranking_noise_demote("decodeTelemetryEnvelope", true, "how is telemetry unpacked")
                < 1.0
        );

        // ...and a short label the query named by hand, tag or no tag. Seek's
        // tokenizer discards tokens of 2 chars or fewer, so a keyword-gated drop
        // would silence exactly the search that asked for it.
        assert!(!is_minifier_shaped_label("id", "what does id do"));
        assert!(ranking_noise_demote("id", true, "what does id do") < 1.0);
    }

    #[test]
    fn noise_tag_namespace_is_prefix_scoped() {
        assert!(is_noise_tag("noise:minified"));
        assert!(is_noise_tag("noise:generated"));
        assert!(!is_noise_tag("public"));
        assert!(!is_noise_tag("async"));
        assert!(!is_noise_tag("denoise"));
    }
}
