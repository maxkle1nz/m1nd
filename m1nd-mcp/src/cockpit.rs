//! cockpit — the navigable m1nd menu (`m1nd-cockpit-v0`).
//!
//! A DEDICATED read-only verb (askGOD verdict "the navigable cockpit",
//! 2026-07-12 — the 10 amendments; `docs/voice/ASKGOD-VERDICT-COCKPIT.md`).
//! It is the human's on-request router over m1nd's read surfaces — a sibling of
//! `north`, NEVER a field of it. The laws it obeys, by amendment:
//!
//! 1. DEDICATED read-only verb, a pure composer sibling of `north`; fail-open
//!    does NOT apply — if it breaks it breaks ALONE, never taking `north` down.
//! 2. The read-only law is DERIVED from the single source: every routed verb is
//!    filtered at compose time against `server::read_only_denied`, and the test
//!    `cockpit_read_verbs ∩ READ_ONLY_DENIED_TOOLS = ∅` pins it — no hand-kept
//!    second list.
//! 3. Pointer entries (the tray) carry NO `verb` field — only door text; there
//!    is nothing to execute even by mistake.
//! 4. The root is argument-less READS only (the map/health/trust/memories/drift
//!    reads + the tray/missions pointers). `impact`/`why`/`trace` stay OUT of
//!    the root (future per-item drill-downs).
//! 5. Max depth 3 (root → collection → item); `select:"0"` re-serves the root.
//! 6. The permanent trio sits in STABLE SLOTS (condition changes the LABEL, not
//!    the slot); `menu_sig` is mandatory; a drill re-asserts `store_version` +
//!    `state_sig` and says "state moved" honestly when the caller's
//!    `seen_store_version` diverged.
//! 8. On-request only — the caller invokes it ("?"/explicit ask); at a landing
//!    the S5 card speaks, never an auto-served menu.
//! 9. Its OWN budget, battery-pinned (north's pin does not cover it).
//! 10. It REUSES the help machine — each read entry's `why` is lifted from
//!     `help_guidance::catalog_entry` (the one canonical catalog), never a
//!     parallel one.
//!
//! The widget payload law (amendment 7) lives in `docs/voice/WIDGET-TEMPLATE.md`:
//! a button carries the SAME short reference a human would type — a number +
//! `menu_sig` — never free command text, never a graph-interpolated string, and
//! never a write verb.

use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use m1nd_core::error::{M1ndError, M1ndResult};

use crate::session::SessionState;

/// Wire schema of the cockpit response.
pub const COCKPIT_SCHEMA: &str = "m1nd-cockpit-v0";

/// The tool name (used by the schema + dispatch arm).
pub const COCKPIT_TOOL: &str = "cockpit";

/// The read verbs the cockpit routes to — the SINGLE list the read-only test
/// (amendment 2) intersects with `READ_ONLY_DENIED_TOOLS`. Each is argument-less
/// (only `agent_id`, plus `boot_memory`'s fixed `action:"list"` projection).
pub fn cockpit_read_verbs() -> &'static [&'static str] {
    &[
        "system_blocks_snapshot",
        "doctor",
        "trust",
        "boot_memory",
        "drift",
    ]
}

/// One root collection. `verb == None` marks a POINTER (a door, amendment 3);
/// `verb == Some` marks an argument-less READ the human/agent runs.
struct Collection {
    slot: u8,
    key: &'static str,
    label: String,
    /// Present iff this is a READ entry — never for a pointer (amendment 3).
    verb: Option<&'static str>,
    /// Present iff this is a POINTER — the door text, never an executable verb.
    door: Option<&'static str>,
}

/// The live facts the cockpit reads once, to label the menu honestly.
struct Facts {
    merge_wait: usize,
    mission_total: usize,
    store_present: bool,
    store_version: Option<u64>,
    ratified_blocks: usize,
    block_count: usize,
    coherence: Option<String>,
    memory_count: usize,
    recv_mismatch: bool,
    reception_honest: Option<String>,
    // P1 (ORGANISM-INSIDE): the served-brain presence roster + derived collisions
    // (slot 8). "This brain" only — never a cross-brain aggregate.
    presences: Vec<crate::presence::PresenceRecord>,
    presence_collisions: Vec<crate::presence::Collision>,
}

