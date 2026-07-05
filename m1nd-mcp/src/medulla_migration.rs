//! MEDULLA slice M5a — the storage-split migration (MEDULLA-PRD §4.2).
//!
//! Today the owner's shared `<runtime_root>/agent-memory/` holds ONE
//! undifferentiated mix (§2.3 S1): m1nd-repo facts (ship notes, hall fixes, CI
//! flake gotchas, code-anchored findings) tangled with maintainer doctrine,
//! preferences, and product vocabulary. The medulla-to-be is polluted by project
//! fact. M5a formalizes the medulla at that SAME path (no move — the kill-the-move
//! precedent, §4.1) and triages the mix ONE claim, ONE row, ONE destination:
//!
//! | class                          | destination                              |
//! |--------------------------------|------------------------------------------|
//! | m1nd-repo fact                 | the m1nd PROJECT brain store             |
//! | maintainer doctrine/vocabulary | STAY on the medulla (stamp Origin-Brain) |
//! | ambiguous                      | STAY + flagged for maintainer triage     |
//!
//! ## Safety posture — CODE-LAND-ONLY (this ladder's absolute rule)
//!
//! This module BUILDS and PROVES the migration; it NEVER runs it on a live owner
//! by default. [`MedullaMigration::plan`] is the default and is pure-read (a
//! dry-run: it enumerates + classifies + reports, mutating nothing).
//! [`MedullaMigration::apply`] is the gated executor — backup-first and
//! count-conserving — and is exercised ONLY in scratch stores by the battery /
//! unit tests. The live maintainer store is migrated by the maintainer, never by
//! an agent.
//!
//! ## Reversibility (proven by a migrate → rollback round-trip)
//!
//! `apply` snapshots the ENTIRE medulla `agent-memory/` dir into a timestamped
//! backup BEFORE the first mutation. [`MedullaMigration::rollback`] restores that
//! backup byte-for-byte. The battery proves a plan → apply → rollback cycle
//! returns the store to its exact original bytes — no claim lost, no claim
//! altered.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use m1nd_core::error::{M1ndError, M1ndResult};
use serde::{Deserialize, Serialize};

use crate::util::now_ms;

/// Where a triaged claim is routed (MEDULLA-PRD §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Destination {
    /// A repo-anchored fact → moves to the project brain's store.
    Project,
    /// Doctrine / preference / vocabulary → stays on the medulla.
    Medulla,
    /// Doubt → stays on the medulla, flagged for maintainer triage
    /// (the M4 hand-curated judgment: doubt → don't move).
    AmbiguousStay,
}

/// One triaged claim in the plan (§4.2, one claim = one row).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimPlan {
    /// The `.light.md` filename (slug + extension) inside the medulla store.
    pub file_name: String,
    /// Where it will land.
    pub destination: Destination,
    /// Whether the file already carries an `Origin-Brain:` frontmatter line.
    pub has_origin_brain: bool,
    /// One-line, human-readable classification reason (for the maintainer's
    /// review before the gated `apply`).
    pub reason: String,
}

/// A ghost pointer found in `ingest_roots.json`: a per-file `.light.md` entry
/// whose file no longer exists (live example: `tokenvalidator.light.md`, §2.2c),
/// or the collapse of per-file entries into the one dir root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhostPointer {
    /// The offending ingest-root entry.
    pub entry: String,
    /// Why it is being pruned.
    pub reason: String,
}

/// The full dry-run plan — everything `apply` WOULD do, computed by pure read
/// (MEDULLA-PRD §11 M5a: the migration is a dry-run by default).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    /// Every live `.light.md` claim in the medulla store, triaged.
    pub claims: Vec<ClaimPlan>,
    /// Ghost `ingest_roots.json` entries the sweep would prune (§4.2).
    pub ghost_pointers: Vec<GhostPointer>,
    /// `count(baseline)` — live claims before migration.
    pub baseline_count: usize,
    /// `count(project-active)` the plan would produce.
    pub project_count: usize,
    /// `count(medulla-active)` the plan would produce (medulla + ambiguous-stay).
    pub medulla_count: usize,
    /// Whether the count-conservation gate holds:
    /// `baseline == project + medulla` (§4.2 gate; no claim lost).
    pub count_conserved: bool,
    /// Absolute path to the medulla `agent-memory/` store this plan is for.
    pub medulla_dir: String,
    /// Absolute path to the m1nd project brain's `agent-memory/` store.
    pub project_dir: String,
}

/// The migration engine (MEDULLA-PRD §4.2). Holds the two store dirs + the
/// `ingest_roots.json` path. Pure filesystem — no live `SessionState`, so it is
/// trivially scratch-testable and structurally incapable of touching a running
/// owner's in-memory graph.
pub struct MedullaMigration {
    /// `<owner runtime_root>/agent-memory/` — the medulla store.
    medulla_dir: PathBuf,
    /// The m1nd project brain's `agent-memory/` store (destination for repo facts).
    project_dir: PathBuf,
    /// `<owner runtime_root>/ingest_roots.json` — swept for ghost pointers.
    ingest_roots_path: PathBuf,
    /// The `Origin-Brain` value stamped on repo-fact claims moved to the project
    /// store (the m1nd repo root, e.g. `/path/to/repo`).
    project_origin: String,
}

