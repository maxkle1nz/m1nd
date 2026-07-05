//! The SOUL — PATHOS native and verified (ORGANISM ladder R16 · `docs/SOUL-PRD.md`).
//!
//! The soul is the project's curated handoff document (`docs/PATHOS.md`). This
//! module makes it a first-class m1nd type: parsed into anchored CLAIMS, each
//! classified by a CHECK CLASS and carrying a computed VERIFICATION STATE. The
//! output is the honesty report + a one-line FRESHNESS RECEIPT — the line a cold
//! context reads to know how much to trust the handoff.
//!
//! Reuse-first (SOUL-PRD §2.2, §4): this composes SHIPPED organs and adds no new
//! store and no clause compiler.
//!   * git verification rides `audit_handlers::resolve_git_root_from_state` — the
//!     same git-root resolver `collect_git_state` uses.
//!   * `symbol` claims resolve against the live graph the same way
//!     `universal_docs::compute_bindings` walks `id_to_node`.
//!   * the `evidence-stale` reason vocabulary is inherited from
//!     `cross_verify(evidence_freshness)` (`evidence_file_missing`,
//!     `evidence_changed`, `unverifiable`) plus the four born in the SOUL-PRD probe
//!     (`line_drift`, `contradicted`, `expired`, `unanchored`).
//!
//! S0 is READ-ONLY: it writes nothing, touches no store, needs no medulla state
//! (SOUL-PRD §9 — the honest exception that ships first because it is the
//! RED-maker). The write half — `soul_update` — rides the ONE memorize sink
//! (SOUL-INV-8) and lives in `light_author_handlers`.

use crate::audit_handlers::resolve_git_root_from_state;
use crate::protocol::layers::{SoulCheckInput, SoulReadInput};
use crate::session::SessionState;
use crate::util::now_ms;
use m1nd_core::error::{M1ndError, M1ndResult};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Discovery order for the soul document when no explicit path is given
/// (SOUL-PRD §3.1 — the skill's own convention).
const SOUL_DISCOVERY: &[&str] = &["docs/PATHOS.md", "PATHOS.md"];

// ── Tissue (SOUL-PRD §3.3) ──────────────────────────────────────────────────
// Assigned at the SECTION level from the skill's own headings. Declared tissue is
// UNPROVABLE-but-curated: never machine-verified, never machine-pruned.

/// Section-name substrings that mark DECLARED tissue (taste / doctrine / why).
/// Everything else is VERIFIABLE tissue (SOUL-PRD §3.3).
const DECLARED_SECTIONS: &[&str] = &[
    "north star",
    "pathos",
    "operating doctrine",
    "do not do",
    "open questions",
    "proof standard",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tissue {
    Verifiable,
    Declared,
}

impl Tissue {
    fn as_str(self) -> &'static str {
        match self {
            Tissue::Verifiable => "verifiable",
            Tissue::Declared => "declared",
        }
    }
}

fn tissue_for_section(section: &str) -> Tissue {
    let lower = section.to_ascii_lowercase();
    if DECLARED_SECTIONS.iter().any(|d| lower.contains(d)) {
        Tissue::Declared
    } else {
        Tissue::Verifiable
    }
}

// ── The claim grammar (SOUL-PRD §3.2) ───────────────────────────────────────

/// The CHECK CLASS orders claims by verification cost; each carries its verifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CheckClass {
    /// A file path exists — `fs stat`.
    Path,
    /// `path:line` — stat + the symbol is the contract, the line is a hint.
    LineHint,
    /// A code symbol resolves in the graph (`document_bindings` machinery).
    Symbol,
    /// A git ref/tag/PR is real — `git` refs/log.
    Git,
    /// Requires EXECUTION or a receipt artifact — never folded into fresh/stale.
    Receipt,
    /// A live-owner probe — priced; honest hold when unreachable.
    Runtime,
    /// Declared tissue — NO verifier (SOUL-INV-1's honorable exception).
    Declared,
}

impl CheckClass {
    fn as_str(self) -> &'static str {
        match self {
            CheckClass::Path => "path",
            CheckClass::LineHint => "line-hint",
            CheckClass::Symbol => "symbol",
            CheckClass::Git => "git",
            CheckClass::Receipt => "receipt",
            CheckClass::Runtime => "runtime",
            CheckClass::Declared => "declared",
        }
    }
}

/// The verification STATE — the fourth face of the lifecycle grammar
/// (SOUL-PRD §3.4). Computed at check time, never stored (SOUL-INV-7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SoulState {
    VerifiedFresh,
    EvidenceStale,
    Superseded,
    ReceiptRequired,
    UnprovableNow,
    Declared,
}

