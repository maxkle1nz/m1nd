// === m1nd-mcp/src/verb_usage.rs ===
//
// What m1nd records about its OWN use: one counter per verb, nothing else.
//
// WHY THIS EXISTS. Until now m1nd recorded nothing durable about how it is
// called. `report` counts the CURRENT session's query log (a 1000-entry ring
// buffer that dies with the process) and `metrics` is about graph nodes, not
// traffic — so the only way to answer "which verbs do agents actually use, and
// how often" was to reconstruct it from the HOST's transcripts, outside the
// product. This ledger is the smallest honest instrument that answers it from
// inside: a verb name, three call counters, and two timestamps.
//
// ── PRIVACY IS A HARD CONSTRAINT — read this before adding a field ──────────
//
// This file may contain VERB NAMES AND COUNTS ONLY. It must never record
// arguments, queries, paths, node labels, agent-authored text, or anything
// derived from them. That is not a style preference: this ledger is durable,
// it is written for every call, and nothing downstream redacts it.
//
// `agent_id` is deliberately ABSENT. It is a self-declared free-text label the
// caller supplies on every verb — unvalidated, unnamespaced, and in the field
// already numbering in the hundreds of distinct values. A label a caller writes
// can hold a path, a branch name, a person, or a fragment of a query, so a
// column that holds it is a column that holds free text. If a future
// contributor needs per-agent attribution, that is a NEW decision with a new
// privacy argument, not a widening of this one.
//
// The mechanism that enforces the constraint, rather than merely stating it:
// [`canonical_verb`] maps the caller's tool string onto a `&'static str` from
// the compiled route table, so the only strings that can ever reach the file
// are literals in this binary — an unrouted or invented name collapses into the
// single [`UNROUTED_VERB`] bucket. Every other value in the persisted shape is
// a `u64`. The test `verb_usage_persisted_shape_holds_no_free_text` walks the
// serialized JSON and fails on any string it does not recognise; a widening
// therefore fails a test at the moment it is written.
//
// ── DURABILITY ─────────────────────────────────────────────────────────────
//
// The ledger is a sidecar in the brain's runtime root, written with the same
// temp-file + rename idiom as its siblings (`boot_memory_state.json`,
// `daemon_state.json`, `auto_ingest_state.json`, the presence records). It is
// deliberately NOT part of the brain checkpoint inventory: usage is not brain
// knowledge, and a graph rollback must not roll back the fact that calls
// happened. Its loss contract is the weakest one in the runtime and is stated
// here so nobody has to guess: LOSING THIS FILE MEANS THE COUNTS START OVER.
// Never `?` a read or a write of it into a boot or dispatch path — a corrupt
// counter file must cost a log line, never a server.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use m1nd_core::error::M1ndResult;
use serde::{Deserialize, Serialize};

/// Schema id of the persisted shape. The ONLY string value in the file.
pub const VERB_USAGE_SCHEMA: &str = "m1nd-verb-usage-v0";

/// Sidecar file name, next to its siblings in the brain's runtime root.
pub const VERB_USAGE_FILE: &str = "verb_usage_state.json";

/// The single bucket every unrouted / unknown tool string collapses into, so a
/// caller-invented name can never become a key. See the privacy note above.
pub const UNROUTED_VERB: &str = "<unrouted>";

/// Minimum gap between two disk writes. Mirrors the presence beat's throttle:
/// counting is per call, publishing is throttled. Declared loss window on a
/// hard kill: up to this many milliseconds of counts.
pub const FLUSH_THROTTLE_MS: u64 = 5_000;

/// What happened to ONE dispatched verb, decided at the single seam that
/// records it (`server::dispatch_generic_tool`). The variants are disjoint:
/// exactly one is recorded per call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerbCallOutcome {
    /// The verb ran and produced a payload. NOT a claim the payload was useful
    /// — a m1nd payload can itself carry a refusal (`"refused": "..."`).
    Answered,
    /// The F-01 generic action-policy gate refused before any handler ran: the
    /// verb is advertised but its authority floor is above generic dispatch.
    RefusedAtAuthorityFloor,
    /// The dispatcher itself returned an error — a retired-primitive tombstone,
    /// the read-only attach gate, the proof gate, or a handler refusal.
    RefusedAtDispatch,
}

