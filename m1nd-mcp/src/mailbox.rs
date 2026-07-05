//! MEDULLA slice M7b — the per-project mailbox (MEDULLA-PRD §9.2, HUMAN-LAYER
//! §4A.11, ORGANISM §C2.2/§C6.2/§C7.5/§C10 R8).
//!
//! The field-report spool (`~/.m1nd/field-reports.jsonl`) is the ONE append-only
//! write archive — every agent's mail slot, never truncated (MED-INV-9). This
//! module is the DISTRIBUTION + READ layer on top of it: it files each letter
//! into exactly one **box** by the ownership law, derives each letter's **fate**
//! from the reply graph, unions all boxes for triage (`inbox_sweep`), and reads
//! one brain's box for the Hall's caixinha view (`GET /api/mailbox`).
//!
//! ## The ownership law (MED-INV-10, binding)
//!
//! *The box is the PROJECT's property, not the brain's.* A letter that names a
//! project files into that project's **repo-side** box at
//! `<repo>/.m1nd/inbox.jsonl` — brain existing or not, because the box travels
//! with git. Only genuinely projectless letters (owner-runtime, `all`, no-repo
//! tool reports like the Context7 letter) reach the **medulla box** at
//! `<owner runtime_root>/inbox.jsonl`. **A letter naming a project directory NEVER
//! lands in the medulla box.** A letter whose repo dir is absent from THIS machine
//! stays in the spool marked `pending_distribution` — named by the sweep and
//! doctor, never re-routed to the medulla.
//!
//! ## Fates are derived, never stored (ORGANISM §C2.2)
//!
//! A letter's fate is a function of the reply graph (`answers[]`), computed at
//! read time exactly like `aged` is computed from `Created` — state that can be
//! recomputed must not be stored where it can drift:
//!
//! - `wet_ink` — no answering letter (aberta, ●)
//! - `in_flight` — referenced by a non-closing answer (picked up, ◍)
//! - `fired_clay` — a receipt/triage letter carries this id in `answers[]`
//!   (respondida, immutable forever, ↳)
//! - `external` — about a tool that is not m1nd (not m1nd's to close, ◌)
//!
//! The "abertas" count = `wet_ink + in_flight`; `external` is visible but NEVER
//! counted (a counter that can never reach zero is pressure, not honesty).
//!
//! ## Consent-deferred box birth (ORGANISM §C7.5)
//!
//! The distributor may create `<repo>/.m1nd/` to file a box, but writes an
//! ignore-by-default `.gitignore` covering `inbox.jsonl` — so the box is local
//! immediately and git-travels only after the repo's own `m1nd init` flips it to
//! committed (the ONE consent moment). An existing `.gitignore` is never rewritten.
//!
//! ## Safety posture — LOCAL telemetry, re-runnable
//!
//! Distribution is safe to run locally: it is telemetry, not memory. It is
//! **idempotent** — append-with-dedup by content id, so a second run appends
//! nothing. Pure filesystem, no live `SessionState`, so it is scratch-testable
//! and structurally incapable of touching a running owner's graph.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use m1nd_core::error::{M1ndError, M1ndResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One field-report letter, parsed from a spool/box JSONL line. The `id` is
/// derived from the raw line bytes (never stored in the file) — stable and
/// machine-independent, so git-traveled letters dedup across machines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Letter {
    /// `sha256(raw line bytes)[0..12]` — the content identity (derived, §9.2).
    /// `default` so it parses from a raw line that never stored an id; the parser
    /// overwrites it from the line bytes immediately (`parse_letter`).
    #[serde(default)]
    pub id: String,
    /// ISO-8601 timestamp string, verbatim from the letter.
    #[serde(default)]
    pub ts: String,
    /// The authoring agent id.
    #[serde(default)]
    pub agent: String,
    /// The letter class (`friction | bug | triage | honesty | win`, plus the
    /// M7b `memory_misdelivery` vocabulary). Free-form by construction — the
    /// spool never validated it.
    #[serde(default)]
    pub class: String,
    /// The raw `repo` field as written (un-normalized).
    #[serde(default)]
    pub repo: String,
    /// The explicit `brain` field (B3's addition) — wins over `repo` when present.
    #[serde(default)]
    pub brain: String,
    /// The tool the letter is about.
    #[serde(default)]
    pub tool: String,
    /// The observation (`what`), verbatim.
    #[serde(default)]
    pub what: String,
    /// Optional expected behavior.
    #[serde(default)]
    pub expected: String,
    /// Optional evidence snippet.
    #[serde(default)]
    pub snippet: String,
    /// Ids this letter answers (the reply graph). A triage/receipt letter SHOULD
    /// carry these; they flip the referenced letters to `fired_clay`.
    #[serde(default)]
    pub answers: Vec<String>,
    /// The exact raw JSONL line the id was hashed over — kept so distribution
    /// re-serializes the letter byte-identically (the archive's own bytes travel).
    #[serde(skip)]
    pub raw: String,
}

/// The `memory_misdelivery` telemetry class (MEDULLA-PRD §9.1). The value written
/// in the letter's `class` field; the `kind` narrows what went wrong.
pub const CLASS_MEMORY_MISDELIVERY: &str = "memory_misdelivery";

/// The closed `kind` vocabulary for a `memory_misdelivery` letter (§9.1). A `kind`
/// outside this set is a review error.
pub const MISDELIVERY_KINDS: &[&str] = &[
    // a claim from brain Y surfaced in brain X's beat without promotion
    "leak",
    // the beat claimed no memory while the store holds live claims
    "false_absence",
    // a write landed in a store the caller's root does not own
    "wrong_store_write",
    // provenance/counters attributed to the wrong brain or agent
    "misattribution",
    // a store on disk invisible to a listing/recall surface
    "vanished",
];

/// A letter's derived fate (ORGANISM §C2.2, grammar 3). Never stored — computed
/// from the reply graph at read time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fate {
    /// No answering letter — aberta (●).
    WetInk,
    /// Referenced by a non-closing answer — picked up, fix in flight (◍).
    InFlight,
    /// A receipt/triage letter carries this id in `answers[]` — respondida (↳).
    FiredClay,
    /// About a tool that is not m1nd — not m1nd's to close (◌).
    External,
}

impl Fate {
    /// The lowercase wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            Fate::WetInk => "wet_ink",
            Fate::InFlight => "in_flight",
            Fate::FiredClay => "fired_clay",
            Fate::External => "external",
        }
    }

    /// Whether this fate counts toward the "abertas" total (`wet_ink + in_flight`).
    /// `external` and `fired_clay` never inflate it (MED-INV-9).
    pub fn is_open(self) -> bool {
        matches!(self, Fate::WetInk | Fate::InFlight)
    }
}

/// Where a letter files (the ownership law, §9.2). A project letter files into
/// that repo's box; a projectless letter files into the medulla box; a letter
/// whose repo dir is absent from this machine waits in the spool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxTarget {
    /// The repo-side box for a present project directory (absolute path).
    Project(PathBuf),
    /// The medulla box — genuinely projectless letters only.
    Medulla,
    /// The named repo is not present on this machine; the letter stays in the
    /// spool marked `pending_distribution`. Carries the normalized name it looked
    /// for, so the sweep/doctor can name it.
    Pending(String),
}

/// Expand a leading `~` to the user's home directory. Returns the input unchanged
/// when there is no home or no leading `~`.
fn expand_home(raw: &str) -> String {
    expand_home_with(raw, home_dir())
}

/// Pure core of [`expand_home`]: expand a leading `~` against an explicit home,
/// so it is testable without mutating the process-global `HOME` env var.
fn expand_home_with(raw: &str, home: Option<PathBuf>) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = home {
            return home.join(rest).to_string_lossy().to_string();
        }
    } else if trimmed == "~" {
        if let Some(home) = home {
            return home.to_string_lossy().to_string();
        }
    }
    trimmed.to_string()
}

