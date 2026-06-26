//! Behavioral lock for the "did you mean?" fuzzy tool-name matchers.
//!
//! `help_guidance::find_similar_tool_names` and `personality::find_similar_tools`
//! are what an agent sees when it calls a tool name that doesn't exist — they
//! turn a typo into a ranked suggestion list. Both drive a private
//! `levenshtein_distance`, and both were entirely uncovered (0 executions in the
//! workspace coverage run) despite being the recovery path the whole MCP surface
//! relies on. A regression (wrong threshold, lost sort, broken case-folding)
//! would silently degrade every mistyped-tool recovery.
//!
//! Tests use the real `live_tool_names()` catalog rather than hard-coded names,
//! so they stay correct as the tool set evolves. (Surfaced by the X-RAY
//! proof-coverage pass.)

use m1nd_mcp::help_guidance::{find_similar_tool_names, live_tool_names};
use m1nd_mcp::personality::find_similar_tools;

#[test]
fn exact_tool_name_ranks_first() {
    let names = live_tool_names();
    assert!(!names.is_empty(), "the help catalog must expose tool names");
    let sample = names[0].clone();

    let out = find_similar_tool_names(&sample, 3);
    assert_eq!(
        out.first().map(String::as_str),
        Some(sample.as_str()),
        "an exact tool name must rank first (distance 0): {out:?}"
    );
}

#[test]
fn single_typo_still_suggests_the_real_tool() {
    let names = live_tool_names();
    let sample = names[0].clone();

    // One extra character => Levenshtein distance 1, well within the threshold.
    let typo = format!("{sample}x");
    let out = find_similar_tool_names(&typo, 5);
    assert!(
        out.iter().any(|n| n == &sample),
        "a one-character typo of {sample:?} should still surface it: {out:?}"
    );
}

#[test]
fn matching_is_case_insensitive() {
    let names = live_tool_names();
    let sample = names[0].clone();

    let out = find_similar_tool_names(&sample.to_uppercase(), 3);
    assert!(
        out.iter().any(|n| n == &sample),
        "an uppercase query must still match {sample:?}: {out:?}"
    );
}

#[test]
fn respects_max_suggestions_and_its_floor() {
    let names = live_tool_names();
    let sample = names[0].clone();

    let two = find_similar_tool_names(&sample, 2);
    assert!(two.len() <= 2, "must not exceed max_suggestions=2: {two:?}");

    // The implementation floors max_suggestions at 1 (`.max(1)`), so even a
    // request for zero yields at most one suggestion, never a panic.
    let zero = find_similar_tool_names(&sample, 0);
    assert!(
        zero.len() <= 1,
        "max_suggestions=0 is floored to 1: {zero:?}"
    );
}

#[test]
fn far_gibberish_yields_no_suggestions() {
    // 30 identical chars is far beyond the edit-distance threshold (<= 6) from
    // every real (short) tool name, so nothing should be suggested.
    let out = find_similar_tool_names("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzz", 5);
    assert!(
        out.is_empty(),
        "gibberish past the distance threshold must suggest nothing: {out:?}"
    );
}

#[test]
fn personality_matcher_caps_at_three() {
    // `find_similar_tools` takes at most 3, regardless of how many tools are
    // within range of a very short, broadly-close query.
    let out = find_similar_tools("a");
    assert!(
        out.len() <= 3,
        "find_similar_tools must return at most 3 suggestions: {out:?}"
    );
}

#[test]
fn personality_matcher_drops_far_gibberish() {
    // personality's matcher uses a tighter threshold (<= 4); 30 chars is well
    // past it for any real tool name.
    let out = find_similar_tools("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzz");
    assert!(
        out.is_empty(),
        "gibberish past the distance threshold must suggest nothing: {out:?}"
    );
}