/// Per-verb counters. Every field is a `u64` — see the privacy note.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerbUsageCounters {
    #[serde(default)]
    pub answered: u64,
    #[serde(default)]
    pub refused_at_authority_floor: u64,
    #[serde(default)]
    pub refused_at_dispatch: u64,
    /// Epoch ms of the first recorded call, 0 when never called.
    #[serde(default)]
    pub first_seen_ms: u64,
    /// Epoch ms of the most recent recorded call, 0 when never called.
    #[serde(default)]
    pub last_seen_ms: u64,
}

impl VerbUsageCounters {
    /// Every recorded call for this verb, whatever its outcome.
    pub fn total(&self) -> u64 {
        self.answered
            .saturating_add(self.refused_at_authority_floor)
            .saturating_add(self.refused_at_dispatch)
    }
}

/// The persisted shape. Keys are `&'static` verb names; values are counters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerbUsageState {
    pub schema: String,
    #[serde(default)]
    pub verbs: BTreeMap<String, VerbUsageCounters>,
}

impl Default for VerbUsageState {
    fn default() -> Self {
        Self {
            schema: VERB_USAGE_SCHEMA.to_string(),
            verbs: BTreeMap::new(),
        }
    }
}

/// Map a caller's tool string onto a name this ledger is allowed to persist.
///
/// PRIVACY MECHANISM — do not replace this with `tool_name.to_string()`. The
/// returned value is always a `&'static str`: either a literal from the
/// compiled MCP route table or [`UNROUTED_VERB`]. Caller-supplied text
/// therefore cannot reach the file even when the caller invents a tool name.
pub fn canonical_verb(tool_name: &str) -> &'static str {
    let bare = tool_name
        .strip_prefix("m1nd.")
        .or_else(|| tool_name.strip_prefix("m1nd_"))
        .unwrap_or(tool_name);
    crate::action_routes::MCP_TOOL_ROUTE_NAMES
        .iter()
        .copied()
        .find(|routed| *routed == bare)
        .unwrap_or(UNROUTED_VERB)
}

/// The in-memory ledger plus its sidecar path.
#[derive(Clone, Debug)]
pub struct VerbUsageLedger {
    path: PathBuf,
    verbs: BTreeMap<String, VerbUsageCounters>,
    dirty: bool,
    last_flush_ms: u64,
}

impl VerbUsageLedger {
    /// Sidecar path for a runtime root.
    pub fn state_path(runtime_root: &Path) -> PathBuf {
        runtime_root.join(VERB_USAGE_FILE)
    }

    /// Load the ledger for a runtime root.
    ///
    /// DEGRADES, never fails: an absent file is an empty ledger, and an
    /// unreadable or corrupt one logs "continuing without it" and starts the
    /// counts over. This function returns no `Result` on purpose — boot must
    /// not be able to die on a counter file.
    pub fn load(runtime_root: &Path) -> Self {
        // RED: the shape exists, the behaviour does not yet.
        Self {
            path: Self::state_path(runtime_root),
            verbs: BTreeMap::new(),
            dirty: false,
            last_flush_ms: 0,
        }
    }

    /// Record ONE dispatched verb. `verb` must come from [`canonical_verb`].
    pub fn record(&mut self, verb: &'static str, outcome: VerbCallOutcome, now_ms: u64) {
        // RED: nothing is counted yet.
        let _ = (verb, outcome, now_ms);
    }

    /// Publish when something changed and the throttle has elapsed. The first
    /// write after a load is never throttled, so a boot that takes one call and
    /// dies still leaves a record.
    pub fn flush_if_due(&mut self, now_ms: u64) -> M1ndResult<()> {
        // RED: nothing is published yet.
        let _ = now_ms;
        Ok(())
    }

    /// Publish now (atomic temp-file + rename, the sibling sidecar idiom).
    pub fn flush(&mut self, now_ms: u64) -> M1ndResult<()> {
        // RED: nothing is published yet.
        let _ = now_ms;
        Ok(())
    }