/// Prefix that marks the timestamped backup dirs `apply` writes before mutating.
const BACKUP_PREFIX: &str = ".m5a-backup-";

impl MedullaMigration {
    /// Build a migration for a medulla store, a project-brain destination store,
    /// and the owner's `ingest_roots.json`.
    pub fn new(
        medulla_dir: impl Into<PathBuf>,
        project_dir: impl Into<PathBuf>,
        ingest_roots_path: impl Into<PathBuf>,
        project_origin: impl Into<String>,
    ) -> Self {
        Self {
            medulla_dir: medulla_dir.into(),
            project_dir: project_dir.into(),
            ingest_roots_path: ingest_roots_path.into(),
            project_origin: project_origin.into(),
        }
    }

    /// Enumerate the LIVE `.light.md` claims in the medulla store — never the
    /// `.history/` archive, never dot-dirs, never backups. Sorted for a stable,
    /// reproducible plan. This is the ONLY source of the baseline (§4.2: the
    /// live inventory at migration time, never from the PRD document).
    fn live_claims(&self) -> M1ndResult<Vec<PathBuf>> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&self.medulla_dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(M1ndError::Io(e)),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            // Skip dot-dirs (.history/.locks) and any dotfile/backup.
            if name.starts_with('.') {
                continue;
            }
            if path.is_file() && name.ends_with(".light.md") {
                out.push(path);
            }
        }
        out.sort();
        Ok(out)
    }

    /// Classify one claim by the shape of its text (§4.2 triage). Repo-fact
    /// signals: an `[𝔻 evidence: ...]` marker anchoring to code, or ship-note /
    /// hall-fix / flake vocabulary. Doctrine signals: preference / vocabulary /
    /// cross-project language. Everything unclear → ambiguous-stay (doubt → don't
    /// move). Returns `(Destination, reason)`.
    fn classify(text: &str) -> (Destination, String) {
        let lower = text.to_ascii_lowercase();

        // STRONGEST signal of all: transversal doctrine the maintainer curates for
        // EVERY project, not one repo's fact. It stays on the medulla even when it
        // cites evidence paths — a doctrine note routinely anchors to the docs that
        // prove it, so the code-evidence heuristic below would otherwise misfile it
        // into a single brain (closeout field letter, 2026-07-05). These markers are
        // deliberately UNAMBIGUOUS about cross-project reach (and bilingual, since the
        // maintainer writes doctrine in pt-BR too) so a plain repo fact never matches.
        const HARD_DOCTRINE_MARKERS: &[&str] = &[
            "doctrine",
            "doutrina",
            "cross-project",
            "transversal",
            "universal across",
            "across any project",
            "every m1nd caller",
            "every agent",
            "todo agente",
            "founder decision",
            "sealed by max",
            "product vocabulary",
            "maintainer preference",
        ];
        if let Some(hit) = HARD_DOCTRINE_MARKERS.iter().find(|m| lower.contains(*m)) {
            return (
                Destination::Medulla,
                format!(
                    "cross-project doctrine: mentions '{hit}' — stays on the medulla \
                     even though it cites evidence"
                ),
            );
        }

        // Strongest project signal: the claim anchors to code evidence.
        let has_code_evidence = text.lines().any(|l| {
            let t = l.trim();
            t.starts_with("[𝔻 evidence:")
                && (t.contains(".rs")
                    || t.contains(".ts")
                    || t.contains(".py")
                    || t.contains(".js")
                    || t.contains(".md")
                    || t.contains('/'))
        });
        if has_code_evidence {
            return (
                Destination::Project,
                "code-anchored: carries a [𝔻 evidence:] marker to a repo path".into(),
            );
        }

        // Repo-fact vocabulary (ship notes, hall fixes, CI flake gotchas, slice work).
        const PROJECT_MARKERS: &[&str] = &[
            "slice",
            "shipped",
            "hall fix",
            "hall-fix",
            "flake",
            "ci flake",
            "ladder",
            "pr #",
            "merged",
            "handler",
            "invariant tt-",
            "regression",
            "gate",
        ];
        if let Some(hit) = PROJECT_MARKERS.iter().find(|m| lower.contains(*m)) {
            return (
                Destination::Project,
                format!("m1nd-repo fact: mentions '{hit}' (ship/fix/flake vocabulary)"),
            );
        }

        // Doctrine / preference / vocabulary — the medulla is already home.
        const DOCTRINE_MARKERS: &[&str] = &[
            "doctrine",
            "preference",
            "prefers",
            "maintainer",
            "vocabulary",
            "cross-project",
            "always",
            "never ",
            "rule:",
            "policy",
        ];
        if let Some(hit) = DOCTRINE_MARKERS.iter().find(|m| lower.contains(*m)) {
            return (
                Destination::Medulla,
                format!("doctrine/preference: mentions '{hit}' — already home on the medulla"),
            );
        }

        // Doubt → don't move (the M4 hand-curated judgment).
        (
            Destination::AmbiguousStay,
            "ambiguous: no clear repo-fact or doctrine signal — stays, flagged for maintainer triage"
                .into(),
        )
    }

    /// True when a `.light.md` already carries an `Origin-Brain:` line (a whole-file
    /// scan — the marker only ever lives in the frontmatter, and the file is tiny).
    fn has_origin_brain(text: &str) -> bool {
        text.lines()
            .any(|l| l.trim_start().starts_with("Origin-Brain:"))
    }

    /// The ghost-pointer sweep (§4.2): `ingest_roots.json` entries that point at
    /// a `.light.md` FILE which no longer exists are pruned; per-file `.light.md`
    /// entries are collapsed into the one dir root. Returns the ghosts found +
    /// the swept root list (the second used by `apply`).
    fn sweep_ingest_roots(&self) -> M1ndResult<(Vec<GhostPointer>, Option<Vec<String>>)> {
        let text = match std::fs::read_to_string(&self.ingest_roots_path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), None)),
            Err(e) => return Err(M1ndError::Io(e)),
        };
        let roots: Vec<String> = serde_json::from_str(&text).map_err(M1ndError::Serde)?;

        let mem_dir_str = self.medulla_dir.to_string_lossy().to_string();
        let mut ghosts = Vec::new();
        let mut swept: Vec<String> = Vec::new();
        let mut collapsed_dir = false;

        for entry in &roots {
            let is_light_file = entry.ends_with(".light.md");
            if is_light_file {
                // A per-file .light.md pointer. Prune if the file is gone (ghost);
                // otherwise collapse it into the single dir root.
                if !Path::new(entry).exists() {
                    ghosts.push(GhostPointer {
                        entry: entry.clone(),
                        reason: "dangling: the .light.md file no longer exists".into(),
                    });
                } else {
                    ghosts.push(GhostPointer {
                        entry: entry.clone(),
                        reason: "collapsed: per-file .light.md pointer folds into the dir root"
                            .into(),
                    });
                    collapsed_dir = true;
                }
            } else {
                swept.push(entry.clone());
            }
        }
        // Ensure the one dir root survives when we collapsed per-file entries.
        if collapsed_dir && !swept.iter().any(|r| r == &mem_dir_str) {
            swept.push(mem_dir_str);
        }

        if ghosts.is_empty() {
            Ok((ghosts, None))
        } else {
            Ok((ghosts, Some(swept)))
        }
    }

    /// THE DRY-RUN (default, pure-read). Enumerate + classify every live claim,
    /// find the ghost pointers, and compute the count-conservation gate — WITHOUT
    /// mutating anything. This is what runs against the live owner: it reports the
    /// migration it WOULD perform, and stops (§11 M5a: dry-run default).
    pub fn plan(&self) -> M1ndResult<MigrationPlan> {
        let files = self.live_claims()?;
        let baseline_count = files.len();

        let mut claims = Vec::with_capacity(baseline_count);
        let mut project_count = 0usize;
        let mut medulla_count = 0usize;

        for path in &files {
            let text = std::fs::read_to_string(path).map_err(M1ndError::Io)?;
            let (destination, reason) = Self::classify(&text);
            match destination {
                Destination::Project => project_count += 1,
                Destination::Medulla | Destination::AmbiguousStay => medulla_count += 1,
            }
            claims.push(ClaimPlan {
                file_name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                destination,
                has_origin_brain: Self::has_origin_brain(&text),
                reason,
            });
        }

        let (ghost_pointers, _swept) = self.sweep_ingest_roots()?;

        Ok(MigrationPlan {
            claims,
            ghost_pointers,
            baseline_count,
            project_count,
            medulla_count,
            count_conserved: baseline_count == project_count + medulla_count,
            medulla_dir: self.medulla_dir.to_string_lossy().to_string(),
            project_dir: self.project_dir.to_string_lossy().to_string(),
        })
    }

    /// Snapshot the WHOLE medulla `agent-memory/` dir into a timestamped backup
    /// beside it, BEFORE any mutation (backup-first posture). Returns the backup
    /// dir path — the rollback anchor. Copies files + the `.history/` subtree so
    /// a rollback restores the store's exact bytes.
    fn backup(&self) -> M1ndResult<PathBuf> {
        let backup_dir = self
            .medulla_dir
            .join(format!("{BACKUP_PREFIX}{}", now_ms()));
        std::fs::create_dir_all(&backup_dir).map_err(M1ndError::Io)?;
        copy_tree(&self.medulla_dir, &backup_dir, &backup_dir)?;
        Ok(backup_dir)
    }

    /// THE GATED EXECUTOR (never run on a live owner by default — scratch/tests
    /// only). Backup-first, then: move project-fact claims into the project store
    /// (stamping `Origin-Brain: <project root>`), stamp `Origin-Brain: medulla`
    /// on doctrine/ambiguous claims that lack it, prune ghost ingest-root
    /// pointers, and verify count-conservation. Returns the [`MigrationReceipt`]
    /// with the backup path for [`rollback`].
    ///
    /// Non-destructive on the project side: a move writes the claim into the
    /// project store then removes it from the medulla (the backup holds the
    /// original, so the move is fully reversible). If count-conservation fails
    /// AFTER the moves, the caller should `rollback` immediately.
    pub fn apply(&self) -> M1ndResult<MigrationReceipt> {
        let plan = self.plan()?;

        // Idempotency guard (field bug 2026-07-05): a store with no repo fact to
        // move AND nothing left to stamp is already migrated. Re-running `apply`
        // must NOT write a fresh (empty) backup nor re-report a phantom
        // count-conservation failure — never degrade an already-migrated store.
        // "Nothing to do" = no Project-bound claim and every staying claim already
        // carries its `Origin-Brain`.
        let nothing_to_move = plan.project_count == 0;
        let nothing_to_stamp = plan.claims.iter().all(|c| c.has_origin_brain);
        if nothing_to_move && nothing_to_stamp {
            let medulla_after = self.live_claims()?.len();
            let project_after = count_project_claims(&self.project_dir);
            return Ok(MigrationReceipt {
                backup_dir: String::new(),
                moved_to_project: 0,
                stamped_medulla: 0,
                ghosts_pruned: 0,
                baseline_count: plan.baseline_count,
                medulla_after,
                project_after,
                count_conserved: true,
                already_migrated: true,
            });
        }

        let backup_dir = self.backup()?;

        std::fs::create_dir_all(&self.project_dir).map_err(M1ndError::Io)?;

        let mut moved = 0usize;
        let mut stamped = 0usize;

        for claim in &plan.claims {
            let src = self.medulla_dir.join(&claim.file_name);
            let text = std::fs::read_to_string(&src).map_err(M1ndError::Io)?;
            match claim.destination {
                Destination::Project => {
                    // Stamp the project origin, write into the project store, then
                    // remove from the medulla. The backup keeps the original.
                    let stamped_text = stamp_origin_brain(&text, &self.project_origin);
                    let dst = self.project_dir.join(&claim.file_name);
                    std::fs::write(&dst, stamped_text).map_err(M1ndError::Io)?;
                    std::fs::remove_file(&src).map_err(M1ndError::Io)?;
                    moved += 1;
                }
                Destination::Medulla | Destination::AmbiguousStay => {
                    // Stays home. Stamp Origin-Brain: medulla if it lacks one.
                    if !claim.has_origin_brain {
                        let stamped_text = stamp_origin_brain(&text, "medulla");
                        std::fs::write(&src, stamped_text).map_err(M1ndError::Io)?;
                        stamped += 1;
                    }
                }
            }
        }

        // Prune ghost ingest-root pointers (write the swept list back).
        let (_ghosts, swept) = self.sweep_ingest_roots()?;
        if let Some(roots) = swept {
            let json = serde_json::to_string_pretty(&roots).map_err(M1ndError::Serde)?;
            std::fs::write(&self.ingest_roots_path, json).map_err(M1ndError::Io)?;
        }

        // Count-conservation gate: the live medulla + project stores together must
        // hold exactly the baseline count.
        let medulla_after = self.live_claims()?.len();
        let project_after = count_project_claims(&self.project_dir);
        let count_conserved = plan.baseline_count == medulla_after + project_after;

        Ok(MigrationReceipt {
            backup_dir: backup_dir.to_string_lossy().to_string(),
            moved_to_project: moved,
            stamped_medulla: stamped,
            ghosts_pruned: plan.ghost_pointers.len(),
            baseline_count: plan.baseline_count,
            medulla_after,
            project_after,
            count_conserved,
            already_migrated: false,
        })
    }

    /// Reverse an `apply`: restore the medulla store from a backup dir written by
    /// `apply` and remove any claims the migration moved into the project store.
    /// After a rollback the medulla store is byte-identical to its pre-migration
    /// state (the reversibility proof).
    ///
    /// `moved_file_names` are the project-store files the migration created — they
    /// are removed so the project store returns to its pre-migration contents.
    pub fn rollback(&self, backup_dir: &str, moved_file_names: &[String]) -> M1ndResult<()> {
        let backup = PathBuf::from(backup_dir);
        if !backup.is_dir() {
            return Err(M1ndError::InvalidParams {
                tool: "medulla_migration".into(),
                detail: format!("backup dir '{backup_dir}' does not exist — cannot rollback"),
            });
        }

        // 1. Remove the claims the migration created in the project store.
        for name in moved_file_names {
            let dst = self.project_dir.join(name);
            if dst.exists() {
                std::fs::remove_file(&dst).map_err(M1ndError::Io)?;
            }
        }

        // 2. Wipe the LIVE medulla claims (not backups/dot-dirs) and restore from
        //    the backup, byte-for-byte.
        for path in self.live_claims()? {
            std::fs::remove_file(&path).map_err(M1ndError::Io)?;
        }
        restore_tree(&backup, &self.medulla_dir)?;
        Ok(())
    }
}

