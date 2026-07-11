//! Human View v2 F11-a — the `candidate_edit` engine.
//!
//! One verb, typed operations, one OCC transaction — never six loose verbs
//! (HUMAN-VIEW-V2-F11-TECH §1). This module is the PURE engine: [`apply_edits`]
//! clones the store, applies the WHOLE batch to the clone, validates every final
//! invariant, and returns the edited clone ONLY on total success. The caller
//! ([`crate::system_blocks::candidate_edit_in_dir`]) owns OCC, the candidate-only
//! gate, persistence, and the single `store_version` bump.
//!
//! The six oracle objections are law here:
//! - **o1 preflight-on-a-clone** — no persistence until every op AND every final
//!   invariant passes; the first failure returns its op index and the ORIGINAL store
//!   is untouched. A partial apply under OCC is less safe than none.
//! - **o2 merge canonicalization before any member op** — a `merged_id → survivor_id`
//!   map is built and validated (no cycle, merge-into-victim, or ambiguity) BEFORE
//!   any member op runs; every `from`/`to`/`block_id` is canonicalized through it, so
//!   an op naming a block another op absorbs resolves to the survivor.
//! - **o3 splits are explicit path groups** — validated (non-empty, disjoint,
//!   total) against the block's current membership; the split is reproducible from
//!   the stored ops alone.
//! - **o5 the naming sanitizer governs the RUNNER seat** — a runner rename is
//!   untrusted LLM output, so every name/purpose it proposes passes the naming-lane
//!   sanitizer ([`crate::naming_runner::sanitize_rename_fields`]) before it can touch
//!   the store; a violation refuses the op (and, via o1, the whole batch). The owner
//!   seat is the human at the screen and is not hard-sanitized.
//! - **o6 provenance is a real state** — a rename stamps [`NamedBy::Owner`] (GUI seat)
//!   or [`NamedBy::Runner`] (agent seat) and clears `needs_owner_naming`, which the
//!   ratify gate enforces.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::Deserialize;
use serde_json::json;

use crate::skeleton_scan::{directory_support, dominant_directory, humanize_module};
use crate::system_blocks::{
    CandidateMeta, Layout, MembershipEntry, MembershipRole, NamedBy, Socket, Sockets, SystemBlock,
    SystemBlockKind, SystemBlockState, SystemBlockStore,
};

/// One typed edit operation (§1). `serde(tag = "op")` so the wire form is a tagged
/// union: `{"op":"rename", ...}` etc.
// NB: `deny_unknown_fields` is intentionally omitted — serde does not support it on
// internally-tagged enums (`tag = "op"`); an extra field on an op is ignored, which
// is acceptable: the op STRUCTURE is trusted MCP input. The VALUES are not — a
// runner-seat rename's name/purpose runs the o5 sanitizer (see `apply_rename`); only
// the owner seat writes its keystrokes verbatim.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditOp {
    /// Rename (and/or re-purpose) a block. Provenance is stamped from the batch seat
    /// (owner via the GUI, runner via an agent). At least one of `name`/`purpose`.
    Rename {
        block_id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        purpose: Option<String>,
    },
    /// Merge `block_ids` INTO the survivor `into`: union membership (dedup, shared
    /// preserved), rewrite every socket `to:` an absorbed id to the survivor, drop
    /// the absorbed blocks, recompute the survivor's `candidate_meta`.
    Merge {
        into: String,
        block_ids: Vec<String>,
    },
    /// Split a block into N children by explicit path groups (§1c / o3).
    Split { block_id: String, by: SplitBy },
    /// Move an exact member `path` from block `from` to block `to`.
    MoveMember {
        path: String,
        from: String,
        to: String,
    },
    /// Resolve a multi-owner seam: `"both"` keeps the path on all owners as `shared`;
    /// `"primary:<block_id>"` makes that block the primary owner and removes the path
    /// from every other owner (supports 3+ owners in one pass).
    ResolveSeam { path: String, resolution: String },
    /// Move an unmapped `path` into a block as an exact member.
    AssignUnmapped { path: String, block_id: String },
}

/// The explicit path groups a [`EditOp::Split`] partitions a block by (o3). Each
/// group is a list of globs; the groups must be non-empty, disjoint, and total over
/// the block's current membership.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitBy {
    pub paths: Vec<Vec<String>>,
}

/// Which seat authored the batch — the provenance a `rename` stamps (§1c). The GUI
/// (F11 screen) is the owner seat; a curation-mission hand is the runner seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditSeat {
    Owner,
    Runner,
}

impl EditSeat {
    /// Parse the wire string; the default (absent) is the owner seat — the F11 screen
    /// is the primary caller and the friction law is owner-first. A runner-naming
    /// client passes `"runner"` explicitly so it never over-claims owner provenance.
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("owner") {
            "owner" => Ok(Self::Owner),
            "runner" => Ok(Self::Runner),
            other => Err(format!(
                "by must be \"owner\" or \"runner\", got \"{other}\""
            )),
        }
    }

    fn named_by(self) -> NamedBy {
        match self {
            EditSeat::Owner => NamedBy::Owner,
            EditSeat::Runner => NamedBy::Runner,
        }
    }
}

/// A batch failure (o1): the op that failed and why. The whole batch is rejected and
/// nothing is persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditError {
    pub op_index: usize,
    pub reason: String,
}

impl EditError {
    fn at(op_index: usize, reason: impl Into<String>) -> Self {
        Self {
            op_index,
            reason: reason.into(),
        }
    }
}