/// The user's home directory (env `HOME`, then `USERPROFILE`). Pure — no crate dep.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Strip a trailing parenthetical annotation, e.g. `repo-a (worktree …)` → `repo-a`,
/// `repo-a (org-critique, read-only seat)` → `repo-a`. Only the FIRST `(` is cut,
/// so a path with no parenthesis is returned unchanged.
fn strip_parenthetical(raw: &str) -> String {
    match raw.find('(') {
        Some(i) => raw[..i].trim().to_string(),
        None => raw.trim().to_string(),
    }
}

/// The base project name a `repo`/`brain` field resolves to, BEFORE presence
/// resolution. Applies the §9.2 normalization: expand `~`, strip parenthetical
/// annotations, and collapse `<base>-*` worktree names onto `<base>` for the
/// project whose worktrees are named that way (the caller supplies the canonical
/// project basename via `worktree_base`). Returns the normalized string — either
/// an absolute path (had a `/`) or a bare basename.
///
/// `worktree_base` is the project basename whose worktree dirs are named
/// `<base>-<suffix>` (in this repo's own telemetry, the m1nd project). Passing it
/// keeps the mechanism neutral: the RULE is "a `<base>-*` sibling name collapses
/// to `<base>`", not "the string m1nd is special".
pub fn normalize_repo(raw: &str, worktree_base: &str) -> String {
    let no_paren = strip_parenthetical(raw);
    let expanded = expand_home(&no_paren);
    let expanded = expanded.trim().trim_end_matches('/');

    // If it's a path (has a separator), the identity is its basename — but a
    // `<base>-<suffix>` basename collapses to `<base>` (worktree names).
    let base = Path::new(expanded)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(expanded)
        .to_string();

    collapse_worktree(&base, worktree_base)
}

/// Collapse a `<base>-<suffix>` worktree/annotation name onto `<base>`. A bare
/// `<base>` stays `<base>`; an unrelated name stays itself.
fn collapse_worktree(name: &str, worktree_base: &str) -> String {
    if name == worktree_base {
        return name.to_string();
    }
    if let Some(rest) = name.strip_prefix(worktree_base) {
        // `<base>-…` (a hyphen or nothing after the base) → the base.
        if rest.starts_with('-') || rest.is_empty() {
            return worktree_base.to_string();
        }
    }
    name.to_string()
}

/// The set of repo names/paths that are genuinely PROJECTLESS — they file into
/// the medulla box, never a project box (§9.2). Matched after `~` expansion +
/// parenthetical strip, case-sensitively on the normalized basename/marker.
fn is_projectless(raw: &str, owner_runtime_root: &Path) -> bool {
    let no_paren = strip_parenthetical(raw);
    let expanded = expand_home(&no_paren);
    let expanded_trimmed = expanded.trim().trim_end_matches('/');

    // The owner runtime root itself (e.g. `~/.m1nd/runtimes/claude`) — owner
    // telemetry, not a project.
    let owner_key = owner_runtime_root.to_string_lossy();
    let owner_key = owner_key.trim_end_matches('/');
    if expanded_trimmed == owner_key {
        return true;
    }

    // Explicit projectless markers (the un-normalized field, lowercased).
    let low = no_paren.to_ascii_lowercase();
    matches!(low.as_str(), "all" | "")
        || low.starts_with("research task")
        || no_paren.trim().is_empty()
}

/// Resolve one letter to its box target (§9.2). `brain` field wins over `repo`;
/// projectless → medulla; a named project → its repo dir if present on this
/// machine (`Project`), else `Pending`.
///
/// `known_repos` maps a normalized project name → its absolute repo directory on
/// this machine (from the registry/store manifests + a path resolution). The
/// medulla box lives at `owner_runtime_root/inbox.jsonl`.
pub fn resolve_box(
    letter: &Letter,
    owner_runtime_root: &Path,
    worktree_base: &str,
    known_repos: &BTreeMap<String, PathBuf>,
) -> BoxTarget {
    // The `brain` field (B3) wins when present; else `repo`.
    let field = if !letter.brain.trim().is_empty() {
        letter.brain.as_str()
    } else {
        letter.repo.as_str()
    };

    if is_projectless(field, owner_runtime_root) {
        return BoxTarget::Medulla;
    }

    let normalized = normalize_repo(field, worktree_base);

    // If the field was an absolute path that exists, that IS the repo dir.
    let no_paren = strip_parenthetical(field);
    let expanded = expand_home(&no_paren);
    let as_path = Path::new(expanded.trim().trim_end_matches('/'));
    if as_path.is_absolute() && as_path.is_dir() {
        return BoxTarget::Project(as_path.to_path_buf());
    }

    // Otherwise resolve the normalized name against the known-repos map.
    if let Some(dir) = known_repos.get(&normalized) {
        if dir.is_dir() {
            return BoxTarget::Project(dir.clone());
        }
    }

    // A named project whose dir is not present → waits in the spool.
    BoxTarget::Pending(normalized)
}

/// Compute a letter id from a raw JSONL line: `sha256(raw line bytes)[0..12]`.
/// The line is trimmed of a trailing newline only — the bytes hashed are the
/// letter's own, so the same letter yields the same id on any machine.
pub fn letter_id(raw_line: &str) -> String {
    let bytes = raw_line.trim_end_matches(['\n', '\r']).as_bytes();
    let digest = Sha256::digest(bytes);
    let hex = format!("{digest:x}");
    hex[..12].to_string()
}

/// Parse one raw JSONL line into a [`Letter`], stamping its derived id and keeping
/// the raw bytes. Returns `None` for a blank line or unparseable JSON (the spool
/// is append-only free-form — a malformed line is skipped, never fatal).
pub fn parse_letter(raw_line: &str) -> Option<Letter> {
    let trimmed = raw_line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut letter: Letter = serde_json::from_str(trimmed).ok()?;
    letter.id = letter_id(raw_line);
    letter.raw = trimmed.to_string();
    Some(letter)
}

/// Read every letter from a JSONL file (spool or box). Missing file → empty vec
/// (not an error — a box that was never written simply has no letters).
pub fn read_letters(path: &Path) -> M1ndResult<Vec<Letter>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(M1ndError::Io(e)),
    };
    Ok(text.lines().filter_map(parse_letter).collect())
}

/// Derive the fate of every letter in a set from the reply graph (§C2.2). One
/// pass builds the answered-set from all `answers[]`; then each letter's fate is:
///   - `external` if its class marks it as about a non-m1nd tool,
///   - `fired_clay` if any letter answers it,
///   - `in_flight` if a non-closing letter references it (reserved: today a
///     reference IS an answer, so this arises only when a letter is referenced
///     but no receipt has closed it — modeled via `in_flight_refs`),
///   - `wet_ink` otherwise.
///
/// `external_ids` is the set of letter ids judged external (about a non-m1nd
/// tool). `in_flight_ids` is the set referenced-but-not-closed. This keeps the
/// derivation pure and testable; [`fates_for`] composes the default policy.
pub fn derive_fate(
    id: &str,
    answered: &BTreeSet<String>,
    external_ids: &BTreeSet<String>,
    in_flight_ids: &BTreeSet<String>,
) -> Fate {
    if external_ids.contains(id) {
        return Fate::External;
    }
    if answered.contains(id) {
        return Fate::FiredClay;
    }
    if in_flight_ids.contains(id) {
        return Fate::InFlight;
    }
    Fate::WetInk
}

/// The answered-set: every id that appears in some letter's `answers[]` — those
/// letters are `fired_clay` (a receipt closed them).
pub fn answered_set(letters: &[Letter]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for l in letters {
        for a in &l.answers {
            out.insert(a.clone());
        }
    }
    out
}