/// Cap on presence rows the drill renders (Budget Law — the roster is a bounded
/// render, never an unbounded list; the count + `capped` flag stay honest when
/// more sessions are live than shown). Sized so the drill holds under the ≤800
/// token ceiling even at worst-case field lengths (battery-pinned below).
const PRESENCE_DRILL_CAP: usize = 6;

/// `cockpit` (READ). Serves the root menu, or drills one collection when
/// `select` names a slot. Pure composer, read-only, breaks alone.
pub fn handle_cockpit(state: &mut SessionState, params: &Value) -> M1ndResult<Value> {
    let agent_id = params
        .get("agent_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: COCKPIT_TOOL.to_string(),
            detail: "agent_id is required".to_string(),
        })?
        .to_string();

    let select = params
        .get("select")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let seen_store_version = params.get("seen_store_version").and_then(Value::as_u64);

    let facts = read_facts(state);
    let state_sig = compose_state_sig(&facts);

    // Build the stable-slot collections, then DERIVE the read-only law: drop any
    // read entry whose verb became a write (amendment 2). Today none do — the
    // filter makes the containment MECHANICAL, not a hand-kept promise.
    let collections: Vec<Collection> = root_collections(&facts)
        .into_iter()
        .filter(|c| {
            c.verb
                .map(|v| !crate::server::read_only_denied(v, &json!({})))
                .unwrap_or(true)
        })
        .collect();

    let menu_sig = compose_menu_sig(&collections, facts.store_version, &state_sig);

    match select {
        None | Some("0") => Ok(compose_root(
            &agent_id,
            &collections,
            &facts,
            &menu_sig,
            &state_sig,
        )),
        Some(sel) => match sel.parse::<u8>() {
            Ok(slot) if (1..=collections.len() as u8).contains(&slot) => Ok(compose_drill(
                &agent_id,
                &collections,
                slot,
                &facts,
                &menu_sig,
                &state_sig,
                seen_store_version,
            )),
            _ => Err(M1ndError::InvalidParams {
                tool: COCKPIT_TOOL.to_string(),
                detail: format!(
                    "select must be \"0\" (re-serve root) or a slot 1..={} — got {sel:?}",
                    collections.len()
                ),
            }),
        },
    }
}

/// Read the live facts ONCE from the cheap surfaces `north` already touches: the
/// mission box (bell + missions), the served brain's SystemBlock store (map),
/// the durable-memory count, and the reception verdict (honesty). Every read
/// FAILS OPEN — an unreadable surface labels as empty, never an error.
fn read_facts(state: &mut SessionState) -> Facts {
    // Bell + missions — the same box the tray and `mission_post` speak.
    let (merge_wait, mission_total) = {
        let box_path = crate::mission_letter_handlers::mission_box_path(state);
        match crate::mailbox::read_letters(&box_path) {
            Ok(letters) => {
                let heads = crate::mission_letter::heads_by_mission(&letters);
                let merge_wait = heads
                    .values()
                    .filter(|h| h.head.phase == crate::mission_letter::Phase::MergeWait)
                    .count();
                (merge_wait, heads.len())
            }
            Err(_) => (0, 0),
        }
    };

    // The map — the SERVED brain's store only (per-brain, never cross-brain).
    let (store_present, store_version, ratified_blocks, block_count, coherence) =
        match crate::system_blocks_handlers::handle_system_blocks_snapshot(
            state,
            crate::system_blocks_handlers::SnapshotInput { agent_id: None },
        ) {
            Ok(snap) if snap["present"] == Value::Bool(true) => {
                let store_version = snap["store_version"].as_u64();
                let block_count = snap["block_count"].as_u64().unwrap_or(0) as usize;
                let ratified_blocks = snap["store"]["blocks"]
                    .as_array()
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter(|b| b["state"] == json!("ratified"))
                            .count()
                    })
                    .unwrap_or(0);
                let coherence = snap["skeleton_coherence"]["status"]
                    .as_str()
                    .map(str::to_string);
                (true, store_version, ratified_blocks, block_count, coherence)
            }
            _ => (false, None, 0, 0, None),
        };

    let memory_count = state.light_memory_count();

    // P1 presences — the served brain's live roster + derived collisions. Reads
    // the presence sidecars (fail-open: an unreadable registry yields an empty
    // roster, never an error). Scoped to THIS brain by construction.
    let (presences, presence_collisions) = state.presence_roster();

    let reception = state.reception_verdict();
    let recv_mismatch = reception
        .as_ref()
        .and_then(|r| r.get("match").and_then(Value::as_str))
        == Some("caller_root_mismatch");
    let reception_honest = reception
        .as_ref()
        .filter(|_| recv_mismatch)
        .and_then(|r| r.get("honest").and_then(Value::as_str))
        .map(str::to_string);

    Facts {
        merge_wait,
        mission_total,
        store_present,
        store_version,
        ratified_blocks,
        block_count,
        coherence,
        memory_count,
        recv_mismatch,
        reception_honest,
        presences,
        presence_collisions,
    }
}