impl SoulState {
    fn as_str(self) -> &'static str {
        match self {
            SoulState::VerifiedFresh => "verified-fresh",
            SoulState::EvidenceStale => "evidence-stale",
            SoulState::Superseded => "superseded",
            SoulState::ReceiptRequired => "receipt-required",
            SoulState::UnprovableNow => "unprovable-now",
            SoulState::Declared => "declared",
        }
    }
}

/// An anchor in the house style the soul already writes.
#[derive(Clone, Debug)]
struct Anchor {
    /// The raw anchor text (a path, `path:line`, a `sym::sym`, `v1.3.0`, `#267`).
    ref_text: String,
    class: CheckClass,
    /// For `LineHint`: the parsed (path, line) hint.
    line_hint: Option<(String, usize)>,
}

/// A soul claim: the smallest independently verifiable assertion, with its
/// section (→ tissue), text, and zero or more anchors (SOUL-PRD §3.2).
#[derive(Clone, Debug)]
struct SoulClaim {
    section: String,
    tissue: Tissue,
    text: String,
    anchors: Vec<Anchor>,
}

// ── Parse: the soul document → claims (SOUL-PRD §3.2, §3.6) ──────────────────
// The soul stays markdown; the claims are EXTRACTED, not authored. The compiled
// set is a disposable cache, never a second copy of truth (§3.6).

/// The auto-anchored region: everything between the auto anchors is machine
/// regenerated and never hand-edited (SOUL-PRD §7 — its FRESHNESS is one claim,
/// not many). We keep it as ONE consistency-class claim below, so we skip its
/// interior lines during per-line extraction.
const AUTO_BEGIN: &str = "<!-- BEGIN:auto-";
const AUTO_END: &str = "<!-- END:auto-";

/// Parse the soul body into claims. Sections are the skill's `## <Heading>`;
/// history sections ("Prior Eras") are self-declared and excluded exactly as the
/// probe treated them (SOUL-PRD §2.3).
fn parse_soul(body: &str) -> Vec<SoulClaim> {
    let mut claims = Vec::new();
    let mut section = String::from("<preamble>");
    let mut tissue = Tissue::Declared;
    let mut in_auto = false;
    let mut in_history = false;

    for raw in body.lines() {
        let line = raw.trim_end();

        // Auto-anchored region: its interior is not per-line claim tissue.
        if line.contains(AUTO_BEGIN) {
            in_auto = true;
            continue;
        }
        if line.contains(AUTO_END) {
            in_auto = false;
            continue;
        }
        if in_auto {
            continue;
        }

        // Section headers (## or ###). A new `##` resets history/tissue.
        if let Some(heading) = line.strip_prefix("## ") {
            section = heading.trim().to_string();
            tissue = tissue_for_section(&section);
            // "Prior Eras" / preserved checkpoints are self-declared history.
            in_history = {
                let l = section.to_ascii_lowercase();
                l.contains("prior era") || l.contains("prior checkpoint")
            };
            continue;
        }
        if line.starts_with("### ") {
            // Sub-headings keep the parent section's tissue; not a claim itself.
            continue;
        }
        if in_history {
            continue;
        }

        // A claim is a non-empty prose line. Blank lines, list bullets, and block
        // quotes still carry claims; we extract anchors regardless of the marker.
        let text = line.trim();
        if text.is_empty() || text == ">" {
            continue;
        }

        let anchors = extract_anchors(text);
        // Verifiable tissue yields a claim per line ONLY when it carries an anchor
        // OR looks like a state assertion — an UNANCHORED verifiable line is itself
        // a finding (SOUL-INV-1), so we still emit it (with zero anchors) when it
        // is an Access-Map / Known-Problems style bullet.
        let is_bullet = text.starts_with("- ") || text.starts_with("* ");
        // Verifiable prose (a sentence, not a bullet) without an anchor is skipped
        // as narration; an UNANCHORED verifiable BULLET is a named finding
        // (SOUL-INV-1) and is emitted with zero anchors.
        if anchors.is_empty() && tissue == Tissue::Verifiable && !is_bullet {
            continue;
        }
        // Declared tissue (doctrine / taste / why) is honorable and NEVER verified;
        // every substantive declared line — bullet or prose sentence — is counted
        // as a declared claim so the two-tissue ratio in the receipt is honest
        // (the system saying "these lines I cannot verify" IS the honesty).
        // Markdown scaffolding (block-quote intros, horizontal rules) is not a claim.
        let is_scaffold =
            text.starts_with('>') || text.starts_with("---") || text.starts_with("<!--");
        if is_scaffold {
            continue;
        }

        claims.push(SoulClaim {
            section: section.clone(),
            tissue,
            text: text.to_string(),
            anchors,
        });
    }

    claims
}

