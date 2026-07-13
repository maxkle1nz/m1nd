//! presence — durable session-presence sidecars (`m1nd-presence-v0`).
//!
//! ORGANISM-INSIDE **P1**, askGOD verdict 2026-07-13 (binding changes 1–2,
//! `docs/voice/ASKGOD-VERDICT-P1.md`). A runtime sidecar per live agent session,
//! MOLDED on [`crate::instance_registry`]: files in the shared registry dir
//! (sibling of `instances/`), TTL'd (minutes-scale), honest-absent on expiry,
//! GC'd at boot. The BEAT rides `track_agent` (throttled — never one write per
//! call, wrapped fail-open); COLLISIONS are DERIVED at read, never materialized.
//!
//! Laws (verdict + PRD §3.3): a presence is self-declared witness tissue — it
//! verifies nothing, gates nothing; a dead presence DISAPPEARS rather than
//! lying; the roster never renders a presence the registry did not serve
//! (INV-10's discipline applied to sessions). Enrichment is measured or declared,
//! never invented: `brain`/`caller_root` come from the session's own binding;
//! `task_ref` is measured from the agent's own mission card; the DECLARED fields
//! (`kind`/`theme`/`intent`/`working_set`/`worktree`) ride the optional
//! `session_handshake` fields.
//!
//! **LIMITATION, written per the verdict's flagged risk (the inverse TTL lie):**
//! *presence == activity VISIBLE TO m1nd.* An executor compiling for 20 minutes
//! makes no m1nd calls, so its beat lapses and it expires from the roster — a
//! live agent can read as absent. The roster answers "who is *talking to m1nd*,
//! on what, since when", never "who is alive". It never invents a heartbeat.

use crate::util::now_ms;
use m1nd_core::error::{M1ndError, M1ndResult};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Wire schema of a presence sidecar.
pub const PRESENCE_SCHEMA: &str = "m1nd-presence-v0";

/// Sidecar directory under the shared registry root (sibling of `instances/`).
const PRESENCE_DIR_NAME: &str = "presences";

/// Minutes-scale TTL: a presence whose last beat is older than this is stale —
/// absent at read, swept at boot. Well above [`PRESENCE_BEAT_THROTTLE_MS`] so a
/// live, talking session never flickers to stale between two beats.
pub const PRESENCE_TTL_MS: u64 = 120_000; // 2 minutes

/// The beat is throttled to at most one disk write per this interval per session
/// (the verdict's binding word: THROTTLED, "not one write per call"). Far below
/// the TTL so freshness is never at risk.
pub const PRESENCE_BEAT_THROTTLE_MS: u64 = 5_000; // 5 seconds

/// Boot-GC grace: reclaim presence files whose last beat is older than this. Set
/// to the TTL — an expired presence is already invisible at read; the sweep only
/// reclaims its file so a restarted owner never inherits orphan sidecars.
const PRESENCE_GC_AFTER_MS: u64 = PRESENCE_TTL_MS;

/// The two honest levels of the mutation signal (verdict c). OBSERVED is measured
/// from the agent's own dispatch; DECLARED is the agent's handshake intent. m1nd
/// does not see git — nothing more is claimable.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MutationSignal {
    /// Epoch ms of the last verb this session dispatched that
    /// `server::read_only_denied` classifies as mutating (the OBSERVED level).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at_ms: Option<u64>,
    /// The session's self-declared handshake intent (the DECLARED level). Free
    /// text; `"mutate"` is the signal word.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_intent: Option<String>,
}

/// A durable presence sidecar. Every field is measured or declared — nothing is
/// invented (G1: measured facts only).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PresenceRecord {
    pub schema: String,
    /// Stable per `(agent_id, brain)` — the beat upserts one file, never N.
    pub presence_id: String,
    pub agent_id: String,
    /// The bound brain's root (the session's `workspace_root`) — the collision
    /// key. Never claimed: it is the session's own binding.
    pub brain: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_root: Option<String>,
    /// DECLARED via `session_handshake`: orchestrator|executor|pool-hand|… .
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// DECLARED via `session_handshake`: one free-text line ("reader slice 1").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// DECLARED via `session_handshake`: the worktree/branch display string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    /// DECLARED via `session_handshake`: repo-relative paths and/or `sb_` blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub working_set: Vec<String>,
    /// MEASURED from the agent's own open `mission_start` charter (`msn_…`), never
    /// free declaration. Absent when the agent holds no open mission card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_ref: Option<String>,
    #[serde(default)]
    pub mutation: MutationSignal,
    pub first_seen_ms: u64,
    pub last_beat_ms: u64,
    pub query_count: u64,
    pub ttl_ms: u64,
}