/// For each letter, who answered it (the receipt ids). Feeds the read surface's
/// `answered_by[]` field so the human layer can render the receipt link.
pub fn answered_by(letters: &[Letter]) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for l in letters {
        for a in &l.answers {
            out.entry(a.clone()).or_default().push(l.id.clone());
        }
    }
    out
}

/// The relative path of a box inside a repo (`.m1nd/inbox.jsonl`).
pub const BOX_REL_PATH: &str = ".m1nd/inbox.jsonl";

/// The medulla box file name inside the owner runtime root (`inbox.jsonl`).
pub const MEDULLA_BOX_FILE: &str = "inbox.jsonl";

/// The `.gitignore` line the consent-deferred birth writes (§C7.5): the box is
/// local until `m1nd init` flips it to committed.
const GITIGNORE_LINE: &str = "inbox.jsonl\n";

/// The `.m1nd/inbox.jsonl` path inside a repo dir.
fn box_path_for_repo(repo_dir: &Path) -> PathBuf {
    repo_dir.join(BOX_REL_PATH)
}

/// The medulla box path under an owner runtime root.
pub fn medulla_box_path(owner_runtime_root: &Path) -> PathBuf {
    owner_runtime_root.join(MEDULLA_BOX_FILE)
}

/// Ensure a repo's `.m1nd/` dir exists with a consent-deferred `.gitignore`
/// covering `inbox.jsonl` (§C7.5). Idempotent: an existing `.gitignore` is NEVER
/// rewritten (a repo that consented via `m1nd init` keeps its committed state).
fn ensure_box_birth(repo_dir: &Path) -> M1ndResult<()> {
    let m1nd_dir = repo_dir.join(".m1nd");
    std::fs::create_dir_all(&m1nd_dir).map_err(M1ndError::Io)?;
    let gitignore = m1nd_dir.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(&gitignore, GITIGNORE_LINE).map_err(M1ndError::Io)?;
    }
    Ok(())
}

/// The ids already present in a box file (for append-with-dedup). A box may hold
/// letters that arrived by git from OTHER machines — those ids are the box's own
/// and must survive un-clobbered.
fn box_ids(box_path: &Path) -> M1ndResult<BTreeSet<String>> {
    Ok(read_letters(box_path)?.into_iter().map(|l| l.id).collect())
}

/// Append a letter's raw bytes to a box file (create if missing), followed by a
/// newline — the box stays valid JSONL.
fn append_letter(box_path: &Path, letter: &Letter) -> M1ndResult<()> {
    use std::io::Write;
    if let Some(parent) = box_path.parent() {
        std::fs::create_dir_all(parent).map_err(M1ndError::Io)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(box_path)
        .map_err(M1ndError::Io)?;
    writeln!(f, "{}", letter.raw).map_err(M1ndError::Io)?;
    Ok(())
}

/// One letter's distribution outcome, for the receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiledLetter {
    /// The letter id.
    pub id: String,
    /// Where it filed (`project:<dir>` | `medulla` | `pending:<name>`).
    pub target: String,
    /// True if it was newly appended (false = dedup no-op, already present).
    pub appended: bool,
}

/// The receipt of a distribution pass (§9.2, idempotent + count-honest).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionReceipt {
    /// Every spool letter, with its box and whether it was newly appended.
    pub filed: Vec<FiledLetter>,
    /// Total letters in the spool.
    pub spool_total: usize,
    /// Letters newly appended this pass (0 on a re-run = idempotence proof).
    pub appended: usize,
    /// Letters that filed into a project box.
    pub to_project: usize,
    /// Letters that filed into the medulla box.
    pub to_medulla: usize,
    /// Letters waiting on an absent repo (`pending_distribution`), by name.
    pub pending: Vec<String>,
}

/// Distribute the spool into per-project boxes + the medulla box (§9.2).
/// Idempotent: append-with-dedup by content id, so a second run appends nothing.
/// A project box gets a consent-deferred birth (`.gitignore`) on first write.
///
/// - `spool_path` — `~/.m1nd/field-reports.jsonl` (read-only here; NEVER truncated).
/// - `owner_runtime_root` — the medulla box's home.
/// - `worktree_base` — the project basename whose worktrees are `<base>-*` (§9.2).
/// - `known_repos` — normalized project name → absolute repo dir on this machine.
/// - `external_ids` — ids judged external (unused by distribution; fates ride the
///   read surface). Present for signature symmetry with the read layer.
pub fn distribute(
    spool_path: &Path,
    owner_runtime_root: &Path,
    worktree_base: &str,
    known_repos: &BTreeMap<String, PathBuf>,
) -> M1ndResult<DistributionReceipt> {
    let letters = read_letters(spool_path)?;
    let spool_total = letters.len();

    // Cache each box's existing ids ONCE, then update in-memory as we append —
    // so multiple letters to the same box within one pass dedup correctly too.
    let medulla_box = medulla_box_path(owner_runtime_root);
    let mut box_id_cache: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();

    let mut filed = Vec::with_capacity(spool_total);
    let mut appended = 0usize;
    let mut to_project = 0usize;
    let mut to_medulla = 0usize;
    let mut pending: Vec<String> = Vec::new();

    for letter in &letters {
        let target = resolve_box(letter, owner_runtime_root, worktree_base, known_repos);
        match target {
            BoxTarget::Project(repo_dir) => {
                ensure_box_birth(&repo_dir)?;
                let box_path = box_path_for_repo(&repo_dir);
                let ids = match box_id_cache.get_mut(&box_path) {
                    Some(ids) => ids,
                    None => {
                        let ids = box_ids(&box_path)?;
                        box_id_cache.entry(box_path.clone()).or_insert(ids)
                    }
                };
                let is_new = !ids.contains(&letter.id);
                if is_new {
                    append_letter(&box_path, letter)?;
                    ids.insert(letter.id.clone());
                    appended += 1;
                }
                to_project += 1;
                filed.push(FiledLetter {
                    id: letter.id.clone(),
                    target: format!("project:{}", repo_dir.to_string_lossy()),
                    appended: is_new,
                });
            }
            BoxTarget::Medulla => {
                let ids = match box_id_cache.get_mut(&medulla_box) {
                    Some(ids) => ids,
                    None => {
                        let ids = box_ids(&medulla_box)?;
                        box_id_cache.entry(medulla_box.clone()).or_insert(ids)
                    }
                };
                let is_new = !ids.contains(&letter.id);
                if is_new {
                    append_letter(&medulla_box, letter)?;
                    ids.insert(letter.id.clone());
                    appended += 1;
                }
                to_medulla += 1;
                filed.push(FiledLetter {
                    id: letter.id.clone(),
                    target: "medulla".to_string(),
                    appended: is_new,
                });
            }
            BoxTarget::Pending(name) => {
                pending.push(name.clone());
                filed.push(FiledLetter {
                    id: letter.id.clone(),
                    target: format!("pending:{name}"),
                    appended: false,
                });
            }
        }
    }

    pending.sort();
    pending.dedup();

    Ok(DistributionReceipt {
        filed,
        spool_total,
        appended,
        to_project,
        to_medulla,
        pending,
    })
}

/// A box the sweep knows about — a repo-side box or the medulla box.
#[derive(Debug, Clone)]
pub struct KnownBox {
    /// A human label (the project name, or `medulla`).
    pub label: String,
    /// The box file path.
    pub path: PathBuf,
    /// Whether the path is reachable (its repo dir is present on this machine).
    /// A box the sweep cannot reach is NAMED in the output, never silently skipped.
    pub reachable: bool,
}