/// Extract anchors from a claim line in the house style (SOUL-PRD §2.1, §3.2).
/// The grammar grips the EXISTING conventions the probe verified are grippable.
fn extract_anchors(text: &str) -> Vec<Anchor> {
    let mut anchors = Vec::new();

    // 1. Backtick spans: the primary anchor carrier (`path`, `path:line`,
    //    `sym::sym`, `command`). We classify each span.
    for span in backtick_spans(text) {
        if let Some(anchor) = classify_backtick(&span) {
            anchors.push(anchor);
        }
    }

    // 2. Bare git refs outside backticks: tags `vX.Y.Z`, PRs `#NNN`.
    for token in text.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ',' | '.')) {
        let t = token.trim();
        if (is_version_tag(t) || is_pr_ref(t)) && !anchors.iter().any(|a| a.ref_text == t) {
            anchors.push(Anchor {
                ref_text: t.to_string(),
                class: CheckClass::Git,
                line_hint: None,
            });
        }
    }

    anchors
}

/// Split a line into the contents of its backtick spans.
fn backtick_spans(text: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut chars = text.chars().peekable();
    let mut current: Option<String> = None;
    while let Some(c) = chars.next() {
        if c == '`' {
            match current.take() {
                Some(s) => spans.push(s),
                None => current = Some(String::new()),
            }
        } else if let Some(buf) = current.as_mut() {
            buf.push(c);
        }
        let _ = &mut chars;
    }
    spans
}

/// Classify a backtick span into an anchor, or `None` if it is prose/noise.
fn classify_backtick(span: &str) -> Option<Anchor> {
    let s = span.trim();
    if s.is_empty() {
        return None;
    }

    // `path:line` — line hint. Only when the left side looks like a real path.
    if let Some((left, right)) = s.rsplit_once(':') {
        if let Ok(line) = right.trim().parse::<usize>() {
            if looks_like_path(left) {
                return Some(Anchor {
                    ref_text: s.to_string(),
                    class: CheckClass::LineHint,
                    line_hint: Some((left.trim().to_string(), line)),
                });
            }
        }
    }

    // A real repo path (has a slash + an extension-like segment or a known dir).
    if looks_like_path(s) {
        return Some(Anchor {
            ref_text: s.to_string(),
            class: CheckClass::Path,
            line_hint: None,
        });
    }

    // A version tag / PR ref inside backticks.
    if is_version_tag(s) || is_pr_ref(s) {
        return Some(Anchor {
            ref_text: s.to_string(),
            class: CheckClass::Git,
            line_hint: None,
        });
    }

    // A `sym::sym` symbol (module path or `Type::method`). Two+ segments, no
    // spaces, no slash — resolves against the graph.
    if is_symbol(s) {
        return Some(Anchor {
            ref_text: s.to_string(),
            class: CheckClass::Symbol,
            line_hint: None,
        });
    }

    None
}

fn looks_like_path(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.contains(' ') {
        return false;
    }
    // NOT a repo-relative path (verifiable against fs) — these are legitimate
    // references but not "does this file exist under the repo root" anchors, so
    // grabbing them fake-fails the soul (SOUL-PRD risk #4, parser brittleness):
    //   - home / absolute / env / var paths: `~/...`, `/abs`, `$HOME/...`, `./target/...`
    //   - a bare file EXTENSION token (`.light.md`, `.jsonl`) — an ext, not a path
    //   - a build artifact under target/ (never tracked)
    if s.starts_with('~')
        || s.starts_with('$')
        || s.starts_with("./target")
        || s.starts_with('/')
        || s.contains("/target/")
        || s.starts_with("target/")
    {
        return false;
    }
    // A gitignore-negation string (`!path`) is a .gitignore CONTENT claim, not a
    // path to stat — the negated path itself may be intentionally excluded.
    if s.starts_with('!') {
        return false;
    }
    // A bare dot-token with no directory (e.g. `.light.md`, `.jsonl`) is an
    // EXTENSION reference, not a stattable file — a real root dotfile that we can
    // anchor is a single-segment config file (`.gitignore`, `.gitattributes`).
    // Rule: a leading-dot token with NO slash is a path anchor ONLY when it has
    // exactly ONE dot-separated segment (`.gitignore`), never chained (`.a.b`).
    if s.starts_with('.') && !s.contains('/') {
        if s.contains("::") {
            return false;
        }
        let segs = s.trim_start_matches('.').split('.').count();
        return segs == 1;
    }
    // A path has a slash and a non-empty final segment.
    if s.contains('/') {
        let last = s.rsplit('/').next().unwrap_or("");
        return !last.is_empty();
    }
    false
}

