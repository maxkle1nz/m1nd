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
        || source_path.contains("m1nd-viz")
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
        for part in ident.split(|c: char| matches!(c, '_' | '-' | '/' | '\\' | ':')) {
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
                .split(|c: char| matches!(c, '/' | '_' | '-' | '.'))
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
                relevance[i] = (source_path_bias(source_path.as_deref(), &tokens)
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
                + source_path_bias(source_path.as_deref(), &tokens)
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
            let family_key =
                Self::family_key(label, &graph.nodes.node_type[i], source_path.as_deref());
            let specificity = Self::node_specificity_bias(
                label,
                &graph.nodes.node_type[i],
                source_path.as_deref(),
            );

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

        results.sort_by(|a, b| b.1.cmp(&a.1));
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

        seeds.sort_by(|a, b| b.1.cmp(&a.1));
        seeds.truncate(max_seeds.min(MAX_SEEDS));
        Ok(seeds)
    }
}