/// The result of an `apply` (also the rollback anchor).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReceipt {
    /// The backup dir written before mutation — pass to `rollback`.
    pub backup_dir: String,
    /// Claims moved into the project store.
    pub moved_to_project: usize,
    /// Claims that stayed and were stamped `Origin-Brain: medulla`.
    pub stamped_medulla: usize,
    /// Ghost ingest-root pointers pruned.
    pub ghosts_pruned: usize,
    /// Live claim count before migration.
    pub baseline_count: usize,
    /// Live medulla claims after migration.
    pub medulla_after: usize,
    /// Project-store claims after migration.
    pub project_after: usize,
    /// The count-conservation gate: `baseline == medulla_after + project_after`.
    pub count_conserved: bool,
    /// True when the store was already migrated (nothing to move, nothing to
    /// stamp): `apply` short-circuited BEFORE any backup or mutation. A
    /// re-invocation of a done migration never degrades the store (field bug
    /// 2026-07-05). On the normal executing path this is `false`.
    #[serde(default)]
    pub already_migrated: bool,
}

/// Count the LIVE `.light.md` claims in a project store (skip dot-dirs/backups),
/// shared by `apply`'s count-conservation gate and its already-migrated guard.
fn count_project_claims(project_dir: &Path) -> usize {
    std::fs::read_dir(project_dir)
        .map(|e| {
            e.flatten()
                .filter(|d| {
                    d.path()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| !n.starts_with('.') && n.ends_with(".light.md"))
                })
                .count()
        })
        .unwrap_or(0)
}