/// The result of `inbox_sweep` (§9.2): the union of the spool ∪ every known box,
/// de-duplicated by content id (each letter once), plus the names of any boxes
/// that could not be reached.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepResult {
    /// Every distinct letter across the spool and all boxes (each id once).
    pub letters: Vec<SweptLetter>,
    /// Total distinct letters.
    pub total: usize,
    /// Open letters (`wet_ink + in_flight`) — `external`/`fired_clay` excluded.
    pub open: usize,
    /// Boxes the sweep could not reach (repo not on this machine), named.
    pub unreachable: Vec<String>,
    /// The `memory_misdelivery` letter count (the confusion signal).
    pub misdelivery: usize,
}

/// A letter as returned by the sweep / the read surface, with its derived fate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweptLetter {
    /// The content id.
    pub id: String,
    /// Timestamp string.
    pub ts: String,
    /// Authoring agent.
    pub agent: String,
    /// Class.
    pub class: String,
    /// Raw repo field.
    pub repo: String,
    /// Explicit brain field.
    pub brain: String,
    /// Tool the letter is about.
    pub tool: String,
    /// The observation.
    pub what: String,
    /// Expected behavior (may be empty).
    pub expected: String,
    /// Evidence snippet (may be empty).
    pub snippet: String,
    /// The reply graph: ids this letter answers.
    pub answers: Vec<String>,
    /// The receipt ids that answered THIS letter (derived).
    pub answered_by: Vec<String>,
    /// The derived fate.
    pub state: String,
}

/// Whether a letter is `external` — about a tool that is NOT m1nd. Derived from
/// the letter's `tool`/`repo`: a report whose tool names a foreign tool (and repo
/// is not this project) is external. Conservative: default is NOT external (only
/// letters that clearly name a non-m1nd tool). `foreign_tools` lets the caller
/// pass the set of tool markers judged external; empty = nothing external.
fn is_external(letter: &Letter, foreign_tools: &BTreeSet<String>) -> bool {
    if foreign_tools.is_empty() {
        return false;
    }
    let tool = letter.tool.to_ascii_lowercase();
    foreign_tools.iter().any(|t| tool.contains(t.as_str()))
}

/// Build the [`SweptLetter`] view of a letter set with derived fates (§C2.2).
fn view_letters(letters: &[Letter], foreign_tools: &BTreeSet<String>) -> Vec<SweptLetter> {
    let answered = answered_set(letters);
    let by = answered_by(letters);
    let external_ids: BTreeSet<String> = letters
        .iter()
        .filter(|l| is_external(l, foreign_tools))
        .map(|l| l.id.clone())
        .collect();
    // Today a reference IS a closing answer (no separate "picked up but open"
    // signal in the spool schema), so `in_flight_ids` is empty by default — a
    // referenced letter is `fired_clay`. The InFlight fate is derivable the moment
    // a non-closing reference class appears; the derivation is ready for it.
    let in_flight_ids: BTreeSet<String> = BTreeSet::new();

    letters
        .iter()
        .map(|l| {
            let fate = derive_fate(&l.id, &answered, &external_ids, &in_flight_ids);
            SweptLetter {
                id: l.id.clone(),
                ts: l.ts.clone(),
                agent: l.agent.clone(),
                class: l.class.clone(),
                repo: l.repo.clone(),
                brain: l.brain.clone(),
                tool: l.tool.clone(),
                what: l.what.clone(),
                expected: l.expected.clone(),
                snippet: l.snippet.clone(),
                answers: l.answers.clone(),
                answered_by: by.get(&l.id).cloned().unwrap_or_default(),
                state: fate.as_str().to_string(),
            }
        })
        .collect()
}

/// `inbox_sweep` (§9.2, §C6.2 — CLI/REST only, OFF the MCP surface): read the
/// spool ∪ every known box, de-duplicate by content id (each letter once), and
/// name any box the sweep could not reach. This is the triage session's whole
/// view — the m1nd team keeps seeing the conjunto; each project keeps what it felt.
pub fn inbox_sweep(
    spool_path: &Path,
    boxes: &[KnownBox],
    foreign_tools: &BTreeSet<String>,
) -> M1ndResult<SweepResult> {
    // Union by id: the spool first (this machine's write truth), then every
    // reachable box (may carry git-traveled letters the spool never saw).
    let mut by_id: BTreeMap<String, Letter> = BTreeMap::new();
    for l in read_letters(spool_path)? {
        by_id.entry(l.id.clone()).or_insert(l);
    }
    let mut unreachable = Vec::new();
    for kb in boxes {
        if !kb.reachable {
            unreachable.push(kb.label.clone());
            continue;
        }
        for l in read_letters(&kb.path)? {
            by_id.entry(l.id.clone()).or_insert(l);
        }
    }

    let union: Vec<Letter> = by_id.into_values().collect();
    let viewed = view_letters(&union, foreign_tools);
    let total = viewed.len();
    let open = viewed
        .iter()
        .filter(|l| l.state == "wet_ink" || l.state == "in_flight")
        .count();
    let misdelivery = viewed
        .iter()
        .filter(|l| l.class == CLASS_MEMORY_MISDELIVERY)
        .count();

    unreachable.sort();
    unreachable.dedup();

    Ok(SweepResult {
        letters: viewed,
        total,
        open,
        unreachable,
        misdelivery,
    })
}

/// The `counts` block on a box read (§9.2) — fate tallies for the header line.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BoxCounts {
    /// `wet_ink` letters (aberta).
    pub wet_ink: usize,
    /// `in_flight` letters (em voo).
    pub in_flight: usize,
    /// `fired_clay` letters (respondida).
    pub fired_clay: usize,
    /// `external` letters (visible, never counted toward "abertas").
    pub external: usize,
}

impl BoxCounts {
    /// The "abertas" count = `wet_ink + in_flight` (MED-INV-9) — `external` and
    /// `fired_clay` never inflate it.
    pub fn open(&self) -> usize {
        self.wet_ink + self.in_flight
    }
}

/// The `GET /api/mailbox?brain=…` payload (§9.2): one brain's box with derived
/// fates + honest counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxView {
    /// The letters in THIS box only, with derived fates.
    pub letters: Vec<SweptLetter>,
    /// The fate tallies.
    pub counts: BoxCounts,
}

/// Read one box (a repo-side box or the medulla box) into its view with derived
/// fates + counts (§9.2). The read is scoped to THIS box's letters only — never a
/// re-fold of the spool (MED-INV-1 / INV-17).
pub fn read_box(box_path: &Path, foreign_tools: &BTreeSet<String>) -> M1ndResult<MailboxView> {
    let letters = read_letters(box_path)?;
    let viewed = view_letters(&letters, foreign_tools);
    let mut counts = BoxCounts::default();
    for l in &viewed {
        match l.state.as_str() {
            "wet_ink" => counts.wet_ink += 1,
            "in_flight" => counts.in_flight += 1,
            "fired_clay" => counts.fired_clay += 1,
            "external" => counts.external += 1,
            _ => {}
        }
    }
    Ok(MailboxView {
        letters: viewed,
        counts,
    })
}

/// The `mailbox_open_count` for a brain's box (the D3 face count) — `wet_ink +
/// in_flight` only. A missing box reads as 0 open (never a fabricated non-zero).
pub fn mailbox_open_count(box_path: &Path, foreign_tools: &BTreeSet<String>) -> M1ndResult<usize> {
    Ok(read_box(box_path, foreign_tools)?.counts.open())
}

/// The bare basename of a project root path (the worktree-base input). Pure —
/// `~` expansion + parenthetical strip + trailing-slash trim, then `file_name`.
/// Public so the CLI binary (a separate crate) can derive the worktree base.
pub fn project_basename(root: &str) -> String {
    let no_paren = strip_parenthetical(root);
    let expanded = expand_home(&no_paren);
    let expanded = expanded.trim().trim_end_matches('/');
    Path::new(expanded)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(expanded)
        .to_string()
}