/// Apply a whole `candidate_edit` batch on a CLONE of `store` (o1). Returns the
/// edited store on total success (the caller persists once + bumps `store_version`
/// once), or the first [`EditError`] with its op index (the caller persists nothing).
/// `store_version` is NOT touched here — that is the caller's single bump.
pub fn apply_edits(
    store: &SystemBlockStore,
    ops: &[EditOp],
    seat: EditSeat,
) -> Result<SystemBlockStore, EditError> {
    let mut clone = store.clone();

    // Ownership of every EXACT member BEFORE the batch — the baseline for the
    // "no unresolved seam CREATED by the batch" invariant (only newly-created
    // multi-ownership is a violation; a pre-existing seam is not ours to flag).
    let before_owners = exact_ownership(store);

    // Phase 0 — classify ops, keeping each op's ORIGINAL index for honest error
    // attribution (ops apply in phase order, not source order).
    let mut merges: Vec<(usize, &String, &Vec<String>)> = Vec::new();
    let mut splits: Vec<(usize, &String, &SplitBy)> = Vec::new();
    let mut members: Vec<(usize, &EditOp)> = Vec::new();
    let mut renames: Vec<(usize, &EditOp)> = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        match op {
            EditOp::Merge { into, block_ids } => merges.push((i, into, block_ids)),
            EditOp::Split { block_id, by } => splits.push((i, block_id, by)),
            EditOp::MoveMember { .. }
            | EditOp::ResolveSeam { .. }
            | EditOp::AssignUnmapped { .. } => members.push((i, op)),
            EditOp::Rename { .. } => renames.push((i, op)),
        }
    }

    // o2 — build + validate the canonical merge map BEFORE any member op.
    let merge_map = build_merge_map(&clone, &merges)?;

    // Track which op removed which block id, so a dangling socket left by the batch
    // is attributed to the op that removed its target.
    let mut removed_by_op: HashMap<String, usize> = HashMap::new();
    // Paths whose ownership the batch touched (added to a block) — the scope of the
    // created-seam invariant.
    let mut touched_paths: HashSet<String> = HashSet::new();

    // Phase 1 — merges (union membership + sockets, drop absorbed).
    apply_merges(&mut clone, &merges, &mut removed_by_op, &mut touched_paths)?;
    // Global socket rewrite (§1d): every `to:` an absorbed id follows to the survivor.
    rewrite_sockets(&mut clone, &merge_map);
    // Recompute each survivor's candidate_meta over its new membership.
    for (_, into, _) in &merges {
        recompute_block_meta_preserving_naming(&mut clone, into);
    }

    // Phase 2 — member ops (move / seam / assign), refs canonicalized through o2.
    for (i, op) in &members {
        apply_member_op(&mut clone, *i, op, &merge_map, &mut touched_paths)?;
    }

    // Phase 3 — splits (partition the block's CURRENT membership into new children).
    for (i, block_id, by) in &splits {
        apply_split(
            &mut clone,
            *i,
            block_id,
            by,
            &merge_map,
            &mut removed_by_op,
            &mut touched_paths,
        )?;
    }

    // Phase 4 — renames (stamp provenance from the seat).
    for (i, op) in &renames {
        if let EditOp::Rename {
            block_id,
            name,
            purpose,
        } = op
        {
            apply_rename(&mut clone, *i, block_id, name, purpose, seat, &merge_map)?;
        }
    }

    // o1 — final invariants over the fully-edited clone.
    validate_final_invariants(
        &clone,
        &before_owners,
        &touched_paths,
        &removed_by_op,
        ops.len(),
    )?;

    Ok(clone)
}

/// Build + validate the canonical `absorbed_id → survivor_id` map (o2). Rejects a
/// self-merge, a duplicate absorption with a different survivor (ambiguity), a
/// merge-into-victim / chain (a survivor that is itself absorbed), and an unknown
/// block reference. The resulting map is flat (one hop).
fn build_merge_map(
    store: &SystemBlockStore,
    merges: &[(usize, &String, &Vec<String>)],
) -> Result<HashMap<String, String>, EditError> {
    let mut map: HashMap<String, String> = HashMap::new();
    let mut survivors: HashSet<String> = HashSet::new();
    for (i, into, block_ids) in merges {
        if !store.blocks.iter().any(|b| &b.block_id == *into) {
            return Err(EditError::at(
                *i,
                format!("merge references unknown survivor block '{into}'"),
            ));
        }
        survivors.insert((*into).clone());
        for absorbed in block_ids.iter() {
            if absorbed == *into {
                return Err(EditError::at(
                    *i,
                    format!("merge cannot absorb the survivor into itself ('{into}')"),
                ));
            }
            if !store.blocks.iter().any(|b| &b.block_id == absorbed) {
                return Err(EditError::at(
                    *i,
                    format!("merge references unknown block '{absorbed}'"),
                ));
            }
            match map.get(absorbed) {
                Some(existing) if existing != *into => {
                    return Err(EditError::at(
                        *i,
                        format!(
                            "ambiguous merge: block '{absorbed}' is absorbed into both '{existing}' and '{into}'"
                        ),
                    ));
                }
                _ => {
                    map.insert(absorbed.clone(), (*into).clone());
                }
            }
        }
    }
    // Reject merge-into-victim / cycles: a survivor that is itself absorbed elsewhere.
    for survivor in &survivors {
        if let Some(target) = map.get(survivor) {
            let victim_op = merges
                .iter()
                .find(|(_, _, ids)| ids.iter().any(|id| id == survivor))
                .map(|(i, _, _)| *i)
                .unwrap_or(0);
            return Err(EditError::at(
                victim_op,
                format!(
                    "merge-into-victim: survivor '{survivor}' is itself absorbed into '{target}' — resolve the merge order into a single survivor"
                ),
            ));
        }
    }
    Ok(map)
}

/// Canonicalize a block ref through the merge map (o2): a name that another op
/// absorbs resolves to the survivor. Flat map — one hop.
fn canon(id: &str, merge_map: &HashMap<String, String>) -> String {
    merge_map.get(id).cloned().unwrap_or_else(|| id.to_string())
}

/// Phase 1 — apply every merge: union each absorbed block's membership + sockets into
/// its survivor (dedup, shared preserved) and drop the absorbed block.
fn apply_merges(
    clone: &mut SystemBlockStore,
    merges: &[(usize, &String, &Vec<String>)],
    removed_by_op: &mut HashMap<String, usize>,
    touched_paths: &mut HashSet<String>,
) -> Result<(), EditError> {
    for (i, into, block_ids) in merges {
        for absorbed in block_ids.iter() {
            let Some(pos) = clone.blocks.iter().position(|b| &b.block_id == absorbed) else {
                // Could only happen if two merges name the same victim; guarded in
                // build_merge_map, but stay honest rather than panic.
                return Err(EditError::at(
                    *i,
                    format!("merge references unknown block '{absorbed}'"),
                ));
            };
            let victim = clone.blocks.remove(pos);
            removed_by_op.insert(absorbed.clone(), *i);
            for entry in &victim.membership {
                touched_paths.insert(entry.path.clone());
            }
            let survivor = clone
                .blocks
                .iter_mut()
                .find(|b| &b.block_id == *into)
                .ok_or_else(|| {
                    EditError::at(
                        *i,
                        format!("merge references unknown survivor block '{into}'"),
                    )
                })?;
            union_membership(&mut survivor.membership, victim.membership);
            survivor.sockets.inputs.extend(victim.sockets.inputs);
            survivor.sockets.outputs.extend(victim.sockets.outputs);
            survivor.sockets.external.extend(victim.sockets.external);
        }
    }
    Ok(())
}

/// Union `src` membership into `dst`: dedup by path, preserving a `shared` role (a
/// multi-owner marking is never silently downgraded), then sort by path for a
/// deterministic result.
fn union_membership(dst: &mut Vec<MembershipEntry>, src: Vec<MembershipEntry>) {
    for entry in src {
        if let Some(existing) = dst.iter_mut().find(|e| e.path == entry.path) {
            if entry.role == MembershipRole::Shared {
                existing.role = MembershipRole::Shared;
            }
            existing.optional = existing.optional && entry.optional;
        } else {
            dst.push(entry);
        }
    }
    dst.sort_by(|a, b| a.path.cmp(&b.path));
}

/// Rewrite every internal socket `to:` target through the merge map (§1d), then drop
/// self-loops and dedup. External sockets (`to: None`) are untouched.
fn rewrite_sockets(clone: &mut SystemBlockStore, merge_map: &HashMap<String, String>) {
    if merge_map.is_empty() {
        return;
    }
    for block in clone.blocks.iter_mut() {
        for socket in block
            .sockets
            .inputs
            .iter_mut()
            .chain(block.sockets.outputs.iter_mut())
            .chain(block.sockets.external.iter_mut())
        {
            if let Some(to) = &socket.to {
                let canonical = canon(to, merge_map);
                if &canonical != to {
                    socket.to = Some(canonical);
                }
            }
        }
    }
    for block in clone.blocks.iter_mut() {
        let own = block.block_id.clone();
        dedup_sockets(&mut block.sockets, &own);
    }
}