/// Insert an `Origin-Brain: <value>` frontmatter line into a `.light.md` if it
/// lacks one, placed after `Source-Agent:` (or, absent that, after the opening
/// `---`). Idempotent: a file already carrying `Origin-Brain:` is returned
/// unchanged. This preserves original `Created`/`Source-Agent` stamps (§4.2:
/// carry the original provenance).
fn stamp_origin_brain(text: &str, value: &str) -> String {
    if text
        .lines()
        .any(|l| l.trim_start().starts_with("Origin-Brain:"))
    {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 40);
    let mut inserted = false;
    let mut seen_open_fence = false;
    for line in text.lines() {
        out.push_str(line);
        out.push('\n');
        if inserted {
            continue;
        }
        let trimmed = line.trim();
        if trimmed == "---" && !seen_open_fence {
            seen_open_fence = true;
            continue;
        }
        // Prefer inserting right after Source-Agent (keeps the §3.3 grammar order).
        if seen_open_fence && trimmed.starts_with("Source-Agent:") {
            out.push_str(&format!("Origin-Brain: {value}\n"));
            inserted = true;
        }
    }
    // If there was no Source-Agent line, insert right after the opening fence.
    if !inserted && seen_open_fence {
        let mut rebuilt = String::with_capacity(out.len() + 40);
        let mut done = false;
        for line in out.lines() {
            rebuilt.push_str(line);
            rebuilt.push('\n');
            if !done && line.trim() == "---" {
                rebuilt.push_str(&format!("Origin-Brain: {value}\n"));
                done = true;
            }
        }
        return rebuilt;
    }
    out
}