/// Build the `known_repos` map (normalized project name → absolute repo dir) and
/// the `boxes` list from a set of project roots present on this machine (from the
/// registry `disk_roster` + the bound root). `worktree_base` is the project whose
/// worktrees are `<base>-*`. A root whose dir is absent is still listed as an
/// UNREACHABLE box (named by the sweep), never dropped.
///
/// A bare `repo` name resolves against `known_repos` by normalized basename, so a
/// name that maps to a SINGLE root routes cleanly. When a basename maps to more
/// than one DISTINCT root (two brains sharing a basename in different parents),
/// the name is left OUT of `known_repos` — a bare letter for it abstains to
/// `Pending` rather than being silently routed into whichever root won a
/// first-wins race. This mirrors `covering_brain`'s unique/abstain law: exactly
/// one match resolves, zero-or-more-than-one is honest doubt, never a guess. Every
/// distinct root is still listed as a named `KnownBox` (the sweep surfaces it).
pub fn boxes_from_roots(
    project_roots: &[String],
    worktree_base: &str,
) -> (BTreeMap<String, PathBuf>, Vec<KnownBox>) {
    // Group the DISTINCT dirs each normalized name resolves to. A repeated exact
    // (name, dir) pair is the same brain seen twice (roster + bound root) — one
    // distinct dir, so it still resolves; two DIFFERENT dirs is the ambiguity.
    let mut dirs_by_name: BTreeMap<String, BTreeSet<PathBuf>> = BTreeMap::new();
    let mut boxes: Vec<KnownBox> = Vec::new();
    let mut seen_box: BTreeSet<(String, PathBuf)> = BTreeSet::new();

    for root in project_roots {
        let dir = PathBuf::from(root.trim());
        let name = normalize_repo(root, worktree_base);
        if name.is_empty() {
            continue;
        }
        let reachable = dir.is_dir();
        if reachable {
            dirs_by_name
                .entry(name.clone())
                .or_default()
                .insert(dir.clone());
        }
        // One named box per distinct (name, dir): every root stays visible, but a
        // dir listed twice is not duplicated in the sweep.
        if seen_box.insert((name.clone(), dir.clone())) {
            boxes.push(KnownBox {
                label: name,
                path: box_path_for_repo(&dir),
                reachable,
            });
        }
    }

    // Only names with exactly ONE distinct reachable dir are resolvable; an
    // ambiguous basename is dropped so a bare letter for it stays Pending.
    let known: BTreeMap<String, PathBuf> = dirs_by_name
        .into_iter()
        .filter_map(
            |(name, dirs)| match dirs.into_iter().collect::<Vec<_>>().as_slice() {
                [only] => Some((name, only.clone())),
                _ => None,
            },
        )
        .collect();

    (known, boxes)
}

/// The spool path derived from a runtime root: `<home>/.m1nd/field-reports.jsonl`
/// is the ONE mail slot. The runtime root is `<home>/.m1nd/runtimes/<domain>`, so
/// the spool is two dirs up. Falls back to `HOME/.m1nd/field-reports.jsonl`.
pub fn spool_path_for_runtime(owner_runtime_root: &Path) -> PathBuf {
    // runtime_root = <home>/.m1nd/runtimes/<domain> → the `.m1nd` dir is its
    // grandparent's parent. Walk up to the `.m1nd` component.
    let mut cur = owner_runtime_root;
    while let Some(name) = cur.file_name().and_then(|n| n.to_str()) {
        if name == ".m1nd" {
            return cur.join("field-reports.jsonl");
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => break,
        }
    }
    // Fallback: <home>/.m1nd/field-reports.jsonl.
    if let Some(home) = home_dir() {
        return home.join(".m1nd").join("field-reports.jsonl");
    }
    owner_runtime_root.join("field-reports.jsonl")
}

/// The `doctor` mailbox block (MEDULLA-PRD §9.3): the confusion metric read as
/// COUNTS ONLY (no uncalibrated quality scores). `confusion_rate` =
/// confirmed `memory_misdelivery` events this week; read against total letters
/// (volume) and `beats_served` (a caller-supplied counter, 0 when unknown).
/// `pending_distribution` = letters whose repo is absent from this machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorMailbox {
    /// Confirmed `memory_misdelivery` letters (the confusion signal), all-time.
    pub memory_misdelivery: usize,
    /// `memory_misdelivery` letters within the last 7 days (the weekly rate).
    pub confusion_rate_7d: usize,
    /// Total letters in the spool (volume — the denominator antifragility rides).
    pub spool_letters: usize,
    /// Letters waiting on an absent repo (`pending_distribution`), by name.
    pub pending_distribution: Vec<String>,
    /// The spool path (the ONE write archive), for the operator.
    pub spool_path: String,
}

