//! Locks `SynonymExpander` (semantic.rs) expand/are_synonyms/build invariants the
//! prod synonym-boost query relies on; surfaced by the X-RAY coverage sweep.
//!
//! `SemanticEngine::query` expands each query token through `expand` and boosts
//! nodes whose labels match a synonym (not the original token). That boost is only
//! correct if `expand` is case-insensitive, always echoes the term, pulls in the
//! rest of the group, dedups, and returns just `[term]` for unknown terms — and if
//! `are_synonyms` is reflexive, symmetric, intra-group true, cross-group false.
//!
//! These are pure, deterministic checks: no graph, no I/O.

use m1nd_core::semantic::SynonymExpander;

/// Build a small, explicit two-group expander used by most cases below.
/// Group 0 is the canonical "fetch" family; group 1 is unrelated.
fn fetch_and_store_expander() -> SynonymExpander {
    SynonymExpander::build(vec![
        vec!["fetch", "get", "retrieve"],
        vec!["store", "save", "persist"],
    ])
    .expect("building from non-overlapping groups must succeed")
}

#[test]
fn expand_is_case_insensitive_and_includes_self_and_group() {
    let expander = fetch_and_store_expander();

    // Upper-case input must resolve to the same lowercased group as lower-case.
    let from_upper = expander.expand("GET");
    let from_lower = expander.expand("get");

    // The term itself (lowercased) is always first / present.
    assert!(
        from_upper.contains(&"get".to_string()),
        "expand must always include the (lowercased) term itself, got {from_upper:?}"
    );

    // The other group members come along, regardless of input casing.
    for member in ["fetch", "retrieve"] {
        assert!(
            from_upper.contains(&member.to_string()),
            "expand(\"GET\") must include group member {member:?}, got {from_upper:?}"
        );
    }

    // Case-insensitivity: same set of results from "GET" and "get".
    let mut upper_sorted = from_upper.clone();
    let mut lower_sorted = from_lower.clone();
    upper_sorted.sort();
    lower_sorted.sort();
    assert_eq!(
        upper_sorted, lower_sorted,
        "expand must be case-insensitive: \"GET\" and \"get\" should expand identically"
    );

    // The fetch group has exactly 3 distinct members, so expansion has 3 entries.
    assert_eq!(
        from_upper.len(),
        3,
        "expand of a 3-member group must yield exactly its 3 members, got {from_upper:?}"
    );
}

#[test]
fn expand_dedups_and_is_self_first() {
    // A group that names the same term twice (with mixed casing) must not yield
    // duplicates after expansion.
    let expander = SynonymExpander::build(vec![vec!["alpha", "Alpha", "beta"]])
        .expect("build with case-duplicated terms must succeed");

    let expanded = expander.expand("alpha");

    // First entry is always the queried term itself.
    assert_eq!(
        expanded.first(),
        Some(&"alpha".to_string()),
        "expand must place the queried term first, got {expanded:?}"
    );

    // No value appears twice.
    let mut seen = expanded.clone();
    seen.sort();
    let before = seen.len();
    seen.dedup();
    assert_eq!(
        before,
        seen.len(),
        "expand must not return duplicate synonyms, got {expanded:?}"
    );

    // Concretely: {alpha, beta} — the duplicate "Alpha" collapses into "alpha".
    assert_eq!(
        seen,
        vec!["alpha".to_string(), "beta".to_string()],
        "expand must collapse case-duplicate members, got {expanded:?}"
    );
}

#[test]
fn expand_unknown_term_returns_only_itself() {
    let expander = fetch_and_store_expander();

    let expanded = expander.expand("UNKNOWN");

    // Unknown term: the only result is the lowercased term, never empty, never a
    // borrowed group from elsewhere.
    assert_eq!(
        expanded,
        vec!["unknown".to_string()],
        "expand of an unknown term must return exactly [lowercased term], got {expanded:?}"
    );
}

#[test]
fn are_synonyms_is_reflexive_even_for_unknown_terms() {
    let expander = fetch_and_store_expander();

    // Reflexive for a known term...
    assert!(
        expander.are_synonyms("get", "get"),
        "are_synonyms must be reflexive for known terms"
    );

    // ...and for an unknown one (equality short-circuits before group lookup).
    assert!(
        expander.are_synonyms("zzz_unknown", "zzz_unknown"),
        "are_synonyms must be reflexive even for terms not in any group"
    );

    // Reflexivity is case-insensitive too.
    assert!(
        expander.are_synonyms("GET", "get"),
        "are_synonyms must treat case-different spellings of one term as equal"
    );
}

#[test]
fn are_synonyms_is_symmetric_and_intra_group_true() {
    let expander = fetch_and_store_expander();

    // Two distinct members of the same group are synonyms, both directions.
    assert!(
        expander.are_synonyms("get", "retrieve"),
        "are_synonyms must be true within a group"
    );
    assert!(
        expander.are_synonyms("retrieve", "get"),
        "are_synonyms must be symmetric within a group"
    );

    // Symmetry holds across casing as well.
    assert!(
        expander.are_synonyms("FETCH", "Retrieve"),
        "are_synonyms must be case-insensitive and symmetric"
    );
}

#[test]
fn are_synonyms_is_false_across_groups() {
    let expander = fetch_and_store_expander();

    // "get" is in the fetch group; "save" is in the store group → not synonyms.
    assert!(
        !expander.are_synonyms("get", "save"),
        "are_synonyms must be false for terms in different groups"
    );
    assert!(
        !expander.are_synonyms("save", "get"),
        "are_synonyms must be false (symmetric) for cross-group terms"
    );

    // A known term and a completely unknown term are not synonyms.
    assert!(
        !expander.are_synonyms("get", "absent_term"),
        "are_synonyms must be false when one term is unknown"
    );
}

#[test]
fn build_default_groups_known_code_synonyms() {
    // The built-in defaults back the prod synonym boost; spot-check a code-domain
    // group documented in DEFAULT_SYNONYM_GROUPS so a silent table edit is caught.
    let expander =
        SynonymExpander::build_default().expect("build_default must succeed with bundled groups");

    // "function" family is grouped together in the defaults.
    assert!(
        expander.are_synonyms("function", "method"),
        "default groups must keep function/method synonymous"
    );
    let expanded = expander.expand("FUNCTION");
    for member in ["function", "fn", "method", "func"] {
        assert!(
            expanded.contains(&member.to_string()),
            "default expand(\"FUNCTION\") must include {member:?}, got {expanded:?}"
        );
    }

    // Cross-family terms from the defaults must stay distinct.
    assert!(
        !expander.are_synonyms("function", "class"),
        "default groups must keep function and class in separate groups"
    );
}