/// Drop self-loop sockets (`to == own id`) and exact duplicates, keeping order.
fn dedup_sockets(sockets: &mut Sockets, own_id: &str) {
    dedup_socket_vec(&mut sockets.inputs, own_id);
    dedup_socket_vec(&mut sockets.outputs, own_id);
    dedup_socket_vec(&mut sockets.external, own_id);
}

fn dedup_socket_vec(list: &mut Vec<Socket>, own_id: &str) {
    let mut seen: Vec<Socket> = Vec::new();
    list.retain(|socket| {
        if socket.to.as_deref() == Some(own_id) {
            return false; // a self-loop is meaningless after a merge
        }
        if seen.contains(socket) {
            return false;
        }
        seen.push(socket.clone());
        true
    });
}

/// Phase 2 — one member op (move / seam / assign), refs canonicalized through o2.
fn apply_member_op(
    clone: &mut SystemBlockStore,
    op_index: usize,
    op: &EditOp,
    merge_map: &HashMap<String, String>,
    touched_paths: &mut HashSet<String>,
) -> Result<(), EditError> {
    match op {
        EditOp::MoveMember { path, from, to } => {
            let from = canon(from, merge_map);
            let to = canon(to, merge_map);
            if from == to {
                return Err(EditError::at(
                    op_index,
                    format!("move_member: from and to resolve to the same block '{from}'"),
                ));
            }
            require_block(clone, op_index, &to, "move_member")?;
            let src = find_block_mut(clone, &from).ok_or_else(|| {
                EditError::at(
                    op_index,
                    format!("move_member: unknown source block '{from}'"),
                )
            })?;
            let Some(pos) = src.membership.iter().position(|e| &e.path == path) else {
                return Err(EditError::at(
                    op_index,
                    format!("move_member: block '{from}' has no exact member '{path}'"),
                ));
            };
            let moved = src.membership.remove(pos);
            let dst = find_block_mut(clone, &to).expect("target existence checked above");
            if let Some(existing) = dst.membership.iter_mut().find(|e| &e.path == path) {
                if moved.role == MembershipRole::Shared {
                    existing.role = MembershipRole::Shared;
                }
            } else {
                dst.membership.push(moved);
                dst.membership.sort_by(|a, b| a.path.cmp(&b.path));
            }
            touched_paths.insert(path.clone());
            Ok(())
        }
        EditOp::ResolveSeam { path, resolution } => {
            apply_resolve_seam(clone, op_index, path, resolution, merge_map, touched_paths)
        }
        EditOp::AssignUnmapped { path, block_id } => {
            let block_id = canon(block_id, merge_map);
            if let Some(owner) = clone
                .blocks
                .iter()
                .find(|b| b.membership.iter().any(|e| &e.path == path))
            {
                return Err(EditError::at(
                    op_index,
                    format!(
                        "assign_unmapped: '{path}' is already a member of block '{}'",
                        owner.block_id
                    ),
                ));
            }
            let block = find_block_mut(clone, &block_id).ok_or_else(|| {
                EditError::at(
                    op_index,
                    format!("assign_unmapped: unknown block '{block_id}'"),
                )
            })?;
            block.membership.push(MembershipEntry {
                path: path.clone(),
                role: MembershipRole::Primary,
                optional: false,
            });
            block.membership.sort_by(|a, b| a.path.cmp(&b.path));
            // Drop it from the materialized unmapped set; a later reconcile owns the
            // honest totals.
            if let Some(pos) = clone.unmapped_files.iter().position(|p| p == path) {
                clone.unmapped_files.remove(pos);
                clone.unmapped_total = clone.unmapped_total.saturating_sub(1);
            }
            touched_paths.insert(path.clone());
            Ok(())
        }
        // Only member ops reach here.
        _ => Ok(()),
    }
}

/// Resolve a multi-owner seam over ALL owners in one pass (o2 — supports 3+ owners).
fn apply_resolve_seam(
    clone: &mut SystemBlockStore,
    op_index: usize,
    path: &str,
    resolution: &str,
    merge_map: &HashMap<String, String>,
    touched_paths: &mut HashSet<String>,
) -> Result<(), EditError> {
    let owners: Vec<String> = clone
        .blocks
        .iter()
        .filter(|b| b.membership.iter().any(|e| e.path == path))
        .map(|b| b.block_id.clone())
        .collect();
    if owners.len() < 2 {
        return Err(EditError::at(
            op_index,
            format!(
                "resolve_seam: '{path}' is owned by {} block(s) — not a multi-owner seam",
                owners.len()
            ),
        ));
    }
    if resolution == "both" {
        // Keep the path on every owner, marked as an acknowledged shared seam.
        for block in clone.blocks.iter_mut() {
            if let Some(entry) = block.membership.iter_mut().find(|e| e.path == path) {
                entry.role = MembershipRole::Shared;
            }
        }
    } else if let Some(primary) = resolution.strip_prefix("primary:") {
        let primary = canon(primary, merge_map);
        if !owners.iter().any(|o| o == &primary) {
            return Err(EditError::at(
                op_index,
                format!("resolve_seam: primary '{primary}' is not one of the seam's owners"),
            ));
        }
        for block in clone.blocks.iter_mut() {
            if block.block_id == primary {
                if let Some(entry) = block.membership.iter_mut().find(|e| e.path == path) {
                    entry.role = MembershipRole::Primary;
                }
            } else {
                block.membership.retain(|e| e.path != path);
            }
        }
    } else {
        return Err(EditError::at(
            op_index,
            format!(
                "resolve_seam: resolution must be \"both\" or \"primary:<block_id>\", got \"{resolution}\""
            ),
        ));
    }
    touched_paths.insert(path.to_string());
    Ok(())
}