/// The eight stable-slot collections (amendment 4 order). Slots 1–3 are the
/// permanent trio (the tray, the map, missions); their LABELS move with state,
/// their SLOTS never do (amendment 6). Slots 4–7 are the argument-less reads.
/// Slot 8 is the P1 presence roster — the team talking to m1nd, this brain only,
/// a cockpit-composed render drilled in place (ORGANISM-INSIDE, verdict
/// 2026-07-13; the collision warning rides the LABEL, no new schema field).
fn root_collections(f: &Facts) -> Vec<Collection> {
    // 1 — the tray (POINTER: bell + the human stamp gesture; no verb).
    let tray_label = if f.merge_wait > 0 {
        format!("the tray · {} await your stamp", f.merge_wait)
    } else {
        "the tray · quiet".to_string()
    };
    // 2 — the map (READ: the served brain's SystemBlock store).
    let map_label = if !f.store_present {
        "the map · no skeleton yet".to_string()
    } else if f.ratified_blocks > 0 {
        format!("the map · {} blocks ratified", f.ratified_blocks)
    } else {
        format!("the map · {} blocks (candidate)", f.block_count)
    };
    // 3 — missions (POINTER → the tray; the box is not an MCP read verb).
    let missions_label = if f.mission_total > 0 {
        format!(
            "missions · {} in flight ({} awaiting)",
            f.mission_total, f.merge_wait
        )
    } else {
        "missions · none in flight".to_string()
    };
    // 6 — recent memories (READ: boot_memory, the fixed-N projection).
    let memories_label = if f.memory_count > 0 {
        format!("memories · {} durable claim(s)", f.memory_count)
    } else {
        "memories · none yet".to_string()
    };
    // 8 — presences (POINTER: the cockpit-composed roster of THIS brain, drilled
    //     in place; no external verb). The collision warning rides the LABEL —
    //     ONE root line whether quiet or colliding (Budget Law).
    let presences_label = if f.presences.is_empty() {
        "presences · none in this brain".to_string()
    } else if !f.presence_collisions.is_empty() {
        format!(
            "presences · {} in this brain · ⚠ {} collision(s)",
            f.presences.len(),
            f.presence_collisions.len()
        )
    } else {
        format!("presences · {} in this brain", f.presences.len())
    };

    vec![
        Collection {
            slot: 1,
            key: "tray",
            label: tray_label,
            verb: None,
            door: Some("open the tray — the stamp lives there"),
        },
        Collection {
            slot: 2,
            key: "map",
            label: map_label,
            verb: Some("system_blocks_snapshot"),
            door: None,
        },
        Collection {
            slot: 3,
            key: "missions",
            label: missions_label,
            verb: None,
            door: Some("the tray lists them — the stamp lands there"),
        },
        Collection {
            slot: 4,
            key: "health",
            label: "health · the doctor's read (topology · verification · git · alerts)"
                .to_string(),
            verb: Some("doctor"),
            door: None,
        },
        Collection {
            slot: 5,
            key: "trust",
            label: "trust · the binding's verdict + freshness".to_string(),
            verb: Some("trust"),
            door: None,
        },
        Collection {
            slot: 6,
            key: "memories",
            label: memories_label,
            verb: Some("boot_memory"),
            door: None,
        },
        Collection {
            slot: 7,
            key: "drift",
            label: "drift · nodes whose evidence moved under the graph".to_string(),
            verb: Some("drift"),
            door: None,
        },
        Collection {
            slot: 8,
            key: "presences",
            label: presences_label,
            verb: None,
            door: Some("the team talking to m1nd — drill to see who, where, since when"),
        },
    ]
}