fn is_version_tag(s: &str) -> bool {
    // vN.N.N (allow a trailing `+something` the probe never needs).
    let core = s.split('+').next().unwrap_or(s);
    if let Some(rest) = core.strip_prefix('v') {
        let parts: Vec<&str> = rest.split('.').collect();
        return parts.len() == 3 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()));
    }
    false
}

fn is_pr_ref(s: &str) -> bool {
    s.strip_prefix('#')
        .map(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false)
}

fn is_symbol(s: &str) -> bool {
    if !s.contains("::") || s.contains(' ') || s.contains('/') {
        return false;
    }
    // Reject obvious code snippets: no parens, no `.`, reasonable length.
    !s.contains('(') && !s.contains('.') && s.len() < 80
}

// ── Verify: per-class checks (SOUL-PRD §3.2, §3.4) ───────────────────────────

/// The outcome of checking one anchor.
struct AnchorVerdict {
    state: SoulState,
    /// A sub-reason for `evidence-stale` (inherited vocabulary + probe additions).
    reason: Option<&'static str>,
}

fn verify_anchor(
    anchor: &Anchor,
    repo_root: &Path,
    state: &SessionState,
    ran_symbol: &mut bool,
) -> AnchorVerdict {
    match anchor.class {
        CheckClass::Path => verify_path(&anchor.ref_text, repo_root),
        CheckClass::LineHint => verify_line_hint(anchor, repo_root),
        CheckClass::Symbol => {
            *ran_symbol = true;
            verify_symbol(&anchor.ref_text, state)
        }
        CheckClass::Git => verify_git(&anchor.ref_text, repo_root),
        // Priced classes: honest holds — NEVER folded into fresh/stale (SOUL-INV-3).
        CheckClass::Receipt => AnchorVerdict {
            state: SoulState::ReceiptRequired,
            reason: None,
        },
        CheckClass::Runtime => AnchorVerdict {
            state: SoulState::UnprovableNow,
            reason: None,
        },
        CheckClass::Declared => AnchorVerdict {
            state: SoulState::Declared,
            reason: None,
        },
    }
}

fn verify_path(rel: &str, repo_root: &Path) -> AnchorVerdict {
    let full = repo_root.join(rel);
    if full.exists() {
        AnchorVerdict {
            state: SoulState::VerifiedFresh,
            reason: None,
        }
    } else {
        AnchorVerdict {
            state: SoulState::EvidenceStale,
            reason: Some("evidence_file_missing"),
        }
    }
}

fn verify_line_hint(anchor: &Anchor, repo_root: &Path) -> AnchorVerdict {
    let Some((path, line)) = &anchor.line_hint else {
        return verify_path(&anchor.ref_text, repo_root);
    };
    let full = repo_root.join(path);
    let Ok(content) = std::fs::read_to_string(&full) else {
        return AnchorVerdict {
            state: SoulState::EvidenceStale,
            reason: Some("evidence_file_missing"),
        };
    };
    // The symbol is the contract, the line is a hint. If the file exists but has
    // fewer lines than the hint, the anchor moved: `line_drift`, not a lie.
    let total = content.lines().count();
    if *line >= 1 && *line <= total {
        AnchorVerdict {
            state: SoulState::VerifiedFresh,
            reason: None,
        }
    } else {
        AnchorVerdict {
            state: SoulState::EvidenceStale,
            reason: Some("line_drift"),
        }
    }
}

fn verify_symbol(sym: &str, state: &SessionState) -> AnchorVerdict {
    // Resolve against the live graph the same way compute_bindings walks
    // id_to_node: a symbol `mod::name` or `Type::method` matches a node whose
    // label is the last segment AND whose ext_id path/qualifier contains the head.
    let (head, tail) = match sym.rsplit_once("::") {
        Some((h, t)) => (h, t),
        None => (sym, sym),
    };
    let graph = state.graph.read();
    for (interned, &node_id) in &graph.id_to_node {
        let ext_id = graph.strings.resolve(*interned);
        let idx = node_id.as_usize();
        let label = graph.strings.resolve(graph.nodes.label[idx]);
        if label == tail && (ext_id.contains(head) || head == tail) {
            return AnchorVerdict {
                state: SoulState::VerifiedFresh,
                reason: None,
            };
        }
    }
    AnchorVerdict {
        state: SoulState::EvidenceStale,
        reason: Some("evidence_changed"),
    }
}