/// Phase 3 — split a block into N children by explicit path groups (o3). The parent's
/// id is consumed; N children get new stable ids reproducible from the ops alone.
fn apply_split(
    clone: &mut SystemBlockStore,
    op_index: usize,
    block_id: &str,
    by: &SplitBy,
    merge_map: &HashMap<String, String>,
    removed_by_op: &mut HashMap<String, usize>,
    touched_paths: &mut HashSet<String>,
) -> Result<(), EditError> {
    let parent_id = canon(block_id, merge_map);
    if by.paths.len() < 2 {
        return Err(EditError::at(
            op_index,
            format!(
                "split: block '{parent_id}' needs at least two path groups, got {}",
                by.paths.len()
            ),
        ));
    }
    for (gi, group) in by.paths.iter().enumerate() {
        if group.is_empty() {
            return Err(EditError::at(
                op_index,
                format!("split: path group {gi} is empty"),
            ));
        }
    }
    let pos = clone
        .blocks
        .iter()
        .position(|b| b.block_id == parent_id)
        .ok_or_else(|| EditError::at(op_index, format!("split: unknown block '{parent_id}'")))?;
    let parent = clone.blocks.remove(pos);
    removed_by_op.insert(parent_id.clone(), op_index);

    // Partition the parent's CURRENT membership by the groups: every entry must match
    // exactly one group (disjoint), and every entry must match a group (total).
    let mut buckets: Vec<Vec<MembershipEntry>> = vec![Vec::new(); by.paths.len()];
    for entry in &parent.membership {
        let matched: Vec<usize> = by
            .paths
            .iter()
            .enumerate()
            .filter(|(_, group)| group_matches(group, &entry.path))
            .map(|(gi, _)| gi)
            .collect();
        match matched.as_slice() {
            [] => {
                return Err(EditError::at(
                    op_index,
                    format!(
                        "split: member '{}' is covered by no group — splits must partition the whole membership (total-or-honest-residue)",
                        entry.path
                    ),
                ));
            }
            [gi] => buckets[*gi].push(entry.clone()),
            _ => {
                return Err(EditError::at(
                    op_index,
                    format!(
                        "split: member '{}' matches {} groups — groups must be disjoint",
                        entry.path,
                        matched.len()
                    ),
                ));
            }
        }
        touched_paths.insert(entry.path.clone());
    }
    for (gi, bucket) in buckets.iter().enumerate() {
        if bucket.is_empty() {
            return Err(EditError::at(
                op_index,
                format!("split: path group {gi} claims no member of block '{parent_id}'"),
            ));
        }
    }

    for (gi, members) in buckets.into_iter().enumerate() {
        let child_id = format!("{parent_id}_split{}", gi + 1);
        if clone.blocks.iter().any(|b| b.block_id == child_id) {
            return Err(EditError::at(
                op_index,
                format!("split: generated child id '{child_id}' collides with an existing block"),
            ));
        }
        let paths: Vec<String> = members.iter().map(|e| e.path.clone()).collect();
        let mut name = humanize_module(&dominant_directory(paths.iter()));
        if name.trim().is_empty() {
            name = format!("{} part {}", parent.name, gi + 1);
        }
        let meta = recompute_meta(
            &members,
            parent.candidate_meta.as_ref(),
            NamedBy::Heuristic,
            true,
        );
        let child = SystemBlock {
            block_id: child_id.clone(),
            name,
            purpose: format!("Split from '{}'.", parent.name),
            kind: SystemBlockKind::Scanned,
            state: SystemBlockState::Candidate,
            boundary_version: 1,
            contract_version: 1,
            membership_source: parent.membership_source,
            membership: members,
            // Sockets are re-derived by a later scan/reconcile — the pure editor does
            // not fabricate how the parent's sockets partition among children.
            sockets: Sockets {
                inputs: Vec::new(),
                outputs: Vec::new(),
                external: Vec::new(),
            },
            receipt_contract: parent.receipt_contract.clone(),
            receipts: Vec::new(),
            layout: Layout {
                x: None,
                y: None,
                locked: false,
                algorithm_seed: Some(json!({
                    "source": "candidate_edit_split",
                    "parent": parent_id,
                    "group": gi,
                })),
                version: 1,
            },
            unmapped_residue: Vec::new(),
            membership_fingerprint: None,
            resolved_members: Vec::new(),
            pre_archive_state: None,
            candidate_meta: Some(meta),
        };
        clone.blocks.push(child);
    }
    Ok(())
}

/// Whether any glob in `group` matches `path` (an exact-string equality also counts,
/// so a literal group entry works without glob metacharacters).
fn group_matches(group: &[String], path: &str) -> bool {
    group.iter().any(|g| {
        g == path
            || glob::Pattern::new(g)
                .map(|pat| pat.matches(path))
                .unwrap_or(false)
    })
}

/// Phase 4 — a rename, stamping provenance from the seat (§1c / o6).
fn apply_rename(
    clone: &mut SystemBlockStore,
    op_index: usize,
    block_id: &str,
    name: &Option<String>,
    purpose: &Option<String>,
    seat: EditSeat,
    merge_map: &HashMap<String, String>,
) -> Result<(), EditError> {
    let block_id = canon(block_id, merge_map);
    if name.is_none() && purpose.is_none() {
        return Err(EditError::at(
            op_index,
            format!(
                "rename: block '{block_id}' — nothing to change (name and purpose both absent)"
            ),
        ));
    }
    if let Some(n) = name {
        if n.trim().is_empty() {
            return Err(EditError::at(
                op_index,
                format!("rename: block '{block_id}' — name cannot be empty"),
            ));
        }
    }
    // o5 — a RUNNER seat is untrusted LLM output (the naming/curation hand), so every
    // name/purpose it proposes runs the naming-lane sanitizer (reused verbatim) BEFORE
    // it can touch the store; a violation refuses the op, and the o1 preflight makes
    // that refusal atomic (the whole batch aborts, nothing persists). The refusal
    // names the field + class in the o5 style. The OWNER seat is the human at the
    // screen — they may write whatever they mean (even a name with `/`), so it is NOT
    // hard-sanitized: o5 governs LLM input, not the owner's keystrokes.
    if seat == EditSeat::Runner {
        crate::naming_runner::sanitize_rename_fields(name.as_deref(), purpose.as_deref()).map_err(
            |reason| EditError::at(op_index, format!("rename: block '{block_id}' — {reason}")),
        )?;
    }
    let block = find_block_mut(clone, &block_id)
        .ok_or_else(|| EditError::at(op_index, format!("rename: unknown block '{block_id}'")))?;
    if let Some(n) = name {
        block.name = n.clone();
    }
    if let Some(p) = purpose {
        block.purpose = p.clone();
    }
    // Provenance: an owner/runner touch clears `needs_owner_naming` (o6). If the block
    // has no candidate_meta yet, synthesize one so the provenance is recorded.
    match block.candidate_meta.as_mut() {
        Some(meta) => {
            meta.named_by = seat.named_by();
            meta.needs_owner_naming = false;
        }
        None => {
            block.candidate_meta = Some(recompute_meta(
                &block.membership,
                None,
                seat.named_by(),
                false,
            ));
        }
    }
    Ok(())
}

/// Recompute a survivor's `candidate_meta` over its new membership while PRESERVING
/// its naming provenance (a merge keeps the survivor's identity — a rename is a
/// separate op).
fn recompute_block_meta_preserving_naming(clone: &mut SystemBlockStore, block_id: &str) {
    let Some(block) = find_block_mut(clone, block_id) else {
        return;
    };
    let (named_by, needs_naming) = block
        .candidate_meta
        .as_ref()
        .map(|m| (m.named_by, m.needs_owner_naming))
        .unwrap_or((NamedBy::Heuristic, true));
    let meta = recompute_meta(
        &block.membership,
        block.candidate_meta.as_ref(),
        named_by,
        needs_naming,
    );
    block.candidate_meta = Some(meta);
}