/// The mechanical anti-repetition / re-affirmation key: the menu's measured
/// state. Same shape family as the human_view state_sig; carries the map's
/// `store_version` so a drill can honestly say "state moved".
fn compose_state_sig(f: &Facts) -> String {
    format!(
        "bell:{}|map:{}|coh:{}|recv:{}|sv:{}",
        f.merge_wait,
        f.ratified_blocks,
        f.coherence.as_deref().unwrap_or("none"),
        if f.recv_mismatch { "mismatch" } else { "match" },
        f.store_version
            .map(|v| v.to_string())
            .unwrap_or_else(|| "none".to_string()),
    )
}

/// `menu_sig` — a short, stable digest of the served menu (slots + labels +
/// verbs) plus the store version and state signature. This is the exact short
/// reference a widget button carries back (amendment 7), never free text.
fn compose_menu_sig(
    collections: &[Collection],
    store_version: Option<u64>,
    state_sig: &str,
) -> String {
    let mut hasher = DefaultHasher::new();
    for c in collections {
        c.slot.hash(&mut hasher);
        c.key.hash(&mut hasher);
        c.label.hash(&mut hasher);
        c.verb.hash(&mut hasher);
        c.door.hash(&mut hasher);
    }
    store_version.hash(&mut hasher);
    state_sig.hash(&mut hasher);
    format!("mc_{:08x}", (hasher.finish() as u32))
}

/// Serialize one collection as a root entry. Read entries carry the canonical
/// `why` lifted from the help catalog (amendment 10); pointer entries carry the
/// door text and NO verb (amendment 3).
fn entry_json(c: &Collection) -> Value {
    match (c.verb, c.door) {
        (Some(verb), _) => {
            let why = crate::help_guidance::catalog_entry(verb)
                .map(|e| e.one_liner)
                .unwrap_or_default();
            json!({
                "slot": c.slot,
                "key": c.key,
                "kind": "read",
                "label": c.label,
                "verb": verb,
                "why": why,
            })
        }
        (None, Some(door)) => json!({
            "slot": c.slot,
            "key": c.key,
            "kind": "pointer",
            "label": c.label,
            "door": door,
        }),
        // A collection with neither verb nor door is a construction bug; render
        // it as an inert pointer rather than fabricating a verb.
        (None, None) => json!({
            "slot": c.slot,
            "key": c.key,
            "kind": "pointer",
            "label": c.label,
        }),
    }
}

/// The slot-8 drill body: the served brain's live presence roster (capped at
/// [`PRESENCE_DRILL_CAP`]) + any derived collisions. A cockpit-composed render —
/// measured from the sidecars, ages ALWAYS rendered, honest-absent on expiry.
/// Scope is decided and LABELED here: the served-brain roster ("this brain").
fn presence_drill_detail(f: &Facts) -> Value {
    let now = crate::util::now_ms();
    let rows: Vec<Value> = f
        .presences
        .iter()
        .take(PRESENCE_DRILL_CAP)
        .map(|p| {
            // where = the most specific declared location, else the brain root.
            let location = p
                .worktree
                .clone()
                .or_else(|| p.caller_root.clone())
                .unwrap_or_else(|| p.brain.clone());
            // The verdict's four fields — agent · where · age · mutation — plus
            // the declared theme (the "on what"). Richer enrichment (kind /
            // task_ref) lives in the unbudgeted /api/health roster, not this
            // budget-bound drill.
            json!({
                "agent": p.agent_id,
                "theme": p.theme,
                "where": location,
                "age_secs": p.age_ms(now) / 1000,
                "mutating": p.has_mutation_signal(now),
            })
        })
        .collect();
    let collisions: Vec<Value> = f
        .presence_collisions
        .iter()
        .map(|c| {
            json!({
                "between": [&c.subject_agent, &c.other_agent],
                "overlap": c.overlap,
            })
        })
        .collect();
    json!({
        "kind": "presences",
        "scope": "this brain",
        "roster_count": f.presences.len(),
        "shown": rows.len(),
        "capped": f.presences.len() > PRESENCE_DRILL_CAP,
        "roster": rows,
        "collisions": collisions,
        "note": "presence == activity VISIBLE TO m1nd — a session compiling for minutes makes no calls and expires from the roster; never read as 'who is alive'",
    })
}