/// Recursively copy `src`'s live files + `.history/` into `dst`, skipping any
/// backup dir under `src` (never back up a backup). `backup_self` names the
/// backup dir currently being written so the walk does not recurse into it.
fn copy_tree(src: &Path, dst: &Path, backup_self: &Path) -> M1ndResult<()> {
    std::fs::create_dir_all(dst).map_err(M1ndError::Io)?;
    for entry in std::fs::read_dir(src).map_err(M1ndError::Io)?.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Never back up prior backups (or the backup we are writing right now).
        if name.starts_with(BACKUP_PREFIX) || path == backup_self {
            continue;
        }
        let target = dst.join(name);
        if path.is_dir() {
            copy_tree(&path, &target, backup_self)?;
        } else {
            std::fs::copy(&path, &target).map_err(M1ndError::Io)?;
        }
    }
    Ok(())
}

/// Restore a backup tree back over the medulla dir (files + `.history/`). Does
/// not delete files already present (the caller wipes live claims first); it
/// overwrites/creates from the backup so the store returns to backup bytes.
fn restore_tree(backup: &Path, dst: &Path) -> M1ndResult<()> {
    std::fs::create_dir_all(dst).map_err(M1ndError::Io)?;
    for entry in std::fs::read_dir(backup).map_err(M1ndError::Io)?.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let target = dst.join(name);
        if path.is_dir() {
            restore_tree(&path, &target)?;
        } else {
            std::fs::copy(&path, &target).map_err(M1ndError::Io)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal `.light.md` with the given frontmatter + body.
    fn light_doc(node: &str, source_agent: &str, body: &str) -> String {
        format!(
            "---\nProtocol: L1GHT/1.0\nNode: {node}\nState: authored\nCreated: 1700000000000\nSource-Agent: {source_agent}\n---\n\n# {node}\n\n## {node}\n\n{body}\n"
        )
    }

    struct Scratch {
        _tmp: tempfile::TempDir,
        medulla: PathBuf,
        project: PathBuf,
        roots: PathBuf,
    }

    fn scratch() -> Scratch {
        let tmp = tempfile::tempdir().expect("tempdir");
        let medulla = tmp.path().join("runtime").join("agent-memory");
        let project = tmp
            .path()
            .join("runtime")
            .join("project-brains")
            .join("fp")
            .join("agent-memory");
        std::fs::create_dir_all(&medulla).expect("medulla dir");
        let roots = tmp.path().join("runtime").join("ingest_roots.json");
        Scratch {
            _tmp: tmp,
            medulla,
            project,
            roots,
        }
    }

    fn write_claim(dir: &Path, file: &str, contents: &str) {
        std::fs::write(dir.join(file), contents).expect("write claim");
    }

    /// RED (closeout field letter, 2026-07-05): a cross-project doctrine note
    /// routinely cites the docs that prove it, so it carries a `[𝔻 evidence:]`
    /// marker. The code-evidence heuristic must NOT pull such a claim into one
    /// repo's brain — the transversal-doctrine signal wins over the code anchor,
    /// and it must fire on the maintainer's bilingual wording ("Doutrina").
    #[test]
    fn cross_project_doctrine_with_evidence_stays_on_the_medulla() {
        let doctrine = "# SixMoves\nDoutrina destilada: every agent applies the six \
             analytical moves across any project.\n\n[𝔻 evidence: docs/HUMAN-LAYER-PRD.md]\n";
        assert_eq!(
            MedullaMigration::classify(doctrine).0,
            Destination::Medulla,
            "transversal doctrine that cites evidence must stay medulla"
        );

        // The guard must stay narrow: a genuine repo fact that cites code
        // evidence still routes to the project brain.
        let repo_fact = "# SliceShip\nThe reception slice shipped on main.\n\n\
             [𝔻 evidence: m1nd-mcp/src/server.rs]\n";
        assert_eq!(
            MedullaMigration::classify(repo_fact).0,
            Destination::Project,
            "a repo fact with code evidence still moves to the project brain"
        );
    }

    /// RED framing (M5a acceptance): a store with mixed claims, zero
    /// `Origin-Brain` fields, and a ghost ingest-root pointer. The plan must
    /// triage them ONE row per claim, count-conserving, without mutating.
    #[test]
    fn plan_triages_mixed_store_count_conserving_and_pure_read() {
        let s = scratch();
        // A code-anchored repo fact.
        write_claim(
            &s.medulla,
            "sliceship.light.md",
            &light_doc(
                "SliceShip",
                "closer-agent",
                "The slice shipped.\n\n[⍂ entity: SliceShip]\n[𝔻 evidence: m1nd-mcp/src/server.rs]\n",
            ),
        );
        // A doctrine/preference claim.
        write_claim(
            &s.medulla,
            "maxpref.light.md",
            &light_doc(
                "MaxPref",
                "orchestrator",
                "The maintainer prefers pt-BR replies always.\n\n[⍂ entity: MaxPref]\n",
            ),
        );
        // An ambiguous claim (no strong signal).
        write_claim(
            &s.medulla,
            "mystery.light.md",
            &light_doc(
                "Mystery",
                "someone",
                "A thing happened once.\n\n[⍂ entity: Mystery]\n",
            ),
        );

        // A ghost ingest-root pointer at a deleted .light.md + the dir root.
        let ghost = s.medulla.join("deleted.light.md");
        std::fs::write(
            &s.roots,
            serde_json::to_string_pretty(&vec![
                s.medulla.to_string_lossy().to_string(),
                ghost.to_string_lossy().to_string(),
            ])
            .unwrap(),
        )
        .unwrap();

        // Snapshot the store bytes BEFORE plan to prove plan is pure-read.
        let before: BTreeMap<String, String> = std::fs::read_dir(&s.medulla)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| {
                (
                    e.file_name().to_string_lossy().to_string(),
                    std::fs::read_to_string(e.path()).unwrap(),
                )
            })
            .collect();

        let mig = MedullaMigration::new(&s.medulla, &s.project, &s.roots, "/path/to/repo");
        let plan = mig.plan().expect("plan");

        assert_eq!(plan.baseline_count, 3, "three live claims");
        assert_eq!(plan.project_count, 1, "one repo fact → project");
        assert_eq!(plan.medulla_count, 2, "doctrine + ambiguous stay");
        assert!(plan.count_conserved, "baseline == project + medulla");
        assert_eq!(plan.ghost_pointers.len(), 1, "one dangling ghost pruned");
        assert!(
            plan.claims.iter().all(|c| !c.has_origin_brain),
            "RED: zero Origin-Brain fields today"
        );

        // PURE-READ: the store is byte-identical after plan.
        let after: BTreeMap<String, String> = std::fs::read_dir(&s.medulla)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| {
                (
                    e.file_name().to_string_lossy().to_string(),
                    std::fs::read_to_string(e.path()).unwrap(),
                )
            })
            .collect();
        assert_eq!(before, after, "plan must not mutate the store (dry-run)");
    }

    /// GREEN: apply moves repo facts, stamps Origin-Brain, prunes ghosts, and the
    /// count-conservation gate holds.
    #[test]
    fn apply_splits_stores_stamps_origin_and_conserves_count() {
        let s = scratch();
        write_claim(
            &s.medulla,
            "sliceship.light.md",
            &light_doc(
                "SliceShip",
                "closer",
                "shipped.\n\n[⍂ entity: SliceShip]\n[𝔻 evidence: m1nd-mcp/src/x.rs]\n",
            ),
        );
        write_claim(
            &s.medulla,
            "maxpref.light.md",
            &light_doc(
                "MaxPref",
                "orch",
                "maintainer doctrine.\n\n[⍂ entity: MaxPref]\n",
            ),
        );
        std::fs::write(
            &s.roots,
            serde_json::to_string_pretty(&vec![s.medulla.to_string_lossy().to_string()]).unwrap(),
        )
        .unwrap();

        let mig = MedullaMigration::new(&s.medulla, &s.project, &s.roots, "/path/to/repo");
        let receipt = mig.apply().expect("apply");

        assert!(receipt.count_conserved, "no claim lost");
        assert_eq!(receipt.moved_to_project, 1);
        assert_eq!(receipt.medulla_after, 1, "only doctrine stays");
        assert_eq!(receipt.project_after, 1, "the repo fact moved");

        // The moved claim is stamped with the project origin.
        let moved = std::fs::read_to_string(s.project.join("sliceship.light.md")).unwrap();
        assert!(
            moved.contains("Origin-Brain: /path/to/repo"),
            "moved claim carries the project origin, got:\n{moved}"
        );
        assert!(
            moved.contains("Source-Agent: closer"),
            "original provenance preserved"
        );
        // The staying claim is stamped medulla.
        let stayed = std::fs::read_to_string(s.medulla.join("maxpref.light.md")).unwrap();
        assert!(
            stayed.contains("Origin-Brain: medulla"),
            "doctrine claim stamped medulla, got:\n{stayed}"
        );
    }

    /// REVERSIBILITY PROOF (§11 M5a): plan → apply → rollback returns the medulla
    /// store to its EXACT original bytes and empties the project store.
    #[test]
    fn migrate_then_rollback_restores_original_bytes() {
        let s = scratch();
        let ship = light_doc(
            "SliceShip",
            "closer",
            "shipped.\n\n[⍂ entity: SliceShip]\n[𝔻 evidence: m1nd-mcp/src/x.rs]\n",
        );
        let pref = light_doc(
            "MaxPref",
            "orch",
            "maintainer doctrine.\n\n[⍂ entity: MaxPref]\n",
        );
        write_claim(&s.medulla, "sliceship.light.md", &ship);
        write_claim(&s.medulla, "maxpref.light.md", &pref);
        std::fs::write(
            &s.roots,
            serde_json::to_string_pretty(&vec![s.medulla.to_string_lossy().to_string()]).unwrap(),
        )
        .unwrap();

        // Capture the exact original bytes of the whole live store.
        let original: BTreeMap<String, String> = std::fs::read_dir(&s.medulla)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| {
                (
                    e.file_name().to_string_lossy().to_string(),
                    std::fs::read_to_string(e.path()).unwrap(),
                )
            })
            .collect();

        let mig = MedullaMigration::new(&s.medulla, &s.project, &s.roots, "/path/to/repo");
        let receipt = mig.apply().expect("apply");
        assert!(receipt.count_conserved);
        // Post-apply the store changed (moved + stamped).
        assert!(s.project.join("sliceship.light.md").exists());

        mig.rollback(&receipt.backup_dir, &["sliceship.light.md".to_string()])
            .expect("rollback");

        // The medulla store is byte-identical to the original.
        let restored: BTreeMap<String, String> = std::fs::read_dir(&s.medulla)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.path().is_file()
                    && e.file_name()
                        .to_string_lossy()
                        .to_string()
                        .ends_with(".light.md")
            })
            .map(|e| {
                (
                    e.file_name().to_string_lossy().to_string(),
                    std::fs::read_to_string(e.path()).unwrap(),
                )
            })
            .collect();
        assert_eq!(
            original, restored,
            "rollback must restore the medulla store byte-for-byte"
        );
        // The project store is empty of the moved claim again.
        assert!(
            !s.project.join("sliceship.light.md").exists(),
            "rollback removed the moved claim from the project store"
        );
    }

    /// Ghost-pointer sweep: a dangling per-file pointer is pruned; a live per-file
    /// pointer collapses into the dir root.
    #[test]
    fn ghost_pointer_sweep_prunes_dangling_and_collapses_live() {
        let s = scratch();
        // One live per-file pointer (real file) + one dangling.
        write_claim(&s.medulla, "real.light.md", &light_doc("Real", "a", "x"));
        let real = s.medulla.join("real.light.md");
        let dangling = s.medulla.join("gone.light.md");
        std::fs::write(
            &s.roots,
            serde_json::to_string_pretty(&vec![
                s.medulla.to_string_lossy().to_string(),
                real.to_string_lossy().to_string(),
                dangling.to_string_lossy().to_string(),
            ])
            .unwrap(),
        )
        .unwrap();

        let mig = MedullaMigration::new(&s.medulla, &s.project, &s.roots, "/path/to/repo");
        let (ghosts, swept) = mig.sweep_ingest_roots().expect("sweep");
        assert_eq!(ghosts.len(), 2, "one dangling + one collapsed");
        let roots = swept.expect("swept list");
        assert!(
            roots.iter().all(|r| !r.ends_with(".light.md")),
            "no per-file .light.md pointers remain, got: {roots:?}"
        );
        assert!(
            roots.contains(&s.medulla.to_string_lossy().to_string()),
            "the dir root survives"
        );
    }

    /// Idempotency guard (field bug 2026-07-05): a SECOND `apply` over a store
    /// with nothing left to move or stamp reports `already_migrated` with zero
    /// moves and an empty backup path — it never writes a fresh (empty) backup nor
    /// reports a phantom count-conservation failure.
    #[test]
    fn apply_is_idempotent_on_already_migrated_store() {
        let s = scratch();
        write_claim(
            &s.medulla,
            "sliceship.light.md",
            &light_doc(
                "SliceShip",
                "closer",
                "shipped.\n\n[⍂ entity: SliceShip]\n[𝔻 evidence: m1nd-mcp/src/x.rs]\n",
            ),
        );
        write_claim(
            &s.medulla,
            "maxpref.light.md",
            &light_doc(
                "MaxPref",
                "orch",
                "maintainer doctrine.\n\n[⍂ entity: MaxPref]\n",
            ),
        );
        std::fs::write(
            &s.roots,
            serde_json::to_string_pretty(&vec![s.medulla.to_string_lossy().to_string()]).unwrap(),
        )
        .unwrap();

        let mig = MedullaMigration::new(&s.medulla, &s.project, &s.roots, "/path/to/repo");
        let first = mig.apply().expect("first apply");
        assert!(!first.already_migrated, "first apply actually migrates");
        assert_eq!(first.moved_to_project, 1);

        // Drop the first (legitimate) backup so the assertion isolates run 2.
        for e in std::fs::read_dir(&s.medulla).unwrap().flatten() {
            if e.file_name().to_string_lossy().starts_with(BACKUP_PREFIX) {
                std::fs::remove_dir_all(e.path()).unwrap();
            }
        }

        let second = mig.apply().expect("second apply");
        assert!(
            second.already_migrated,
            "a re-applied migration reports already_migrated"
        );
        assert_eq!(second.moved_to_project, 0, "nothing moved the second time");
        assert!(
            second.count_conserved,
            "already-migrated must not report a count-conservation failure"
        );
        assert!(
            second.backup_dir.is_empty(),
            "already-migrated writes no backup, got: {}",
            second.backup_dir
        );
        let backups: Vec<_> = std::fs::read_dir(&s.medulla)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(BACKUP_PREFIX))
            .collect();
        assert!(
            backups.is_empty(),
            "no fresh backup dir on the second apply"
        );
    }

    /// Idempotent stamping: a claim already carrying Origin-Brain is untouched.
    #[test]
    fn stamp_origin_brain_is_idempotent() {
        let already = "---\nProtocol: L1GHT/1.0\nNode: X\nState: authored\nSource-Agent: a\nOrigin-Brain: medulla\n---\n\n# X\n";
        assert_eq!(stamp_origin_brain(already, "medulla"), already);

        let fresh =
            "---\nProtocol: L1GHT/1.0\nNode: X\nState: authored\nSource-Agent: a\n---\n\n# X\n";
        let stamped = stamp_origin_brain(fresh, "/path/to/repo");
        assert!(stamped.contains("Origin-Brain: /path/to/repo"));
        assert!(
            stamped.find("Origin-Brain:").unwrap() > stamped.find("Source-Agent:").unwrap(),
            "Origin-Brain lands after Source-Agent"
        );
    }
}