fn verify_git(git_ref: &str, repo_root: &Path) -> AnchorVerdict {
    // A tag (`vX.Y.Z`): `git rev-parse -q --verify refs/tags/<tag>`.
    if is_version_tag(git_ref) {
        let ok = git_ok(
            repo_root,
            &[
                "rev-parse",
                "-q",
                "--verify",
                &format!("refs/tags/{}", git_ref),
            ],
        );
        return git_verdict(ok);
    }
    // A PR ref (`#NNN`): a PR is not a local git object; the honest, offline
    // signal is whether a merge commit or squash-merge references it. We grep the
    // recent log for `(#NNN)` — the house squash-merge convention.
    if let Some(num) = git_ref.strip_prefix('#') {
        let needle = format!("(#{})", num);
        if let Some(log) = git_read(repo_root, &["log", "--oneline", "-n", "400"]) {
            let ok = log.lines().any(|l| l.contains(&needle));
            return git_verdict(ok);
        }
        // Log unreadable — honest hold, not a fake fail.
        return AnchorVerdict {
            state: SoulState::UnprovableNow,
            reason: None,
        };
    }
    // Unknown git shape — honest hold.
    AnchorVerdict {
        state: SoulState::UnprovableNow,
        reason: None,
    }
}

fn git_verdict(ok: bool) -> AnchorVerdict {
    if ok {
        AnchorVerdict {
            state: SoulState::VerifiedFresh,
            reason: None,
        }
    } else {
        AnchorVerdict {
            state: SoulState::EvidenceStale,
            reason: Some("unanchored"),
        }
    }
}