impl PresenceRecord {
    /// A presence is stale once its last beat is older than its own TTL. Stale
    /// presences are absent at read and swept at boot (a dead presence
    /// disappears rather than lying).
    pub fn is_stale(&self, now: u64) -> bool {
        now.saturating_sub(self.last_beat_ms) > self.ttl_ms
    }

    /// Age of the session in ms (always rendered, per the verdict).
    pub fn age_ms(&self, now: u64) -> u64 {
        now.saturating_sub(self.first_seen_ms)
    }

    /// Whether this session carries a mutation signal at either honest level.
    /// OBSERVED counts only while still within the presence's TTL (a mutation
    /// seen long ago is not a live collision signal); DECLARED counts whenever
    /// the handshake intent is `mutate`.
    pub fn has_mutation_signal(&self, now: u64) -> bool {
        let observed = self
            .mutation
            .observed_at_ms
            .map(|ts| now.saturating_sub(ts) <= self.ttl_ms)
            .unwrap_or(false);
        let declared = self
            .mutation
            .declared_intent
            .as_deref()
            .map(|intent| intent.eq_ignore_ascii_case("mutate"))
            .unwrap_or(false);
        observed || declared
    }
}

/// A DERIVED collision (never stored): two live, mutating presences on the same
/// brain whose declared work co-locates. `overlap` names the measurable signal
/// (shared caller_root/worktree, or shared working-set entries).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Collision {
    pub subject_agent: String,
    pub subject_presence: String,
    pub other_agent: String,
    pub other_presence: String,
    pub overlap: Vec<String>,
}

// ---------------------------------------------------------------------------
// Paths + ids
// ---------------------------------------------------------------------------

fn presence_dir(registry_root: &Path) -> PathBuf {
    registry_root.join(PRESENCE_DIR_NAME)
}

fn presence_path(registry_root: &Path, presence_id: &str) -> PathBuf {
    presence_dir(registry_root).join(format!("{presence_id}.json"))
}

/// Stable presence id for a `(agent_id, brain)` pair: the same session on the
/// same brain always upserts ONE file. `prs_<12hex>`.
pub fn stable_presence_id(agent_id: &str, brain: &str) -> String {
    let mut hasher = DefaultHasher::new();
    agent_id.hash(&mut hasher);
    0u8.hash(&mut hasher); // domain separator so "a"+"b" ≠ "ab"
    brain.hash(&mut hasher);
    format!("prs_{:012x}", hasher.finish() & 0xffff_ffff_ffff)
}

// ---------------------------------------------------------------------------
// Write (the throttled beat's disk half) — fail-open at the call site
// ---------------------------------------------------------------------------

/// Atomically upsert a presence sidecar. Callers wrap this in the vigil
/// fail-open guard so a broken write can never break a tool call.
pub fn write_presence(registry_root: &Path, record: &PresenceRecord) -> M1ndResult<()> {
    let path = presence_path(registry_root, &record.presence_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(record).map_err(|error| {
        M1ndError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "failed to serialize presence {}: {error}",
                record.presence_id
            ),
        ))
    })?;
    let temp = path.with_extension("tmp");
    fs::write(&temp, json)?;
    fs::rename(temp, &path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Read (honest-absent on expiry)
// ---------------------------------------------------------------------------

/// Read every parseable presence sidecar. Unparseable/foreign files are skipped
/// (never surfaced, never deleted). No staleness filter — see [`list_live`].
fn read_all(registry_root: &Path) -> Vec<PresenceRecord> {
    let dir = presence_dir(registry_root);
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return out; // dir absent = no presences yet
    };
    for item in entries.flatten() {
        let path = item.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(record) = serde_json::from_str::<PresenceRecord>(&raw) {
                out.push(record);
            }
        }
    }
    out
}