fn reception_note(f: &Facts) -> Option<Value> {
    f.recv_mismatch.then(|| {
        json!({
            "match": "caller_root_mismatch",
            "honest": f
                .reception_honest
                .clone()
                .unwrap_or_else(|| "this graph does NOT cover your repo".to_string()),
            "note": "this menu describes the BOUND brain, not your repo — ingest your root to route to your own brain",
        })
    })
}

fn non_claims() -> Value {
    json!([
        "cockpit composes read-only menu entries; it never mutates, ratifies, imports, or lands anything.",
        "the menu describes the BOUND brain only — never a cross-brain aggregate.",
        "pointer entries carry no verb — the stamp is a human gesture at the tray, never a cockpit click.",
        "labels are facts measured at compose time; run the routed read to see live detail.",
    ])
}

fn compose_root(
    agent_id: &str,
    collections: &[Collection],
    f: &Facts,
    menu_sig: &str,
    state_sig: &str,
) -> Value {
    let entries: Vec<Value> = collections.iter().map(entry_json).collect();
    let mut out = json!({
        "schema": COCKPIT_SCHEMA,
        "depth": 0,
        "title": "m1nd cockpit — reply with a slot number to look; 0 re-serves this root",
        "menu_sig": menu_sig,
        "state_sig": state_sig,
        "store_version": f.store_version,
        "entries": entries,
        "on_request_only": true,
        "agent_id": agent_id,
        "non_claims": non_claims(),
    });
    if let Some(recv) = reception_note(f) {
        out.as_object_mut()
            .unwrap()
            .insert("reception".to_string(), recv);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn compose_drill(
    agent_id: &str,
    collections: &[Collection],
    slot: u8,
    f: &Facts,
    menu_sig: &str,
    state_sig: &str,
    seen_store_version: Option<u64>,
) -> Value {
    let c = collections
        .iter()
        .find(|c| c.slot == slot)
        .expect("slot bounds checked by caller");

    // Amendment 6: a drill re-asserts store_version + state_sig, and says "state
    // moved" honestly when the caller's snapshot diverged.
    let state_moved =
        matches!((seen_store_version, f.store_version), (Some(seen), sv) if Some(seen) != sv);

    let mut detail = if c.key == "presences" {
        // Slot 8: a cockpit-composed roster drilled IN PLACE (no external verb).
        presence_drill_detail(f)
    } else {
        match (c.verb, c.door) {
            (Some(verb), _) => {
                let why = crate::help_guidance::catalog_entry(verb)
                    .map(|e| e.one_liner)
                    .unwrap_or_default();
                // Present the argument-less call to RUN (router pattern, like help):
                // its own output carries the receipts/hashes, rendered in the item
                // view (amendment 5) — the cockpit never fabricates a receipt.
                let arguments = if verb == "boot_memory" {
                    json!({ "agent_id": agent_id, "action": "list" })
                } else {
                    json!({ "agent_id": agent_id })
                };
                json!({
                    "kind": "read",
                    "verb": verb,
                    "arguments": arguments,
                    "why": why,
                    "note": "run this read — its output carries the live detail (receipts/hashes render in the item view)",
                })
            }
            (None, door) => json!({
                "kind": "pointer",
                "door": door.unwrap_or("open the tray — the stamp lives there"),
                // The pointer's facts are already measured — no verb, nothing to run.
                "facts": {
                    "merge_wait": f.merge_wait,
                    "missions_in_flight": f.mission_total,
                },
                "note": "the stamp is a human gesture at the tray — the cockpit never lands it",
            }),
        }
    };

    let obj = detail.as_object_mut().unwrap();
    obj.insert("schema".to_string(), json!(COCKPIT_SCHEMA));
    obj.insert("depth".to_string(), json!(1));
    obj.insert("slot".to_string(), json!(slot));
    obj.insert("key".to_string(), json!(c.key));
    obj.insert("label".to_string(), json!(c.label));
    obj.insert("menu_sig".to_string(), json!(menu_sig));
    obj.insert("state_sig".to_string(), json!(state_sig));
    obj.insert("store_version".to_string(), json!(f.store_version));
    obj.insert("state_moved".to_string(), json!(state_moved));
    if state_moved {
        obj.insert(
            "state_moved_note".to_string(),
            json!("the store moved since you last read it — re-serve the root (select 0) before you act"),
        );
    }
    obj.insert("back".to_string(), json!("select 0 to re-serve the root"));
    if let Some(recv) = reception_note(f) {
        obj.insert("reception".to_string(), recv);
    }
    obj.insert("non_claims".to_string(), non_claims());
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Amendment 2 — the read-only law, DERIVED, not hand-kept: NO cockpit read
    /// verb may appear in the write deny-list. This is the mechanical guard the
    /// verdict demanded (`entries.verb ∩ READ_ONLY_DENIED_TOOLS = ∅`).
    #[test]
    fn cockpit_read_verbs_are_never_write_denied() {
        for verb in cockpit_read_verbs() {
            assert!(
                !crate::server::read_only_denied(verb, &json!({})),
                "cockpit routes to `{verb}`, but it is on the read-only write deny-list — \
                 a menu entry must never name a mutation (amendment 2)"
            );
        }
    }

    fn empty_facts() -> Facts {
        Facts {
            merge_wait: 0,
            mission_total: 0,
            store_present: false,
            store_version: None,
            ratified_blocks: 0,
            block_count: 0,
            coherence: None,
            memory_count: 0,
            recv_mismatch: false,
            reception_honest: None,
            presences: Vec::new(),
            presence_collisions: Vec::new(),
        }
    }

    fn presence(agent: &str, brain: &str) -> crate::presence::PresenceRecord {
        let now = crate::util::now_ms();
        crate::presence::PresenceRecord {
            schema: crate::presence::PRESENCE_SCHEMA.to_string(),
            presence_id: crate::presence::stable_presence_id(agent, brain),
            agent_id: agent.to_string(),
            brain: brain.to_string(),
            caller_root: None,
            kind: Some("executor".to_string()),
            theme: Some("slice".to_string()),
            worktree: None,
            working_set: Vec::new(),
            task_ref: None,
            mutation: crate::presence::MutationSignal::default(),
            first_seen_ms: now,
            last_beat_ms: now,
            query_count: 3,
            ttl_ms: crate::presence::PRESENCE_TTL_MS,
        }
    }

    /// Amendment 3 — pointer entries (the tray, missions) carry NO verb field;
    /// there is nothing to execute even by mistake.
    #[test]
    fn pointer_entries_carry_no_verb() {
        let cols = root_collections(&empty_facts());
        for c in &cols {
            if c.door.is_some() {
                assert!(
                    c.verb.is_none(),
                    "pointer collection `{}` must not carry a verb",
                    c.key
                );
                let e = entry_json(c);
                assert_eq!(e["kind"], "pointer");
                assert!(e.get("verb").is_none(), "no verb serialized for a pointer");
            }
        }
        // The tray and missions are pointers; the map/health/trust/memories/drift
        // are reads.
        let tray = cols.iter().find(|c| c.key == "tray").unwrap();
        assert!(tray.verb.is_none() && tray.door.is_some());
        let map = cols.iter().find(|c| c.key == "map").unwrap();
        assert_eq!(map.verb, Some("system_blocks_snapshot"));
    }

    /// Amendment 6 — the permanent trio sits in stable slots 1..=3, condition
    /// changing only the LABEL; and the root always serves the eight collections.
    #[test]
    fn slots_are_stable_across_conditions() {
        let quiet = root_collections(&empty_facts());
        let mut busy = empty_facts();
        busy.merge_wait = 3;
        busy.mission_total = 4;
        busy.store_present = true;
        busy.ratified_blocks = 12;
        busy.store_version = Some(7);
        let loud = root_collections(&busy);

        assert_eq!(quiet.len(), 8);
        assert_eq!(loud.len(), 8);
        for (a, b) in quiet.iter().zip(loud.iter()) {
            assert_eq!(a.slot, b.slot, "slot is stable");
            assert_eq!(a.key, b.key, "collection identity is stable");
            assert_eq!(a.verb, b.verb, "verb (or its absence) is stable");
        }
        // The label DID move for the stateful trio members.
        let tray_quiet = &quiet[0].label;
        let tray_loud = &loud[0].label;
        assert_ne!(tray_quiet, tray_loud, "the tray label moves with the bell");
        assert!(loud[0].label.contains('3'), "the loud tray names the count");
        assert!(loud[1].label.contains("12 blocks ratified"));
    }

    /// The menu_sig is deterministic for an equal menu and shifts when the menu
    /// shifts — the short reference a widget button carries (amendment 7).
    #[test]
    fn menu_sig_is_stable_and_shifts_with_state() {
        let f = empty_facts();
        let cols = root_collections(&f);
        let sig_a = compose_menu_sig(&cols, f.store_version, &compose_state_sig(&f));
        let sig_b = compose_menu_sig(&cols, f.store_version, &compose_state_sig(&f));
        assert_eq!(sig_a, sig_b, "equal menu ⇒ equal sig");
        assert!(
            sig_a.starts_with("mc_"),
            "menu_sig is the short mc_ reference"
        );

        let mut moved = empty_facts();
        moved.merge_wait = 2;
        let cols2 = root_collections(&moved);
        let sig_c = compose_menu_sig(&cols2, moved.store_version, &compose_state_sig(&moved));
        assert_ne!(sig_a, sig_c, "a changed menu ⇒ a changed sig");
    }

    /// A drill re-asserts store_version + state_sig and flags a diverged snapshot
    /// honestly (amendment 6).
    #[test]
    fn drill_flags_state_moved_when_store_version_diverged() {
        let mut f = empty_facts();
        f.store_present = true;
        f.store_version = Some(9);
        let cols = root_collections(&f);
        let sig = compose_menu_sig(&cols, f.store_version, &compose_state_sig(&f));
        let ss = compose_state_sig(&f);

        // Caller saw v7, store is now v9 → state moved.
        let drill = compose_drill("t", &cols, 2, &f, &sig, &ss, Some(7));
        assert_eq!(drill["state_moved"], json!(true));
        assert!(drill.get("state_moved_note").is_some());
        assert_eq!(drill["store_version"], json!(9));
        assert_eq!(drill["depth"], json!(1));

        // Caller saw the current version → no divergence.
        let fresh = compose_drill("t", &cols, 2, &f, &sig, &ss, Some(9));
        assert_eq!(fresh["state_moved"], json!(false));
    }

    /// A pointer drill carries the door + measured facts and NO verb to run
    /// (amendment 3); a read drill carries the argument-less call to run.
    #[test]
    fn drill_shapes_match_the_entry_kind() {
        let mut f = empty_facts();
        f.merge_wait = 1;
        let cols = root_collections(&f);
        let sig = compose_menu_sig(&cols, f.store_version, &compose_state_sig(&f));
        let ss = compose_state_sig(&f);

        // Slot 1 = the tray pointer.
        let tray = compose_drill("t", &cols, 1, &f, &sig, &ss, None);
        assert_eq!(tray["kind"], "pointer");
        assert!(tray.get("verb").is_none(), "a pointer drill runs no verb");
        assert_eq!(tray["facts"]["merge_wait"], json!(1));

        // Slot 6 = memories read → fixed action=list projection.
        let mem = compose_drill("t", &cols, 6, &f, &sig, &ss, None);
        assert_eq!(mem["kind"], "read");
        assert_eq!(mem["verb"], json!("boot_memory"));
        assert_eq!(mem["arguments"]["action"], json!("list"));
    }

    /// P1 budget RE-PIN (battery): the cockpit's own budget holds after adding
    /// the 8th collection — root AND the presences drill both stay under the ≤800
    /// ceiling (amendment 9). Measured with the same `chars/4` estimator the north
    /// battery uses. The 8th ROOT line costs ~one line (the ~105-token root slack
    /// the verdict noted absorbs it); the presences DRILL is a bounded render
    /// (capped at PRESENCE_DRILL_CAP rows).
    #[test]
    fn cockpit_budget_holds_with_the_eighth_slot() {
        // A LOUD root: every label populated + a colliding presence roster (the
        // longest presence root line).
        let mut f = empty_facts();
        f.merge_wait = 7;
        f.mission_total = 12;
        f.store_present = true;
        f.store_version = Some(4096);
        f.ratified_blocks = 128;
        f.block_count = 130;
        f.coherence = Some("mismatch".to_string());
        f.memory_count = 342;
        f.presences = (0..PRESENCE_DRILL_CAP)
            .map(|i| {
                let mut p = presence(&format!("executor-agent-number-{i}"), "/Users/x/m1nd");
                p.worktree = Some(format!("feat/some-longish-worktree-name-{i}"));
                p.caller_root = Some(format!("/Users/x/m1nd-worktree-{i}"));
                p.task_ref = Some(format!("msn_1783903278219_executoragent{i}"));
                p.mutation.observed_at_ms = Some(crate::util::now_ms());
                p
            })
            .collect();
        f.presence_collisions = vec![crate::presence::Collision {
            subject_agent: "executor-agent-number-0".into(),
            subject_presence: "prs_000000000000".into(),
            other_agent: "executor-agent-number-1".into(),
            other_presence: "prs_000000000001".into(),
            overlap: vec!["worktree:feat/some-longish-worktree-name-0".into()],
        }];

        let cols = root_collections(&f);
        let ss = compose_state_sig(&f);
        let sig = compose_menu_sig(&cols, f.store_version, &ss);

        // Pretty-print basis (the conservative, larger number — comparable to the
        // pre-P1 manual pins ~695 root / ~430 drill).
        let root = compose_root("budget-probe-agent", &cols, &f, &sig, &ss);
        let root_chars = serde_json::to_string_pretty(&root).unwrap().chars().count();
        let root_tokens = crate::result_shaping::estimate_tokens_from_chars(root_chars);

        let drill = compose_drill("budget-probe-agent", &cols, 8, &f, &sig, &ss, None);
        let drill_chars = serde_json::to_string_pretty(&drill)
            .unwrap()
            .chars()
            .count();
        let drill_tokens = crate::result_shaping::estimate_tokens_from_chars(drill_chars);

        eprintln!(
            "cockpit budget re-pin (P1, 8 slots): root ~{root_tokens} tokens ({root_chars} chars); presences drill ~{drill_tokens} tokens ({drill_chars} chars)"
        );
        assert!(
            root_tokens <= 800,
            "cockpit root budget breach: ~{root_tokens} tokens (>800 ceiling, amendment 9)"
        );
        assert!(
            drill_tokens <= 800,
            "cockpit presences-drill budget breach: ~{drill_tokens} tokens (>800 ceiling)"
        );
    }

    /// P1 — slot 8 is the presence roster. Its ROOT line is ONE line whether
    /// quiet or colliding (Budget Law), the collision warning rides the LABEL
    /// (no new schema field), and the DRILL renders the capped roster + the
    /// derived collisions, scope LABELED "this brain".
    #[test]
    fn presences_slot_root_line_and_drill() {
        // Quiet: one root line, no warning.
        let mut quiet = empty_facts();
        quiet.presences = vec![presence("exec-a", "/m1nd")];
        let cols = root_collections(&quiet);
        assert_eq!(cols.len(), 8);
        let slot8 = cols.iter().find(|c| c.key == "presences").unwrap();
        assert_eq!(slot8.slot, 8);
        assert!(slot8.verb.is_none(), "presences is a pointer, never a verb");
        assert!(slot8.label.contains("1 in this brain"));
        assert!(
            !slot8.label.contains("collision"),
            "quiet root names no collision"
        );

        // Colliding: the SAME one root line, now carrying the warning.
        let mut loud = empty_facts();
        loud.presences = vec![presence("hand-a", "/m1nd"), presence("hand-b", "/m1nd")];
        loud.presence_collisions = vec![crate::presence::Collision {
            subject_agent: "hand-a".into(),
            subject_presence: "prs_a".into(),
            other_agent: "hand-b".into(),
            other_presence: "prs_b".into(),
            overlap: vec!["worktree:feat/x".into()],
        }];
        let loud_cols = root_collections(&loud);
        assert_eq!(loud_cols.len(), 8, "the root is still ONE line per slot");
        let loud8 = loud_cols.iter().find(|c| c.key == "presences").unwrap();
        assert!(
            loud8.label.contains("⚠ 1 collision"),
            "the warning rides the label"
        );

        // Drill: capped roster + collisions, scope labeled.
        let sig = compose_menu_sig(&loud_cols, loud.store_version, &compose_state_sig(&loud));
        let ss = compose_state_sig(&loud);
        let drill = compose_drill("t", &loud_cols, 8, &loud, &sig, &ss, None);
        assert_eq!(drill["kind"], "presences");
        assert_eq!(drill["scope"], "this brain");
        assert_eq!(drill["roster_count"], json!(2));
        assert_eq!(drill["collisions"].as_array().unwrap().len(), 1);
        assert!(
            drill["roster"][0].get("age_secs").is_some(),
            "age is always rendered"
        );
    }
}