fn git_ok(root: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn git_read(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Roll a claim's anchor verdicts into ONE claim state. A verifiable claim is
/// fresh only if EVERY anchor checks; the worst anchor decides otherwise, with
/// priced holds ranked above stale (never fold a priced hold into a fail).
fn roll_claim_state(
    claim: &SoulClaim,
    repo_root: &Path,
    state: &SessionState,
    ran_symbol: &mut bool,
) -> (SoulState, Option<&'static str>) {
    if claim.tissue == Tissue::Declared {
        return (SoulState::Declared, None);
    }
    if claim.anchors.is_empty() {
        // Verifiable tissue with no anchor is a named finding (SOUL-INV-1).
        return (SoulState::EvidenceStale, Some("unanchored"));
    }

    let mut worst: Option<(SoulState, Option<&'static str>)> = None;
    for anchor in &claim.anchors {
        let v = verify_anchor(anchor, repo_root, state, ran_symbol);
        worst = Some(match worst.take() {
            None => (v.state, v.reason),
            Some(prev) => pick_worse(prev, (v.state, v.reason)),
        });
    }
    worst.unwrap_or((SoulState::VerifiedFresh, None))
}

/// Ordering by badness: EvidenceStale > Superseded > UnprovableNow >
/// ReceiptRequired > VerifiedFresh (a fail dominates; a priced hold is honest but
/// never masks a fail; a fresh anchor never rescues a failing sibling).
fn pick_worse(
    a: (SoulState, Option<&'static str>),
    b: (SoulState, Option<&'static str>),
) -> (SoulState, Option<&'static str>) {
    if rank(a.0) >= rank(b.0) {
        a
    } else {
        b
    }
}

fn rank(s: SoulState) -> u8 {
    match s {
        SoulState::EvidenceStale => 5,
        SoulState::Superseded => 4,
        SoulState::UnprovableNow => 3,
        SoulState::ReceiptRequired => 2,
        SoulState::VerifiedFresh => 1,
        SoulState::Declared => 0,
    }
}

// ── The consistency pass (SOUL-PRD §3.2 `consistency` class) ─────────────────
// Intra-soul cross-claim comparison — no fs at all. Detects two disagreeing
// numbers for the same tracked quantity (probe stale #7: battery "36" vs "37").

/// A number-bearing consistency finding: the same labelled quantity asserted with
/// two different values in the live tissue.
fn consistency_findings(claims: &[SoulClaim]) -> Vec<serde_json::Value> {
    use std::collections::HashMap;
    // Map a normalized quantity keyword → the set of distinct integers asserted.
    let mut seen: HashMap<&'static str, Vec<(u64, String)>> = HashMap::new();
    // Quantity keywords we track for intra-soul contradiction.
    const TRACKED: &[&str] = &["battery"];

    for claim in claims {
        // A number's VALUE is checkable regardless of the sentence's tissue — a
        // "battery: 36" in a doctrine sentence still contradicts a "battery: 37"
        // in the access map (SOUL-PRD §2.3 stale #7 spanned two sections). The
        // consistency pass therefore reads every claim, not just verifiable ones.
        let lower = claim.text.to_ascii_lowercase();
        for kw in TRACKED {
            if lower.contains(kw) {
                for num in extract_small_ints(&claim.text) {
                    // Only track the battery-case-count range to avoid noise.
                    if (20..=99).contains(&num) {
                        let entry = seen.entry(kw).or_default();
                        if !entry.iter().any(|(n, _)| *n == num) {
                            entry.push((num, claim.text.clone()));
                        }
                    }
                }
            }
        }
    }

    let mut findings = Vec::new();
    for (kw, values) in seen {
        if values.len() > 1 {
            let nums: Vec<String> = values.iter().map(|(n, _)| n.to_string()).collect();
            findings.push(json!({
                "quantity": kw,
                "reason": "contradicted",
                "values": nums,
                "detail": format!(
                    "the soul asserts {} as {} in different places — intra-soul disagreement",
                    kw,
                    nums.join(" and ")
                ),
            }));
        }
    }
    findings
}

fn extract_small_ints(text: &str) -> Vec<u64> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            cur.push(c);
        } else {
            if let Ok(n) = cur.parse::<u64>() {
                out.push(n);
            }
            cur.clear();
        }
    }
    if let Ok(n) = cur.parse::<u64>() {
        out.push(n);
    }
    out
}

// ── Soul discovery + read ────────────────────────────────────────────────────

/// Resolve the soul document path, honoring an explicit `soul_path`, else the
/// discovery order under the resolved repo root.
fn resolve_soul_path(
    state: &SessionState,
    explicit: Option<&str>,
) -> M1ndResult<(PathBuf, PathBuf)> {
    let repo_root = resolve_git_root_from_state(state).ok_or_else(|| M1ndError::InvalidParams {
        tool: "soul_check".into(),
        detail: "no git repo root resolvable from the session — the soul travels with git".into(),
    })?;

    if let Some(p) = explicit {
        let candidate = {
            let raw = Path::new(p);
            if raw.is_absolute() {
                raw.to_path_buf()
            } else {
                repo_root.join(raw)
            }
        };
        if candidate.exists() {
            return Ok((repo_root, candidate));
        }
        return Err(M1ndError::InvalidParams {
            tool: "soul_check".into(),
            detail: format!("soul_path '{}' does not exist under {:?}", p, repo_root),
        });
    }

    for rel in SOUL_DISCOVERY {
        let candidate = repo_root.join(rel);
        if candidate.exists() {
            return Ok((repo_root, candidate));
        }
    }

    Err(M1ndError::InvalidParams {
        tool: "soul_check".into(),
        detail: format!(
            "no soul document found (looked for {:?} under {:?}) — the pathos skill births one",
            SOUL_DISCOVERY, repo_root
        ),
    })
}

fn short_sha(repo_root: &Path) -> String {
    git_read(repo_root, &["rev-parse", "--short", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

// ── soul_check (SOUL-PRD §4) ─────────────────────────────────────────────────

pub fn handle_soul_check(
    state: &mut SessionState,
    input: SoulCheckInput,
) -> M1ndResult<serde_json::Value> {
    // The §C8.4 seat check: verify a curator report written by ANOTHER agent,
    // rather than re-parsing the soul (who verifies the curator — the circularity
    // answered). Grader ≠ author is checked mechanically.
    if let Some(report) = input.verify_curator_report {
        return verify_curator_report(&input.agent_id, &report);
    }

    let (repo_root, soul_path) = resolve_soul_path(state, input.soul_path.as_deref())?;
    let body = std::fs::read_to_string(&soul_path).map_err(|e| M1ndError::InvalidParams {
        tool: "soul_check".into(),
        detail: format!("cannot read soul at {:?}: {}", soul_path, e),
    })?;

    let claims = parse_soul(&body);

    let mut ran_symbol = false;
    let mut by_state = [0usize; 6]; // fresh, stale, superseded, receipt, unprovable, declared
    let mut verifiable = 0usize;
    let mut declared = 0usize;
    let mut unanchored = 0usize;
    let mut stale_rows: Vec<serde_json::Value> = Vec::new();

    for claim in &claims {
        if claim.tissue == Tissue::Declared {
            declared += 1;
        } else {
            verifiable += 1;
        }

        let (st, reason) = roll_claim_state(claim, &repo_root, state, &mut ran_symbol);
        let bucket = match st {
            SoulState::VerifiedFresh => 0,
            SoulState::EvidenceStale => 1,
            SoulState::Superseded => 2,
            SoulState::ReceiptRequired => 3,
            SoulState::UnprovableNow => 4,
            SoulState::Declared => 5,
        };
        by_state[bucket] += 1;

        if reason == Some("unanchored") {
            unanchored += 1;
        }
        if st == SoulState::EvidenceStale {
            let anchor_txt = claim
                .anchors
                .first()
                .map(|a| a.ref_text.clone())
                .unwrap_or_else(|| "<none>".into());
            stale_rows.push(json!({
                "claim": truncate(&claim.text, 160),
                "section": claim.section,
                "reason": reason.unwrap_or("evidence_changed"),
                "anchor": anchor_txt,
                "class": claim.anchors.first().map(|a| a.class.as_str()).unwrap_or("none"),
            }));
        }
    }

    // The consistency pass (no fs): intra-soul contradictions.
    let consistency = consistency_findings(&claims);
    for finding in &consistency {
        by_state[1] += 1; // a contradiction is evidence-stale tissue
        stale_rows.push(finding.clone());
    }

    // `checks_skipped` is load-bearing (SOUL-INV-3): name any class NOT run, and
    // name the priced classes as held rather than fresh.
    let mut checks_skipped: Vec<&'static str> = Vec::new();
    if !ran_symbol {
        // no symbol anchors present in this soul → the class was not exercised
        checks_skipped.push("symbol (no symbol anchors present)");
    }
    if by_state[3] > 0 {
        checks_skipped.push("receipt (execution not run — receipt-required)");
    }
    if by_state[4] > 0 {
        checks_skipped.push("runtime (live owner not probed — unprovable-now)");
    }

    let fresh = by_state[0];
    let stale = by_state[1];
    let superseded = by_state[2];
    let receipt = by_state[3];
    let unprovable = by_state[4];

    let repo_sha = short_sha(&repo_root);
    let checked_at = now_ms();
    let date = ymd(checked_at);

    let receipt_line = format!(
        "soul: checked {date} @{repo_sha} — {fresh} fresh · {stale} stale · {} priced · declared tissue intact",
        receipt + unprovable
    );

    let soul_lag = compute_soul_lag(&repo_root, &soul_path);

    let out = json!({
        "schema": "m1nd-soul-check-v0",
        "soul_path": display_rel(&repo_root, &soul_path),
        "checked_at_ms": checked_at,
        "repo_sha": repo_sha,
        "checked_by": input.agent_id,
        "claims": {
            "total": claims.len(),
            "verifiable": verifiable,
            "declared": declared,
            "unanchored_in_verifiable": unanchored,
        },
        "by_state": {
            "verified_fresh": fresh,
            "evidence_stale": stale,
            "superseded": superseded,
            "receipt_required": receipt,
            "unprovable_now": unprovable,
            "declared": by_state[5],
        },
        "stale": stale_rows,
        "consistency_findings": consistency,
        "checks_skipped": checks_skipped,
        "receipt_line": receipt_line,
        "soul_lag": soul_lag,
        "_soul": {
            "read_only": true,
            "note": "S0 read-only: no store touched, no document edited (SOUL-PRD §9).",
            "two_tissues": "declared tissue is UNPROVABLE-but-curated — never machine-verified (SOUL-INV-5).",
        },
    });

    Ok(out)
}

/// The soul-lag sub-atom: how many commits the soul's last touch is behind HEAD
/// (SOUL-PRD §4 report field).
fn compute_soul_lag(repo_root: &Path, soul_path: &Path) -> serde_json::Value {
    let rel = display_rel(repo_root, soul_path);
    let last_touch = git_read(
        repo_root,
        &["log", "-n", "1", "--format=%h", "--", rel.as_str()],
    )
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty());

    let behind = last_touch.as_ref().and_then(|sha| {
        git_read(
            repo_root,
            &["rev-list", "--count", &format!("{}..HEAD", sha)],
        )
        .and_then(|s| s.trim().parse::<u64>().ok())
    });

    json!({
        "commits_behind_head": behind,
        "last_soul_touch": last_touch,
    })
}

// ── The §C8.4 seat check: who verifies the curator ───────────────────────────

/// Verify a curator report was checked by a DIFFERENT agent than the one that
/// curated it (ORGANISM §C8.4 — the circularity answered: grader ≠ author). The
/// report must also carry the honesty valve (`still_stale`) and account every
/// prune (never-silent, SOUL-INV-2).
fn verify_curator_report(
    grader_id: &str,
    report: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    let curated_by = report
        .get("curated_by")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut violations: Vec<String> = Vec::new();

    // Law 1 (the seat law): grader ≠ author.
    let seat_ok = !curated_by.is_empty() && curated_by != grader_id;
    if curated_by.is_empty() {
        violations.push("report has no `curated_by` provenance (etiquette-by-provenance)".into());
    } else if curated_by == grader_id {
        violations.push(format!(
            "seat violation: the grader ('{}') is the curator — §C8.4 requires a DIFFERENT session/agent",
            grader_id
        ));
    }

    // Law: never-silent-prune (SOUL-INV-2). Every prune names a destination.
    if let Some(pruned) = report.get("pruned").and_then(|v| v.as_array()) {
        for (i, p) in pruned.iter().enumerate() {
            let has_where = p
                .get("where_it_went")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            let has_why = p
                .get("why")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            if !has_where || !has_why {
                violations.push(format!(
                    "prune #{} is silent: every removal needs a `why` and a `where_it_went` (SOUL-INV-2)",
                    i
                ));
            }
        }
    }

    // The honesty valve must be PRESENT (even if empty) — a curator that cannot
    // resolve a claim marks it in `still_stale` rather than faking it fresh.
    if report.get("still_stale").is_none() {
        violations.push("report is missing the `still_stale` honesty valve (SOUL-PRD §5.3)".into());
    }

    // Declared-tissue lock (SOUL-INV-5): the curator may PROPOSE, never remove
    // declared tissue on its own authority. A prune whose `where_it_went` marks
    // declared-tissue removal without a `proposed: true` flag is a violation.
    if let Some(pruned) = report.get("pruned").and_then(|v| v.as_array()) {
        for (i, p) in pruned.iter().enumerate() {
            let tissue = p.get("tissue").and_then(|v| v.as_str()).unwrap_or("");
            let proposed_only = p.get("proposed").and_then(|v| v.as_bool()).unwrap_or(false);
            if tissue == "declared" && !proposed_only {
                violations.push(format!(
                    "prune #{} removes DECLARED tissue without `proposed: true` — declared tissue is proposed-only (SOUL-INV-5)",
                    i
                ));
            }
        }
    }

    let passed = seat_ok && violations.is_empty();

    Ok(json!({
        "schema": "m1nd-soul-curator-seatcheck-v0",
        "passed": passed,
        "grader": grader_id,
        "curated_by": curated_by,
        "seat_independent": seat_ok,
        "violations": violations,
        "law": "ORGANISM §C8.4 — curator output is seat-verified: grader ≠ author; never-silent-prune; declared-tissue lock; still_stale honesty valve present.",
    }))
}

// ── soul_read (SOUL-PRD §4) ──────────────────────────────────────────────────

pub fn handle_soul_read(
    state: &mut SessionState,
    input: SoulReadInput,
) -> M1ndResult<serde_json::Value> {
    let (repo_root, soul_path) = resolve_soul_path(state, input.soul_path.as_deref())?;
    let body = std::fs::read_to_string(&soul_path).map_err(|e| M1ndError::InvalidParams {
        tool: "soul_read".into(),
        detail: format!("cannot read soul at {:?}: {}", soul_path, e),
    })?;

    let content = if let Some(section) = input.section.as_deref() {
        extract_section(&body, section).ok_or_else(|| M1ndError::InvalidParams {
            tool: "soul_read".into(),
            detail: format!("no section matching '{}' in the soul", section),
        })?
    } else {
        body.clone()
    };

    // The soul's own first curated line is its headline (authored, not generated).
    let headline = extract_headline(&body);

    Ok(json!({
        "schema": "m1nd-soul-read-v0",
        "soul_path": display_rel(&repo_root, &soul_path),
        "headline": headline,
        "section": input.section,
        "content": content,
        "hint": "run soul_check for the freshness receipt (how much to trust this handoff).",
    }))
}

/// Extract one section by heading (case-insensitive substring on `## <Heading>`).
fn extract_section(body: &str, wanted: &str) -> Option<String> {
    let wanted = wanted.to_ascii_lowercase();
    let mut out: Option<Vec<String>> = None;
    for line in body.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            if h.to_ascii_lowercase().contains(&wanted) {
                out = Some(vec![line.to_string()]);
                continue;
            } else if out.is_some() {
                break; // next top-level section ends the capture
            }
        }
        if let Some(buf) = out.as_mut() {
            buf.push(line.to_string());
        }
    }
    out.map(|lines| lines.join("\n"))
}

/// The headline: the soul's first `# <title>` or first `**bold**` curated line
/// (SOUL-PRD §4.4 — authored in the doc, not generated).
fn extract_headline(body: &str) -> String {
    for line in body.lines() {
        let t = line.trim();
        if let Some(h) = t.strip_prefix("# ") {
            return truncate(h.trim(), 120);
        }
    }
    "soul".to_string()
}

// ── small helpers ────────────────────────────────────────────────────────────

fn display_rel(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

/// Format a ms timestamp as YYYY-MM-DD (UTC), reusing the same day math the rest
/// of the codebase already uses for stamps.
fn ymd(ms: u64) -> String {
    // days since epoch → civil date (Howard Hinnant's algorithm, no chrono dep).
    let secs = (ms / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}