/// Recompute `candidate_meta` from membership alone after a structural edit. The
/// membership-derived fields are exact; the GRAPH-derived confidence is honestly
/// marked unknown — the pure editor has no graph, so it never fabricates cohesion
/// (§3b: `graph_cohesion` is `None`, never faked). `coverage_ratio` is carried from
/// the last real scan (`base`); a later `reconcile`/scan re-measures it.
fn recompute_meta(
    membership: &[MembershipEntry],
    base: Option<&CandidateMeta>,
    named_by: NamedBy,
    needs_owner_naming: bool,
) -> CandidateMeta {
    let paths: Vec<String> = membership.iter().map(|e| e.path.clone()).collect();
    CandidateMeta {
        named_by,
        needs_owner_naming,
        graph_cohesion: None,
        edge_sample_size: 0,
        directory_support: directory_support(paths.iter()),
        coverage_ratio: base.map(|m| m.coverage_ratio).unwrap_or(0.0),
        shared_member_count: membership
            .iter()
            .filter(|e| e.role == MembershipRole::Shared)
            .count(),
    }
}

/// The final-invariant gate (o1): no dangling socket, no empty block, and no
/// non-shared multi-owner seam CREATED by the batch. Any violation aborts the whole
/// batch (nothing was persisted).
fn validate_final_invariants(
    clone: &SystemBlockStore,
    before_owners: &HashMap<String, Vec<String>>,
    touched_paths: &HashSet<String>,
    removed_by_op: &HashMap<String, usize>,
    op_count: usize,
) -> Result<(), EditError> {
    let last = op_count.saturating_sub(1);
    let live: HashSet<&str> = clone.blocks.iter().map(|b| b.block_id.as_str()).collect();

    // (1) No dangling internal socket (§1d). Attributed to the op that removed the
    //     now-missing target when known.
    for block in &clone.blocks {
        for socket in block
            .sockets
            .inputs
            .iter()
            .chain(block.sockets.outputs.iter())
        {
            if let Some(to) = &socket.to {
                if !live.contains(to.as_str()) {
                    let op_index = removed_by_op.get(to).copied().unwrap_or(last);
                    return Err(EditError::at(
                        op_index,
                        format!(
                            "dangling socket: block '{}' points to '{to}', which no longer exists after the batch",
                            block.block_id
                        ),
                    ));
                }
            }
        }
    }

    // (2) No empty block.
    for block in &clone.blocks {
        if block.membership.is_empty() {
            return Err(EditError::at(
                last,
                format!(
                    "empty block: '{}' has no members after the batch",
                    block.block_id
                ),
            ));
        }
    }

    // (3) No non-shared multi-owner seam CREATED by the batch. A pre-existing seam is
    //     not ours to flag; a fully-shared seam is an acknowledged resolution.
    let after_owners = exact_ownership(clone);
    for path in touched_paths {
        let Some(owners) = after_owners.get(path) else {
            continue;
        };
        if owners.len() < 2 {
            continue;
        }
        let all_shared = owners.iter().all(|owner_id| {
            clone
                .blocks
                .iter()
                .find(|b| &b.block_id == owner_id)
                .and_then(|b| b.membership.iter().find(|e| &e.path == path))
                .map(|e| e.role == MembershipRole::Shared)
                .unwrap_or(false)
        });
        if all_shared {
            continue;
        }
        let before = before_owners.get(path);
        if before != Some(owners) {
            return Err(EditError::at(
                last,
                format!(
                    "unresolved seam: the batch left '{path}' claimed by {} blocks without resolving it — resolve_seam it (both, or a primary)",
                    owners.len()
                ),
            ));
        }
    }

    Ok(())
}

/// Every EXACT (non-glob) member path and the sorted block ids that claim it.
fn exact_ownership(store: &SystemBlockStore) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, BTreeSet<String>> = HashMap::new();
    for block in &store.blocks {
        for entry in &block.membership {
            out.entry(entry.path.clone())
                .or_default()
                .insert(block.block_id.clone());
        }
    }
    out.into_iter()
        .map(|(path, ids)| (path, ids.into_iter().collect()))
        .collect()
}

fn find_block_mut<'a>(
    clone: &'a mut SystemBlockStore,
    block_id: &str,
) -> Option<&'a mut SystemBlock> {
    clone.blocks.iter_mut().find(|b| b.block_id == block_id)
}