    /// The counters, verb-name ordered. Read-only: nothing here mutates.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &VerbUsageCounters)> {
        self.verbs
            .iter()
            .map(|(verb, counters)| (verb.as_str(), counters))
    }

    /// Counters for one verb, `None` when it was never called.
    pub fn counters(&self, verb: &str) -> Option<&VerbUsageCounters> {
        self.verbs.get(verb)
    }

    /// How many distinct verbs have been called at least once.
    pub fn distinct_verbs(&self) -> usize {
        self.verbs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_ledger(root: &Path) -> VerbUsageLedger {
        VerbUsageLedger::load(root)
    }

    #[test]
    fn verb_usage_counts_increment_per_verb() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut ledger = open_ledger(temp.path());

        ledger.record(canonical_verb("north"), VerbCallOutcome::Answered, 10);
        ledger.record(canonical_verb("north"), VerbCallOutcome::Answered, 20);
        ledger.record(canonical_verb("seek"), VerbCallOutcome::Answered, 30);

        let north = ledger.counters("north").expect("north counted");
        assert_eq!(north.answered, 2, "two north calls");
        assert_eq!(north.first_seen_ms, 10, "first seen is the FIRST call");
        assert_eq!(north.last_seen_ms, 20, "last seen is the LAST call");
        assert_eq!(
            ledger.counters("seek").map(|c| c.answered),
            Some(1),
            "seek counts under its own name, never folded into north"
        );
        assert_eq!(ledger.distinct_verbs(), 2);
    }

    #[test]
    fn verb_usage_refusal_never_lands_in_the_answered_counter() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut ledger = open_ledger(temp.path());

        ledger.record(
            canonical_verb("apply"),
            VerbCallOutcome::RefusedAtAuthorityFloor,
            10,
        );
        ledger.record(
            canonical_verb("apply"),
            VerbCallOutcome::RefusedAtDispatch,
            20,
        );

        let apply = ledger.counters("apply").expect("apply counted");
        assert_eq!(
            apply.answered, 0,
            "a refused verb must never be counted as answered"
        );
        assert_eq!(apply.refused_at_authority_floor, 1);
        assert_eq!(apply.refused_at_dispatch, 1);
        assert_eq!(apply.total(), 2, "both refusals are still calls");
    }

    #[test]
    fn verb_usage_survives_a_simulated_restart() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut ledger = open_ledger(temp.path());
        ledger.record(canonical_verb("impact"), VerbCallOutcome::Answered, 100);
        ledger.record(
            canonical_verb("impact"),
            VerbCallOutcome::RefusedAtDispatch,
            200,
        );
        ledger.flush(200).expect("flush");

        // The restart: a brand-new ledger over the same runtime root.
        let reloaded = open_ledger(temp.path());
        let impact = reloaded.counters("impact").expect("impact survived");
        assert_eq!(impact.answered, 1);
        assert_eq!(impact.refused_at_dispatch, 1);
        assert_eq!(impact.first_seen_ms, 100);
        assert_eq!(impact.last_seen_ms, 200);
    }

    #[test]
    fn verb_usage_first_write_is_never_throttled_then_the_throttle_holds() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut ledger = open_ledger(temp.path());
        ledger.record(canonical_verb("north"), VerbCallOutcome::Answered, 1_000);
        ledger.flush_if_due(1_000).expect("first flush is due");
        assert!(
            VerbUsageLedger::state_path(temp.path()).exists(),
            "the first call after a load must publish, not wait out the throttle"
        );

        ledger.record(canonical_verb("north"), VerbCallOutcome::Answered, 1_100);
        ledger
            .flush_if_due(1_100)
            .expect("throttled flush is a no-op");
        assert_eq!(
            open_ledger(temp.path())
                .counters("north")
                .map(|c| c.answered),
            Some(1),
            "the throttled second call stays in memory until the window elapses"
        );

        ledger
            .flush_if_due(1_000 + FLUSH_THROTTLE_MS + 1)
            .expect("flush after the window");
        assert_eq!(
            open_ledger(temp.path())
                .counters("north")
                .map(|c| c.answered),
            Some(2),
            "once the window elapses the pending count is published"
        );
    }

    #[test]
    fn verb_usage_corrupt_or_absent_file_degrades_to_empty_counts() {
        let temp = tempfile::tempdir().expect("tempdir");

        // ABSENT: an empty ledger, no error, no file created by reading.
        let absent = open_ledger(temp.path());
        assert_eq!(absent.distinct_verbs(), 0);
        assert!(!VerbUsageLedger::state_path(temp.path()).exists());

        // CORRUPT: truncated JSON, foreign JSON, and raw bytes all degrade.
        for corrupt in ["{\"schema\":", "[]", "not json at all", ""] {
            std::fs::write(VerbUsageLedger::state_path(temp.path()), corrupt)
                .expect("write corrupt sidecar");
            let mut recovered = open_ledger(temp.path());
            assert_eq!(
                recovered.distinct_verbs(),
                0,
                "a corrupt counter file must degrade to empty counts, not to a failure"
            );
            // And it heals: the next call publishes a clean file over it.
            recovered.record(canonical_verb("health"), VerbCallOutcome::Answered, 7);
            recovered.flush(7).expect("flush over a corrupt file");
            assert_eq!(
                open_ledger(temp.path())
                    .counters("health")
                    .map(|c| c.answered),
                Some(1)
            );
        }
    }

    #[test]
    fn verb_usage_unrouted_names_collapse_into_one_bucket() {
        // The privacy mechanism at the door: a caller-invented tool name — here
        // one shaped like a secret and one like a path — can never become a key.
        assert_eq!(canonical_verb("sk-live-0000-not-a-verb"), UNROUTED_VERB);
        assert_eq!(canonical_verb("/Users/someone/private/repo"), UNROUTED_VERB);
        assert_eq!(canonical_verb("north"), "north");
        assert_eq!(canonical_verb("m1nd.north"), "north", "dotted alias");
        assert_eq!(canonical_verb("m1nd_north"), "north", "underscore alias");
    }

    /// THE PRIVACY PIN. Walks the persisted JSON and fails on anything capable
    /// of holding free text: any string value other than the schema id, and any
    /// key that is not a known verb name or a declared counter field.
    ///
    /// This test is meant to BITE. If you are here because you added a field
    /// and this failed, the answer is not to widen the allow-list — it is to
    /// re-read the privacy note at the top of this file and make the case that
    /// your field cannot carry user content.
    #[test]
    fn verb_usage_persisted_shape_holds_no_free_text() {
        const COUNTER_FIELDS: &[&str] = &[
            "answered",
            "refused_at_authority_floor",
            "refused_at_dispatch",
            "first_seen_ms",
            "last_seen_ms",
        ];

        let temp = tempfile::tempdir().expect("tempdir");
        let mut ledger = open_ledger(temp.path());
        // Drive every outcome, plus the unrouted bucket, so the pin sees the
        // widest shape the ledger can produce.
        ledger.record(canonical_verb("north"), VerbCallOutcome::Answered, 1);
        ledger.record(
            canonical_verb("apply"),
            VerbCallOutcome::RefusedAtAuthorityFloor,
            2,
        );
        ledger.record(
            canonical_verb("edit_commit"),
            VerbCallOutcome::RefusedAtDispatch,
            3,
        );
        ledger.record(
            canonical_verb("secret-argument-shaped-name"),
            VerbCallOutcome::Answered,
            4,
        );
        ledger.flush(4).expect("flush");

        let raw = std::fs::read_to_string(VerbUsageLedger::state_path(temp.path()))
            .expect("read persisted ledger");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("persisted JSON");

        let object = value.as_object().expect("top level object");
        let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["schema", "verbs"],
            "the persisted shape has exactly two top-level fields"
        );
        assert_eq!(
            object["schema"].as_str(),
            Some(VERB_USAGE_SCHEMA),
            "the schema id is the ONLY string value the file may contain"
        );

        let verbs = object["verbs"].as_object().expect("verbs map");
        for (verb, counters) in verbs {
            assert!(
                verb == UNROUTED_VERB
                    || crate::action_routes::MCP_TOOL_ROUTE_NAMES.contains(&verb.as_str()),
                "persisted verb key '{verb}' is not a compiled route name — caller text \
                 reached the ledger"
            );
            let counters = counters
                .as_object()
                .unwrap_or_else(|| panic!("counters for {verb} must be an object"));
            for (field, number) in counters {
                assert!(
                    COUNTER_FIELDS.contains(&field.as_str()),
                    "unknown counter field '{field}' on '{verb}' — a new field must be \
                     argued against the privacy note before it is persisted"
                );
                assert!(
                    number.is_u64(),
                    "counter field '{field}' on '{verb}' is {number} — every value in this \
                     ledger must be a number; a string is a place free text can hide"
                );
            }
        }

        // The blunt backstop: no key or value anywhere in the file may hold a
        // fragment of the caller-supplied name that produced the last record.
        assert!(
            !raw.contains("secret-argument-shaped-name"),
            "an invented tool name reached the persisted ledger verbatim"
        );
    }
}