/// Whether an ISO-8601-ish `ts` string falls within the last `days` days of
/// `now_ms`. Best-effort: parses the leading `YYYY-MM-DD` and compares by day.
/// A ts that does not parse is treated as OUTSIDE the window (never inflates the
/// rate on bad data).
fn within_days(ts: &str, now_ms: u64, days: u64) -> bool {
    // Parse YYYY-MM-DD from the head.
    let head: String = ts.chars().take(10).collect();
    let parts: Vec<&str> = head.split('-').collect();
    if parts.len() != 3 {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (
        parts[0].parse::<i64>(),
        parts[1].parse::<i64>(),
        parts[2].parse::<i64>(),
    ) else {
        return false;
    };
    // Days since epoch (proleptic Gregorian, civil calendar algorithm).
    let letter_days = days_from_civil(y, m, d);
    let now_days = (now_ms / 86_400_000) as i64;
    now_days.saturating_sub(letter_days) <= days as i64 && letter_days <= now_days + 1
}

/// Days from 1970-01-01 for a civil (y, m, d) — Howard Hinnant's `days_from_civil`.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Compute the doctor mailbox block from the spool (§9.3). Counts only; the
/// distribution rule is applied to find `pending_distribution` (a named project
/// whose dir is absent). `now_ms` dates the weekly window.
pub fn doctor_mailbox(
    owner_runtime_root: &Path,
    worktree_base: &str,
    known_repos: &BTreeMap<String, PathBuf>,
    now_ms: u64,
) -> M1ndResult<DoctorMailbox> {
    let spool = spool_path_for_runtime(owner_runtime_root);
    let letters = read_letters(&spool)?;
    let spool_letters = letters.len();

    let mut memory_misdelivery = 0usize;
    let mut confusion_rate_7d = 0usize;
    let mut pending: Vec<String> = Vec::new();

    for l in &letters {
        if l.class == CLASS_MEMORY_MISDELIVERY {
            memory_misdelivery += 1;
            if within_days(&l.ts, now_ms, 7) {
                confusion_rate_7d += 1;
            }
        }
        if let BoxTarget::Pending(name) =
            resolve_box(l, owner_runtime_root, worktree_base, known_repos)
        {
            pending.push(name);
        }
    }
    pending.sort();
    pending.dedup();

    Ok(DoctorMailbox {
        memory_misdelivery,
        confusion_rate_7d,
        spool_letters,
        pending_distribution: pending,
        spool_path: spool.to_string_lossy().to_string(),
    })
}

// ===========================================================================
// Battery — M7b pure-logic cases. NEUTRAL fixtures only (repo-a / repo-b /
// /path/to/repo): no project names, no personal paths in the CODE (the DATA
// the live migration touches DOES carry project names, and that is fine — boxes
// are local per-repo files). Each case cites its MEDULLA-PRD §11 M7b acceptance.
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch dir cleaned on drop (mirrors medulla_migration's Scratch).
    struct Scratch {
        dir: PathBuf,
    }
    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "m1nd-mailbox-test-{tag}-{}-{}",
                std::process::id(),
                now_nanos()
            ));
            std::fs::create_dir_all(&dir).expect("mk scratch");
            Self { dir }
        }
        fn path(&self, rel: &str) -> PathBuf {
            self.dir.join(rel)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    fn now_nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }

    /// Build a raw JSONL letter line from fields (neutral fixtures).
    fn line(fields: serde_json::Value) -> String {
        fields.to_string()
    }

    fn write_spool(path: &Path, lines: &[String]) {
        std::fs::write(path, lines.join("\n") + "\n").expect("write spool");
    }

    /// A known-repos map from neutral names to scratch dirs that EXIST.
    fn mk_repo(scratch: &Scratch, name: &str) -> PathBuf {
        let dir = scratch.path(name);
        std::fs::create_dir_all(&dir).expect("mk repo dir");
        dir
    }

    // --- normalization rule (§9.2) -----------------------------------------

    #[test]
    fn normalize_collapses_worktree_and_annotation_variants() {
        // `<base>` and `<base>-<suffix>` and `<base> (annotation)` all collapse.
        assert_eq!(normalize_repo("repo-a", "repo-a"), "repo-a");
        assert_eq!(normalize_repo("repo-a-soul", "repo-a"), "repo-a");
        assert_eq!(
            normalize_repo("repo-a (worktree repo-a-x)", "repo-a"),
            "repo-a"
        );
        assert_eq!(
            normalize_repo("repo-a (org-critique, read-only seat)", "repo-a"),
            "repo-a"
        );
        // An unrelated repo is untouched.
        assert_eq!(normalize_repo("repo-b", "repo-a"), "repo-b");
        // A path resolves to its basename.
        assert_eq!(normalize_repo("/path/to/repo-b", "repo-a"), "repo-b");
    }

    #[test]
    fn expand_home_and_strip_parenthetical() {
        // Use the pure core with an explicit home — mutating the process-global
        // HOME here would poison every other parallel test that resolves ~/.m1nd.
        let home_path = PathBuf::from("/path/to/home");
        let home = Some(home_path.clone());
        // Expected via join() too, so the OS path separator matches (Windows: `\`).
        let expected = home_path.join("repo-b").to_string_lossy().to_string();
        assert_eq!(expand_home_with("~/repo-b", home.clone()), expected);
        assert_eq!(
            expand_home_with("~", home.clone()),
            home_path.to_string_lossy()
        );
        assert_eq!(expand_home_with("repo-b", home), "repo-b");
        assert_eq!(strip_parenthetical("repo-a (worktree x)"), "repo-a");
        assert_eq!(strip_parenthetical("repo-a"), "repo-a");
    }

    // --- distribution files every letter into exactly one box (§9.2 GREEN) --

    #[test]
    fn distribution_files_each_letter_into_exactly_one_box() {
        let s = Scratch::new("distribute");
        let runtime = s.path("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        let repo_a = mk_repo(&s, "repo-a");
        let repo_b = mk_repo(&s, "repo-b");

        let mut known = BTreeMap::new();
        known.insert("repo-a".to_string(), repo_a.clone());
        known.insert("repo-b".to_string(), repo_b.clone());

        let spool = s.path("spool.jsonl");
        write_spool(
            &spool,
            &[
                line(
                    serde_json::json!({"ts":"t1","agent":"a","repo":"repo-a","tool":"x","class":"bug","what":"w1"}),
                ),
                line(
                    serde_json::json!({"ts":"t2","agent":"a","repo":"repo-a-soul","tool":"x","class":"friction","what":"w2"}),
                ),
                line(
                    serde_json::json!({"ts":"t3","agent":"a","repo":"repo-b","tool":"x","class":"win","what":"w3"}),
                ),
                line(
                    serde_json::json!({"ts":"t4","agent":"a","repo":"all","tool":"x","class":"friction","what":"w4"}),
                ),
                // projectless: the owner runtime root itself.
                line(
                    serde_json::json!({"ts":"t5","agent":"a","repo": runtime.to_string_lossy(),"tool":"x","class":"friction","what":"w5"}),
                ),
            ],
        );

        let receipt = distribute(&spool, &runtime, "repo-a", &known).unwrap();
        assert_eq!(receipt.spool_total, 5);
        assert_eq!(receipt.appended, 5, "first run appends all");
        assert_eq!(receipt.to_project, 3, "repo-a, repo-a-soul→repo-a, repo-b");
        assert_eq!(receipt.to_medulla, 2, "all + owner-runtime");
        assert!(receipt.pending.is_empty());

        // repo-a's box holds exactly the two m1nd-variant letters.
        let box_a = read_letters(&box_path_for_repo(&repo_a)).unwrap();
        assert_eq!(box_a.len(), 2);
        // repo-b's box holds exactly one.
        let box_b = read_letters(&box_path_for_repo(&repo_b)).unwrap();
        assert_eq!(box_b.len(), 1);
        // the medulla box holds exactly the two projectless letters.
        let box_med = read_letters(&medulla_box_path(&runtime)).unwrap();
        assert_eq!(box_med.len(), 2);
    }

    // --- MED-INV-10: no repo-bearing letter in the medulla box --------------

    #[test]
    fn med_inv_10_no_repo_letter_ever_in_medulla_box() {
        let s = Scratch::new("medinv10");
        let runtime = s.path("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        let repo_a = mk_repo(&s, "repo-a");
        let mut known = BTreeMap::new();
        known.insert("repo-a".to_string(), repo_a.clone());

        let spool = s.path("spool.jsonl");
        write_spool(
            &spool,
            &[
                line(
                    serde_json::json!({"ts":"t1","agent":"a","repo":"repo-a","tool":"x","class":"bug","what":"repo letter"}),
                ),
                line(
                    serde_json::json!({"ts":"t2","agent":"a","repo":"all","tool":"x","class":"friction","what":"projectless"}),
                ),
                // a repo present via absolute path (not in known map) still files project-side.
                line(
                    serde_json::json!({"ts":"t3","agent":"a","repo": repo_a.to_string_lossy(),"tool":"x","class":"win","what":"abs path"}),
                ),
            ],
        );
        distribute(&spool, &runtime, "repo-a", &known).unwrap();

        let box_med = read_letters(&medulla_box_path(&runtime)).unwrap();
        // ONLY the projectless letter reached the medulla box.
        assert_eq!(box_med.len(), 1);
        assert_eq!(box_med[0].what, "projectless");
        // NO letter naming a project dir landed here (the invariant).
        for l in &box_med {
            let named_project =
                !l.repo.trim().is_empty() && l.repo != "all" && !is_projectless(&l.repo, &runtime);
            assert!(
                !named_project,
                "MED-INV-10 breached: {} in medulla box",
                l.repo
            );
        }
    }

    // --- idempotence: a second run appends nothing (§9.2 GREEN) --------------

    #[test]
    fn distribution_is_idempotent() {
        let s = Scratch::new("idempotent");
        let runtime = s.path("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        let repo_a = mk_repo(&s, "repo-a");
        let mut known = BTreeMap::new();
        known.insert("repo-a".to_string(), repo_a.clone());

        let spool = s.path("spool.jsonl");
        write_spool(
            &spool,
            &[
                line(
                    serde_json::json!({"ts":"t1","agent":"a","repo":"repo-a","tool":"x","class":"bug","what":"w1"}),
                ),
                line(
                    serde_json::json!({"ts":"t2","agent":"a","repo":"all","tool":"x","class":"friction","what":"w2"}),
                ),
            ],
        );

        let first = distribute(&spool, &runtime, "repo-a", &known).unwrap();
        assert_eq!(first.appended, 2);
        let box_len_1 = read_letters(&box_path_for_repo(&repo_a)).unwrap().len();

        let second = distribute(&spool, &runtime, "repo-a", &known).unwrap();
        assert_eq!(second.appended, 0, "re-run appends NOTHING (idempotent)");
        let box_len_2 = read_letters(&box_path_for_repo(&repo_a)).unwrap().len();
        assert_eq!(box_len_1, box_len_2, "box unchanged on re-run");
    }

    // --- git-inlet dedup: a box seeded with a git-traveled letter survives ---

    #[test]
    fn git_traveled_letter_survives_distribution_un_clobbered() {
        let s = Scratch::new("gitinlet");
        let runtime = s.path("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        let repo_a = mk_repo(&s, "repo-a");
        let mut known = BTreeMap::new();
        known.insert("repo-a".to_string(), repo_a.clone());

        // Pre-seed the box with a letter the LOCAL spool never saw (arrived by git).
        std::fs::create_dir_all(repo_a.join(".m1nd")).unwrap();
        let travelled = line(
            serde_json::json!({"ts":"tG","agent":"other-machine","repo":"repo-a","tool":"x","class":"win","what":"from another machine"}),
        );
        std::fs::write(box_path_for_repo(&repo_a), travelled.clone() + "\n").unwrap();
        let travelled_id = letter_id(&travelled);

        let spool = s.path("spool.jsonl");
        write_spool(
            &spool,
            &[line(
                serde_json::json!({"ts":"t1","agent":"a","repo":"repo-a","tool":"x","class":"bug","what":"local letter"}),
            )],
        );

        distribute(&spool, &runtime, "repo-a", &known).unwrap();
        let box_a = read_letters(&box_path_for_repo(&repo_a)).unwrap();
        // Both survive, deduped by id — the git-traveled one un-clobbered.
        assert_eq!(box_a.len(), 2);
        assert!(
            box_a.iter().any(|l| l.id == travelled_id),
            "git-traveled letter preserved"
        );

        // A second distribution still appends nothing (dedup holds against the box's own).
        let again = distribute(&spool, &runtime, "repo-a", &known).unwrap();
        assert_eq!(again.appended, 0);
        assert_eq!(read_letters(&box_path_for_repo(&repo_a)).unwrap().len(), 2);
    }

    // --- pending_distribution: an absent repo waits, files when it appears ---

    #[test]
    fn absent_repo_stays_pending_then_files_when_dir_exists() {
        let s = Scratch::new("pending");
        let runtime = s.path("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        // repo-c is NOT created — its dir is absent.
        let known: BTreeMap<String, PathBuf> = BTreeMap::new();

        let spool = s.path("spool.jsonl");
        write_spool(
            &spool,
            &[line(
                serde_json::json!({"ts":"t1","agent":"a","repo":"repo-c","tool":"x","class":"bug","what":"waiting"}),
            )],
        );

        let first = distribute(&spool, &runtime, "repo-a", &known).unwrap();
        assert_eq!(first.to_project, 0);
        assert_eq!(first.to_medulla, 0, "NEVER re-routed to medulla");
        assert_eq!(first.pending, vec!["repo-c".to_string()]);
        // It did NOT leak into the medulla box.
        assert!(read_letters(&medulla_box_path(&runtime))
            .unwrap()
            .is_empty());

        // Now repo-c appears; a re-run files it.
        let repo_c = mk_repo(&s, "repo-c");
        let mut known2 = BTreeMap::new();
        known2.insert("repo-c".to_string(), repo_c.clone());
        let second = distribute(&spool, &runtime, "repo-a", &known2).unwrap();
        assert_eq!(second.to_project, 1);
        assert!(second.pending.is_empty());
        assert_eq!(read_letters(&box_path_for_repo(&repo_c)).unwrap().len(), 1);
    }

    // --- consent-deferred box birth writes an ignore-by-default .gitignore ---

    #[test]
    fn box_birth_writes_consent_deferred_gitignore() {
        let s = Scratch::new("birth");
        let runtime = s.path("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        let repo_a = mk_repo(&s, "repo-a");
        let mut known = BTreeMap::new();
        known.insert("repo-a".to_string(), repo_a.clone());

        let spool = s.path("spool.jsonl");
        write_spool(
            &spool,
            &[line(
                serde_json::json!({"ts":"t1","agent":"a","repo":"repo-a","tool":"x","class":"bug","what":"w"}),
            )],
        );
        distribute(&spool, &runtime, "repo-a", &known).unwrap();

        let gitignore = repo_a.join(".m1nd/.gitignore");
        assert!(gitignore.exists(), "birth writes .gitignore");
        let body = std::fs::read_to_string(&gitignore).unwrap();
        assert!(
            body.contains("inbox.jsonl"),
            "ignore covers inbox.jsonl until m1nd init"
        );

        // An existing .gitignore (repo consented) is NEVER rewritten.
        std::fs::write(&gitignore, "# committed by m1nd init\n").unwrap();
        distribute(&spool, &runtime, "repo-a", &known).unwrap();
        assert_eq!(
            std::fs::read_to_string(&gitignore).unwrap(),
            "# committed by m1nd init\n",
            "consented .gitignore preserved"
        );
    }

    // --- fate derivation on a synthetic thread (§C2.2) ----------------------

    #[test]
    fn fate_derivation_wet_ink_fired_clay_external() {
        // Letter L1 open; L2 open; a receipt R answers L1.
        let l1 = line(
            serde_json::json!({"ts":"t1","agent":"a","repo":"repo-a","tool":"seek","class":"bug","what":"the bug"}),
        );
        let l1_id = letter_id(&l1);
        let l2 = line(
            serde_json::json!({"ts":"t2","agent":"a","repo":"repo-a","tool":"context7","class":"friction","what":"external tool friction"}),
        );
        let receipt = line(
            serde_json::json!({"ts":"t3","agent":"triage","repo":"repo-a","tool":"triage","class":"triage","what":"fixed","answers":[l1_id]}),
        );

        let letters: Vec<Letter> = [l1, l2, receipt]
            .iter()
            .filter_map(|s| parse_letter(s))
            .collect();
        let mut foreign = BTreeSet::new();
        foreign.insert("context7".to_string());
        let viewed = view_letters(&letters, &foreign);

        let f = |id: &str| viewed.iter().find(|l| l.id == id).map(|l| l.state.clone());
        assert_eq!(
            f(&l1_id),
            Some("fired_clay".to_string()),
            "L1 answered → fired_clay"
        );
        let l2_view = viewed.iter().find(|l| l.tool == "context7").unwrap();
        assert_eq!(l2_view.state, "external", "context7 letter → external");
        let receipt_view = viewed.iter().find(|l| l.class == "triage").unwrap();
        assert_eq!(
            receipt_view.state, "wet_ink",
            "unanswered receipt itself → wet_ink"
        );
        // The receipt's answered_by wiring: L1 is answered_by the receipt.
        let l1_view = viewed.iter().find(|l| l.id == l1_id).unwrap();
        assert_eq!(l1_view.answered_by, vec![receipt_view.id.clone()]);
    }

    // --- external excluded from the "abertas" count -------------------------

    #[test]
    fn external_letters_visible_but_never_counted() {
        let s = Scratch::new("external");
        let box_path = s.path("box.jsonl");
        std::fs::write(
            &box_path,
            [
                line(serde_json::json!({"ts":"t1","agent":"a","repo":"repo-a","tool":"seek","class":"bug","what":"open bug"})),
                line(serde_json::json!({"ts":"t2","agent":"a","repo":"repo-a","tool":"context7","class":"friction","what":"external"})),
            ]
            .join("\n")
                + "\n",
        )
        .unwrap();

        let mut foreign = BTreeSet::new();
        foreign.insert("context7".to_string());
        let view = read_box(&box_path, &foreign).unwrap();
        assert_eq!(view.counts.external, 1, "external is visible");
        assert_eq!(view.counts.wet_ink, 1);
        assert_eq!(view.counts.open(), 1, "external NEVER inflates abertas");
    }

    // --- inbox_sweep unions with each letter once (§9.2 GREEN) ---------------

    #[test]
    fn sweep_unions_spool_and_boxes_each_letter_once() {
        let s = Scratch::new("sweep");
        let repo_a = mk_repo(&s, "repo-a");
        std::fs::create_dir_all(repo_a.join(".m1nd")).unwrap();

        // A letter that is BOTH in the spool AND in the box (same id).
        let shared = line(
            serde_json::json!({"ts":"t1","agent":"a","repo":"repo-a","tool":"x","class":"bug","what":"shared"}),
        );
        // A letter only in the box (git-traveled).
        let box_only = line(
            serde_json::json!({"ts":"t2","agent":"o","repo":"repo-a","tool":"x","class":"win","what":"box only"}),
        );
        // A letter only in the spool.
        let spool_only = line(
            serde_json::json!({"ts":"t3","agent":"a","repo":"all","tool":"x","class":"friction","what":"spool only"}),
        );

        let spool = s.path("spool.jsonl");
        write_spool(&spool, &[shared.clone(), spool_only]);
        std::fs::write(
            box_path_for_repo(&repo_a),
            [shared, box_only].join("\n") + "\n",
        )
        .unwrap();

        let boxes = vec![
            KnownBox {
                label: "repo-a".into(),
                path: box_path_for_repo(&repo_a),
                reachable: true,
            },
            KnownBox {
                label: "repo-absent".into(),
                path: s.path("nope/.m1nd/inbox.jsonl"),
                reachable: false,
            },
        ];
        let sweep = inbox_sweep(&spool, &boxes, &BTreeSet::new()).unwrap();
        assert_eq!(sweep.total, 3, "3 distinct letters, shared counted ONCE");
        assert_eq!(
            sweep.unreachable,
            vec!["repo-absent".to_string()],
            "unreachable box named"
        );
    }

    // --- the memory_misdelivery vocabulary (§9.1) ---------------------------

    #[test]
    fn misdelivery_class_and_kinds_are_the_closed_vocabulary() {
        assert_eq!(CLASS_MEMORY_MISDELIVERY, "memory_misdelivery");
        assert_eq!(MISDELIVERY_KINDS.len(), 5);
        for k in [
            "leak",
            "false_absence",
            "wrong_store_write",
            "misattribution",
            "vanished",
        ] {
            assert!(
                MISDELIVERY_KINDS.contains(&k),
                "kind {k} in the closed vocabulary"
            );
        }
    }

    // --- a memory_misdelivery letter is counted by the sweep ----------------

    #[test]
    fn sweep_counts_memory_misdelivery_letters() {
        let s = Scratch::new("misdelivery-count");
        let spool = s.path("spool.jsonl");
        write_spool(
            &spool,
            &[
                // a synthetic `leak` letter (no live precedent — the invariant keeps reads clean).
                line(
                    serde_json::json!({"ts":"t1","agent":"probe","repo":"repo-a","tool":"seek","class":"memory_misdelivery","kind":"leak","what":"brain Y claim surfaced in brain X (synthetic)"}),
                ),
                line(
                    serde_json::json!({"ts":"t2","agent":"a","repo":"repo-a","tool":"north","class":"honesty","what":"unrelated"}),
                ),
            ],
        );
        let sweep = inbox_sweep(&spool, &[], &BTreeSet::new()).unwrap();
        assert_eq!(
            sweep.misdelivery, 1,
            "the memory_misdelivery letter is counted"
        );
    }

    // --- letter id is content-derived + stable (machine-independent) --------

    #[test]
    fn letter_id_is_stable_sha256_prefix() {
        let raw = r#"{"ts":"t","agent":"a","repo":"repo-a","tool":"x","class":"bug","what":"w"}"#;
        let id1 = letter_id(raw);
        let id2 = letter_id(&format!("{raw}\n")); // trailing newline ignored
        assert_eq!(id1, id2, "trailing newline does not change the id");
        assert_eq!(id1.len(), 12, "id is the 12-hex-char sha256 prefix");
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // --- bare repo names resolve against the roster by unique basename --------
    //
    // The first real sweep left most letters pending: a letter whose `repo` is a
    // bare name (or a worktree variant of it) must resolve to the roster brain
    // whose bound_project_root basename matches UNIQUELY — even when that brain is
    // NOT the bound worktree_base. Mirrors covering_brain's unique/abstain law:
    // exactly one match resolves; zero or >1 stays Pending (honest, never a guess).

    #[test]
    fn bare_name_resolves_against_unique_roster_basename() {
        let s = Scratch::new("bare-name");
        let runtime = s.path("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        // A roster brain bound deep under an unrelated path; only its basename
        // ("repo-alpha") matches the bare letter. worktree_base is a DIFFERENT
        // project, so the collapse rule alone cannot save the bare name.
        let repo_alpha = mk_repo(&s, "place").join("repo-alpha");
        std::fs::create_dir_all(&repo_alpha).unwrap();

        let roots = vec![repo_alpha.to_string_lossy().to_string()];
        let (known, _boxes) = boxes_from_roots(&roots, "some-bound-project");

        // Bare name → resolves to the unique roster brain.
        let bare: Letter =
            serde_json::from_str(r#"{"repo":"repo-alpha","tool":"x","class":"bug"}"#).unwrap();
        assert_eq!(
            resolve_box(&bare, &runtime, "some-bound-project", &known),
            BoxTarget::Project(repo_alpha.clone()),
            "a bare name resolves to the roster brain whose basename matches uniquely"
        );

        // Worktree/annotation variant of the same bare name → same resolution.
        let variant: Letter = serde_json::from_str(
            r#"{"repo":"repo-alpha (worktree repo-alpha-x)","tool":"x","class":"bug"}"#,
        )
        .unwrap();
        assert_eq!(
            resolve_box(&variant, &runtime, "some-bound-project", &known),
            BoxTarget::Project(repo_alpha),
            "a parenthetical worktree variant collapses to the same bare name and resolves"
        );
    }

    #[test]
    fn ambiguous_basename_abstains_to_pending_never_guesses() {
        let s = Scratch::new("ambiguous-basename");
        let runtime = s.path("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        // TWO distinct roster roots that share the basename "repo-alpha".
        let a1 = mk_repo(&s, "place1").join("repo-alpha");
        let a2 = mk_repo(&s, "place2").join("repo-alpha");
        std::fs::create_dir_all(&a1).unwrap();
        std::fs::create_dir_all(&a2).unwrap();

        let roots = vec![
            a1.to_string_lossy().to_string(),
            a2.to_string_lossy().to_string(),
        ];
        let (known, boxes) = boxes_from_roots(&roots, "bound");

        // The ambiguous basename must NOT be a resolvable known repo (a guess
        // would silently misroute every "repo-alpha" letter into whichever root
        // won a first-wins race).
        assert!(
            !known.contains_key("repo-alpha"),
            "an ambiguous basename must not resolve to a single root: {known:?}"
        );

        // A bare "repo-alpha" letter therefore stays Pending — honest, not a guess.
        let bare: Letter =
            serde_json::from_str(r#"{"repo":"repo-alpha","tool":"x","class":"bug"}"#).unwrap();
        assert_eq!(
            resolve_box(&bare, &runtime, "bound", &known),
            BoxTarget::Pending("repo-alpha".to_string()),
            "an ambiguous bare name abstains to Pending (0-or->1 → never chute)"
        );

        // Both ambiguous roots are still NAMED as boxes (visible, never silently
        // dropped) — the sweep surfaces them, it just cannot route a bare name.
        assert!(
            boxes.iter().filter(|b| b.label == "repo-alpha").count() >= 1,
            "the ambiguous roots remain visible as named boxes"
        );
    }
}