fn require_block(
    clone: &SystemBlockStore,
    op_index: usize,
    block_id: &str,
    verb: &str,
) -> Result<(), EditError> {
    if clone.blocks.iter().any(|b| b.block_id == block_id) {
        Ok(())
    } else {
        Err(EditError::at(
            op_index,
            format!("{verb}: unknown block '{block_id}'"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_blocks::{
        MembershipSource, ReceiptContract, SeedFile, SeedRatification, SeedRepo, SeedSkeleton,
        SeedSkeletonState, UnmappedDefaultAction, UnmappedPolicy, SYSTEM_BLOCK_SEED_SCHEMA,
    };

    fn member(path: &str, role: MembershipRole) -> MembershipEntry {
        MembershipEntry {
            path: path.to_string(),
            role,
            optional: false,
        }
    }

    fn prim(path: &str) -> MembershipEntry {
        member(path, MembershipRole::Primary)
    }

    fn internal_socket(to: &str) -> Socket {
        Socket {
            to: Some(to.to_string()),
            type_: Some("depends_on".to_string()),
            alias: None,
            class_: None,
        }
    }

    fn heuristic_meta() -> CandidateMeta {
        CandidateMeta {
            named_by: NamedBy::Heuristic,
            needs_owner_naming: true,
            graph_cohesion: Some(0.7),
            edge_sample_size: 9,
            directory_support: 1.0,
            coverage_ratio: 0.6,
            shared_member_count: 0,
        }
    }

    fn block(id: &str, name: &str, members: Vec<MembershipEntry>) -> SystemBlock {
        SystemBlock {
            block_id: id.to_string(),
            name: name.to_string(),
            purpose: format!("Purpose of {id}."),
            kind: SystemBlockKind::Scanned,
            state: SystemBlockState::Candidate,
            boundary_version: 1,
            contract_version: 1,
            membership_source: MembershipSource::Proposed,
            membership: members,
            sockets: Sockets {
                inputs: Vec::new(),
                outputs: Vec::new(),
                external: Vec::new(),
            },
            receipt_contract: ReceiptContract {
                version: 1,
                required: Vec::new(),
                optional: Vec::new(),
                waived: Vec::new(),
                declared_by: None,
                declared_at: None,
            },
            receipts: Vec::new(),
            layout: Layout {
                x: None,
                y: None,
                locked: false,
                algorithm_seed: None,
                version: 1,
            },
            unmapped_residue: Vec::new(),
            membership_fingerprint: None,
            resolved_members: Vec::new(),
            pre_archive_state: None,
            candidate_meta: Some(heuristic_meta()),
        }
    }

    fn store_of(blocks: Vec<SystemBlock>) -> SystemBlockStore {
        let seed = SeedFile {
            schema: SYSTEM_BLOCK_SEED_SCHEMA.to_string(),
            repo: SeedRepo {
                repo_id: "r".to_string(),
                root: ".".to_string(),
                source_commit: "c".to_string(),
            },
            skeleton: SeedSkeleton {
                skeleton_id: "sk".to_string(),
                version: 1,
                state: SeedSkeletonState::Candidate,
                ratification: SeedRatification {
                    method: String::new(),
                    ratifier: String::new(),
                    ratified_at: String::new(),
                    commit: String::new(),
                },
            },
            blocks,
            unmapped_policy: UnmappedPolicy {
                visible: true,
                default_action: UnmappedDefaultAction::LeaveUnmappedUntilRatified,
            },
        };
        SystemBlockStore::from_seed(seed)
    }

    fn find<'a>(store: &'a SystemBlockStore, id: &str) -> Option<&'a SystemBlock> {
        store.blocks.iter().find(|b| b.block_id == id)
    }

    fn members_of(store: &SystemBlockStore, id: &str) -> Vec<String> {
        find(store, id)
            .map(|b| b.membership.iter().map(|e| e.path.clone()).collect())
            .unwrap_or_default()
    }

    // --- merge: unions membership (dedup, shared preserved) + rewrites sockets -----

    #[test]
    fn merge_unions_membership_and_rewrites_sockets() {
        let mut a = block("sb_a", "A", vec![prim("a1"), prim("shared")]);
        let b = block(
            "sb_b",
            "B",
            vec![member("shared", MembershipRole::Shared), prim("b1")],
        );
        let mut c = block("sb_c", "C", vec![prim("c1")]);
        // sb_c points at sb_b (the block that will be absorbed) and sb_a has an
        // outbound socket to sb_b too (which becomes a self-loop after the merge).
        c.sockets.outputs.push(internal_socket("sb_b"));
        a.sockets.outputs.push(internal_socket("sb_b"));
        let store = store_of(vec![a, b, c]);

        let ops = vec![EditOp::Merge {
            into: "sb_a".to_string(),
            block_ids: vec!["sb_b".to_string()],
        }];
        let out = apply_edits(&store, &ops, EditSeat::Owner).expect("merge applies");

        assert!(find(&out, "sb_b").is_none(), "the absorbed block is gone");
        // Survivor unions membership; the shared entry is preserved (dedup keeps shared).
        assert_eq!(
            members_of(&out, "sb_a"),
            vec!["a1".to_string(), "b1".to_string(), "shared".to_string()]
        );
        let a_out = find(&out, "sb_a").unwrap();
        let shared = a_out
            .membership
            .iter()
            .find(|e| e.path == "shared")
            .unwrap();
        assert_eq!(shared.role, MembershipRole::Shared, "shared role preserved");
        // sb_c's socket to the absorbed block now points at the survivor.
        let c_out = find(&out, "sb_c").unwrap();
        assert_eq!(c_out.sockets.outputs[0].to.as_deref(), Some("sb_a"));
        // The survivor's own socket to the absorbed block became a self-loop and was dropped.
        assert!(
            a_out
                .sockets
                .outputs
                .iter()
                .all(|s| s.to.as_deref() != Some("sb_a")),
            "no self-loop socket on the survivor"
        );
        // Meta is recomputed but the graph-derived cohesion is honestly unknown.
        let meta = a_out.candidate_meta.as_ref().unwrap();
        assert!(
            meta.graph_cohesion.is_none(),
            "cohesion never fabricated after an edit"
        );
        assert_eq!(meta.edge_sample_size, 0);
        assert_eq!(meta.shared_member_count, 1, "one shared member counted");
    }

    // --- seam: a member owned by 3 blocks, resolved to one primary in one pass -----

    #[test]
    fn seam_resolves_all_owners() {
        let x = block(
            "sb_x",
            "X",
            vec![member("shared.ts", MembershipRole::Shared), prim("x1")],
        );
        let y = block(
            "sb_y",
            "Y",
            vec![member("shared.ts", MembershipRole::Shared), prim("y1")],
        );
        let z = block(
            "sb_z",
            "Z",
            vec![member("shared.ts", MembershipRole::Shared), prim("z1")],
        );
        let store = store_of(vec![x, y, z]);

        let ops = vec![EditOp::ResolveSeam {
            path: "shared.ts".to_string(),
            resolution: "primary:sb_y".to_string(),
        }];
        let out = apply_edits(&store, &ops, EditSeat::Owner).expect("seam resolves");

        // Exactly one owner remains, as primary; the other two lost the member.
        assert!(members_of(&out, "sb_y").contains(&"shared.ts".to_string()));
        let y_entry = find(&out, "sb_y")
            .unwrap()
            .membership
            .iter()
            .find(|e| e.path == "shared.ts")
            .unwrap();
        assert_eq!(y_entry.role, MembershipRole::Primary, "primary owner");
        assert!(!members_of(&out, "sb_x").contains(&"shared.ts".to_string()));
        assert!(!members_of(&out, "sb_z").contains(&"shared.ts".to_string()));
    }

    #[test]
    fn seam_resolve_both_marks_all_owners_shared() {
        let x = block("sb_x", "X", vec![prim("shared.ts"), prim("x1")]);
        let y = block("sb_y", "Y", vec![prim("shared.ts"), prim("y1")]);
        let store = store_of(vec![x, y]);
        let ops = vec![EditOp::ResolveSeam {
            path: "shared.ts".to_string(),
            resolution: "both".to_string(),
        }];
        let out = apply_edits(&store, &ops, EditSeat::Owner).expect("both resolves");
        for id in ["sb_x", "sb_y"] {
            let entry = find(&out, id)
                .unwrap()
                .membership
                .iter()
                .find(|e| e.path == "shared.ts")
                .unwrap();
            assert_eq!(entry.role, MembershipRole::Shared, "{id} marked shared");
        }
    }

    // --- split: explicit path groups, reproducible; overlapping groups rejected ----

    #[test]
    fn split_by_path_groups_is_reproducible() {
        let p = block(
            "sb_p",
            "P",
            vec![
                prim("src/api/a.rs"),
                prim("src/api/b.rs"),
                prim("src/db/c.rs"),
                prim("src/db/d.rs"),
            ],
        );
        let store = store_of(vec![p]);
        let ops = vec![EditOp::Split {
            block_id: "sb_p".to_string(),
            by: SplitBy {
                paths: vec![
                    vec!["src/api/**".to_string()],
                    vec!["src/db/**".to_string()],
                ],
            },
        }];
        let out1 = apply_edits(&store, &ops, EditSeat::Owner).expect("split applies");
        let out2 = apply_edits(&store, &ops, EditSeat::Owner).expect("split applies again");
        assert_eq!(out1, out2, "the split is reproducible from the ops alone");

        assert!(find(&out1, "sb_p").is_none(), "the parent id is consumed");
        assert_eq!(
            members_of(&out1, "sb_p_split1"),
            vec!["src/api/a.rs".to_string(), "src/api/b.rs".to_string()]
        );
        assert_eq!(
            members_of(&out1, "sb_p_split2"),
            vec!["src/db/c.rs".to_string(), "src/db/d.rs".to_string()]
        );
        // New boundaries are provisional — they need an owner touch before ratify (o6).
        let child_meta = find(&out1, "sb_p_split1")
            .unwrap()
            .candidate_meta
            .as_ref()
            .unwrap();
        assert!(child_meta.needs_owner_naming, "split children need naming");
        assert!(child_meta.graph_cohesion.is_none());
    }

    #[test]
    fn split_rejects_overlapping_groups() {
        let p = block("sb_p", "P", vec![prim("src/api/a.rs"), prim("src/db/c.rs")]);
        let store = store_of(vec![p]);
        // `src/**` and `src/api/**` both claim `src/api/a.rs` — overlap.
        let ops = vec![EditOp::Split {
            block_id: "sb_p".to_string(),
            by: SplitBy {
                paths: vec![vec!["src/**".to_string()], vec!["src/api/**".to_string()]],
            },
        }];
        let err = apply_edits(&store, &ops, EditSeat::Owner).expect_err("overlap rejected");
        assert_eq!(err.op_index, 0);
        assert!(err.reason.contains("disjoint"), "reason: {}", err.reason);
    }

    #[test]
    fn split_rejects_uncovered_member() {
        let p = block("sb_p", "P", vec![prim("src/api/a.rs"), prim("orphan.rs")]);
        let store = store_of(vec![p]);
        let ops = vec![EditOp::Split {
            block_id: "sb_p".to_string(),
            by: SplitBy {
                paths: vec![
                    vec!["src/api/**".to_string()],
                    vec!["src/db/**".to_string()],
                ],
            },
        }];
        let err =
            apply_edits(&store, &ops, EditSeat::Owner).expect_err("uncovered member rejected");
        assert!(err.reason.contains("orphan.rs"), "reason: {}", err.reason);
    }

    // --- o2: a member op naming a block another op absorbs canonicalizes -----------

    #[test]
    fn move_member_to_a_block_absorbed_in_same_batch_canonicalizes() {
        let a = block("sb_a", "A", vec![prim("a1"), prim("a2")]);
        let b = block("sb_b", "B", vec![prim("b1")]);
        let survivor = block("sb_s", "S", vec![prim("s1")]);
        let store = store_of(vec![a, b, survivor]);

        // Merge sb_b into sb_s, and (in the same batch) move a1 INTO sb_b — the move's
        // target is absorbed, so it canonicalizes to the survivor sb_s.
        let ops = vec![
            EditOp::Merge {
                into: "sb_s".to_string(),
                block_ids: vec!["sb_b".to_string()],
            },
            EditOp::MoveMember {
                path: "a1".to_string(),
                from: "sb_a".to_string(),
                to: "sb_b".to_string(),
            },
        ];
        let out = apply_edits(&store, &ops, EditSeat::Owner).expect("canonicalized move applies");
        assert!(find(&out, "sb_b").is_none());
        // a1 landed on the survivor, not a resurrected sb_b.
        assert!(members_of(&out, "sb_s").contains(&"a1".to_string()));
        assert!(members_of(&out, "sb_s").contains(&"b1".to_string()));
        assert!(!members_of(&out, "sb_a").contains(&"a1".to_string()));
        assert!(members_of(&out, "sb_a").contains(&"a2".to_string()));
    }

    // --- o1d: an edit that leaves a dangling socket aborts the whole batch ---------

    #[test]
    fn dangling_socket_after_edit_aborts() {
        let x = block("sb_x", "X", vec![prim("a/1.rs"), prim("b/2.rs")]);
        let mut refr = block("sb_ref", "Ref", vec![prim("r1")]);
        refr.sockets.outputs.push(internal_socket("sb_x"));
        let store = store_of(vec![x, refr]);
        // Splitting sb_x consumes its id; sb_ref's socket to sb_x now dangles.
        let ops = vec![EditOp::Split {
            block_id: "sb_x".to_string(),
            by: SplitBy {
                paths: vec![vec!["a/**".to_string()], vec!["b/**".to_string()]],
            },
        }];
        let err = apply_edits(&store, &ops, EditSeat::Owner).expect_err("dangling socket aborts");
        assert_eq!(
            err.op_index, 0,
            "attributed to the op that removed the target"
        );
        assert!(err.reason.contains("dangling"), "reason: {}", err.reason);
    }

    // --- provenance: an owner rename stamps NamedBy::Owner + clears needs_naming ---

    #[test]
    fn rename_by_owner_stamps_named_by_owner_and_clears_needs_naming() {
        let a = block("sb_a", "Provisional", vec![prim("a1")]);
        assert!(a.candidate_meta.as_ref().unwrap().needs_owner_naming);
        let store = store_of(vec![a]);
        let ops = vec![EditOp::Rename {
            block_id: "sb_a".to_string(),
            name: Some("Auth".to_string()),
            purpose: Some("Authentication boundary.".to_string()),
        }];
        let out = apply_edits(&store, &ops, EditSeat::Owner).expect("owner rename applies");
        let a_out = find(&out, "sb_a").unwrap();
        assert_eq!(a_out.name, "Auth");
        assert_eq!(a_out.purpose, "Authentication boundary.");
        let meta = a_out.candidate_meta.as_ref().unwrap();
        assert_eq!(
            meta.named_by,
            NamedBy::Owner,
            "owner is the strongest label"
        );
        assert!(
            !meta.needs_owner_naming,
            "an owner touch clears the naming gate"
        );
    }

    #[test]
    fn rename_by_runner_stamps_named_by_runner_and_clears_needs_naming() {
        let store = store_of(vec![block("sb_a", "Provisional", vec![prim("a1")])]);
        let ops = vec![EditOp::Rename {
            block_id: "sb_a".to_string(),
            name: Some("Runner Named".to_string()),
            purpose: None,
        }];
        let out = apply_edits(&store, &ops, EditSeat::Runner).expect("runner rename applies");
        let meta = find(&out, "sb_a").unwrap().candidate_meta.as_ref().unwrap();
        assert_eq!(meta.named_by, NamedBy::Runner);
        assert!(
            !meta.needs_owner_naming,
            "a runner name also clears the gate"
        );
    }

    // --- o5: a RUNNER-seat rename is sanitized; the OWNER seat is not --------------
    //
    // A runner is untrusted LLM output. Every name/purpose it proposes through the
    // candidate_edit verb runs the naming-lane o5 sanitizer; a hostile value refuses
    // the op (and, via the o1 preflight, the whole batch), naming the field + class
    // byte-compatibly with the naming lane. The owner seat is the human at the screen
    // and is NOT hard-sanitized — the exact vector the o5 gate used to live only in
    // the naming lane, now enforced on the write verb too.

    #[test]
    fn runner_rename_with_hostile_input_is_refused_naming_the_field_and_class() {
        let store = store_of(vec![block("sb_a", "Provisional", vec![prim("a1")])]);
        // A URL, a control char, a path traversal, a token-like blob — each is refused
        // for a runner, the reason naming the field + the o5 class.
        let cases: [(Option<&str>, Option<&str>, &str); 4] = [
            (Some("See https://evil.example/x"), None, "name: URL"),
            (Some("Auth\nInjected"), None, "name: control character"),
            (Some("../../etc/passwd"), None, "name: path separator"),
            (
                None,
                Some("rotate session ABCDEFGHIJKLMNOP0123456789"),
                "purpose: token-like string",
            ),
        ];
        for (name, purpose, expected) in cases {
            let ops = vec![EditOp::Rename {
                block_id: "sb_a".to_string(),
                name: name.map(str::to_string),
                purpose: purpose.map(str::to_string),
            }];
            let err = apply_edits(&store, &ops, EditSeat::Runner)
                .expect_err("a hostile runner rename must be refused");
            assert_eq!(err.op_index, 0, "the offending op index is named");
            assert!(
                err.reason.contains(expected),
                "reason must name the field + o5 class ({expected}): {}",
                err.reason
            );
        }
    }

    #[test]
    fn owner_rename_is_not_hard_sanitized_where_the_runner_is_refused() {
        // The SAME batch: the owner (a human typing a slash into a name) applies; the
        // runner seat refuses it (`name: path separator`). o5 is for LLM input.
        let store = store_of(vec![block("sb_a", "Provisional", vec![prim("a1")])]);
        let ops = vec![EditOp::Rename {
            block_id: "sb_a".to_string(),
            name: Some("Payments / Billing".to_string()),
            purpose: None,
        }];
        let err =
            apply_edits(&store, &ops, EditSeat::Runner).expect_err("runner slash name is refused");
        assert!(
            err.reason.contains("name: path separator"),
            "reason: {}",
            err.reason
        );
        let out = apply_edits(&store, &ops, EditSeat::Owner).expect("owner rename applies");
        assert_eq!(
            find(&out, "sb_a").unwrap().name,
            "Payments / Billing",
            "the owner's keystrokes are stored verbatim"
        );
    }

    #[test]
    fn legitimate_runner_rename_passes_byte_identically() {
        // The shape the naming lane emits — single-line plain text — passes o5 and is
        // stored exactly as given (validation only, no rewrite).
        let store = store_of(vec![block("sb_a", "Provisional", vec![prim("a1")])]);
        let ops = vec![EditOp::Rename {
            block_id: "sb_a".to_string(),
            name: Some("Payment Gateway".to_string()),
            purpose: Some("Charges, refunds, and settlement.".to_string()),
        }];
        let out =
            apply_edits(&store, &ops, EditSeat::Runner).expect("a clean runner rename applies");
        let a = find(&out, "sb_a").unwrap();
        assert_eq!(
            a.name, "Payment Gateway",
            "a clean runner name is stored byte-for-byte"
        );
        assert_eq!(a.purpose, "Charges, refunds, and settlement.");
        assert_eq!(a.candidate_meta.as_ref().unwrap().named_by, NamedBy::Runner);
    }

    // --- assign_unmapped moves a path from the unmapped set into a block ----------

    #[test]
    fn assign_unmapped_moves_path_into_a_block() {
        let mut store = store_of(vec![block("sb_a", "A", vec![prim("a1")])]);
        store.unmapped_files = vec!["scripts/x.sh".to_string()];
        store.unmapped_total = 1;
        let ops = vec![EditOp::AssignUnmapped {
            path: "scripts/x.sh".to_string(),
            block_id: "sb_a".to_string(),
        }];
        let out = apply_edits(&store, &ops, EditSeat::Owner).expect("assign applies");
        assert!(members_of(&out, "sb_a").contains(&"scripts/x.sh".to_string()));
        assert!(
            out.unmapped_files.is_empty(),
            "removed from the unmapped set"
        );
        assert_eq!(out.unmapped_total, 0);
    }

    // --- o2: merge validation rejects ambiguity / self-merge / merge-into-victim ---

    #[test]
    fn merge_rejects_ambiguous_absorption() {
        let store = store_of(vec![
            block("sb_a", "A", vec![prim("a1")]),
            block("sb_b", "B", vec![prim("b1")]),
            block("sb_c", "C", vec![prim("c1")]),
        ]);
        // sb_c absorbed into BOTH sb_a and sb_b -> ambiguous.
        let ops = vec![
            EditOp::Merge {
                into: "sb_a".to_string(),
                block_ids: vec!["sb_c".to_string()],
            },
            EditOp::Merge {
                into: "sb_b".to_string(),
                block_ids: vec!["sb_c".to_string()],
            },
        ];
        let err = apply_edits(&store, &ops, EditSeat::Owner).expect_err("ambiguity rejected");
        assert!(err.reason.contains("ambiguous"), "reason: {}", err.reason);
    }

    #[test]
    fn merge_rejects_into_victim() {
        let store = store_of(vec![
            block("sb_a", "A", vec![prim("a1")]),
            block("sb_b", "B", vec![prim("b1")]),
        ]);
        // sb_a is a survivor AND absorbed elsewhere -> merge-into-victim.
        let ops = vec![
            EditOp::Merge {
                into: "sb_a".to_string(),
                block_ids: vec!["sb_b".to_string()],
            },
            EditOp::Merge {
                into: "sb_b".to_string(),
                block_ids: vec!["sb_a".to_string()],
            },
        ];
        let err = apply_edits(&store, &ops, EditSeat::Owner).expect_err("victim rejected");
        assert!(err.reason.contains("victim"), "reason: {}", err.reason);
    }

    #[test]
    fn empty_block_after_move_aborts() {
        // Moving the only member out of a block would empty it -> abort.
        let store = store_of(vec![
            block("sb_a", "A", vec![prim("only")]),
            block("sb_b", "B", vec![prim("b1")]),
        ]);
        let ops = vec![EditOp::MoveMember {
            path: "only".to_string(),
            from: "sb_a".to_string(),
            to: "sb_b".to_string(),
        }];
        let err = apply_edits(&store, &ops, EditSeat::Owner).expect_err("empty block aborts");
        assert!(err.reason.contains("empty"), "reason: {}", err.reason);
    }

    #[test]
    fn unknown_block_reference_is_rejected_with_its_op_index() {
        let store = store_of(vec![block("sb_a", "A", vec![prim("a1")])]);
        let ops = vec![
            EditOp::Rename {
                block_id: "sb_a".to_string(),
                name: Some("Renamed".to_string()),
                purpose: None,
            },
            EditOp::Rename {
                block_id: "sb_ghost".to_string(),
                name: Some("Ghost".to_string()),
                purpose: None,
            },
        ];
        let err = apply_edits(&store, &ops, EditSeat::Owner).expect_err("unknown block rejected");
        assert_eq!(err.op_index, 1, "the second op is the offender");
    }

    #[test]
    fn edit_op_deserializes_from_tagged_json() {
        // The wire form is a tagged union — prove each op parses.
        let json = r#"[
            {"op":"rename","block_id":"sb_x","name":"Auth"},
            {"op":"merge","into":"sb_x","block_ids":["sb_y"]},
            {"op":"split","block_id":"sb_x","by":{"paths":[["a/**"],["b/**"]]}},
            {"op":"move_member","path":"src/h.ts","from":"sb_x","to":"sb_y"},
            {"op":"resolve_seam","path":"src/s.ts","resolution":"primary:sb_y"},
            {"op":"assign_unmapped","path":"x.sh","block_id":"sb_x"}
        ]"#;
        let ops: Vec<EditOp> = serde_json::from_str(json).expect("ops parse");
        assert_eq!(ops.len(), 6);
        assert!(matches!(ops[0], EditOp::Rename { .. }));
        assert!(matches!(ops[2], EditOp::Split { .. }));
        assert!(matches!(ops[4], EditOp::ResolveSeam { .. }));
    }
}