/// The live roster across ALL brains served by this registry: every non-stale
/// presence. A stale presence is honest-absent (a dead presence disappears).
pub fn list_live(registry_root: &Path, now: u64) -> Vec<PresenceRecord> {
    let mut live: Vec<PresenceRecord> = read_all(registry_root)
        .into_iter()
        .filter(|p| !p.is_stale(now))
        .collect();
    // Youngest beat first, then a stable agent order.
    live.sort_by(|a, b| {
        b.last_beat_ms
            .cmp(&a.last_beat_ms)
            .then_with(|| a.agent_id.cmp(&b.agent_id))
    });
    live
}

/// The live roster scoped to ONE brain (the served-brain roster the cockpit and
/// north render — "this brain", never a cross-brain aggregate).
pub fn roster_for_brain(registry_root: &Path, brain: &str, now: u64) -> Vec<PresenceRecord> {
    list_live(registry_root, now)
        .into_iter()
        .filter(|p| p.brain == brain)
        .collect()
}

// ---------------------------------------------------------------------------
// Boot GC — reclaim orphan sidecars after an owner restart
// ---------------------------------------------------------------------------

/// Sweep presence files whose last beat is older than [`PRESENCE_GC_AFTER_MS`].
/// Conservative: any file that fails to read/parse is SKIPPED (never deleted),
/// so corrupt/foreign files are left untouched. Returns the number removed. Safe
/// to call at boot: a still-live session's fresh file is never touched.
pub fn gc_stale(registry_root: &Path) -> usize {
    let dir = presence_dir(registry_root);
    let now = now_ms();
    let mut removed = 0usize;
    let Ok(entries) = fs::read_dir(&dir) else {
        return 0;
    };
    for item in entries.flatten() {
        let path = item.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        // Conservative: unreadable/unparseable -> skip (do NOT delete).
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<PresenceRecord>(&raw) else {
            continue;
        };
        if now.saturating_sub(record.last_beat_ms) > PRESENCE_GC_AFTER_MS
            && fs::remove_file(&path).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

// ---------------------------------------------------------------------------
// Collision derivation (read-time, pure — never stored)
// ---------------------------------------------------------------------------

/// The co-location signal (verdict binding change 2): same caller_root/worktree
/// OR declared working-set overlap. Empty values never count. `None` when the
/// two sessions share no measurable work signal.
fn overlap_signal(a: &PresenceRecord, b: &PresenceRecord) -> Option<Vec<String>> {
    let mut sig = Vec::new();
    if let (Some(ca), Some(cb)) = (a.caller_root.as_deref(), b.caller_root.as_deref()) {
        if !ca.is_empty() && ca == cb {
            sig.push(format!("caller_root:{ca}"));
        }
    }
    if let (Some(wa), Some(wb)) = (a.worktree.as_deref(), b.worktree.as_deref()) {
        if !wa.is_empty() && wa == wb {
            sig.push(format!("worktree:{wa}"));
        }
    }
    for path in &a.working_set {
        if !path.is_empty() && b.working_set.contains(path) {
            sig.push(format!("path:{path}"));
        }
    }
    if sig.is_empty() {
        None
    } else {
        Some(sig)
    }
}

/// The full collision predicate (verdict binding change 2): SAME brain AND (same
/// caller_root/worktree OR working-set overlap) AND BOTH with a mutation signal.
/// Same-brain alone NEVER warns — three executors in isolated worktrees on one
/// brain is the NORMAL burst shape. Returns the overlap descriptor on a hit.
fn collision_between(a: &PresenceRecord, b: &PresenceRecord, now: u64) -> Option<Vec<String>> {
    if a.brain.is_empty() || a.brain != b.brain {
        return None; // same brain required (and never on an unbound sentinel)
    }
    if !(a.has_mutation_signal(now) && b.has_mutation_signal(now)) {
        return None; // both must carry a mutation signal
    }
    overlap_signal(a, b) // and the work must co-locate
}

/// All collisions in a brain-scoped roster, from the SUBJECT agent's point of
/// view. Each returned collision names `subject_agent` first — so north can push
/// the gap line onto exactly the sessions involved (the P1 gate requires it on
/// BOTH sessions' packets, and each session derives its own).
pub fn collisions_for_agent(roster: &[PresenceRecord], agent_id: &str, now: u64) -> Vec<Collision> {
    let mut out = Vec::new();
    let subjects = roster.iter().filter(|p| p.agent_id == agent_id);
    for subject in subjects {
        for other in roster {
            if other.presence_id == subject.presence_id || other.agent_id == subject.agent_id {
                continue;
            }
            if let Some(overlap) = collision_between(subject, other, now) {
                out.push(Collision {
                    subject_agent: subject.agent_id.clone(),
                    subject_presence: subject.presence_id.clone(),
                    other_agent: other.agent_id.clone(),
                    other_presence: other.presence_id.clone(),
                    overlap,
                });
            }
        }
    }
    out
}

/// Every unordered colliding pair in a brain-scoped roster (for the cockpit root
/// line + drill). Each pair appears once.
pub fn collisions_in(roster: &[PresenceRecord], now: u64) -> Vec<Collision> {
    let mut out = Vec::new();
    for i in 0..roster.len() {
        for j in (i + 1)..roster.len() {
            let a = &roster[i];
            let b = &roster[j];
            if a.agent_id == b.agent_id {
                continue;
            }
            if let Some(overlap) = collision_between(a, b, now) {
                out.push(Collision {
                    subject_agent: a.agent_id.clone(),
                    subject_presence: a.presence_id.clone(),
                    other_agent: b.agent_id.clone(),
                    other_presence: b.presence_id.clone(),
                    overlap,
                });
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Wire shape — the P1-UI contract (`GET /api/presences`, P1-UI-CONTRACT.md)
// ---------------------------------------------------------------------------

/// One contract `PresenceEntry` for the `/api/presences` wire: `{agent_id, root,
/// caller_root?, first_seen_ms, last_seen_ms, query_count, mutation, task_ref?}`.
/// `root` is the sidecar's `brain`; `caller_root` is OMITTED when it equals the
/// root (the contract's absent-when-equal rule); `last_seen_ms` is the last real
/// sighting (the throttled beat — never a synthetic ping); optional fields are
/// absent, never null-fabricated.
pub fn wire_entry(p: &PresenceRecord) -> serde_json::Value {
    let mut entry = serde_json::json!({
        "agent_id": p.agent_id,
        "root": p.brain,
        "first_seen_ms": p.first_seen_ms,
        "last_seen_ms": p.last_beat_ms,
        "query_count": p.query_count,
        // MutationSignal serializes exactly the contract's PresenceMutation:
        // {observed_at_ms?, declared_intent?}, both skip-if-none.
        "mutation": serde_json::to_value(&p.mutation).unwrap_or_else(|_| serde_json::json!({})),
    });
    if let Some(caller_root) = p.caller_root.as_deref().filter(|c| *c != p.brain) {
        entry["caller_root"] = serde_json::json!(caller_root);
    }
    if let Some(task_ref) = p.task_ref.as_deref() {
        entry["task_ref"] = serde_json::json!(task_ref);
    }
    entry
}

/// One contract `PresenceCollision`: `{brain_root, caller_root?, agent_ids,
/// reason}`. `reason` maps the derived overlap onto the contract's two arms —
/// any shared `caller_root:`/`worktree:` signal is the measurable
/// `same_worktree` arm (carrying the shared location), a pure working-set
/// overlap is `declared_overlap` (no location claimed). `None` only when the
/// collision's subject presence is not in the roster it was derived from
/// (impossible by construction; skipped fail-open rather than fabricated).
pub fn wire_collision(roster: &[PresenceRecord], c: &Collision) -> Option<serde_json::Value> {
    let subject = roster
        .iter()
        .find(|p| p.presence_id == c.subject_presence)?;
    // Prefer the caller_root signal (a measured path), then the declared
    // worktree string — both are the same_worktree arm.
    let shared_location = c
        .overlap
        .iter()
        .find_map(|o| o.strip_prefix("caller_root:"))
        .or_else(|| c.overlap.iter().find_map(|o| o.strip_prefix("worktree:")));
    let mut out = serde_json::json!({
        "brain_root": subject.brain,
        "agent_ids": [c.subject_agent, c.other_agent],
        "reason": if shared_location.is_some() { "same_worktree" } else { "declared_overlap" },
    });
    if let Some(location) = shared_location {
        out["caller_root"] = serde_json::json!(location);
    }
    Some(out)
}

/// The `/api/presences` wire body over a (possibly brain-scoped) roster:
/// `{presences: [...], collisions: [...]}`. `collisions` is ALWAYS present (even
/// empty) — per the contract, a present array makes the SERVER authoritative and
/// the client renders it verbatim instead of re-deriving. The caller attaches
/// the optional `served_brain` echo when the `?brain=` selector was used.
pub fn wire_response(roster: &[PresenceRecord], now: u64) -> serde_json::Value {
    let collisions: Vec<serde_json::Value> = collisions_in(roster, now)
        .iter()
        .filter_map(|c| wire_collision(roster, c))
        .collect();
    serde_json::json!({
        "presences": roster.iter().map(wire_entry).collect::<Vec<_>>(),
        "collisions": collisions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(agent: &str, brain: &str, now: u64) -> PresenceRecord {
        PresenceRecord {
            schema: PRESENCE_SCHEMA.to_string(),
            presence_id: stable_presence_id(agent, brain),
            agent_id: agent.to_string(),
            brain: brain.to_string(),
            caller_root: None,
            kind: None,
            theme: None,
            worktree: None,
            working_set: Vec::new(),
            task_ref: None,
            mutation: MutationSignal::default(),
            first_seen_ms: now,
            last_beat_ms: now,
            query_count: 1,
            ttl_ms: PRESENCE_TTL_MS,
        }
    }

    #[test]
    fn stable_id_is_deterministic_and_pair_scoped() {
        assert_eq!(
            stable_presence_id("exec-a", "/repo/x"),
            stable_presence_id("exec-a", "/repo/x"),
            "same (agent, brain) -> same id (the beat upserts one file)"
        );
        assert_ne!(
            stable_presence_id("exec-a", "/repo/x"),
            stable_presence_id("exec-a", "/repo/y"),
            "same agent, different brain -> different file"
        );
        assert_ne!(
            stable_presence_id("a", "b/x"),
            stable_presence_id("ab", "/x"),
            "domain separator: concatenation collisions are avoided"
        );
        assert!(stable_presence_id("a", "b").starts_with("prs_"));
    }

    #[test]
    fn write_read_roundtrip_and_ttl_absence() {
        let tmp = std::env::temp_dir().join(format!("m1nd-presence-test-{}", now_ms()));
        let now = now_ms();
        let r = rec("exec-1", "/brain/one", now);
        write_presence(&tmp, &r).unwrap();

        // Fresh -> present at read.
        let live = list_live(&tmp, now);
        assert_eq!(live.len(), 1, "a fresh presence is present at read");

        // Aged past its TTL -> honest-absent (a dead presence disappears).
        let future = now + PRESENCE_TTL_MS + 1;
        assert!(
            list_live(&tmp, future).is_empty(),
            "an expired presence is absent at read, never a lingering ghost"
        );
        // ...but the file still exists until GC reclaims it.
        assert_eq!(read_all(&tmp).len(), 1, "the file lingers until GC");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn gc_reclaims_only_expired_sidecars() {
        let tmp = std::env::temp_dir().join(format!("m1nd-presence-gc-{}", now_ms()));
        let now = now_ms();
        // A live one and an orphan aged well past the GC grace.
        let live = rec("live", "/b", now);
        let mut orphan = rec("orphan", "/b", now);
        orphan.last_beat_ms = now.saturating_sub(PRESENCE_GC_AFTER_MS + 60_000);
        write_presence(&tmp, &live).unwrap();
        write_presence(&tmp, &orphan).unwrap();

        let removed = gc_stale(&tmp);
        assert_eq!(
            removed, 1,
            "boot GC reclaims the orphan sidecar, not the live one"
        );
        let remaining = read_all(&tmp);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].agent_id, "live");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn mutation_signal_two_honest_levels() {
        let now = now_ms();
        let mut observed = rec("o", "/b", now);
        observed.mutation.observed_at_ms = Some(now);
        assert!(
            observed.has_mutation_signal(now),
            "OBSERVED counts while fresh"
        );
        assert!(
            !observed.has_mutation_signal(now + PRESENCE_TTL_MS + 1),
            "OBSERVED lapses past the TTL"
        );

        let mut declared = rec("d", "/b", now);
        declared.mutation.declared_intent = Some("mutate".to_string());
        assert!(declared.has_mutation_signal(now), "DECLARED mutate counts");

        let mut reader = rec("r", "/b", now);
        reader.mutation.declared_intent = Some("read".to_string());
        assert!(
            !reader.has_mutation_signal(now),
            "a declared reader carries no signal"
        );
    }

    #[test]
    fn collision_fires_on_shared_worktree_when_both_mutate() {
        let now = now_ms();
        let mut a = rec("hand-a", "/m1nd", now);
        let mut b = rec("hand-b", "/m1nd", now);
        a.worktree = Some("feat/x".to_string());
        b.worktree = Some("feat/x".to_string());
        a.mutation.declared_intent = Some("mutate".to_string());
        b.mutation.declared_intent = Some("mutate".to_string());

        let roster = vec![a, b];
        let pairs = collisions_in(&roster, now);
        assert_eq!(pairs.len(), 1, "two mutating hands in ONE worktree collide");
        assert!(pairs[0]
            .overlap
            .iter()
            .any(|o| o.contains("worktree:feat/x")));

        // Both sessions derive it from their own point of view (the P1 gate).
        assert_eq!(collisions_for_agent(&roster, "hand-a", now).len(), 1);
        assert_eq!(collisions_for_agent(&roster, "hand-b", now).len(), 1);
    }

    #[test]
    fn collision_fires_on_working_set_overlap() {
        let now = now_ms();
        let mut a = rec("a", "/m1nd", now);
        let mut b = rec("b", "/m1nd", now);
        a.working_set = vec!["src/server.rs".into(), "src/a.rs".into()];
        b.working_set = vec!["src/b.rs".into(), "src/server.rs".into()];
        a.mutation.observed_at_ms = Some(now);
        b.mutation.observed_at_ms = Some(now);
        let roster = vec![a, b];
        let pairs = collisions_in(&roster, now);
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].overlap.iter().any(|o| o == "path:src/server.rs"));
    }

    #[test]
    fn anti_test_isolated_worktrees_never_warn() {
        // The NORMAL burst shape: three executors, same brain, DISTINCT worktrees,
        // non-overlapping work, all mutating. Same-brain alone must NEVER warn.
        let now = now_ms();
        let mut mk = |agent: &str, wt: &str| {
            let mut r = rec(agent, "/m1nd", now);
            r.worktree = Some(wt.to_string());
            r.caller_root = Some(format!("/Users/dev/{wt}"));
            r.mutation.declared_intent = Some("mutate".to_string());
            r
        };
        let roster = vec![
            mk("p1-server", "m1nd-p1srv"),
            mk("p1-ui", "m1nd-p1ui"),
            mk("g1", "m1nd-g1"),
        ];
        assert!(
            collisions_in(&roster, now).is_empty(),
            "three mutating executors in isolated worktrees on one brain do NOT collide"
        );
        assert!(collisions_for_agent(&roster, "p1-server", now).is_empty());
    }

    #[test]
    fn no_collision_when_only_one_mutates() {
        let now = now_ms();
        let mut a = rec("a", "/m1nd", now);
        let mut b = rec("b", "/m1nd", now);
        a.caller_root = Some("/shared/wt".to_string());
        b.caller_root = Some("/shared/wt".to_string());
        a.mutation.declared_intent = Some("mutate".to_string());
        // b declares read.
        b.mutation.declared_intent = Some("read".to_string());
        let roster = vec![a, b];
        assert!(
            collisions_in(&roster, now).is_empty(),
            "co-located but only one hand mutating is not a collision"
        );
    }

    #[test]
    fn roster_for_brain_scopes_to_one_brain() {
        let tmp = std::env::temp_dir().join(format!("m1nd-presence-scope-{}", now_ms()));
        let now = now_ms();
        write_presence(&tmp, &rec("a", "/brain/one", now)).unwrap();
        write_presence(&tmp, &rec("b", "/brain/two", now)).unwrap();
        let one = roster_for_brain(&tmp, "/brain/one", now);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].brain, "/brain/one");
        let _ = fs::remove_dir_all(&tmp);
    }

    /// P1-UI contract — the wire `PresenceEntry`: `root` carries the brain,
    /// `caller_root` is OMITTED when it equals the root and present when it
    /// differs, `last_seen_ms` mirrors the beat, optional fields are absent
    /// (never null-fabricated), and mutation carries the two honest levels.
    #[test]
    fn wire_entry_matches_the_ui_contract_shape() {
        let now = now_ms();
        let mut p = rec("exec-a", "/brain/one", now);
        p.caller_root = Some("/brain/one".to_string()); // equals root → omitted
        let e = wire_entry(&p);
        assert_eq!(e["agent_id"], "exec-a");
        assert_eq!(e["root"], "/brain/one");
        assert!(
            e.get("caller_root").is_none(),
            "caller_root == root is omitted"
        );
        assert!(e.get("task_ref").is_none(), "absent task_ref stays absent");
        assert_eq!(e["last_seen_ms"], serde_json::json!(p.last_beat_ms));
        assert_eq!(e["first_seen_ms"], serde_json::json!(p.first_seen_ms));
        assert_eq!(
            e["mutation"],
            serde_json::json!({}),
            "quiet read-only presence"
        );

        p.caller_root = Some("/wt/lane-a".to_string()); // differs → present
        p.task_ref = Some("msn_000000000000_neutral".to_string());
        p.mutation.observed_at_ms = Some(now);
        let e2 = wire_entry(&p);
        assert_eq!(e2["caller_root"], "/wt/lane-a");
        assert_eq!(e2["task_ref"], "msn_000000000000_neutral");
        assert_eq!(e2["mutation"]["observed_at_ms"], serde_json::json!(now));
        assert!(e2["mutation"].get("declared_intent").is_none());
    }

    /// P1-UI contract — the wire `PresenceCollision`: the measurable arm maps to
    /// reason `same_worktree` carrying the shared location; a pure working-set
    /// overlap maps to `declared_overlap` with NO location claimed; `agent_ids`
    /// names both hands; the envelope's `collisions` is present even when empty
    /// (server-authoritative).
    #[test]
    fn wire_collision_and_response_match_the_ui_contract() {
        let now = now_ms();
        // same_worktree arm (shared caller_root).
        let mut a = rec("hand-a", "/brain/one", now);
        let mut b = rec("hand-b", "/brain/one", now);
        a.caller_root = Some("/wt/shared".to_string());
        b.caller_root = Some("/wt/shared".to_string());
        a.mutation.declared_intent = Some("mutate".to_string());
        b.mutation.observed_at_ms = Some(now);
        let roster = vec![a, b];
        let body = wire_response(&roster, now);
        assert_eq!(body["presences"].as_array().unwrap().len(), 2);
        let collisions = body["collisions"].as_array().unwrap();
        assert_eq!(collisions.len(), 1);
        assert_eq!(collisions[0]["reason"], "same_worktree");
        assert_eq!(collisions[0]["brain_root"], "/brain/one");
        assert_eq!(collisions[0]["caller_root"], "/wt/shared");
        let ids = collisions[0]["agent_ids"].as_array().unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&serde_json::json!("hand-a")));
        assert!(ids.contains(&serde_json::json!("hand-b")));

        // declared_overlap arm (working-set only, no shared location).
        let mut c = rec("hand-c", "/brain/one", now);
        let mut d = rec("hand-d", "/brain/one", now);
        c.working_set = vec!["src/shared.rs".into()];
        d.working_set = vec!["src/shared.rs".into()];
        c.mutation.declared_intent = Some("mutate".to_string());
        d.mutation.declared_intent = Some("mutate".to_string());
        let roster2 = vec![c, d];
        let body2 = wire_response(&roster2, now);
        let col2 = &body2["collisions"].as_array().unwrap()[0];
        assert_eq!(col2["reason"], "declared_overlap");
        assert!(
            col2.get("caller_root").is_none(),
            "declared_overlap claims no location"
        );

        // Empty roster → honest empty envelope, collisions PRESENT (authoritative).
        let empty = wire_response(&[], now);
        assert_eq!(empty["presences"], serde_json::json!([]));
        assert_eq!(empty["collisions"], serde_json::json!([]));
    }
}
