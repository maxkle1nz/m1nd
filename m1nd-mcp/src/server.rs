// === crates/m1nd-mcp/src/server.rs ===

use crate::auto_ingest;
use crate::brain_runtime::BrainSessionCell;
use crate::help_guidance;
use crate::layer_handlers;
use crate::mission_handlers;
use crate::personality;
use crate::protocol::layers;
use crate::protocol::*;
use crate::report_handlers;
use crate::runtime_jobs::RuntimeJobFailure;
use crate::search_handlers;
use crate::session::SessionState;
use crate::surgical_handlers;
use crate::tools;
use crate::universal_docs;
use crate::util::now_ms;
use m1nd_core::domain::DomainConfig;
use m1nd_core::error::{M1ndError, M1ndResult};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// ---------------------------------------------------------------------------
// MCP protocol instructions — injected into initialize response so agents
// automatically understand how to use m1nd effectively.
// ---------------------------------------------------------------------------
const M1ND_INSTRUCTIONS: &str = "\
m1nd is a neuro-symbolic code graph over this repo. It answers with calibrated \
trust, not vibes: absent / null / abstain / insufficient_evidence are REAL answers — \
prefer them over guessing. Every tool call needs an `agent_id`. Knowledge is shared: \
what one agent proves and memorizes, the next agent reads. Operate in a loop.

## 0. FIRST CONTACT — heed reception

A response may carry a `reception` block. `reception.match == \"caller_root_mismatch\"` \
means the bound graph does NOT cover your current repo (its root ≠ your resolved \
`caller_root`) — do NOT trust retrieval for THIS repo; read `reception.options[]`. \
Generic cross-root `ingest` remains withdrawn (no `project_root` in the schema; \
POSITIVE_SOVEREIGN, fails closed), and the birth verb refuses every wire client with \
`human_gesture_required` — but a repo with NO brain now has a real path: the HUMAN \
runs the one-time ceremony `m1nd init --birth <repo>`. An agent's honest move is to \
OFFER that exact command and stop; running it is not the agent's to do. Until the human runs it: continue only against the bound graph (with the mismatch \
warning intact), or reconnect to an owner that already hosts the intended repo. Absent \
`reception` = your root matches the brain serving you (silent bind is legal only on a \
match, TT-INV-12).

**Reception governs WRITES, not just reads.** A read under `caller_root_mismatch` is a \
warning; a WRITE under it is PROHIBITED by doctrine — `memorize`, `skeleton_candidate`, \
`candidate_edit`, `system_blocks_seed_import`/`_ratify`/`_reconcile`, and `mission_post` would \
each land in the WRONG brain (this is exactly how a foreign skeleton once overwrote a bound \
brain). Do not attempt a write from this mismatched session. The honest recovery code is \
`brain_bootstrap_consumer_not_installed`; no AGENT-callable one-call bootstrap or overlap \
escape hatch exists — the sovereign consumer is human-gated, and the human's ceremony \
(`m1nd init --birth <repo>`) is the only door.

**ADVERTISED ≠ CALLABLE — the authority floors.** Bootstrap is not the only closed door. Every \
semantic action carries an M1ND-10 authority floor and generic MCP/REST dispatch admits only \
`ORDINARY`; a verb above it (`SCOPED_GRANT_A2` / `POSITIVE_SOVEREIGN` / `SERVICE_IDENTITY`) \
refuses with `generic_action_authority_required`. No payload shape, capability claim or retry \
lifts that — only an exact typed G2/G3 consumer (an authority lease), and none is installed for \
those actions yet. 40 of the 141 advertised verbs are affected today, INCLUDING ones this \
document teaches: `learn`, `debrief`, `promote`, `calibrate_predict` / `calibrate_envelope`, \
`ghost_edges`, `runtime_overlay`, `apply` / `apply_batch` / `edit_commit`, `daemon_start` / \
`_stop` / `_tick`, `auto_ingest_start` / `_stop` / `_tick` (their `_status` reads stay open), \
the `xray_*` commit branch, `boot_memory` set/delete, \
`mission_close write_light_memory:true`, and every system-blocks writer. `tools/list` marks each \
one: its description is prefixed `POLICY-DISABLED (authority floor …)`. READ THAT before planning \
around a verb, and do not spend turns retrying a refusal. Still ORDINARY and genuinely callable: \
every read, `memorize`, `delegate`, `trail_save`, the perspective family, plain `mission_start` / \
`mission_event` / `mission_verify` / `mission_handoff` / `mission_close`.

## 1. PRE-ORIENT — never start cold

Call `north(task)` FIRST, before reading or editing anything. One round-trip returns: \
binding trust (`trust_mode`; the repair travels with it when degraded), task context \
(focus nodes + PageRank anchors), prior cross-session memory (each claim with its real \
age + author — absent, never faked, when unknown), a sufficiency signal, one \
`next_move`, `honest_gaps` (what m1nd does NOT yet know), and — when missions \
await the human landing — the `landing_bell` (a `merge_wait` count + one honest \
line, absent when none do). If it returns `needs_ingest`, do not call generic \
`ingest` to REPLACE or MERGE — those stay policy-disabled. For an existing brain, \
use the exact authority flow plus `external_mutation_service`; under \
`caller_root_mismatch`, creating or rebinding a brain remains unavailable until the \
typed bootstrap consumer is installed. The ONE ingest you CAN run is \
`ingest` with mode=refresh — it re-scans a root this brain already declared, and only \
when your caller root is exactly that root; it refuses rather than shrink the graph. \
`north` \
composes trust_selftest + orient + boot_memory + focus — reach for the pieces directly \
only when you need just one.

**Memory is PULL, never PUSH (the medulla law).** Your default recall beat carries exactly \
TWO feeds: your own project brain's memory + the shared `medulla` (promoted/doctrine claims). \
Another repo's private claim NEVER appears in your beat — it can only reach you if it was \
promoted to the medulla. Every recall row is labeled with its `tier` (`project` | `medulla`) \
and `origin_brain` (WHICH brain it came from), so you always know a claim's provenance. \
Need to inspect across projects? Pass `tier` on `seek`/`north`/`boot_memory`: `project` (your \
store only), `medulla` (doctrine only), `project+medulla` (the default), or `all-brains` — the \
EXPLICIT cross-project fan-out that reads every hosted brain, each hit labeled by `origin_brain`. \
`all-brains` is one argument away and never ambient; don't reach for it unless you actually need \
another project's knowledge.

## 2. ACT ON VERDICTS — trust the calibration, don't override it

Retrieval and prediction return a calibrated verdict; obey it:
- **`act` / `reverify` / `abstain`** — `abstain` means uncalibrated or insufficient \
evidence: do NOT guess past it. The prediction gate is armed per-repo by running \
`calibrate_predict` ONCE, and the seek trust envelope by running `calibrate_envelope` \
(from the ledger's learn outcomes); until each is armed its verdict caps at `reverify`, \
never `act`.
- **`why` answers carry a `closure` verdict** — `blocked` means the path rests on an \
unresolved (guessed/dropped) edge: verify that edge before you rely on the path.
- **`seek` carries a `trust_envelope` + a sufficiency stop-signal** — `sufficient` \
means stop gathering; `gathering`/`saturated` mean widen or refine.
- **`trust_band: insufficient_evidence` means NO evidence — not medium risk.** \
It is the honest cold-start answer, distinct from low/medium/high risk.

## 3. POST-CAPTURE — leave the graph warmer than you found it

Before ending, `memorize` every durable finding (a decision, a verified fact, why code \
is the way it is, an open design point). Pass structured claims with `confidence` and — \
crucially — repo-relative `evidence` paths to the code that backs each claim. `memorize` \
anchors each path to the real code node, so the knowledge lives in the same activation \
space as code, surfaces in later `seek`/`north`, and self-flags as stale via \
`cross_verify(check:[\"evidence_freshness\"])` when that code changes. Closing a mission? \
Pass `write_light_memory:true` to `mission_close` to persist its verified claims in one \
step. This is how knowledge compounds instead of being lost between sessions.

Every `memorize` is stamped with an `Origin-Brain` (the project root it was born in, or \
`medulla` for the owner's own doctrine store) so recall can always say WHICH brain a claim \
came from. If your session's root has no project brain, a `memorize` is REFUSED (not \
silently written into the shared medulla). Its refusal reports \
`brain_bootstrap_consumer_not_installed`; it never fabricates an executable repair call.

**PROMOTION — the audited crossing (do it deliberately, rarely).** A `memorize` is ALWAYS \
project-private; a finding does NOT become shared doctrine by being written. When a VERIFIED \
claim is genuinely transversal (true across projects, not one repo's fact), `promote \
{brain: <project_root>, claim: <slug>, reason: <one line>}` copies it UP into the medulla \
with the full readable chain (Origin-Brain, Origin-Claim, Promoted-By, Promotion-Reason); the \
project original stays in place, stamped Promoted-To — promotion ELEVATES, never moves. Gates \
you must respect: only `State: verified` (or a founder claim) may promote; a secret or a \
conflict-marker is refused at the hygiene floor (the medulla is the most-read store); a \
promoted claim's code evidence is origin-qualified so freshness delegates back to its home \
brain, or the claim is marked `evidence_unverifiable` — a medulla claim never reads fresher \
than it can prove. ETIQUETTE: promotion is an ORCHESTRATOR / maintainer act — a maker \
PROPOSES (\"candidate for promotion\" is just a claim it memorizes), the orchestrator executes. \
Any id CAN call `promote` (it is not a security boundary), but every promotion is auditably \
attributed by `Promoted-By`; don't promote a maker's unverified hunch. To DEMOTE (un-share a \
claim that turned out to be one repo's quirk), `learn wrong` on the MEDULLA copy or supersede \
it with a `moved_to:` medulla `memorize` — this never touches the project witness (un-share, \
never destroy).

Then leave ONE field-telemetry signal and keep working (report, never detour): when a \
retrieval was right or wrong, `learn(correct|wrong|partial)`; when m1nd ITSELF misbehaves \
(a bug, friction, or an honesty miss — it claimed fresh/closed/act and was wrong), append \
one JSON line to `~/.m1nd/field-reports.jsonl` \
(`{ts,agent,repo,tool,class:\"bug|honesty|friction|win\",what,expected,snippet}`) — \
local-only, it never phones home. A memory-delivery fault (a claim leaked across brains, a \
false absence, a write to the wrong store) is `class:\"memory_misdelivery\"` with a \
`kind` of leak|false_absence|wrong_store_write|misattribution|vanished. The spool is the ONE \
write slot; letters distribute LOCALLY into per-project boxes (`<repo>/.m1nd/inbox.jsonl`, \
travels with git) + the medulla box for projectless letters — a letter naming a project \
NEVER lands in the medulla. Triage is `m1nd-mcp --inbox-sweep` (CLI) or `GET /api/inbox_sweep` \
(the union of spool + every box, each letter once); a project's box reads via \
`GET /api/mailbox?brain=<root>`. These are CLI/REST surfaces — NOT MCP tools (never in the loop).

## 4. DELEGATE — hand a grounded packet down, debrief the return

Spawning a subagent? `delegate {agent_id, task}` composes the RETRIEVAL half of its spec in ONE \
read-only call — ranked anchors, a memory slice with real age + author, known static dependents, a \
staleness header, a proof-command heuristic, and an explicit list of what m1nd could NOT determine — \
and renders it as `prompt_markdown` you APPEND to your brief (the packet is an appendix: your text \
wins on what-to-do, the file wins on what-is, the packet outranks assumption only). The packet's \
`mission.binding` NAMES the brain the child must land on — it is the SAME datum reception uses \
(`M1nd-Caller-Root` ↔ `covers_root`), so the child VERIFIES it landed (silent on match) rather than \
choosing (the child law, ORGANISM §C5.3). `delegate` abstains HONESTLY — `needs_ingest` on an empty \
graph, `unscopable` when the task activates no coherent subgraph, `seeds_unresolvable` when every \
seed fails — always with evidence + a `next_move`, never a bare no. It is PROJECT-TIER: no \
medulla-doctrine block, and no predict/trust/tremor/xray enrichment yet — each omission is stated in \
`non_claims`, never hidden. \
When the subagent returns, `debrief {agent_id, delegation_id, outcome, diff|touched_paths, findings}` \
grades its real diff against that packet and TEACHES the graph — the only mutation, through \
`memorize`/`learn` only. It classifies each touched path (in_scope | expected_change | \
dependent_contact | unpredicted) with a worst-of verdict that carries fence existence (\"stayed — no \
ratified boundaries existed\"), memorizes the subagent's findings under the subagent's id and any \
map-miss lessons under yours (clean runs memorize nothing), and appends one `outcomes.jsonl` row \
stamped `outcome_unverified` unless you attach `evidence`. Conformance grades PATHS, never code \
quality — it never says merge-safe. Every debrief visibly deposits memory the next packet will \
surface, so skipping it wastes knowledge; do it.

**WORK RUNS INSIDE — the burst wears the wire.** When you ORCHESTRATE a burst (dispatching \
≥2 executors, or landing a BIG change), open ONE mission card so the organism SEES the work \
instead of it happening off-book: `mission_start {agent_id, repo, mode, budget, risk, task}` at \
the start (over the wire, or the REST loopback `POST /api/tools/mission_start`), `mission_event` \
at each milestone, `mission_close` with the honest outcome at the end. A mission-control card is \
SINGLE-AGENT — `mission_event`/`mission_close` require the card's own `agent_id` — so the burst \
posts under the orchestrator's id: executors report back and the orchestrator posts, they do NOT \
each open a card (anti-spam: ONE card per burst THEME, never one per executor). NEGATIVE DEFAULT, \
like the voice: a card is for a REAL burst, never a trivial one-file touch. The card is a TRAIL, \
never a GATE — it records what happened; the deterministic gate still proves the work, and no card \
auto-lands (the map colors only by a human `receipt_import` on the mission-letter board — a \
`mission_close` closes a trail, it never colors a block).

## 5. THE SOUL — trust the handoff by a receipt, not by faith

A repo's `docs/PATHOS.md` is its SOUL: the curated handoff — north, state, doctrine, access, \
known problems, next moves. The pathos skill is the AUTHORING guide (how souls are born, which \
sections exist); m1nd is the ENGINE that verifies one. `soul_check` parses the soul into \
anchored CLAIMS, classifies each (path/line-hint/symbol/git/consistency/receipt/runtime/declared), \
verifies per class, and returns the honesty report + a one-line FRESHNESS RECEIPT — \"N fresh · \
M stale · K receipt-priced, checked <date> @<sha>\" — the line a cold context reads to know how \
much to trust the handoff. THE TWO TISSUES hold: verifiable tissue (state/access/known-problems) \
is machine-checkable; DECLARED tissue (taste, doctrine, why-we-work-this-way) is \
UNPROVABLE-but-curated and NEVER fake-verified — the system knowing what it cannot verify IS the \
honesty. `soul_read` pulls the body (whole or a section) — the explicit pull surface, never \
ambient. THE CURATOR is a near-PR/doc-gate WORKFLOW (not a verb): sweep with soul_check → verify \
against code/git/runtime → update durable claims via `memorize` (with `soul_source` provenance, \
the ONE write door) → prune stale NEVER silently (every removal named, git keeps the text) → \
re-check → carry the receipt in the PR body. WHO VERIFIES THE CURATOR (§C8.4): its report must \
pass `soul_check {verify_curator_report: <report>}` run by a DIFFERENT agent — grader ≠ author.

## 6. THE WRITE SURFACE — the candidate map & missions (RATIFY IS HUMAN)

Two write surfaces beyond `memorize`, both candidate/human-gated by design. THE CANDIDATE MAP: \
`skeleton_candidate` scans a repo into a CANDIDATE block map (with `naming:\"auto\"` + a live \
naming-runner it is born NAMED — the zero-touch default); `candidate_edit` is the ONE verb that \
edits it — six typed ops (rename/merge/split/move_member/resolve_seam/assign_unmapped) in ONE \
atomic OCC batch under `expected_store_version` (one bad op persists NOTHING), and it REFUSES on a \
ratified skeleton (candidate-only). `candidate_lease` is advisory (TTL, reclaimable) and NEVER \
blocks the owner. RATIFY IS EXCLUSIVELY HUMAN — no agent ratifies a skeleton, ever (mechanically \
enforced: generic `system_blocks_ratify` is closed because a client-authored origin string proves \
nothing; ratification requires a future exact typed G2/G3 sovereign lease path — \
and raw `receipt_import` is a permanent external G3 tombstone), and an untouched raw-heuristic block cannot be ratified; the hand proposes (even a whole-candidate \
curation mission edits via `candidate_edit`), the human signs. CURATION IS PROPOSE-APPLY (F12): the \
`curation_spawn` verb sends the candidate to a pinned hand-runner that PROPOSES `candidate_edit` ops \
as data; the OWNER sanitizes (o5, seat `runner`) and applies them under OCC — the hand never holds a \
write surface, and it can NEVER ratify. THE MISSION LETTER: `mission_post` \
records one mission's live state as `m1nd-mission-letter-v0` — `brain_ref` is the brain's DISPLAY \
NAME (the basename of its root, never an absolute path; a wrong one is refused `brain_mismatch`), \
`block_id` must name a real block in the bound skeleton (else `unknown_block`; a legitimate \
smoke/probe letter sets `synthetic:true`), and a letter is STATE, never evidence: it never colors a \
block — only `receipt_import` does — and `landed` is reserved for a confirmed imported receipt.

## 7. THE M1ND VOICE — rendering `human_view` (the card law)

`north` carries `human_view` (`m1nd-human-view-v0`): the m1nd voice for the HUMAN in the \
conversation — a server-composed card with `state` (clean|bell|coherence|mismatch|needs_ingest), \
a mechanical `state_sig`, and `lines[]` already MOUNTED (the `m1nd` wordmark + the PULSE row + \
`│` gutter, ≤4 lines, ≤80 chars). Render it by joining `lines` with newlines inside a fenced \
code block — never re-compose, re-order, or decorate it. Every line is a measured fact or a \
verbatim server string (brand law G1: no uncalibrated adjectives, no benefit claims — silence \
over ornament). Line 1 may also carry a `map <N> blocks` segment — the served brain's ratified \
SystemBlock count (per-brain; the packet's `map` field), omitted when zero.

THE PULSE — the official signature of the voice (owner's stamp 2026-07-12). Line 1 hangs \
`m1nd ` then FIVE pulse cells: `trust · graph · focus · bell · coherence`, each calm `╷` or \
raised `│` (e.g. `m1nd ╷╷╷│╷` = only the bell is calling). READ IT AS AN EXPRESSION, never \
cell-by-cell: all low = calm; one stem standing up = look. The cell order is FIXED — never \
reorder or add a cell. Under a repo mismatch the pulse is DROPPED and the plain spine `m1nd │ ` \
returns (the vitals would read the wrong brain). Its meaning belongs in the deep rung, never as \
a per-cell caption in the compact card.

CADENCE — the default is NEGATIVE: Do NOT render the card unless m1nd contributed structurally \
to the mission AND the content is useful to the human NOW; never in consecutive messages; never \
the same state_sig twice in a session; on state change or first orient. When in doubt, stay \
silent — silence is the honest card.

TRANSLATION DUTY: translate the card's CONTENT into the conversation's language while keeping \
the geometry intact (the pulse + gutter at column 6, ≤80 cols) and ids, hashes, tool names, and \
state tokens (`merge_wait`, `needs_ingest`, `full_trust`) verbatim.

THE DEEP RUNG (R2) IS YOURS: when the human asks (\"what's the bell?\", \"show me m1nd\") or at a \
landing moment, render a deeper card FROM THE PACKET'S STRUCTURED FIELDS (`landing_bell`, the \
mission tray, blocks) in the SAME grammar — wordmark + gutter, one measured fact per line — \
never a fact the packet does not carry. In the deep rung you MAY render the pulse legend \
(`pulse ≔ trust ╷ · graph ╷ · focus ╷ · bell │ · coherence ╷`) so the human learns to read the \
row — this legend is YOURS to render, never served in the compact card. Two proof glyphs are \
also yours, ONLY where the packet proves them, never as decoration: `⊢` (evidence ⊢ receipt) on \
a receipt line (`441 tests green ⊢ receipt sha256:…`; ASCII `>`), and `∎` on a consummated \
landing (`msn_… landed ∎`; ASCII `#`).

ASCII FALLBACK (1:1): when the surface cannot hold unicode, map `╷`→`.`, `│`→`|`, `·`→`.`, \
`—`→`-`, `⊢`→`>`, `∎`→`#` — widths are identical, the geometry never moves.

THE COCKPIT (`cockpit`) is the human's ON-REQUEST menu — call it ONLY when the human asks to \
look around (\"?\", \"show me m1nd\", \"what can I check?\"); never auto-serve it, and NEVER at a \
landing (there the card speaks and the door is the tray). It is read-only: its entries are \
argument-less reads and pointer doors (the tray carries no verb — the stamp is a human gesture, \
never a cockpit click). Carry its `menu_sig` back verbatim when navigating.

ATTRIBUTION — the second half of the voice: narrate where m1nd was useful ONLY when it passes \
the counterfactual test (\"without it, would I have decided differently or worse?\") — it changed \
a decision, avoided a rediscovery, opened a front, proved or refuted something. Consulting \
without effect = silence. Never claim \"used m1nd\" as merit; state facts, never estimated \
savings (G1).

## SECONDARY VERBS (one line each)

- `seek(query)` / `focus(task)` — budgeted retrieval; carry the trust + sufficiency signals above.
- `impact(node)` — directional blast radius before a change; `why(a,b)` — the load-bearing path between two nodes.
- `trust_selftest` / `recovery_playbook` — run when trust looks off or retrieval is blocked; `doctor` for deeper host-surface/graph diagnosis.
- `am_i_stale(claim)` — check BEFORE editing on the strength of remembered/cached knowledge.
- `soul_check` / `soul_read` — verify (freshness receipt) / pull the project's PATHOS handoff soul.
- `coverage_session` — surface the blind spots in what you have and haven't looked at this session.
- `ingest` — `replace`/`merge` are policy-disabled and must not be called. `ingest` with mode=refresh is the one exception: re-scan a root this brain already declared, from exactly that root.
- `external_mutation_service` — governed elevated graph ingest for an existing brain after the exact authority flow; it does not create project brains.
";

/// Stdio MCP framing mode auto-detected on the inbound stream. The matching
/// outbound write MUST use the same mode so the host's framing assumptions hold.
/// Exposed `pub` so the `--attach` stdio↔HTTP bridge reuses the exact same
/// framing primitives instead of hand-rolling a divergent encoder.
#[derive(Clone, Copy, Debug)]
pub enum TransportMode {
    Framed,
    Line,
}

/// Read one JSON-RPC payload from `reader`, auto-detecting Content-Length framing
/// vs newline framing, and report which mode was seen so the response can be
/// written back in the same framing. `Ok(None)` signals EOF. Reused verbatim by
/// the `--attach` bridge — see `attach_client.rs`.
pub fn read_request_payload<R: BufRead>(
    reader: &mut R,
) -> std::io::Result<Option<(String, TransportMode)>> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(None);
        }

        let first_non_ws = buffer
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace());
        let starts_framed = matches!(first_non_ws, Some(byte) if byte != b'{' && byte != b'[');
        if starts_framed {
            let mut content_length: Option<usize> = None;
            loop {
                let mut header_line = String::new();
                let bytes = reader.read_line(&mut header_line)?;
                if bytes == 0 {
                    return Ok(None);
                }
                let trimmed = header_line.trim_end_matches(['\r', '\n']);
                if trimmed.is_empty() {
                    break;
                }
                if let Some((name, value)) = trimmed.split_once(':') {
                    if name.trim().eq_ignore_ascii_case("Content-Length") {
                        content_length = value.trim().parse::<usize>().ok();
                    }
                }
            }

            let length = content_length.ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Missing Content-Length header",
                )
            })?;
            let mut body = vec![0_u8; length];
            reader.read_exact(&mut body)?;
            let payload = String::from_utf8(body)
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
            return Ok(Some((payload, TransportMode::Framed)));
        }

        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return Ok(Some((trimmed.to_owned(), TransportMode::Line)));
    }
}

/// Write a `JsonRpcResponse` to `writer` in the given framing mode (the one
/// detected by [`read_request_payload`] for the matching request). Reused
/// verbatim by the `--attach` bridge so its stdout framing is byte-identical to
/// the embedded stdio server's.
pub fn write_response<W: Write>(
    writer: &mut W,
    response: &JsonRpcResponse,
    mode: TransportMode,
) -> std::io::Result<()> {
    let json = serde_json::to_string(response).unwrap_or_default();
    match mode {
        TransportMode::Framed => {
            write!(writer, "Content-Length: {}\r\n\r\n{}", json.len(), json)?;
        }
        TransportMode::Line => {
            writeln!(writer, "{}", json)?;
        }
    }
    writer.flush()
}

// ---------------------------------------------------------------------------
// McpConfig — server configuration
// Replaces: 03-MCP Section 1.2 initialization config
// ---------------------------------------------------------------------------

/// MCP server configuration.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct McpConfig {
    pub graph_source: PathBuf,
    pub plasticity_state: PathBuf,
    #[serde(default)]
    pub runtime_dir: Option<PathBuf>,
    #[serde(default)]
    pub registry_dir: Option<PathBuf>,
    pub auto_persist_interval: u32,
    pub learning_rate: f32,
    pub decay_rate: f32,
    pub xlr_enabled: bool,
    pub max_concurrent_reads: usize,
    pub write_queue_size: usize,
    /// Domain name: "code" (default), "music", or "generic".
    /// Controls temporal decay half-lives and relation types.
    #[serde(default)]
    pub domain: Option<String>,
    /// Attach read-only: the session loads the snapshot and serves queries but
    /// never persists to disk and never holds an exclusive lease. Mutation tools
    /// are disabled. Set via `--read-only` CLI flag or `M1ND_READ_ONLY=1`.
    #[serde(default)]
    pub read_only: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            graph_source: PathBuf::from("./graph_snapshot.json"),
            plasticity_state: PathBuf::from("./plasticity_state.json"),
            runtime_dir: None,
            registry_dir: None,
            auto_persist_interval: 50,
            learning_rate: 0.08,
            decay_rate: 0.005,
            xlr_enabled: true,
            max_concurrent_reads: 32,
            write_queue_size: 64,
            domain: None,
            read_only: false,
        }
    }
}

// ---------------------------------------------------------------------------
// McpServer — JSON-RPC stdio server
// Replaces: 03-MCP Section 1.1 deployment model
// ---------------------------------------------------------------------------

/// MCP server over JSON-RPC stdio. Single process, shared PropertyGraph.
/// Replaces: 03-MCP server architecture
///
/// Raw dispatch functions are an internal actor implementation detail, not a
/// Rust embedding API.
///
/// ```compile_fail,E0603
/// use m1nd_mcp::server::dispatch_tool;
/// ```
///
/// ```compile_fail,E0603
/// use m1nd_mcp::server::handle_mcp_method;
/// ```
pub struct McpServer {
    config: McpConfig,
    /// Raw construction state exists only until [`Self::start`] installs it in
    /// the bound per-brain actor. No transport dispatch can reach this value.
    boot_state: Option<SessionState>,
    actor_runtime: Option<StdioActorRuntime>,
    daemon_runtime: Option<DaemonRuntimeControl>,
    offline_context: (PathBuf, Option<String>),
    shutdown_requested: Arc<AtomicBool>,
    shutdown_wake: Arc<std::sync::Mutex<Option<mpsc::SyncSender<ServerEvent>>>>,
    stopped: bool,
}

struct StdioActorRuntime {
    session: Arc<BrainSessionCell>,
    project_brains: Arc<crate::project_brains::ProjectBrainRegistry>,
}

/// Opaque cooperative stop capability for the blocking stdio loop.
///
/// It can only ask the transport loop to return. It exposes no session, actor,
/// instance lease, callback, lock, or lifecycle-release capability.
#[derive(Clone)]
pub struct McpShutdownHandle {
    requested: Arc<AtomicBool>,
    wake: Arc<std::sync::Mutex<Option<mpsc::SyncSender<ServerEvent>>>>,
}

impl McpShutdownHandle {
    pub fn request_shutdown(&self) {
        self.requested.store(true, Ordering::Release);
        if let Ok(wake) = self.wake.lock() {
            if let Some(sender) = wake.as_ref() {
                let _ = sender.try_send(ServerEvent::Shutdown);
            }
        }
    }
}

/// Cloneable, actor-backed embedding surface for in-process tool callers.
///
/// The server remains the lifecycle owner. This client exposes only one complete
/// tool transaction and cannot yield a session, callback, actor, lock, instance
/// lease, or shutdown capability.
#[derive(Clone)]
pub struct McpToolClient {
    session: std::sync::Weak<BrainSessionCell>,
    project_brains: std::sync::Weak<crate::project_brains::ProjectBrainRegistry>,
}

impl McpToolClient {
    pub fn call_tool(&self, tool: &str, args: &serde_json::Value) -> M1ndResult<serde_json::Value> {
        let session = self.session.upgrade().ok_or_else(|| {
            M1ndError::PersistenceFailed(
                "stdio tool client lost its McpServer lifecycle owner".to_string(),
            )
        })?;
        let project_brains = self.project_brains.upgrade().ok_or_else(|| {
            M1ndError::PersistenceFailed(
                "stdio tool client lost its McpServer lifecycle owner".to_string(),
            )
        })?;
        let mutating = read_only_denied(tool, args);
        let tool = tool.to_string();
        let args = args.clone();
        project_brains.execute_target_m1nd(session, None, true, mutating, move |state| {
            dispatch_generic_tool(state, &tool, &args)
        })
    }
}

#[derive(Debug)]
enum ServerEvent {
    Request(String, TransportMode),
    StdinClosed,
    WatchNotice,
    WatchError(String),
    Shutdown,
}

struct LiveDaemonWatcher {
    _watcher: RecommendedWatcher,
    dropped_counter: Arc<AtomicU64>,
}

struct DaemonRuntimeControl {
    event_tx: mpsc::SyncSender<ServerEvent>,
    watcher: Option<LiveDaemonWatcher>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct DaemonLoopView {
    active: bool,
    read_only: bool,
    watch_paths: Vec<String>,
    git_root_present: bool,
    watch_backend: String,
    watch_backend_error: Option<String>,
    coalesce_window_ms: u64,
    wait_duration_ms: u64,
}

struct CoalescedWatchBurst {
    watch_events_seen: u64,
    coalesced_at_ms: u64,
    backend_error: Option<String>,
    watch_errors: u64,
    pending_request: Option<(String, TransportMode)>,
    stdin_closed: bool,
    shutdown: bool,
}

fn coalesce_watch_burst(
    rx: &mpsc::Receiver<ServerEvent>,
    coalesce_window_ms: u64,
) -> CoalescedWatchBurst {
    let mut burst = CoalescedWatchBurst {
        watch_events_seen: 1,
        coalesced_at_ms: now_ms(),
        backend_error: None,
        watch_errors: 0,
        pending_request: None,
        stdin_closed: false,
        shutdown: false,
    };
    loop {
        // A sliding silence window alone can starve under continuous churn.
        if now_ms().saturating_sub(burst.coalesced_at_ms)
            >= crate::daemon_handlers::BURST_COALESCE_CAP_MS
        {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(coalesce_window_ms.max(1))) {
            Ok(ServerEvent::WatchNotice) => {
                burst.watch_events_seen = burst.watch_events_seen.saturating_add(1);
            }
            Ok(ServerEvent::WatchError(error)) => {
                burst.watch_errors = burst.watch_errors.saturating_add(1);
                burst.backend_error = Some(error);
            }
            Ok(ServerEvent::Request(payload, mode)) => {
                burst.pending_request = Some((payload, mode));
                break;
            }
            Ok(ServerEvent::StdinClosed) => {
                burst.stdin_closed = true;
                break;
            }
            Ok(ServerEvent::Shutdown) => {
                burst.shutdown = true;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                burst.stdin_closed = true;
                break;
            }
        }
    }
    burst
}

struct ShutdownWakeRegistration {
    wake: Arc<std::sync::Mutex<Option<mpsc::SyncSender<ServerEvent>>>>,
}

impl Drop for ShutdownWakeRegistration {
    fn drop(&mut self) {
        if let Ok(mut wake) = self.wake.lock() {
            *wake = None;
        }
    }
}

// ---------------------------------------------------------------------------
// Tool tier gate
// ---------------------------------------------------------------------------

/// The curated ESSENTIAL tool set (48 tools) advertised by default.
///
/// These are the high-frequency tools agents need for orientation, trust, and
/// everyday graph queries. All other tools are "advanced" and are hidden from
/// `tools/list` unless `M1ND_TOOL_TIER=full` is set — they remain fully
/// callable via `tools/call` dispatch at all times.
///
/// To expose everything, set `M1ND_TOOL_TIER=full` in the MCP environment.
pub const ESSENTIAL_TOOLS: &[&str] = &[
    "trust_selftest",
    "session_handshake",
    "orient",
    "north",
    "delegate",
    "debrief",
    "am_i_stale",
    "recovery_playbook",
    "health",
    "doctor",
    "help",
    "ingest",
    "audit",
    "search",
    "seek",
    "focus",
    "activate",
    "learn",
    "glob",
    "view",
    "batch_view",
    "impact",
    "why",
    "trace",
    "predict",
    "validate_plan",
    "surgical_context_v2",
    "cross_verify",
    "soul_check",
    "soul_read",
    "mission_start",
    "mission_next",
    "mission_close",
    "mission_service",
    "external_mutation_service",
    "graph_ingest_preview",
    "authority_session_challenge",
    "authority_session_authenticate",
    "authority_authorize",
    "persist",
    "memorize",
    "promote",
    "xray_retag",
    "xray_apply",
    "xray_orient",
    "xray_gate",
    "xray_paint",
    "xray_ledger",
];

/// Returns the active tool tier based on the `M1ND_TOOL_TIER` env var.
///
/// - Unset or `essential` (case-insensitive) → `"essential"` (curated 43)
/// - `full` (case-insensitive) → `"full"` (all 132 external tools)
/// - Any unrecognized value → defaults to `"essential"`
pub fn active_tool_tier() -> &'static str {
    match std::env::var("M1ND_TOOL_TIER")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "full" => "full",
        _ => "essential",
    }
}

/// Whether the additive `_m1nd` response envelope is attached to tool results.
///
/// Gated by `M1ND_RESPONSE_ENVELOPE`, default ON. Only an explicit `"0"` or
/// `"false"` (case-insensitive) disables it; any other value (or unset) is ON.
pub fn response_envelope_enabled() -> bool {
    match std::env::var("M1ND_RESPONSE_ENVELOPE") {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            v != "0" && v != "false"
        }
        Err(_) => true,
    }
}

/// Whether the M1ND_PROOF_GATE write guard is active.
///
/// Default-on safety flag. Only explicit `"0"` or `"false"` disables it. The
/// guard is selected by semantic effect union, not tool name: every action with
/// `SOURCE_FILESYSTEM_WRITE` must consume an exact one-shot proof mark.
pub fn proof_gate_enabled() -> bool {
    match std::env::var("M1ND_PROOF_GATE") {
        Ok(value) => {
            let value = value.trim().to_ascii_lowercase();
            value != "0" && value != "false"
        }
        Err(_) => true,
    }
}

/// Returns ALL registered MCP tool schemas regardless of tier.
/// Use this when you always need the full 132-tool registry (e.g., health
/// contract counts, internal tests that verify advanced tool registration).
pub fn all_tool_schemas() -> serde_json::Value {
    all_tool_schemas_inner()
}

/// Returns the tier-gated tool list for `tools/list` advertisement.
///
/// With `M1ND_TOOL_TIER=full` → returns all tools.
/// Otherwise → returns only the ESSENTIAL_TOOLS curated set.
/// Hidden tools remain callable via `tools/call` dispatch (handlers untouched).
pub fn tool_schemas() -> serde_json::Value {
    tool_schemas_for_tier(active_tool_tier())
}

/// Returns the tool list for the given tier string.
/// Used internally and by tests to avoid env-var races.
/// `tier`: "full" → all tools; anything else → essential set only.
pub fn tool_schemas_for_tier(tier: &str) -> serde_json::Value {
    let all = all_tool_schemas_inner();
    if tier.eq_ignore_ascii_case("full") {
        return all;
    }
    // Filter to essential set only
    let essential_set: std::collections::HashSet<&str> = ESSENTIAL_TOOLS.iter().copied().collect();
    let filtered: Vec<serde_json::Value> = all["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|tool| {
            tool.get("name")
                .and_then(|n| n.as_str())
                .map(|name| essential_set.contains(name))
                .unwrap_or(false)
        })
        .collect();
    serde_json::json!({ "tools": filtered })
}

/// Internal: the complete external tool registry. Legacy raw mission writes
/// remain tombstoned in dispatch but are never advertised.
fn all_tool_schemas_inner() -> serde_json::Value {
    let mut registry = serde_json::json!({
        "tools": [
            {
                "name": "orient",
                "description": "Boot into a task in one call. Give your free-form task and get your STARTING CONTEXT pre-packed: the focus nodes the task activates (ranked), prior memorized conclusions nearby, the global PageRank attention backbone, coverage so far, and the concrete first calls to make. Call this FIRST when you receive a task instead of doing exploratory reads. Read-only safe.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "task": { "type": "string", "description": "Free-form description of the task you are about to start. The graph spread-activates on this text to find your starting context." },
                        "top_k": { "type": "integer", "default": 8, "description": "How many focus nodes to return (ranked by activation from the task)" },
                        "scope": { "type": "string", "description": "Optional scope hint to bound orientation" }
                    },
                    "required": ["agent_id", "task"]
                }
            },
            {
                "name": "north",
                "description": "Pre-orient in ONE call so you never start cold: the honest north packet. Composes binding trust (trust_mode + fingerprint + the repair when degraded), task context (focus nodes + PageRank anchors from orient), durable cross-session memory (each claim with its real age + author, absent when unknown — never faked to 'now'), an answer-free sufficiency signal, one suggested next_move, and honest_gaps (what m1nd does NOT yet know). When missions await the human landing it also rings the landing_bell (a merge_wait count + one honest line, absent when none do) so an opening agent can nudge the owner to the tray. It also carries human_view (m1nd-human-view-v0): the m1nd voice — a server-mounted ≤4-line card (state, state_sig, lines) the agent renders for the human under the negative-default cadence in the instructions (§7), verbatim, never re-composed. On an empty/unbound graph it honestly returns needs_ingest + the repair, not a fabricated orientation. One round-trip instead of trust_selftest → orient → boot_memory → focus. Read-only safe.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "task": { "type": "string", "description": "Free-form description of the task you are about to start. The graph spread-activates on this text to find your starting context." },
                        "top_k": { "type": "integer", "default": 8, "description": "How many focus nodes to return (ranked by activation from the task)" },
                        "scope": { "type": "string", "description": "Optional scope hint to bound orientation and trust binding" },
                        "tier": { "type": "string", "enum": ["project", "medulla", "project+medulla", "all-brains"], "description": "Memory-tier for the packet's recall beat (pull-not-push). Default project+medulla: this brain's own memory + the shared medulla (promoted/doctrine). 'all-brains' fans out over EVERY hosted brain, each memory row labeled origin_brain — the explicit cross-project inspection, never ambient. Another brain's claim reaches your default beat only if it was promoted to the medulla." }
                    },
                    "required": ["agent_id", "task"]
                }
            },
            {
                "name": "cockpit",
                "description": "The navigable m1nd menu (m1nd-cockpit-v0) — the human's ON-REQUEST router over m1nd's read surfaces, a read-only sibling of north (never a north field; if it breaks it breaks alone). Call it ONLY when the human asks ('?', 'show me m1nd', 'what can I look at') — at a landing the human_view card speaks, never an auto-served menu. The root is argument-less READS only, in eight stable slots (labels move with state, slots never do): the tray (a POINTER — the human stamp gesture, no verb), the map (system_blocks_snapshot), missions (a POINTER to the tray), health (doctor), trust, recent-memories (boot_memory, fixed projection), drift, and presences (the P1 control-room roster of THIS brain — who is talking to m1nd, on what, since when — with a collision warning on the label when two mutating hands share the work). Pointer entries carry NO verb — nothing to execute even by mistake. Pass select=\"<slot>\" to drill one collection (depth ≤3; select=\"0\" re-serves the root); a drill re-asserts store_version + state_sig and says 'state moved' honestly when your seen_store_version diverged. Every response carries menu_sig — the SAME short reference a widget button carries back (never free command text, never a write verb). The read entries are DERIVED and filtered against the write deny-list, so a verb that becomes a mutation drops itself. Read-only safe.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier." },
                        "select": { "type": "string", "description": "Optional slot to drill (a number 1..=7), or \"0\" to re-serve the root. Absent = the root menu." },
                        "seen_store_version": { "type": "integer", "description": "Optional: the SystemBlock store_version you last read. If the store has moved since, the drill flags 'state moved' before you act." }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "delegate",
                "description": "Hand a grounded packet DOWN to a subagent in ONE read-only call: the retrieval half of its spec, composed from the live graph. Returns an m1nd-delegation-packet-v0 — ranked anchors, a memory slice with real age + author, known static dependents (file-level static), a staleness header, a coverage header, a proof-command heuristic, an explicit list of what m1nd could NOT determine, and a deterministic prompt_markdown the subagent reads straight. The packet's mission.binding NAMES the brain the child must land on (the child verifies via reception, never chooses). Abstains honestly (needs_ingest on an empty graph; unscopable when the task activates no coherent subgraph; seeds_unresolvable when every seed fails) with evidence + a next_move, never a bare no. This is PROJECT-TIER: no medulla-doctrine block, and no stage-5 enrichment (predict/trust/tremor/xray) — each omission is stated in non_claims. Read-only safe.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling (orchestrator) agent identifier" },
                        "task": { "type": "string", "description": "Free-form description of what the subagent is about to do. The graph spread-activates on this text to compose the packet's context." },
                        "scope": {
                            "type": "object",
                            "description": "Optional scope. `paths` declares the subagent's may-touch set (orchestrator authority); `seeds` are node ids that inject extra context — the only context-injection channel (delegate never sees the orchestrator's conversation).",
                            "properties": {
                                "paths": { "type": "array", "items": { "type": "string" }, "description": "Declared may_touch paths. Absent → derived from activation." },
                                "seeds": { "type": "array", "items": { "type": "string" }, "description": "Node ids to seed the packet's context. Every seed unresolvable → abstain." }
                            }
                        },
                        "budget": {
                            "type": "object",
                            "description": "Packet budget — a CAP, not a quota. Default tokens 2000 (hard ceiling 8000), max_nodes 40.",
                            "properties": {
                                "tokens": { "type": "integer", "default": 2000, "description": "Approx token cap for the rendered packet." },
                                "max_nodes": { "type": "integer", "default": 40, "description": "Blast-radius cap for the dependents pass." }
                            }
                        },
                        "subagent_hint": { "type": "string", "description": "Optional free-form hint about which subagent this packet is for (carried into mission, never a gate)." },
                        "evidence_link": {
                            "type": "object",
                            "additionalProperties": false,
                            "description": "Optional owner-emitted G5 correlation link from a prior MissionService result. It is accepted only when the exact G3 mission/head/iteration/transaction anchor already exists; it never creates authority.",
                            "properties": {
                                "schema": { "const": crate::evidence_spine::EVIDENCE_CORRELATION_LINK_SCHEMA },
                                "mission_id": { "type": "string" },
                                "iteration_id": { "type": "integer", "minimum": 1 },
                                "mission_head_id": { "type": "string" },
                                "transaction_id": { "type": ["string", "null"] }
                            },
                            "required": ["schema", "mission_id", "iteration_id", "mission_head_id", "transaction_id"]
                        }
                    },
                    "required": ["agent_id", "task"]
                }
            },
            {
                "name": "debrief",
                "description": "Grade a spawned subagent's real diff against the packet it was handed, and teach the graph — the ONLY mutation in the delegation layer, and it mutates only through existing verbs (memorize/learn). Load the registry record by delegation_id (unknown id is a hard error, no guessing), re-check staleness (a graph_drifted caveat when the graph moved under the packet), resolve the touched set (diff `+++ b/` headers or touched_paths), classify each path (in_scope | expected_change | dependent_contact | unpredicted) with a worst-of verdict that ALWAYS carries fence existence ('stayed — no ratified boundaries existed'), memorize findings under the subagent's id (breach/unpredicted lessons under the grader's id; clean runs memorize nothing), teach asymmetrically (unpredicted → learn partial, dependent-contact → learn correct; untouched dependents never punished), flip the record to debriefed, and append ONE outcomes.jsonl row (stamped outcome_unverified unless evidence is attached). Conformance grades PATHS, never code quality — it never says merge-safe. NOT read-only.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "The grading (orchestrator) agent identifier — breach/unpredicted lessons memorize under this id." },
                        "delegation_id": { "type": "string", "description": "The dlg_* id from the packet delegate returned. Unknown id is a hard error." },
                        "outcome": { "type": "string", "enum": ["success", "failure", "partial"], "description": "Self-reported task outcome. Exactly these three values." },
                        "evidence": {
                            "type": "object",
                            "description": "Optional proof of the outcome. Its absence stamps the row outcome_unverified.",
                            "properties": {
                                "cmd": { "type": "string", "description": "The proof command that was run." },
                                "exit_status": { "type": "integer", "description": "Its exit status." }
                            }
                        },
                        "diff": { "type": "string", "description": "Unified diff of the subagent's work — `+++ b/` headers give the touched set. Use this OR touched_paths." },
                        "touched_paths": { "type": "array", "items": { "type": "string" }, "description": "Explicit touched paths, when a diff is not available." },
                        "findings": { "type": "array", "items": { "type": "string" }, "description": "Up to 3 durable findings the subagent reported — memorized under the subagent's id." },
                        "subagent_id": { "type": "string", "description": "The subagent's id (findings memorize under it). Falls back to agent_id when omitted." }
                    },
                    "required": ["agent_id", "delegation_id", "outcome"]
                }
            },
            {
                "name": "evidence_query",
                "description": "Read-only G5 EvidenceQuery over the owner-selected brain. Verifies the persisted identity and committed hash-chain prefix, filters correlation records, and never creates a lock, repairs a torn tail, writes cache state, or accepts client-authored evidence events. REST and Streamable MCP share this exact handler.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "correlation_id": { "type": "string" },
                        "mission_id": { "type": "string" },
                        "mission_head_id": { "type": "string" },
                        "transaction_id": { "type": "string" },
                        "receipt_id": { "type": "string" },
                        "delegation_id": { "type": "string" },
                        "mission_control_id": { "type": "string" }
                    }
                }
            },
            {
                "name": "am_i_stale",
                "description": "Self-awareness check a long-running agent should reach for OFTEN: which files in your working set changed on disk SINCE m1nd ingested them, so you know to re-read before acting. You can't see the filesystem change under you (the user edits, another agent edits, a build runs); this gives you that perception. Pass `files` and/or `nodes` to check specific targets, or pass NEITHER and m1nd checks every file you've touched this session. Returns stale (changed|missing), fresh, and unknown (never-ingested) paths. Read-only safe.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "files": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional explicit file paths to check. If omitted (and no `nodes`), defaults to the files you've visited this session."
                        },
                        "nodes": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional node ids to check; each is resolved to its backing file path."
                        }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "activate",
                "description": "Spreading activation query across the connectome",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query for spreading activation" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "top_k": { "type": "integer", "default": 20, "description": "Number of top results to return" },
                        "dimensions": {
                            "type": "array",
                            "items": { "type": "string", "enum": ["structural", "semantic", "temporal", "causal"] },
                            "default": ["structural", "semantic", "temporal", "causal"],
                            "description": "Activation dimensions to include"
                        },
                        "xlr": { "type": "boolean", "default": true, "description": "Enable XLR noise cancellation" },
                        "include_ghost_edges": { "type": "boolean", "default": true, "description": "Include ghost edge detection" },
                        "include_structural_holes": { "type": "boolean", "default": false, "description": "Include structural hole detection" },
                        "token_budget": { "type": "integer", "minimum": 1, "description": "Optional approx context-token budget. m1nd keeps the highest-activation nodes that fit, drops the rest, and returns a 'budget' block (estimate = chars/4, not exact tokenization)" }
                    },
                    "required": ["query", "agent_id"]
                }
            },
            {
                "name": "impact",
                "description": "Impact radius / blast analysis for a node",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "node_id": { "type": "string", "description": "Target node identifier" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "direction": {
                            "type": "string",
                            "enum": ["forward", "reverse", "both"],
                            "default": "forward",
                            "description": "Propagation direction for impact analysis"
                        },
                        "include_causal_chains": { "type": "boolean", "default": true, "description": "Include causal chain detection" }
                    },
                    "required": ["node_id", "agent_id"]
                }
            },
            {
                "name": "missing",
                "description": "Detect structural holes and missing connections",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query to find structural holes around" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "min_sibling_activation": { "type": "number", "default": 0.3, "description": "Minimum sibling activation threshold" }
                    },
                    "required": ["query", "agent_id"]
                }
            },
            {
                "name": "why",
                "description": "Path explanation between two nodes",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source": { "type": "string", "description": "Source node identifier" },
                        "target": { "type": "string", "description": "Target node identifier" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "max_hops": { "type": "integer", "default": 6, "description": "Maximum hops in path search" }
                    },
                    "required": ["source", "target", "agent_id"]
                }
            },
            {
                "name": "warmup",
                "description": "Task-based warmup and priming",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_description": { "type": "string", "description": "Description of the task to warm up for" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "boost_strength": { "type": "number", "default": 0.15, "description": "Priming boost strength" }
                    },
                    "required": ["task_description", "agent_id"]
                }
            },
            {
                "name": "counterfactual",
                "description": "What-if node removal simulation",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "node_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Node identifiers to simulate removal of"
                        },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "include_cascade": { "type": "boolean", "default": true, "description": "Include cascade analysis" }
                    },
                    "required": ["node_ids", "agent_id"]
                }
            },
            {
                "name": "predict",
                "description": "Co-change prediction for a modified node",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "changed_node": { "type": "string", "description": "Node identifier that was changed" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "top_k": { "type": "integer", "default": 10, "description": "Number of top predictions to return" },
                        "include_velocity": { "type": "boolean", "default": true, "description": "Include velocity scoring" }
                    },
                    "required": ["changed_node", "agent_id"]
                }
            },
            {
                "name": "fingerprint",
                "description": "Activation fingerprint and equivalence detection",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "target_node": { "type": "string", "description": "Optional target node to find equivalents for" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "similarity_threshold": { "type": "number", "default": 0.85, "description": "Cosine similarity threshold for equivalence" },
                        "probe_queries": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional probe queries for fingerprinting"
                        }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "drift",
                "description": "Weight and structural drift analysis",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "since": { "type": "string", "default": "last_session", "description": "Baseline reference point for drift comparison" },
                        "include_weight_drift": { "type": "boolean", "default": true, "description": "Include edge weight drift analysis" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "learn",
                "description": "Explicit feedback-based edge adjustment",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Original query this feedback relates to" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "feedback": {
                            "type": "string",
                            "enum": ["correct", "wrong", "partial"],
                            "description": "Feedback type: correct, wrong, or partial"
                        },
                        "node_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Node identifiers to apply feedback to"
                        },
                        "strength": { "type": "number", "default": 0.2, "description": "Feedback strength for edge adjustment" }
                    },
                    "required": ["query", "agent_id", "feedback", "node_ids"]
                }
            },
            {
                "name": "ingest",
                "description": "POLICY-DISABLED generic graph mutation compatibility surface, with ONE exception. mode='replace' and mode='merge' are refused for every client: use the exact authority flow plus external_mutation_service for an existing brain, and cross-root project-brain bootstrap is unavailable until an exact typed G2/G3 consumer is installed. mode='refresh' IS callable at the SCOPED_GRANT_A2 floor, admitted A2-locally with no lease: it re-scans a root this brain has already declared, and only when your caller root is EXACTLY that root. It never creates a brain, never adds a root, never crosses to another brain, and refuses (refresh_would_shrink_graph) rather than replace a wide graph with a narrow scan.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Filesystem path within the already bound brain's project root" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "incremental": { "type": "boolean", "default": false, "description": "Incremental ingest (code adapter only)" },
                        "adapter": {
                            "type": "string",
                            "default": "code",
                            "enum": ["code", "json", "memory", "light", "patent", "article", "bibtex", "rfc", "crossref", "auto"],
                            "description": "Adapter to use for parsing the input corpus"
                        },
                        "mode": {
                            "type": "string",
                            "default": "replace",
                            "enum": ["replace", "merge", "refresh"],
                            "description": "Replace the bound graph, merge the ingest into it, or 'refresh' — re-scan a root this brain already declared (the only mode a plain client can execute; requires your caller root to be exactly that root)"
                        },
                        "namespace": {
                            "type": "string",
                            "description": "Optional namespace tag for memory/non-code nodes"
                        },
                        "include_dotfiles": {
                            "type": "boolean",
                            "default": false,
                            "description": "Include selected dotfiles and hidden config directories during ingest"
                        },
                        "dotfile_patterns": {
                            "type": "array",
                            "items": { "type": "string" },
                            "default": [],
                            "description": "Allowed dotfile patterns when include_dotfiles=true (for example '.codex/**')"
                        }
                    },
                    "required": ["path", "agent_id"]
                }
            },
            {
                "name": "brain_birth",
                "description": "Birth a NEW project brain for a repo that has none. This is the HUMAN's one-time gesture, not an agent's call: admission is an origin the OWNER stamps from a fact it observes about itself, and an origin string sent as a parameter grants nothing. Over generic MCP/REST this verb is refused for every client, however the call is dressed. The one path that exists today is the P2 ceremony a HUMAN runs in their terminal: `m1nd init --birth <repo>`. If you are an agent and a repo has no brain, OFFER that exact command and stop; running it is not yours to do. Birth refuses unless the destination is empty ON DISK (no manifest, snapshot or checkpoint), refuses any root that overlaps an existing brain, never touches the owner's bound graph, and is not the way to adopt an existing brain — that is migration, a boot-time fact with no verb.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "root": { "type": "string", "description": "Repo root to birth a brain for" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["root", "agent_id"]
                }
            },
            {
                "name": "document_resolve",
                "description": "Resolve a canonical universal-document artifact by source path or node id",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "path": { "type": "string", "description": "Original source path or canonical markdown path" },
                        "node_id": { "type": "string", "description": "Graph node id emitted from universal ingest" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "document_provider_health",
                "description": "Report availability and install hints for universal document providers",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "document_bindings",
                "description": "Resolve deterministic document-to-code bindings for a universal document",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "path": { "type": "string", "description": "Original source path or canonical markdown path" },
                        "node_id": { "type": "string", "description": "Graph node id emitted from universal ingest" },
                        "top_k": { "type": "integer", "default": 10, "description": "Maximum bindings to return" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "document_drift",
                "description": "Analyze stale, missing, or ambiguous document/code bindings for a universal document",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "path": { "type": "string", "description": "Original source path or canonical markdown path" },
                        "node_id": { "type": "string", "description": "Graph node id emitted from universal ingest" },
                        "scope": { "type": "string", "description": "Optional drift scope hint" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "auto_ingest_start",
                "description": "Start local-first document auto-ingest watchers for supported l1ght-family formats",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "roots": { "type": "array", "items": { "type": "string" }, "description": "Filesystem roots to watch recursively" },
                        "formats": {
                            "type": "array",
                            "items": { "type": "string", "enum": ["universal", "light", "article", "bibtex", "crossref", "rfc", "patent"] },
                            "default": ["universal", "light", "article", "bibtex", "crossref", "rfc", "patent"],
                            "description": "Supported document formats to auto-ingest"
                        },
                        "debounce_ms": { "type": "integer", "default": 200, "description": "Minimum quiet period before a change is eligible for ingestion" },
                        "namespace": { "type": "string", "description": "Optional namespace for non-code document nodes" }
                    },
                    "required": ["agent_id", "roots"]
                }
            },
            {
                "name": "auto_ingest_stop",
                "description": "Stop active document auto-ingest watchers and persist manifest state",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "auto_ingest_status",
                "description": "Report current auto-ingest runtime state, counters, manifest size, and queue depth",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "auto_ingest_tick",
                "description": "Drain queued document changes immediately and apply them to the active graph",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "health",
                "description": "Server health and statistics",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "session_handshake",
                "description": "Cheap session trust handshake before relying on m1nd retrieval. Optionally DECLARE your presence for the control room (kind/theme/intent/worktree/working_set) so the cockpit, Hall, and north can see who is working, on what, since when — advisory telemetry, never a gate.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "observed_tool_count": { "type": "integer", "description": "Optional tools/list count seen by the host client" },
                        "available_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional tool names exposed by the host client" },
                        "missing_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional required tool names missing from the host client surface" },
                        "scope": { "type": "string", "description": "Optional absolute or repo-relative scope/path to validate against the active workspace binding" },
                        "kind": { "type": "string", "description": "Optional presence role: orchestrator | executor | pool-hand | runner | oracle | human-ui" },
                        "theme": { "type": "string", "description": "Optional one-line presence theme (e.g. 'reader slice 1')" },
                        "intent": { "type": "string", "description": "Optional declared mutation level: 'read' or 'mutate' (advisory collision signal)" },
                        "worktree": { "type": "string", "description": "Optional worktree/branch display string for collision derivation" },
                        "working_set": { "type": "array", "items": { "type": "string" }, "description": "Optional declared working set: repo-relative paths and/or sb_ block ids (collision overlap signal)" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "trust_selftest",
                "description": "One-call diagnostic verdict for m1nd host binding, graph, and recovery trust",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "observed_tool_count": { "type": "integer", "description": "Optional tools/list count seen by the host client" },
                        "available_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional tool names exposed by the host client" },
                        "missing_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional required tool names missing from the host client surface" },
                        "observed_tool": { "type": "string", "description": "Optional tool that produced a suspicious result" },
                        "observed_proof_state": { "type": "string", "description": "Optional proof_state from the suspicious result" },
                        "observed_candidates": { "type": "integer", "description": "Optional candidate count from retrieval" },
                        "scope": { "type": "string", "description": "Optional repo or scope path associated with the incident" },
                        "error_text": { "type": "string", "description": "Optional error text or host message" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "recovery_playbook",
                "description": "Deterministic recovery playbook for degraded bindings, empty graphs, and stale-looking sessions",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "trust_mode": { "type": "string", "description": "Optional prior handshake trust mode to preserve in the diagnostic trail" },
                        "observed_tool": { "type": "string", "description": "Optional tool that produced a suspicious result" },
                        "observed_proof_state": { "type": "string", "description": "Optional proof_state from the suspicious result" },
                        "observed_candidates": { "type": "integer", "description": "Optional candidate count from retrieval" },
                        "observed_tool_count": { "type": "integer", "description": "Optional tools/list count seen by the host client" },
                        "available_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional tool names exposed by the host client" },
                        "missing_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional required tool names missing from the host client surface" },
                        "scope": { "type": "string", "description": "Optional repo or scope path associated with the incident" },
                        "error_text": { "type": "string", "description": "Optional error text or host message" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "doctor",
                "description": "Diagnose active graph, runtime, session, and stale binding symptoms",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "observed_tool": { "type": "string", "description": "Optional tool that produced a suspicious result" },
                        "observed_proof_state": { "type": "string", "description": "Optional proof_state from the suspicious result" },
                        "observed_candidates": { "type": "integer", "description": "Optional candidate count from retrieval" },
                        "observed_tool_count": { "type": "integer", "description": "Optional tools/list count seen by the host client" },
                        "available_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional tool names exposed by the host client" },
                        "missing_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional required tool names missing from the host client surface" },
                        "scope": { "type": "string", "description": "Optional scope/path used by the suspicious call" },
                        "error_text": { "type": "string", "description": "Optional error text or host message" }
                    },
                    "required": ["agent_id"]
                }
            },
            // --- Perspective MCP tools (12-PERSPECTIVE-SYNTHESIS) ---
            {
                "name": "perspective_start",
                "description": "Enter a perspective: creates a navigable route surface from a query",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "query": { "type": "string", "description": "Seed query for route synthesis" },
                        "anchor_node": { "type": "string", "description": "Optional: anchor to a specific node (activates anchored mode)" },
                        "lens": { "type": "object", "description": "Optional: starting lens configuration" }
                    },
                    "required": ["agent_id", "query"]
                }
            },
            {
                "name": "perspective_routes",
                "description": "Browse the current route set with pagination",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "page": { "type": "integer", "default": 1, "description": "Page number (1-based)" },
                        "page_size": { "type": "integer", "default": 6, "description": "Routes per page (clamped to 1-10)" },
                        "route_set_version": { "type": "integer", "description": "Version from previous response for staleness check" }
                    },
                    "required": ["agent_id", "perspective_id"]
                }
            },
            {
                "name": "perspective_inspect",
                "description": "Expand a route with fuller path, metrics, provenance, and affinity",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "route_id": { "type": "string", "description": "Stable content-addressed route ID" },
                        "route_index": { "type": "integer", "description": "1-based page-local position" },
                        "route_set_version": { "type": "integer" }
                    },
                    "required": ["agent_id", "perspective_id", "route_set_version"]
                }
            },
            {
                "name": "perspective_peek",
                "description": "Extract a small relevant code/doc slice from a route target",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "route_id": { "type": "string" },
                        "route_index": { "type": "integer" },
                        "route_set_version": { "type": "integer" }
                    },
                    "required": ["agent_id", "perspective_id", "route_set_version"]
                }
            },
            {
                "name": "perspective_follow",
                "description": "Follow a route: move focus to target, synthesize new routes",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "route_id": { "type": "string" },
                        "route_index": { "type": "integer" },
                        "route_set_version": { "type": "integer" }
                    },
                    "required": ["agent_id", "perspective_id", "route_set_version"]
                }
            },
            {
                "name": "perspective_suggest",
                "description": "Get the next best move suggestion based on navigation history",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "route_set_version": { "type": "integer" }
                    },
                    "required": ["agent_id", "perspective_id", "route_set_version"]
                }
            },
            {
                "name": "perspective_affinity",
                "description": "Discover probable connections a route target might have",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "route_id": { "type": "string" },
                        "route_index": { "type": "integer" },
                        "route_set_version": { "type": "integer" }
                    },
                    "required": ["agent_id", "perspective_id", "route_set_version"]
                }
            },
            {
                "name": "perspective_branch",
                "description": "Fork the current navigation state into a new branch",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "branch_name": { "type": "string", "description": "Optional branch name" }
                    },
                    "required": ["agent_id", "perspective_id"]
                }
            },
            {
                "name": "perspective_back",
                "description": "Navigate back to previous focus, restoring checkpoint state",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" }
                    },
                    "required": ["agent_id", "perspective_id"]
                }
            },
            {
                "name": "perspective_compare",
                "description": "Compare two perspectives on shared/unique nodes and dimension deltas",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id_a": { "type": "string" },
                        "perspective_id_b": { "type": "string" },
                        "dimensions": { "type": "array", "items": { "type": "string" }, "description": "Dimensions to compare (empty = all)" }
                    },
                    "required": ["agent_id", "perspective_id_a", "perspective_id_b"]
                }
            },
            {
                "name": "perspective_list",
                "description": "List all perspectives for an agent",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "perspective_close",
                "description": "Close a perspective and release associated locks",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" }
                    },
                    "required": ["agent_id", "perspective_id"]
                }
            },
            // =================================================================
            // L2: Semantic Search
            // =================================================================
            {
                "name": "seek",
                "description": "Intent-aware semantic code search — find code by PURPOSE, not text pattern",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Natural language description of what the agent is looking for" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "top_k": { "type": "integer", "default": 20, "description": "Maximum results to return" },
                        "scope": { "type": "string", "description": "File path prefix to limit search scope" },
                        "node_types": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Filter by node type: function, class, struct, module, file" },
                        "min_score": { "type": "number", "default": 0.1, "description": "Minimum combined score threshold" },
                        "graph_rerank": { "type": "boolean", "default": true, "description": "Whether to run graph re-ranking on embedding candidates" },
                        "token_budget": { "type": "integer", "minimum": 1, "description": "Optional approx context-token budget. m1nd keeps the highest graph-importance hits that fit, drops the rest, and returns a 'budget' block (estimate = chars/4, not exact tokenization)" },
                        "tier": { "type": "string", "enum": ["project", "medulla", "project+medulla", "all-brains"], "description": "Memory-tier for cross-brain recall (pull-not-push). Default project+medulla: this brain's own memory + the shared medulla (promoted/doctrine). 'all-brains' fans out over EVERY hosted brain, each hit labeled origin_brain — the explicit cross-project inspection, never ambient. A claim from another brain only reaches your default beat if it was promoted to the medulla." }
                    },
                    "required": ["query", "agent_id"]
                }
            },
            {
                "name": "focus",
                "description": "Attention runtime — given a GOAL and a token budget, return the minimal focus_set worth loading, an honest account of what was left out (ignored), and an answer-free sufficiency signal (sufficient | gathering | saturated) telling you whether to act, gather more, or re-scope. Use it to decide WHAT context to pull and WHEN you have enough, instead of over-reading.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "goal": { "type": "string", "description": "Natural-language description of the goal you are working toward" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "token_budget": { "type": "integer", "minimum": 1, "default": 2000, "description": "Approx context-token budget for the focus set (estimate = chars/4). The set keeps the highest-salience nodes that fit; the rest are reported under 'ignored'" },
                        "top_k": { "type": "integer", "default": 60, "description": "Upper bound on ranked candidates before budget packing; keep generous so the budget is the real limiter" },
                        "scope": { "type": "string", "description": "File path prefix to limit the focus scope" },
                        "node_types": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Filter by node type: function, class, struct, module, file" },
                        "min_score": { "type": "number", "default": 0.1, "description": "Minimum combined score for a node to be eligible for the focus set" }
                    },
                    "required": ["goal", "agent_id"]
                }
            },
            {
                "name": "scan",
                "description": "Keyword/label pattern scan over graph nodes with curated anti-pattern sets, severity, and optional graph-edge context (graph_validate populates graph_context when edges are present; commonly empty for sparse graphs)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Pattern ID (error_handling, resource_cleanup, api_surface, state_mutation, concurrency, auth_boundary, test_coverage, dependency_injection) or custom pattern" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scan scope" },
                        "severity_min": { "type": "number", "default": 0.3, "description": "Minimum severity threshold [0.0, 1.0]" },
                        "graph_validate": { "type": "boolean", "default": true, "description": "Whether to validate findings against graph edges" },
                        "limit": { "type": "integer", "default": 50, "description": "Maximum findings to return" }
                    },
                    "required": ["pattern", "agent_id"]
                }
            },
            // =================================================================
            // L3: Temporal Intelligence
            // =================================================================
            {
                "name": "timeline",
                "description": "Git-based temporal history for a node — changes, co-changes, velocity, stability",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "node": { "type": "string", "description": "Node external_id (e.g. file::backend/chat_handler.py)" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "depth": { "type": "string", "default": "30d", "description": "Time depth: 7d, 30d, 90d, all" },
                        "include_co_changes": { "type": "boolean", "default": true, "description": "Include co-changed files with coupling scores" },
                        "include_churn": { "type": "boolean", "default": true, "description": "Include lines added/deleted churn data" },
                        "top_k": { "type": "integer", "default": 10, "description": "Max co-change partners to return" }
                    },
                    "required": ["node", "agent_id"]
                }
            },
            {
                "name": "diverge",
                "description": "Detect structural drift between a baseline and current graph state",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "baseline": { "type": "string", "description": "Baseline reference: ISO date, git ref, or last_session" },
                        "scope": { "type": "string", "description": "File path glob to limit scope" },
                        "include_coupling_changes": { "type": "boolean", "default": true, "description": "Include coupling matrix delta" },
                        "include_anomalies": { "type": "boolean", "default": true, "description": "Detect anomalies (test deficits, velocity spikes)" }
                    },
                    "required": ["agent_id", "baseline"]
                }
            },
            // =================================================================
            // L4: Investigation Memory
            // =================================================================
            {
                "name": "trail_save",
                "description": "Persist current investigation state — nodes visited, hypotheses, conclusions",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "label": { "type": "string", "description": "Human-readable label for this investigation" },
                        "hypotheses": { "type": "array", "items": { "type": "object" }, "default": [], "description": "Hypotheses formed during investigation" },
                        "conclusions": { "type": "array", "items": { "type": "object" }, "default": [], "description": "Conclusions reached" },
                        "open_questions": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Open questions remaining" },
                        "tags": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Tags for organization and search" },
                        "summary": { "type": "string", "description": "Optional summary (auto-generated if omitted)" },
                        "visited_nodes": { "type": "array", "items": { "type": "object" }, "default": [], "description": "Explicitly list visited nodes with annotations" },
                        "activation_boosts": { "type": "object", "default": {}, "description": "Map of node_external_id -> boost weight [0.0, 1.0]" }
                    },
                    "required": ["agent_id", "label"]
                }
            },
            {
                "name": "trail_resume",
                "description": "Restore a saved investigation — re-inject activation boosts, detect staleness",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "trail_id": { "type": "string", "description": "Trail ID to resume" },
                        "force": { "type": "boolean", "default": false, "description": "Resume even if trail is stale (>50% missing nodes)" }
                    },
                    "required": ["agent_id", "trail_id"]
                }
            },
            {
                "name": "trail_merge",
                "description": "Combine two or more investigation trails — discover cross-connections",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "trail_ids": { "type": "array", "items": { "type": "string" }, "description": "Two or more trail IDs to merge" },
                        "label": { "type": "string", "description": "Label for the merged trail (auto-generated if omitted)" }
                    },
                    "required": ["agent_id", "trail_ids"]
                }
            },
            {
                "name": "trail_list",
                "description": "List saved investigation trails with optional filters",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "filter_agent_id": { "type": "string", "description": "Filter to a specific agent's trails" },
                        "filter_status": { "type": "string", "description": "Filter by status: active, saved, archived, stale, merged" },
                        "filter_tags": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Filter by tags (any match)" }
                    },
                    "required": ["agent_id"]
                }
            },
            // =================================================================
            // L5: Hypothesis Engine
            // =================================================================
            {
                "name": "hypothesize",
                "description": "Test a structural claim about the codebase — graph-based hypothesis testing",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "claim": { "type": "string", "description": "Natural language claim (e.g. 'chat_handler never validates session tokens')" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "max_hops": { "type": "integer", "default": 5, "description": "Max BFS hops for evidence search" },
                        "include_ghost_edges": { "type": "boolean", "default": true, "description": "Include ghost edges as weak evidence" },
                        "include_partial_flow": { "type": "boolean", "default": true, "description": "Include partial flow when full path not found" },
                        "path_budget": { "type": "integer", "default": 1000, "description": "Budget cap for all-paths enumeration" }
                    },
                    "required": ["claim", "agent_id"]
                }
            },
            {
                "name": "differential",
                "description": "Focused structural diff between two graph snapshots",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "snapshot_a": { "type": "string", "description": "Path to snapshot A, or 'current'" },
                        "snapshot_b": { "type": "string", "description": "Path to snapshot B, or 'current'" },
                        "question": { "type": "string", "description": "Focus question to narrow the diff output" },
                        "focus_nodes": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Limit diff to neighborhood of specific nodes" }
                    },
                    "required": ["agent_id", "snapshot_a", "snapshot_b"]
                }
            },
            // =================================================================
            // L6: Execution Feedback
            // =================================================================
            {
                "name": "trace",
                "description": "Map runtime errors to structural root causes via stacktrace analysis",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "error_text": { "type": "string", "description": "Full error output (stacktrace + error message)" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "language": { "type": "string", "description": "Language hint: python, rust, typescript, javascript, go (auto-detected if omitted)" },
                        "window_hours": { "type": "number", "default": 24.0, "description": "Temporal window (hours) for co-change suspect scan" },
                        "top_k": { "type": "integer", "default": 10, "description": "Max suspects to return" }
                    },
                    "required": ["error_text", "agent_id"]
                }
            },
            {
                "name": "validate_plan",
                "description": "Validate a modification plan against the code graph — detect gaps and risk",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "actions": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "action_type": { "type": "string", "description": "modify, create, delete, rename, or test" },
                                    "file_path": { "type": "string", "description": "Relative file path" },
                                    "description": { "type": "string" },
                                    "depends_on": { "type": "array", "items": { "type": "string" }, "default": [] }
                                },
                                "required": ["action_type", "file_path"]
                            },
                            "description": "Ordered list of planned actions"
                        },
                        "include_test_impact": { "type": "boolean", "default": true, "description": "Analyze test coverage for modified files" },
                        "include_risk_score": { "type": "boolean", "default": true, "description": "Compute composite risk score" },
                        "scope": { "type": "string", "description": "Optional repo or scope path for multi-repo binding diagnostics" }
                    },
                    "required": ["agent_id", "actions"]
                }
            },
            // =================================================================
            // L7: Multi-Repository Federation
            // =================================================================
            {
                "name": "federate",
                "description": "Ingest multiple repos into a unified federated graph with cross-repo edge detection",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "repos": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string", "description": "Repository name (namespace prefix)" },
                                    "path": { "type": "string", "description": "Absolute path to repository root" },
                                    "adapter": { "type": "string", "default": "code", "description": "Ingest adapter override" }
                                },
                                "required": ["name", "path"]
                            },
                            "description": "List of repositories to federate"
                        },
                        "detect_cross_repo_edges": { "type": "boolean", "default": true, "description": "Auto-detect cross-repo edges" },
                        "incremental": { "type": "boolean", "default": false, "description": "Only re-ingest repos that changed" }
                    },
                    "required": ["agent_id", "repos"]
                }
            },
            // =================================================================
            // Superpowers: Antibody / Flow / Epidemic / Tremor / Trust / Layers
            // =================================================================
            {
                "name": "antibody_scan",
                "description": "Scan code graph against stored bug antibodies (immune memory patterns). Returns matches where known bug patterns recur.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "default": "all", "description": "\"all\" = entire graph, \"changed\" = nodes since last scan" },
                        "antibody_ids": { "type": "array", "items": { "type": "string" }, "description": "Optional: only scan specific antibodies" },
                        "max_matches": { "type": "integer", "default": 50, "description": "Maximum matches to return" },
                        "min_severity": { "type": "string", "default": "info", "description": "Minimum severity: info, warning, critical" },
                        "similarity_threshold": { "type": "number", "default": 0.7, "description": "Fuzzy match threshold for label matching (0.0-1.0)" },
                        "match_mode": { "type": "string", "default": "substring", "description": "Label match mode: exact, substring, regex" },
                        "max_matches_per_antibody": { "type": "integer", "default": 50, "description": "Maximum matches per individual antibody" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "antibody_list",
                "description": "List all stored bug antibodies with metadata, match history, and specificity scores.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "include_disabled": { "type": "boolean", "default": false, "description": "Include disabled antibodies" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "antibody_create",
                "description": "Create, disable, enable, or delete a bug antibody pattern.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "action": { "type": "string", "default": "create", "description": "Action: create, disable, enable, delete" },
                        "antibody_id": { "type": "string", "description": "Required for disable/enable/delete" },
                        "name": { "type": "string", "description": "Antibody name (for create)" },
                        "description": { "type": "string", "description": "What this pattern detects" },
                        "severity": { "type": "string", "default": "warning", "description": "info, warning, critical" },
                        "pattern": { "type": "object", "description": "Pattern definition with nodes/edges/negative_edges" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "flow_simulate",
                "description": "Simulate concurrent execution flow. Detects race conditions via particle collision on shared mutable state without synchronization.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "entry_nodes": { "type": "array", "items": { "type": "string" }, "description": "Starting nodes. Auto-discovered if empty." },
                        "num_particles": { "type": "integer", "default": 2, "description": "Particles per entry point" },
                        "lock_patterns": { "type": "array", "items": { "type": "string" }, "description": "Regex patterns for lock/mutex detection" },
                        "read_only_patterns": { "type": "array", "items": { "type": "string" }, "description": "Regex patterns for read-only operations" },
                        "max_depth": { "type": "integer", "default": 15, "description": "Maximum BFS depth" },
                        "turbulence_threshold": { "type": "number", "default": 0.5, "description": "Minimum score to report" },
                        "include_paths": { "type": "boolean", "default": true, "description": "Include particle paths in output" },
                        "max_total_steps": { "type": "integer", "default": 50000, "description": "Global step budget across all particles" },
                        "scope_filter": { "type": "string", "description": "Substring filter to limit which nodes particles can enter" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "epidemic",
                "description": "Predict bug propagation via SIR epidemiological model. Given known buggy modules, predicts which neighbors are most likely to harbor undiscovered bugs.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "infected_nodes": { "type": "array", "items": { "type": "string" }, "description": "Known buggy node IDs" },
                        "recovered_nodes": { "type": "array", "items": { "type": "string" }, "description": "Already-fixed node IDs" },
                        "infection_rate": { "type": "number", "description": "Uniform infection rate. If omitted, derived from edge weights." },
                        "recovery_rate": { "type": "number", "default": 0, "description": "SIR recovery rate" },
                        "iterations": { "type": "integer", "default": 50, "description": "Simulation iterations" },
                        "direction": { "type": "string", "default": "both", "description": "Propagation direction: forward, backward, both" },
                        "top_k": { "type": "integer", "default": 20, "description": "Max predictions to return" },
                        "auto_calibrate": { "type": "boolean", "default": true, "description": "Auto-adjust infection_rate based on graph density" },
                        "scope": { "type": "string", "default": "all", "description": "Filter predictions: files, functions, all" },
                        "min_probability": { "type": "number", "default": 0.001, "description": "Filter out predictions below this probability" }
                    },
                    "required": ["agent_id", "infected_nodes"]
                }
            },
            {
                "name": "tremor",
                "description": "Detect code tremors: modules with accelerating change frequency (second derivative). Earthquake precursor analogy for imminent bugs.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "window": { "type": "string", "default": "30d", "description": "Time window: 7d, 30d, 90d, all" },
                        "threshold": { "type": "number", "default": 0.1, "description": "Minimum magnitude to report" },
                        "top_k": { "type": "integer", "default": 20, "description": "Max results" },
                        "node_filter": { "type": "string", "description": "Filter to nodes matching this prefix" },
                        "include_history": { "type": "boolean", "default": false, "description": "Include observation history" },
                        "min_observations": { "type": "integer", "default": 3, "description": "Minimum data points to compute tremor" },
                        "sensitivity": { "type": "number", "default": 1.0, "description": "Multiplier on acceleration threshold (higher = more sensitive)" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "trust",
                "description": "Per-module trust scores from defect history. Actuarial risk assessment: more confirmed bugs = lower trust = higher risk.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "default": "file", "description": "Node type scope: file, function, class, all" },
                        "min_history": { "type": "integer", "default": 1, "description": "Minimum learn events for inclusion" },
                        "top_k": { "type": "integer", "default": 20, "description": "Max results" },
                        "node_filter": { "type": "string", "description": "Filter to nodes matching this prefix" },
                        "sort_by": { "type": "string", "default": "trust_asc", "description": "Sort: trust_asc, trust_desc, defects_desc, recency" },
                        "decay_half_life_days": { "type": "number", "default": 30.0, "description": "How fast old defects lose weight (days)" },
                        "risk_cap": { "type": "number", "default": 3.0, "description": "Maximum risk multiplier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "layers",
                "description": "Auto-detect architectural layers from graph topology. Returns layer assignments plus dependency violations (edges going against expected flow).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "max_layers": { "type": "integer", "default": 8, "description": "Maximum layers to detect" },
                        "include_violations": { "type": "boolean", "default": true, "description": "Include violation analysis" },
                        "min_nodes_per_layer": { "type": "integer", "default": 2, "description": "Minimum nodes for a layer to be reported" },
                        "node_types": { "type": "array", "items": { "type": "string" }, "description": "Filter by node types" },
                        "naming_strategy": { "type": "string", "default": "auto", "description": "Layer naming: auto, path_prefix, pagerank" },
                        "exclude_tests": { "type": "boolean", "default": false, "description": "Exclude test files from layer detection" },
                        "violation_limit": { "type": "integer", "default": 100, "description": "Maximum violations to return" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "layer_inspect",
                "description": "Inspect a specific architectural layer: nodes, connections, violations, and health metrics.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "level": { "type": "integer", "description": "Layer level to inspect" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "include_edges": { "type": "boolean", "default": true, "description": "Include inter-layer edges" },
                        "top_k": { "type": "integer", "default": 50, "description": "Max nodes to return per layer" }
                    },
                    "required": ["agent_id", "level"]
                }
            },
            // =================================================================
            // RETROBUILDER modules — temporal edges, taint, twins, refactors,
            // and runtime overlays
            // =================================================================
            {
                "name": "ghost_edges",
                "description": "Parse git history and surface temporal co-change ghost edges between files that move together without explicit static dependencies.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "depth": { "type": "string", "default": "30d", "description": "Git history window: 7d, 30d, 90d, all" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "top_k": { "type": "integer", "default": 50, "description": "Maximum ghost edges to return" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "calibrate_predict",
                "description": "OMEGA Move 0: calibrate predict/co-change from this repo's own git history. Date-splits commits, measures precision-at-coverage on held-out commits, derives a split-conformal threshold τ against a risk budget α, and persists it so predict can gate each result with an act|reverify|abstain verdict.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "alpha": { "type": "number", "default": 0.1, "description": "Operator risk budget (target miscoverage), e.g. 0.1 = accept up to 10% error among act-gated predictions" },
                        "top_k": { "type": "integer", "default": 10, "description": "Top-k predictions to score per held-out node" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "calibrate_envelope",
                "description": "OMEGA Move 1: calibrate the seek TRUST ENVELOPE from the trust ledger's real learn outcomes. Each node with learn history is a label — a confirmed defect (learn `correct`) means trusting it would have been WRONG (a miss), a false alarm (learn `wrong`) means trusting it was RIGHT (a hit) — scored by the reliability the envelope assigns its trust band. Derives a split-conformal τ (on the envelope's own [0,1] scale) + precision-at-coverage and persists it under the `envelope` signal, so the seek envelope can reach `act`. With no labeled ledger corpus it stays honestly uncalibrated (reason `envelope_uncalibrated`, capped at `reverify`), never a fabricated `act`.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "alpha": { "type": "number", "default": 0.1, "description": "Operator risk budget (target miscoverage), e.g. 0.1 = accept up to 10% error among act-gated envelope decisions" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "taint_trace",
                "description": "Inject taint at entry points and trace propagation through the graph to detect missed validation, auth, or sanitization boundaries.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "entry_nodes": { "type": "array", "items": { "type": "string" }, "description": "Entry point node IDs to inject taint" },
                        "taint_type": { "type": "string", "default": "user_input", "description": "Taint type: user_input, sensitive_data, or custom" },
                        "boundary_patterns": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Custom boundary patterns when taint_type=custom" },
                        "max_depth": { "type": "integer", "default": 15, "description": "Maximum propagation depth" },
                        "min_probability": { "type": "number", "default": 0.01, "description": "Minimum propagation probability to report" }
                    },
                    "required": ["agent_id", "entry_nodes"]
                }
            },
            {
                "name": "twins",
                "description": "Find structurally similar or identical nodes via topological signature similarity.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "similarity_threshold": { "type": "number", "default": 0.80, "description": "Minimum cosine similarity threshold" },
                        "top_k": { "type": "integer", "default": 50, "description": "Maximum twin pairs to return" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "node_types": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Optional node type filter" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "refactor_plan",
                "description": "Propose graph-native refactoring communities and extraction candidates for a scoped region of the codebase.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "max_communities": { "type": "integer", "default": 10, "description": "Maximum communities to consider" },
                        "min_community_size": { "type": "integer", "default": 3, "description": "Minimum nodes for an extractable community" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "runtime_overlay",
                "description": "Overlay OpenTelemetry span activity onto the graph to paint runtime heat, latency, and error signals onto nodes.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "spans": { "type": "array", "items": { "type": "object" }, "description": "OTel spans to ingest" },
                        "service_name": { "type": "string", "default": "", "description": "Optional service name for scoping" },
                        "mapping_strategy": { "type": "string", "default": "label_match", "description": "Mapping strategy: label_match, code_attribute, exact_id" },
                        "boost_strength": { "type": "number", "default": 0.15, "description": "Activation boost strength" }
                    },
                    "required": ["agent_id", "spans"]
                }
            },
            // =================================================================
            // Surgical: context + apply
            // =================================================================
            {
                "name": "heuristics_surface",
                "description": "Return an explicit explainability surface for a code target, showing why heuristics ranked it as risky or important.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "node_id": { "type": "string", "description": "Graph node ID to inspect" },
                        "file_path": { "type": "string", "description": "Absolute or workspace-relative path to inspect" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "surgical_context",
                "description": "Return full context for surgical LLM editing: file contents, symbols, and graph neighbourhood (callers, callees, tests). Use before apply.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string", "description": "Absolute or workspace-relative path to the file being edited" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "symbol": { "type": "string", "description": "Optional: narrow context to a specific symbol (function/struct/class name)" },
                        "radius": { "type": "integer", "default": 1, "description": "BFS radius for graph neighbourhood (1 or 2)" },
                        "include_tests": { "type": "boolean", "default": true, "description": "Include test files in the neighbourhood" }
                    },
                    "required": ["file_path", "agent_id"]
                }
            },
            {
                "name": "apply",
                "description": "Write LLM-edited code back to a file and trigger incremental re-ingest so the graph stays coherent. Always call surgical_context first.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string", "description": "Absolute or workspace-relative path of the file to overwrite" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "new_content": { "type": "string", "description": "New file contents (full replacement, UTF-8)" },
                        "description": { "type": "string", "description": "Human-readable description of the edit" },
                        "reingest": { "type": "boolean", "default": true, "description": "Re-ingest the file after writing (recommended)" }
                    },
                    "required": ["file_path", "agent_id", "new_content"]
                }
            },
            // =================================================================
            // View: lightweight file reader
            // =================================================================
            {
                "name": "view",
                "description": "Fast file reader with line numbers. Replaces View/cat/head/tail. No graph traversal — just reads the file. Auto-ingests if not in graph. Use for quick file inspection before surgical_context.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string", "description": "Absolute or workspace-relative path to the file" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "offset": { "type": "integer", "default": 0, "description": "Start line (0-based)" },
                        "limit": { "type": "integer", "description": "Max lines to return (default: all)" },
                        "auto_ingest": { "type": "boolean", "default": true, "description": "Auto-ingest file into graph if not present" },
                        "max_output_chars": { "type": "integer", "description": "Optional cap for returned characters after line-number formatting" }
                    },
                    "required": ["file_path", "agent_id"]
                }
            },
            {
                "name": "batch_view",
                "description": "Read multiple files or glob patterns in one call with stable delimiters, optional summaries, and auto-ingest.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "files": { "type": "array", "items": { "type": "string" }, "description": "File paths and/or glob-like patterns to expand" },
                        "max_lines_per_file": { "type": "integer", "default": 100, "description": "Maximum lines to return per file" },
                        "summary_mode": { "type": "boolean", "default": true, "description": "Add an inline summary for each returned file" },
                        "auto_ingest": { "type": "boolean", "default": true, "description": "Auto-ingest discovered files before reading" },
                        "max_output_chars": { "type": "integer", "description": "Optional cap for the concatenated response body" }
                    },
                    "required": ["agent_id", "files"]
                }
            },
            // =================================================================
            // Surgical V2: context_v2 + apply_batch
            // =================================================================
            {
                "name": "surgical_context_v2",
                "description": "Get full surgical context for a file PLUS source code of connected files (callers, callees, tests). Returns a complete workspace snapshot in one call. Superset of surgical_context.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "file_path": { "type": "string", "description": "Absolute or workspace-relative path to the primary file" },
                        "symbol": { "type": "string", "description": "Optional: narrow context to a specific symbol (function/struct/class name)" },
                        "include_tests": { "type": "boolean", "default": true, "description": "Include test files in the neighbourhood" },
                        "radius": { "type": "integer", "default": 1, "description": "BFS radius for graph neighbourhood (1 or 2)" },
                        "max_connected_files": { "type": "integer", "default": 5, "description": "Maximum number of connected files to include source for" },
                        "max_lines_per_file": { "type": "integer", "default": 60, "description": "Maximum lines per connected file (primary file is unbounded)" }
                    },
                    "required": ["agent_id", "file_path"]
                }
            },
            {
                "name": "apply_batch",
                "description": "Atomically write multiple files and trigger a single bulk re-ingest. Use after surgical_context_v2 when editing a file and its callers/tests together. All-or-nothing by default.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "edits": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "file_path": { "type": "string", "description": "Absolute or workspace-relative path of the file to write" },
                                    "new_content": { "type": "string", "description": "New file contents (full replacement, UTF-8)" },
                                    "description": { "type": "string", "description": "Optional human-readable label for this edit" }
                                },
                                "required": ["file_path", "new_content"]
                            },
                            "description": "List of file edits to apply"
                        },
                        "atomic": { "type": "boolean", "default": true, "description": "All-or-nothing: if any file fails, none are written" },
                        "reingest": { "type": "boolean", "default": true, "description": "Re-ingest all modified files after writing" },
                        "verify": { "type": "boolean", "default": false, "description": "Run post-write verification after the batch finishes" }
                    },
                    "required": ["agent_id", "edits"]
                }
            },
            {
                "name": "edit_preview",
                "description": "Build an in-memory preview of a single-file full-replacement edit. Returns a preview handle, source snapshot, diff, and validation report. Does not touch disk.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "file_path": { "type": "string", "description": "Absolute or workspace-relative path of the file to preview" },
                        "new_content": { "type": "string", "description": "Candidate file contents (full replacement, UTF-8)" },
                        "description": { "type": "string", "description": "Optional human-readable description of the preview" }
                    },
                    "required": ["agent_id", "file_path", "new_content"]
                }
            },
            {
                "name": "edit_commit",
                "description": "Commit a previously created edit_preview handle after re-checking source freshness. Persists atomically through the existing apply path.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "preview_id": { "type": "string", "description": "Preview handle returned by edit_preview" },
                        "confirm": { "type": "boolean", "default": false, "description": "Must be true to confirm the commit. Safety guard against accidental writes." },
                        "reingest": { "type": "boolean", "default": true, "description": "Re-ingest the modified file after commit" }
                    },
                    "required": ["agent_id", "preview_id", "confirm"]
                }
            },
            {
                "name": "transplant",
                "description": "Move a top-level Rust `fn` between files of the same crate BY REFERENCE: you name the symbol and two paths, the server computes everything from the graph — widened extent (doc comments and attributes travel), dependency trichotomy from call edges (private deps travel; shared deps stay, gain pub(crate) and a back-import), referencers re-qualified across every file that names it — then writes atomically, re-ingests, and returns an honest receipt (refs_unresolved is never silently empty-when-wrong; state_left_behind names node-addressed state the re-ingest orphaned). Refusals never touch a byte and TEACH the retry: a collision names the occupant, a poisonous stem (lib/main/mod) names the invalid module path, a cross-crate move names both crate roots. v1 boundaries: top-level fn only, module = file stem, same crate, destination file must already exist, macro-generated references are invisible. See docs/TRANSPLANT-PRD.md.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "symbol": { "type": "string", "description": "Bare name of the top-level fn to move (matches the graph node label)" },
                        "source_file": { "type": "string", "description": "File the symbol currently lives in (absolute or workspace-relative)" },
                        "dest_file": { "type": "string", "description": "File the symbol is moved into; must already exist and share the source's crate root" },
                        "allow_protected": { "type": "string", "description": "Explicit reason for crossing a ci/protected-zones.json path. Omitted (the default) means a zone match refuses instead of writing; the reason is recorded in the receipt when it unlocks a crossing." }
                    },
                    "required": ["agent_id", "symbol", "source_file", "dest_file"]
                }
            },
            {
                "name": "transplant_preview",
                "description": "Stage a transplant WITHOUT touching disk. Takes the same inputs as transplant and computes the whole plan — every new file content, referencer discovery, the rustfmt pass and the candidate receipt — then returns a preview_id (5-minute TTL) plus the per-file plan with each file's base hash and line deltas. Redeem it with transplant_commit.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "symbol": { "type": "string", "description": "Bare name of the top-level fn to move (matches the graph node label)" },
                        "source_file": { "type": "string", "description": "File the symbol currently lives in (absolute or workspace-relative)" },
                        "dest_file": { "type": "string", "description": "File the symbol is moved into; must already exist and share the source's crate root" },
                        "allow_protected": { "type": "string", "description": "Explicit reason for crossing a ci/protected-zones.json path; same law as transplant" }
                    },
                    "required": ["agent_id", "symbol", "source_file", "dest_file"]
                }
            },
            {
                "name": "transplant_commit",
                "description": "Land a staged transplant_preview after re-validating the on-disk hash of EVERY planned file — source, destination and each derived referencer. Any drift since the preview refuses the commit as stale and writes nothing; otherwise the plan lands atomically and returns the finalized receipt.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "preview_id": { "type": "string", "description": "Handle returned by transplant_preview (expires after 5 minutes)" },
                        "confirm": { "type": "boolean", "default": false, "description": "Must be true to land the staged plan. Safety guard against accidental writes." }
                    },
                    "required": ["agent_id", "preview_id", "confirm"]
                }
            },
            // =================================================================
            // v0.4.0: search, help, report, panoramic
            // =================================================================
            {
                "name": "search",
                "description": "Unified code search: literal, regex (with multiline), or semantic. Searches graph node labels AND file contents on disk. Supports invert (grep -v), count-only (grep -c), multiline regex (rg -U), and filename pattern filtering (grep --include). v0.5.0: regex mode now searches file contents (not just node IDs).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "query": { "type": "string", "description": "Search query string" },
                        "mode": {
                            "type": "string",
                            "enum": ["literal", "regex", "semantic"],
                            "default": "literal",
                            "description": "Search mode: literal (substring), regex (pattern), semantic (graph-aware)"
                        },
                        "scope": { "type": "string", "description": "File path prefix filter" },
                        "top_k": { "type": "integer", "default": 50, "description": "Max results (1-500)" },
                        "context_lines": { "type": "integer", "default": 2, "description": "Lines of context before/after match (0-10)" },
                        "case_sensitive": { "type": "boolean", "default": false, "description": "Case-sensitive matching" },
                        "invert": { "type": "boolean", "default": false, "description": "Return lines that DON'T match (grep -v)" },
                        "count_only": { "type": "boolean", "default": false, "description": "Return just the count, no results (grep -c)" },
                        "multiline": { "type": "boolean", "default": false, "description": "Enable multiline regex: dot matches newline (rg -U). Only for regex mode." },
                        "auto_ingest": { "type": "boolean", "default": false, "description": "Auto-ingest exactly one resolved scope path outside current ingest roots before searching; ambiguous scopes return an error that lists candidate paths in detail" },
                        "filename_pattern": { "type": "string", "description": "Glob pattern to filter filenames (e.g. '*.rs', 'test_*.py')" },
                        "max_output_chars": { "type": "integer", "description": "Optional cap for total returned characters across serialized matches" },
                        "token_budget": { "type": "integer", "minimum": 1, "description": "Optional approx context-token budget. m1nd keeps the highest-ranked rows that fit, drops the rest, and returns a 'budget' block (estimate = chars/4, not exact tokenization; ignored when count_only)" }
                    },
                    "required": ["agent_id", "query"]
                }
            },
            {
                "name": "glob",
                "description": "Graph-aware file glob: find files in the ingested graph by glob pattern. Zero I/O — pure graph query. Replaces find/glob for indexed codebases.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "pattern": { "type": "string", "description": "Glob pattern (e.g. '**/*.rs', 'src/**/mod.rs', '*.toml')" },
                        "scope": { "type": "string", "description": "Root directory prefix to narrow scope" },
                        "top_k": { "type": "integer", "default": 200, "description": "Max results (1-10000)" },
                        "sort": {
                            "type": "string",
                            "enum": ["path", "activation"],
                            "default": "path",
                            "description": "Sort order: path (alphabetical) or activation (most connected first)"
                        }
                    },
                    "required": ["agent_id", "pattern"]
                }
            },
            {
                "name": "scan_all",
                "description": "Run all structural scan patterns in one call and return grouped findings by pattern.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "severity_min": { "type": "number", "default": 0.3, "description": "Minimum severity threshold across all patterns" },
                        "graph_validate": { "type": "boolean", "default": true, "description": "Whether to validate findings against graph edges" },
                        "limit_per_pattern": { "type": "integer", "default": 50, "description": "Maximum findings per pattern" },
                        "patterns": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Optional subset of patterns to run; empty means all built-ins" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "cross_verify",
                "description": "Compare graph state against disk truth: missing files, LOC drift, hash mismatches, and evidence_freshness (flags memorized L1GHT claims whose cited code changed). Empty check = run all.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "check": { "type": "array", "items": { "type": "string", "enum": ["existence", "loc", "hash", "evidence_freshness"] }, "default": [], "description": "Checks to run (empty = all): existence, loc, hash, evidence_freshness. evidence_freshness reports stale_evidence — memorize claims whose grounded_in code changed since ingest." },
                        "include_dotfiles": { "type": "boolean", "default": false, "description": "Include selected dotfiles while verifying disk state" },
                        "dotfile_patterns": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Allowed dotfile patterns when include_dotfiles=true" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "soul_check",
                "description": "Verify the SOUL (the project's PATHOS handoff): parse it into anchored claims, classify each (path/line-hint/symbol/git/consistency/receipt/runtime/declared), verify per class, and emit the honesty report + one-line FRESHNESS RECEIPT (N fresh / M stale / K receipt-priced, dated @sha). The two tissues hold: declared tissue (taste/doctrine) is UNPROVABLE-but-curated, never fake-verified. Read-only. Pass verify_curator_report to run the §C8.4 seat check (a curator's output must pass by a DIFFERENT agent — grader ≠ author).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "soul_path": { "type": "string", "description": "Soul document path (default discovery: docs/PATHOS.md then PATHOS.md at the repo root)" },
                        "verify_curator_report": { "type": "object", "description": "A curator report to seat-verify (ORGANISM §C8.4): checks grader ≠ its curated_by, never-silent-prune, declared-tissue lock, and the still_stale honesty valve. When set, no document is re-parsed." }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "soul_read",
                "description": "Pull the SOUL (PATHOS) body — whole or one section — plus its headline. The explicit pull surface behind the pull-not-push law: the body is never ambient. Run soul_check for the freshness receipt.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "soul_path": { "type": "string", "description": "Soul document path (default discovery order)" },
                        "section": { "type": "string", "description": "Return only this section (case-insensitive substring on the '## <Heading>'); absent = whole document" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "coverage_session",
                "description": "Report what the current agent session has and has not visited across files, nodes, and tool usage.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "external_references",
                "description": "Scan graph-tracked files for explicit references to paths outside the current ingest roots.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "federate_auto",
                "description": "Discover candidate external repositories from the current workspace and optionally federate them in one step.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit discovery sources" },
                        "current_repo_name": { "type": "string", "description": "Optional namespace override for the current workspace inside the federated graph" },
                        "max_repos": { "type": "integer", "default": 8, "description": "Maximum discovered external repos to include" },
                        "detect_cross_repo_edges": { "type": "boolean", "default": true, "description": "Whether a follow-up federate execution should auto-detect cross-repo edges" },
                        "execute": { "type": "boolean", "default": false, "description": "When true, immediately run federate with the current repo plus discovered candidates" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "help",
                "description": "Context-aware help for m1nd tools. Returns tool doctrine, route guidance, workflow sequences, or recovery guidance for agents.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "tool_name": { "type": "string", "description": "Specific tool name for detailed help (omit for overview routing)" },
                        "mode": {
                            "type": "string",
                            "enum": ["overview", "tool", "route", "recovery", "workflow"],
                            "description": "Help mode: overview, tool, route, recovery, or workflow"
                        },
                        "intent": { "type": "string", "description": "Short statement of what the agent is trying to do" },
                        "stage": {
                            "type": "string",
                            "enum": ["orient", "find", "ground", "diagnose", "plan", "edit", "review", "operate", "handoff"],
                            "description": "Current working stage for the agent"
                        },
                        "path": { "type": "string", "description": "Current path or target in focus when known" },
                        "error_text": { "type": "string", "description": "Observed error text, stacktrace, or failure summary" },
                        "recent_tools": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Tools already used in the current flow"
                        },
                        "max_suggestions": { "type": "integer", "default": 3, "description": "Maximum ranked suggestions to return in route or recovery mode" },
                        "render": {
                            "type": "string",
                            "enum": ["full", "compact", "none"],
                            "default": "full",
                            "description": "Render mode for formatted help text"
                        }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "mission_start",
                "description": "Start a bounded agent mission with route, budget envelope, starter moves, and non-claims.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "repo": { "type": "string", "description": "Absolute or host-resolved repository path this mission is scoped to" },
                        "task": { "type": "string", "description": "Mission task in plain language" },
                        "mode": {
                            "type": "string",
                            "enum": ["bug_hunt", "review", "refactor", "docs_drift", "architecture", "release"],
                            "default": "review",
                            "description": "Mission mode"
                        },
                        "budget": {
                            "type": "string",
                            "enum": ["short", "normal", "deep"],
                            "default": "normal",
                            "description": "Mission budget envelope"
                        },
                        "risk": {
                            "type": "string",
                            "enum": ["low", "medium", "high"],
                            "default": "medium",
                            "description": "Risk level for routing"
                        },
                        "parent_mission_id": { "type": "string", "description": "Optional parent mission id for handoff or sub-mission tracking" },
                        "evidence_link": {
                            "type": "object",
                            "additionalProperties": false,
                            "description": "Optional owner-emitted G5 correlation link. Mission Control accepts it only when the exact G3 anchor already exists; the record cannot create mission authority.",
                            "properties": {
                                "schema": { "const": crate::evidence_spine::EVIDENCE_CORRELATION_LINK_SCHEMA },
                                "mission_id": { "type": "string" },
                                "iteration_id": { "type": "integer", "minimum": 1 },
                                "mission_head_id": { "type": "string" },
                                "transaction_id": { "type": ["string", "null"] }
                            },
                            "required": ["schema", "mission_id", "iteration_id", "mission_head_id", "transaction_id"]
                        }
                    },
                    "required": ["agent_id", "repo", "task"]
                }
            },
            {
                "name": "mission_next",
                "description": "Append the latest mission event and return exactly one recommended next move plus do-not guardrails.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "mission_id": { "type": "string", "description": "Mission id returned by mission_start" },
                        "last_event": {
                            "type": "object",
                            "description": "Optional event from the action just taken, such as graph_query, file_read, test_run, or dissent"
                        }
                    },
                    "required": ["agent_id", "mission_id"]
                }
            },
            {
                "name": "mission_event",
                "description": "Record one observed mission action with evidence class, event id, and local digest.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "mission_id": { "type": "string", "description": "Mission id returned by mission_start" },
                        "event": {
                            "type": ["object", "string"],
                            "description": "Observed action, such as file_read, test_run, graph_query, dissent, or coverage_sweep"
                        },
                        "payload": {
                            "description": "Optional structured evidence payload for string-style events"
                        },
                        "outcome": {
                            "type": "string",
                            "description": "Optional observed outcome, such as hypothesis_supported or inconclusive"
                        },
                        "agent_confidence": {
                            "type": "number",
                            "description": "Optional caller confidence captured as telemetry, not proof"
                        }
                    },
                    "required": ["agent_id", "mission_id", "event"]
                }
            },
            {
                "name": "mission_verify",
                "description": "Verify whether a mission claim has enough direct evidence; graph-only evidence is rejected.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "mission_id": { "type": "string", "description": "Mission id returned by mission_start" },
                        "claim": { "type": "string", "description": "Candidate conclusion to validate" },
                        "evidence_refs": {
                            "type": "array",
                            "items": { "type": "string" },
                            "default": [],
                            "description": "Evidence references such as file_read:path:line, test_run:name, compiler:error, or runtime_probe:id"
                        },
                        "confidence": { "type": "number", "description": "Optional agent confidence before verification" }
                    },
                    "required": ["agent_id", "mission_id", "claim"]
                }
            },
            {
                "name": "mission_handoff",
                "description": "Serialize a resumable mission handoff with verified claims, open hypotheses, dead paths, graph anchors, and next move.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "mission_id": { "type": "string", "description": "Mission id returned by mission_start" },
                        "summary": { "type": "string", "description": "Optional handoff summary" },
                        "recipient_agent_id": { "type": "string", "description": "Optional recipient agent id" },
                        "include_events": { "type": "boolean", "default": false, "description": "Include full event stream in the handoff packet" }
                    },
                    "required": ["agent_id", "mission_id"]
                }
            },
            {
                "name": "mission_close",
                "description": "Close a mission with a proof packet containing verified claims, rejected claims, events, gaps, and non-claims.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "mission_id": { "type": "string", "description": "Mission id returned by mission_start" },
                        "summary": { "type": "string", "description": "Optional concise mission summary" },
                        "non_claims": {
                            "type": "array",
                            "items": { "type": "string" },
                            "default": [],
                            "description": "Extra non-claims to preserve in the proof packet"
                        },
                        "gaps": {
                            "type": "array",
                            "items": { "type": "string" },
                            "default": [],
                            "description": "Known remaining gaps"
                        },
                        "write_light_memory": {
                            "type": "boolean",
                            "default": false,
                            "description": "If true, persist the mission's verified claims as L1GHT memory (.light.md, anchored to code, auto-loads next session). Path returned under light_memory."
                        }
                    },
                    "required": ["agent_id", "mission_id"]
                }
            },
            {
                "name": "report",
                "description": "Session intelligence report: query counts, elapsed time, graph size, and the highest-risk heuristic hotspots in the current graph.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "max_output_chars": { "type": "integer", "description": "Optional cap for markdown summary size" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "audit",
                "description": "Profile-aware one-call audit for topology, scans, verification, git state, and recommendations.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "path": { "type": "string", "description": "Root path to audit" },
                        "profile": { "type": "string", "default": "auto", "description": "Audit profile: auto, quick, coordination, production, security, migration" },
                        "depth": { "type": "string", "default": "full", "description": "Audit depth: quick, surface, full" },
                        "cross_verify": { "type": "boolean", "default": true, "description": "Compare graph vs filesystem state" },
                        "include_git": { "type": "boolean", "default": true, "description": "Include git state and recent history" },
                        "include_config": { "type": "boolean", "default": false, "description": "Include selected dotfiles/config directories" },
                        "scan_patterns": { "type": "string", "default": "all", "description": "Scan selection: all, default, or a comma-separated list" },
                        "external_refs": { "type": "boolean", "default": true, "description": "Discover explicit external path references" },
                        "report_format": { "type": "string", "default": "markdown", "description": "Output format: markdown or json" },
                        "max_output_chars": { "type": "integer", "description": "Optional cap for returned narrative/report size" }
                    },
                    "required": ["agent_id", "path"]
                }
            },
            {
                "name": "daemon_start",
                "description": "Arm the per-brain code daemon: persist watched paths and advance freshness when seen — ticks ride verb traffic (and, on a stdio owner, watch events / the idle clock); this is not a free-running monitor.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "watch_paths": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Paths the daemon should treat as watched roots" },
                        "poll_interval_ms": { "type": "integer", "default": 500, "description": "Fallback polling interval in milliseconds" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "daemon_stop",
                "description": "Stop persisted daemon state without deleting alerts or runtime state.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "daemon_status",
                "description": "Report daemon state, watched paths, alert counts, and generation counters.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "daemon_tick",
                "description": "Poll watched roots once, incrementally re-ingest changed files, and surface drift alerts for deleted files.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "max_files": { "type": "integer", "default": 32, "description": "Maximum changed files to process in one tick" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "alerts_list",
                "description": "List persisted daemon/proactive alerts.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "include_acked": { "type": "boolean", "default": false, "description": "Include acknowledged alerts" },
                        "limit": { "type": "integer", "default": 50, "description": "Maximum number of alerts to return" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "alerts_ack",
                "description": "Acknowledge one or more daemon/proactive alerts.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "alert_ids": { "type": "array", "items": { "type": "string" }, "description": "Alert IDs to acknowledge" }
                    },
                    "required": ["agent_id", "alert_ids"]
                }
            },
            {
                "name": "panoramic",
                "description": "Panoramic graph health overview: per-module risk scores combining blast radius, centrality, and churn signals.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix filter" },
                        "top_n": { "type": "integer", "default": 50, "description": "Max modules to return (1-1000)" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "persist",
                "description": "Persist/load graph and plasticity state; supports binary snapshots",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "action": { "type": "string", "enum": ["save", "load", "checkpoint", "status"], "description": "Action to perform" },
                        "format": { "type": "string", "enum": ["json", "bin"], "default": "json", "description": "Snapshot format" }
                    },
                    "required": ["agent_id", "action"]
                }
            },
            {
                "name": "boot_memory",
                "description": "Read the migrated legacy Boot KV projection. set/delete are retained only as explicit compatibility tombstones and refuse after migration; use typed Boot Config for configuration or memorize/L1GHT for durable knowledge.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "action": { "type": "string", "enum": ["set", "get", "list", "delete", "status"], "description": "Read action to perform; set/delete are retired compatibility requests and do not mutate after migration" },
                        "key": { "type": "string", "description": "Canonical boot memory key" },
                        "value": { "description": "JSON value to persist for the boot memory entry" },
                        "tags": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Optional tags for organization" },
                        "source_refs": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Optional source references backing this boot memory" },
                        "tier": { "type": "string", "enum": ["project", "medulla", "project+medulla", "all-brains"], "description": "For action=list: memory-tier for cross-brain recall (pull-not-push). Default project+medulla: this brain's own entries + the shared medulla. 'all-brains' fans out over every hosted brain, each entry labeled origin_brain — the explicit cross-project inspection, never ambient." }
                    },
                    "required": ["agent_id", "action"]
                }
            },
            // =================================================================
            // v0.7.0: Diagnostic tools — metrics, type_trace, diagram
            // =================================================================
            {
                "name": "metrics",
                "description": "Structural codebase metrics: LOC, child counts, degree, PageRank per file/function/struct. Supports scope filtering and sorting.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "node_types": { "type": "array", "items": { "type": "string" }, "default": ["file"], "description": "Filter by node type: file, function, class, struct, module" },
                        "top_k": { "type": "integer", "default": 50, "description": "Maximum results to return" },
                        "sort": { "type": "string", "default": "loc_desc", "description": "Sort order: loc_desc, complexity_desc, name_asc" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "type_trace",
                "description": "Cross-file type usage tracing. BFS from a type/struct/enum node to find all usage sites across the codebase. Supports forward, reverse, and bidirectional tracing.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "target": { "type": "string", "description": "Type name or external_id to trace" },
                        "direction": { "type": "string", "default": "forward", "description": "BFS direction: forward, reverse, both" },
                        "max_hops": { "type": "integer", "default": 4, "description": "Maximum BFS hops" },
                        "top_k": { "type": "integer", "default": 50, "description": "Maximum results" },
                        "group_by_file": { "type": "boolean", "default": true, "description": "Group results by file" }
                    },
                    "required": ["agent_id", "target"]
                }
            },
            {
                "name": "diagram",
                "description": "Generate a visual graph diagram in Mermaid or DOT format. Centers on a node/query or shows top-N by PageRank. Supports scope, type filtering, and layout options.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "center": { "type": "string", "description": "Seed query or node_id to center the diagram on" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "format": { "type": "string", "default": "mermaid", "description": "Output format: mermaid or dot" },
                        "max_nodes": { "type": "integer", "default": 30, "description": "Maximum nodes in diagram" },
                        "depth": { "type": "integer", "default": 2, "description": "Max BFS depth from center" },
                        "node_types": { "type": "array", "items": { "type": "string" }, "description": "Filter by node types" },
                        "show_relations": { "type": "boolean", "default": true, "description": "Show edge labels" },
                        "show_pagerank": { "type": "boolean", "default": false, "description": "Show PageRank in node labels" },
                        "direction": { "type": "string", "default": "TD", "description": "Layout direction: TD (top-down) or LR (left-right)" }
                    },
                    "required": ["agent_id"]
                }
            },
            // =================================================================
            // v0.8.0: memorize — first L1GHT writer; agent durable memory
            // =================================================================
            {
                "name": "memorize",
                "description": "Write structured knowledge claims as a valid .light.md (L1GHT protocol) file, then ingest it so evidence markers bridge to real code nodes. Returns path + ingest counts. The first tool that generates L1GHT markdown rather than only parsing it.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "node_label": { "type": "string", "description": "Entity name — becomes the Node: frontmatter header and # title" },
                        "title": { "type": "string", "description": "Section heading (## <title>); defaults to node_label" },
                        "state": { "type": "string", "description": "State: frontmatter value (default 'authored')" },
                        "claims": {
                            "type": "array",
                            "description": "Knowledge claims to encode as L1GHT markers",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "label": { "type": "string", "description": "Entity name for the marker" },
                                    "text": { "type": "string", "description": "Prose line above the marker (defaults to label)" },
                                    "kind": { "type": "string", "enum": ["entity", "state", "event"], "default": "entity", "description": "Claim kind — controls the glyph (⍂/⍐/⍌)" },
                                    "confidence": { "type": ["string", "number"], "description": "Confidence value or word, e.g. 0.7, '0.7', or 'high' (a number is coerced to its string form)" },
                                    "ambiguity": { "type": ["string", "number"], "description": "Ambiguity descriptor (a number is coerced to its string form)" },
                                    "evidence": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Repo-relative code paths (one [𝔻 evidence:] per path)" },
                                    "depends_on": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Dependency labels (one [⟁ depends_on:] per entry)" }
                                },
                                "required": ["label"]
                            }
                        },
                        "namespace": { "type": "string", "description": "Graph namespace for ingest (default 'light')" },
                        "ingest_after": { "type": "boolean", "default": true, "description": "Run ingest after writing (default true)" },
                        "mode": { "type": "string", "default": "merge", "description": "Ingest merge mode: 'merge' (default) or 'replace'" }
                    },
                    "required": ["agent_id", "node_label", "claims"]
                }
            },
            // =================================================================
            // MEDULLA M6: promote — the audited crossing (project → medulla)
            // =================================================================
            {
                "name": "promote",
                "description": "Elevate a VERIFIED project-private claim UP into the shared medulla (the doctrine tier every session's default beat reads). An EXPLICIT orchestrator act — you judge a claim is transversal (true across projects), not one repo's fact. Copies the claim into the medulla stamped with the full origin chain (Origin-Brain, Origin-Claim, Promoted-By, Promotion-Reason); the project original stays in place, stamped Promoted-To (promotion ELEVATES, never moves). Gates: only State: verified OR Source-Agent: human:maintainer may promote (C8.3); a secret/conflict-marker in the claim is refused at the hygiene floor; evidence paths are origin-qualified so freshness delegates to the origin brain, or the claim is marked evidence_unverifiable (C8.2 — a medulla claim never reads fresher than it can prove); a weaker re-promotion bounces (WouldDowngrade). Demote by learn-wrong / consolidation on the MEDULLA copy — never touches the witness. Etiquette: any id may call, every promotion is auditably attributed (Promoted-By).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier — recorded as Promoted-By (etiquette-by-provenance: promotion is an orchestrator/maintainer act)" },
                        "brain": { "type": "string", "description": "The SOURCE project root whose store holds the claim (the Origin-Brain to promote FROM)" },
                        "claim": { "type": "string", "description": "The slug (or node label) of the claim to promote — hard error if no such claim exists in the source brain (no guessing)" },
                        "reason": { "type": "string", "description": "One line: WHY this is transversal doctrine, not one repo's fact — recorded as Promotion-Reason" }
                    },
                    "required": ["agent_id", "brain", "claim", "reason"]
                }
            },
            // =================================================================
            // X-RAY write verb: xray_retag — bulk graph-tag mutation
            // =================================================================
            {
                "name": "xray_retag",
                "description": "X-RAY write verb. One call fans a tag mutation across every node matching a selector, with a dry-run-by-default / explicit-commit contract. Supply a SELECTOR (any-match filter_tags, exact node_type, external_id path_prefix) plus a TRANSFORM (op add/remove/set + tags). Returns the plan (selected/planned/skipped_noop counts + a sample of before/after) without mutating unless mode='commit'. On commit it applies the columnar tag mutators and persists the graph snapshot. Mutates graph metadata only — never source files.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "selector": {
                            "type": "object",
                            "description": "Node selector — a node must satisfy every provided predicate (empty selector matches all nodes)",
                            "properties": {
                                "filter_tags": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Node matches if it carries at least one of these tags (any-match)" },
                                "node_type": { "type": "integer", "description": "Exact node-type as canonical u8 (File=0, Directory=1, Function=2, Class=3, Struct=4, Enum=5, Type=6, Module=7, …, Custom=100+v)" },
                                "path_prefix": { "type": "string", "description": "Node matches if its external_id starts with this prefix" }
                            }
                        },
                        "op": { "type": "string", "enum": ["add", "remove", "set"], "description": "Tag transform: add (idempotent), remove (absent tags are no-ops), or set (replace the whole tag set)" },
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags to add / remove / set" },
                        "mode": { "type": "string", "enum": ["dry_run", "commit"], "default": "dry_run", "description": "dry_run (default) plans only and writes nothing; commit applies and persists" },
                        "expect_version": { "type": "string", "description": "Optional cross-call OCC token from a prior dry_run's `version`. On commit the selection fingerprint is recomputed; if it no longer matches, the commit ABORTS (status 'aborted_conflicts', applied 0, nothing written) so a concurrent tag change between dry_run and commit cannot clobber work. Omit for an unconditional commit." }
                    },
                    "required": ["agent_id", "selector", "op", "tags"]
                }
            },
            // =================================================================
            // X-RAY read verb: xray_orient — structural conformance ledger
            // =================================================================
            {
                "name": "xray_orient",
                "description": "X-RAY read verb (read-only). One call computes a conformance LEDGER over the live graph: derives each node's MODULE from its external_id (first path segment after 'file::'), walks the boundary edges (imports / depends_on), builds a cross-module dependency_matrix, and classifies each cross-module edge against a MANIFESTO (forbid pairs + layer_order) into convergence vs divergence — reported HONESTLY as 'erosion_candidates' (never confirmed violations). Also runs an existence axis: each require_exists substring is present (BEDROCK) or absent (BLUEPRINT). With an empty manifest it just reports the module census + matrix (instrument not aimed yet). Never mutates, never persists — safe in read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "Optional external_id path-prefix filter — only nodes whose external_id starts with this prefix are counted, and only edges from in-scope source nodes contribute" },
                        "manifest": {
                            "type": "object",
                            "description": "North-star layer ruleset. Empty manifest => empty erosion_candidates (honest: instrument not aimed yet, report structure only)",
                            "properties": {
                                "forbid": { "type": "array", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 }, "default": [], "description": "Pairs [A, B] meaning module A must not depend on module B" },
                                "layer_order": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Modules ordered low->high; a module may depend only on its own level or LOWER — depending on a higher layer is a candidate divergence" },
                                "require_exists": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Substrings that must appear in some node external_id (present=BEDROCK, absent=BLUEPRINT)" }
                            }
                        },
                        "manifest_path": { "type": "string", "description": "Optional path to a North-Star manifest JSON file. Used only when the inline `manifest` is empty; takes precedence over auto-discovery of <workspace_root>/xray.manifest.json. A file's `ratified` flag drives the gate's block/caution decision. The resolved provenance is echoed back as `manifest_source`." }
                    },
                    "required": ["agent_id"]
                }
            },
            // =================================================================
            // X-RAY read verb: xray_gate — North-Star pre-edit guardrail
            // =================================================================
            {
                "name": "xray_gate",
                "description": "X-RAY read verb (read-only). The North-Star guardrail an agent calls BEFORE editing code: 'am I about to violate the North Star?'. Supply the `node` (external_id) being edited plus `planned_imports` (module names this change would add an outgoing dependency to). The verb derives the node's MODULE, walks its live outgoing imports/depends_on edges, and evaluates BOTH those existing cross-module edges AND each planned edge node_module->M through the SAME rule predicate as xray_orient (forbid pairs + layer_order). Returns verdict clear|caution|blocked: it BLOCKS only on a layer-rule violation (EROSION) AND only when manifest_ratified=true; otherwise a violation is 'caution' (anti-guardrail-fatigue). Empty manifest, unmapped node, or a node not in the graph => 'clear' (honest: nothing to gate). Never mutates, never persists — safe in read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "node": { "type": "string", "description": "external_id of the node about to be edited" },
                        "planned_imports": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Module names this change would add an OUTGOING dependency to; each is evaluated as a planned edge node_module->M" },
                        "manifest": {
                            "type": "object",
                            "description": "North-star layer ruleset (same shape as xray_orient). Empty manifest => verdict clear (nothing declared to violate)",
                            "properties": {
                                "forbid": { "type": "array", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 }, "default": [], "description": "Pairs [A, B] meaning module A must not depend on module B" },
                                "layer_order": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Modules ordered low->high; depending on a higher layer is a violation" },
                                "require_exists": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Unused by the gate (accepted for manifest parity with xray_orient)" }
                            }
                        },
                        "manifest_ratified": { "type": "boolean", "default": false, "description": "When true, any violation escalates the verdict to 'blocked'. When false (default), a violation is only 'caution' — the North Star is not yet ratified, so the gate informs without obstructing (anti-guardrail-fatigue). Used only when the resolved manifest source is INLINE; a FILE-sourced manifest's own `ratified` flag overrides this" },
                        "manifest_path": { "type": "string", "description": "Optional path to a North-Star manifest JSON file. Used only when the inline `manifest` is empty; takes precedence over auto-discovery of <workspace_root>/xray.manifest.json. A file's `ratified` flag drives the gate's block/caution decision. The resolved provenance is echoed back as `manifest_source`." }
                    },
                    "required": ["agent_id", "node"]
                }
            },
            // =================================================================
            // X-RAY physical-write verb: xray_apply — atomic source-file codemod
            // =================================================================
            {
                "name": "xray_apply",
                "description": "X-RAY physical-write verb. WRITES SOURCE FILES TO DISK. One call applies an idempotent, deterministic transform across many source files via an ATOMIC 2-phase apply with content-hash optimistic-concurrency — dry-run by default. Supply a SELECTOR (path_prefix relative to project root + extensions filter) plus a TRANSFORM. TWO transform kinds: (1) kind=ensure_header_tag + tag — FILE-driven, idempotently ensures `tag` appears in each selected file's first 3 lines; (2) kind=annotate_symbol + annotation [+ node_type] — GRAPH/AST-driven: selects symbol NODES from the live graph (tree-sitter provenance captured at ingest — re-parses NOTHING), and inserts `annotation` as its own line immediately ABOVE each symbol's recorded line (bottom-up so line numbers stay valid). For annotate_symbol the selector's path_prefix matches a node's MODULE (first path segment of its external_id) and node_type filters by canonical type u8 (Function=2, Struct=4, …); only symbols whose provenance source_path resolves UNDER the project root are touched. Engine (both kinds): SELECT/plan -> STAGE (write `<file>.xray.tmp` with create-new so a pre-existing temp/symlink can never be followed or clobbered, fsync, never touching originals) -> REHASH all originals -> if any drifted: CONFLICT, abort the whole batch and delete every temp with ZERO partial writes -> else atomic rename ALL (parent dirs fsync'd for durability). Returns counts (matched/planned/skipped_noop/skipped_binary/applied/conflicts + symbols_matched for AST selection) + a planned sample; only writes when mode='commit'. Binary (non-UTF-8) files are skipped and counted as skipped_binary — never planned, staged, or written. status is 'committed' (all swapped), 'partial' (a rename failed mid-swap after ≥1 success — applied = the count actually swapped, remaining temps are LEFT for a retry to complete), 'aborted_conflicts' (drift/contention, ZERO writes), or 'dry_run'. IMPORTANT: a commit that writes files rewrites source bytes WITHOUT updating the in-memory graph — when `graph_resync_required` is true the caller MUST trigger a re-ingest to reconcile the graph with disk (and for annotate_symbol, to refresh the now-shifted line numbers BEFORE any re-run). Confined to the project root; never touches runtime/VCS/build artifacts.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "selector": {
                            "type": "object",
                            "description": "File selector, resolved relative to the project root",
                            "properties": {
                                "path_prefix": { "type": "string", "description": "Optional path prefix (relative to project root) to narrow the walk" },
                                "extensions": { "type": "array", "items": { "type": "string" }, "default": [], "description": "File extensions to include (e.g. [\"rs\"]); empty = any extension" }
                            }
                        },
                        "transform": {
                            "type": "object",
                            "description": "The transform to apply. ensure_header_tag is FILE-driven (needs `tag`); annotate_symbol is GRAPH/AST-driven (needs `annotation`, optional `node_type`).",
                            "properties": {
                                "kind": { "type": "string", "enum": ["ensure_header_tag", "annotate_symbol"], "description": "Transform kind. ensure_header_tag idempotently ensures `tag` appears in the file's first 3 lines. annotate_symbol inserts `annotation` as its own line immediately above each selected symbol node's line (resolved from graph tree-sitter provenance; no re-parse)." },
                                "tag": { "type": "string", "description": "ensure_header_tag only: the header tag to ensure (e.g. \"//! @xray:state:bedrock\")" },
                                "annotation": { "type": "string", "description": "annotate_symbol only: the line inserted immediately above each selected symbol (e.g. \"// @xray:reviewed\")" },
                                "node_type": { "type": "integer", "description": "annotate_symbol only (optional): restrict to a node type via its canonical u8 (File=0, Function=2, Class=3, Struct=4, Enum=5, …). Omit to match any symbol type." },
                                "position": { "type": "string", "enum": ["above"], "default": "above", "description": "annotate_symbol only: insertion position. MVP supports only 'above'." }
                            },
                            "required": ["kind"]
                        },
                        "mode": { "type": "string", "enum": ["dry_run", "commit"], "default": "dry_run", "description": "dry_run (default) plans only and writes nothing; commit applies the atomic 2-phase swap" },
                        "expect_version": { "type": "string", "description": "Cross-call OCC token from a prior dry_run's `version`; REQUIRED when mode='commit'. On commit the SHA-256 planned-files fingerprint is recomputed after SELECT; if it no longer matches, the commit ABORTS BEFORE staging (status 'aborted_conflicts', applied 0, NO file written) so a concurrent edit between dry_run and commit cannot clobber work. Unconditional source commits are refused by owner dispatch." }
                    },
                    "required": ["agent_id", "selector", "transform"]
                }
            },
            // =================================================================
            // X-RAY write verb: xray_paint — the PAINT pass (persist proof-state tags)
            // =================================================================
            {
                "name": "xray_paint",
                "description": "X-RAY write verb (the PAINT pass). One call classifies every in-scope node into a STRUCTURAL proof-state from REAL graph signals and writes it as a persistent tag `xray:state:<state>` — making proof-states QUERYABLE tags instead of ephemeral per-call computations. Per node (honest, proof-grown): `erosion-candidate` if it is the SOURCE of a cross-module edge the manifest flags (candidate, not confirmed — same predicate as xray_orient); else `bedrock` if it has PROOF EVIDENCE — it is exercised by a TEST (a test-source node imports/calls/references it) OR has an incoming `grounded_in` edge (evidence-backed, NOT a mere reference count); else `overgrowth` if it is an orphan (zero incoming reference edges, off-lattice); else `unproven` — used (something references it) but with no proof evidence (the honest majority). BLUEPRINT is a manifest-level absence, never a node tag. Re-paint is idempotent: existing `xray:state:*` tags are REPLACED, never accumulated. Returns counts (scanned/bedrock/overgrowth/unproven/erosion_candidate/painted) plus `proof_coverage` (bedrock/scanned, the fraction with proof evidence) and `manifest_source` (manifest provenance: inline/file:<path>/none), without mutating unless mode='commit'; on commit it applies the columnar tag mutators and persists the graph snapshot. Mutates graph metadata only — never source files.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "Optional external_id path-prefix filter — only nodes whose external_id starts with this prefix are classified and painted" },
                        "manifest": {
                            "type": "object",
                            "description": "North-star ruleset used only to flag `erosion-candidate` source nodes (same shape + predicate as xray_orient). Empty manifest => no erosion candidates (every referenced node is bedrock, every orphan is overgrowth)",
                            "properties": {
                                "forbid": { "type": "array", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 }, "default": [], "description": "Pairs [A, B] meaning module A must not depend on module B" },
                                "layer_order": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Modules ordered low->high; depending on a higher layer flags the source as an erosion-candidate" },
                                "require_exists": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Unused by paint (accepted for manifest parity with xray_orient)" }
                            }
                        },
                        "manifest_path": { "type": "string", "description": "Optional path to a North-Star manifest JSON file. Used only when the inline `manifest` is empty; takes precedence over auto-discovery of <workspace_root>/xray.manifest.json. The resolved provenance is echoed back as `manifest_source`." },
                        "mode": { "type": "string", "enum": ["dry_run", "commit"], "default": "dry_run", "description": "dry_run (default) classifies and counts but writes nothing; commit replaces each node's xray:state:* tag and persists" }
                    },
                    "required": ["agent_id"]
                }
            },
            // =================================================================
            // X-RAY read verb: xray_ledger — replay the append-only audit ledger
            // =================================================================
            {
                "name": "xray_ledger",
                "description": "X-RAY read verb (read-only). Replays the append-only AUDIT LEDGER that xray_retag / xray_paint / xray_apply append to on every committed bulk write (one JSON line per write, stored beside the graph snapshot as xray.ledger.jsonl), so a write is traceable and manually reversible. Each record carries a monotonic `seq`, the `verb`, the OCC `version` token, a `summary` (the op's counts) and `changes` (per-node before/after tags for retag/paint, or per-file path + before_hash/after_hash for apply, capped at 1000 with `changes_truncated` when overflowed). Returns the LAST `limit` entries MOST RECENT FIRST, optionally filtered by `verb`, plus `total_entries` (full line count) and the resolved `ledger_path`. A missing ledger yields an empty list (honest, not an error). Never mutates, never persists — safe in read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "limit": { "type": "integer", "default": 20, "description": "Max entries to return, most recent first (default 20)" },
                        "verb": { "type": "string", "description": "Optional verb-name filter (e.g. \"xray_paint\"); only records whose `verb` equals this are returned and counted toward `limit`" }
                    },
                    "required": ["agent_id"]
                }
            },
            // =================================================================
            // Human View v2 F0a: the SystemBlock store verbs (Slice 2)
            // =================================================================
            {
                "name": "system_blocks_snapshot",
                "description": "Human View v2 F0a READ verb. Returns the ENTIRE live SystemBlock store for this project brain — its schema, the global OCC `store_version` (the token every mutation must echo back), the skeleton (id/version/state/ratification), every block (id, name, purpose, kind, state, boundary/contract versions, membership, sockets, receipt_contract, receipts, layout, residue), and the unmapped policy. When the brain has no store yet it returns `{present:false, honest:\"no skeleton yet — import a seed or run a scan\"}` — a first-class normal state, never an error. The store is a sidecar beside the brain's other runtime artifacts (never in the graph snapshot or medulla). Read-only: never writes, safe under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "skeleton_candidate",
                "description": "Human View v2 F0c-a WRITE verb. Scans the bound repo graph and git-backed file list into a proposed candidate skeleton. Transaction law: absent store + expected_store_version:null creates a candidate store v1; candidate store + OCC replaces wholesale with heranca-zero (no receipts, fingerprints, resolved members, unmapped cache, or candidate_revision inheritance); ratified store + OCC writes only store.candidate_revision and leaves live blocks untouched. The emitted seed is complete; review_limit only bounds later UI review. F11-b zero-touch: with naming:auto and a LIVE announced naming-runner, blocks arrive runner-named (named_by:runner, needs_owner_naming:false — ratifiable without an individual touch); the report's naming block says honestly what was applied (runner / runner_partial / heuristic). Mutation — refused under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "expected_store_version": { "type": ["integer", "null"], "description": "OCC key. Null/absent is valid only when no SystemBlock store exists yet." },
                        "review_limit": { "type": "integer", "default": 16, "description": "Review queue hint for UI paging only; the backend emits every candidate block." },
                        "naming": { "type": "string", "enum": ["auto", "heuristic"], "default": "auto", "description": "auto (default) calls the pinned live naming-runner via the announced runner daemon (F11-b): one packet per block (member paths + dominant kinds + top symbols, no file bodies), per-block timeout, hostile-output sanitization (o5), per-block heuristic fallback — partial is normal; with no live runnerd the scan is exactly the offline heuristic behavior. heuristic skips the runner entirely." }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "system_blocks_seed_import",
                "description": "Human View v2 F0a WRITE verb. Imports a ratified `m1nd-system-block-seed-v0` seed into this brain's live store, producing a fresh store at `store_version` 1. Supply the seed inline as `seed_json` OR as `seed_path` (a REPO-RELATIVE path — absolute paths, `~`, and `..` are refused, the same anti-absolute law the seed's own member paths obey). The seed is fully validated before anything is written (schema, repo-relative paths, receipt scope binding, and the anti-poison evidence contract). If a store already exists this refuses honestly (`already_present`) unless `force:true`, which overwrites and reports it in `warning` (the prior live state is lost). Mutation — refused under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "seed_json": { "type": "string", "description": "Inline seed JSON (m1nd-system-block-seed-v0). Mutually exclusive with seed_path." },
                        "seed_path": { "type": "string", "description": "Repo-relative path to a seed file (e.g. docs/system-blocks/<repo>.seed.v0.json). Absolute/~/.. are refused. Mutually exclusive with seed_json." },
                        "force": { "type": "boolean", "default": false, "description": "Overwrite an existing store instead of refusing with already_present. The prior live state is lost." }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "system_blocks_ratify",
                "description": "Sovereign ratification action. Generic REST/MCP dispatch is disabled: a client-authored origin string is not authority. The action remains unavailable until an exact typed G2/G3 ratification lease consumer is installed.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "expected_store_version": { "type": "integer", "description": "The store_version you read (OCC key). A mismatch rejects the write with a conflict; nothing is applied." },
                        "block_ids": { "type": "array", "items": { "type": "string" }, "description": "Blocks to ratify. Omit to ratify every block. An unknown id is a hard error." },
                        "ratifier": { "type": "string", "description": "Claimed ratifier identity; it grants no authority on generic ingress." }
                    },
                    "required": ["agent_id", "expected_store_version", "ratifier"]
                }
            },
            {
                "name": "receipt_import",
                "description": "Human View v2 F0a WRITE verb. Attaches a typed evidence receipt to a block after the human-origin gate and the anti-poison gates all pass, then bumps `store_version`. Gates, in order: (0) HUMAN-ORIGIN — `imported_via` must be a value on the closed server-side allow-list (`\"human-ui\"`, the owner's screen, or `\"human-touchid\"`, the h4nd tray's native prompt landed behind Touch ID); absent or off-list is refused `human_gesture_required` and nothing is applied — landing a receipt is the human gesture, never an agent's write (the same law `ratify` carries); (1) optimistic-concurrency — `expected_store_version` must match or the write is rejected with a `conflict` and nothing is applied; (2) the block exists; (3) the receipt's `scope` binds to the block's CURRENT `(block_id, boundary_version, contract_version)` — otherwise `stale_scope` (PRD §3.1: evidence is never counted for a version it did not see); (4) evidence obeys the contract — the universal anchor `artifact_hash` + `evidence_refs` is present and non-empty for EVERY receipt, and a `test` receipt additionally carries its execution identity (command/cwd/exit_status/started_at/ended_at); (5) a captured execution window must have `started_at < ended_at`, neither timestamp may be future-dated at import, and the window may not exceed 24 hours. Any gate failure leaves the store untouched. Mutation — refused under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "expected_store_version": { "type": "integer", "description": "The store_version you read (OCC key). A mismatch rejects the write with a conflict; nothing is applied." },
                        "block_id": { "type": "string", "description": "The block this receipt is evidence for." },
                        "receipt": { "type": "object", "description": "The receipt (m1nd-system-block receipt shape): type, emitter, scope {block_id, boundary_version, contract_version, resolution_hash}, evidence {artifact_hash + evidence_refs required for every type; command/cwd/exit_status/started_at/ended_at additionally required for type=test}, validity." },
                        "imported_via": { "type": "string", "description": "Origin token — a human-gesture value on the closed allow-list: \"human-ui\" (the owner's screen) or \"human-touchid\" (the h4nd tray, landed behind Touch ID). Absent or any off-list value refuses the call `human_gesture_required`: landing a receipt is the human gesture, never an agent's write. (The closed allow-list grows only in code as future native gestures ship.)" }
                    },
                    "required": ["agent_id", "expected_store_version", "block_id", "receipt", "imported_via"]
                }
            },
            {
                "name": "system_blocks_reconcile",
                "description": "Human View v2 F0a WRITE verb (Slice 3) — the architectural git status. Resolves every block's declared membership (exact paths + globs like `src/**`) against the REAL repo file list, then makes the skeleton react without lying: (1) each block's effective member set becomes a deterministic fingerprint — the first reconcile records it as the honest baseline (no bump); (2) a block whose resolved set later CHANGES gets its `boundary_version` bumped, which by the existing rollup law makes every receipt earned against the older boundary stale by scope (no new staleness code); (3) files claimed by NO block are surfaced as the real unmapped (never hidden), materialized capped with an honest `unmapped_total`. The file list defaults to git (`git ls-files`, tracked + untracked, honoring .gitignore) at the bound workspace root, with a filesystem-walk fallback; pass `file_list` to inject one explicitly. The whole reconcile is ONE atomic OCC mutation: on any change `store_version` bumps once; a no-op reconcile changes nothing (idempotent). Optimistic-concurrency: a stale `expected_store_version` rejects with a `conflict` and nothing is applied. Mutation — refused under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "expected_store_version": { "type": "integer", "description": "The store_version you read (OCC key). A mismatch rejects the write with a conflict; nothing is applied." },
                        "file_list": { "type": "array", "items": { "type": "string" }, "description": "Optional explicit repo-relative file list to reconcile against. Omit to read the working set from the bound workspace root (git, else a filesystem walk)." }
                    },
                    "required": ["agent_id", "expected_store_version"]
                }
            },
            {
                "name": "receipt_recompute",
                "description": "Human View v2 F0a READ verb (Slice 3). Re-evaluates each receipt's freshness against its block's CURRENT `(block_id, boundary_version, contract_version)` and its `expires_on`, returning per-receipt `fresh` or `stale` with the first failing `reason` (`block` | `boundary` | `contract` | `expired`), plus fresh/stale counts. A pure read: receipts are NEVER deleted (history is history) — the report is the truth. Pass `block_id` to recompute a single block, or omit it for every block. Read-only: safe under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "block_id": { "type": "string", "description": "Recompute only this block. Omit to recompute every block." }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "system_blocks_archive",
                "description": "Human View v2 F0a WRITE verb (Slice 3). Archives blocks (flip state to `archived`, remembering each block's prior state so a restore is honest) or restores them (return to that REAL prior state — never a fabricated one). Archived blocks are excluded from active rollup counts; the backend only MARKS the state and never deletes data. `mode` is `\"archive\"` or `\"restore\"`; `block_ids` names one or more blocks (an unknown id is a hard error; an already-in-target-state block is a silent no-op). Optimistic-concurrency: a stale `expected_store_version` rejects with a `conflict` and nothing is applied. Mutation — refused under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "expected_store_version": { "type": "integer", "description": "The store_version you read (OCC key). A mismatch rejects the write with a conflict; nothing is applied." },
                        "block_ids": { "type": "array", "items": { "type": "string" }, "description": "The blocks to archive or restore. An unknown id is a hard error." },
                        "mode": { "type": "string", "enum": ["archive", "restore"], "description": "archive = retire (remembering the prior state); restore = return to the real prior state." }
                    },
                    "required": ["agent_id", "expected_store_version", "block_ids", "mode"]
                }
            },
            {
                "name": "system_blocks_delete",
                "description": "Human View v2 F0a WRITE verb (Slice 3). Removes a block from the store FOR REAL, reporting how many receipts died with it. `force:true` is MANDATORY — without it the call refuses honestly and suggests archive (which keeps the history). An unknown block_id is a hard error. Optimistic-concurrency: a stale `expected_store_version` rejects with a `conflict` and nothing is applied. Mutation — refused under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "expected_store_version": { "type": "integer", "description": "The store_version you read (OCC key). A mismatch rejects the write with a conflict; nothing is applied." },
                        "block_id": { "type": "string", "description": "The block to remove." },
                        "force": { "type": "boolean", "default": false, "description": "Mandatory guard. Without force:true the delete is refused (archive is suggested). With it, the block and all its receipts are permanently removed." }
                    },
                    "required": ["agent_id", "expected_store_version", "block_id"]
                }
            },
            {
                "name": "candidate_edit",
                "description": "Human View v2 F11-a WRITE verb. One typed batch of edits to a CANDIDATE skeleton under a single OCC transaction — never six loose verbs. Ops: rename (block_id, name?, purpose? — stamps provenance from the seat), merge (into, block_ids[] — unions membership with dedup, shared preserved, rewrites internal sockets to: the survivor, drops the absorbed), split (block_id, by.paths[[glob]] — partitions into N children with new stable ids by explicit, disjoint, total path groups), move_member (path, from, to), resolve_seam (path, resolution:\"both\"|\"primary:<block_id>\" — rewrites the member's role on ALL owners, 3+ supported), assign_unmapped (path, block_id). Atomicity is PREFLIGHT-ON-A-CLONE (o1): the whole batch AND every final invariant (no dangling socket, no empty block, no unresolved seam it created) is validated on a working copy before ANY persistence — the FIRST invalid op aborts with its index and NOTHING is applied; on full success the store is saved once and store_version bumps once. Merge canonicalization runs before any member op (o2): an op naming a block another op absorbs resolves to the survivor. Candidate-only (§1a): a ratified skeleton refuses every op (skeleton_not_candidate) — editing a signed boundary is a separate ceremony. Optimistic-concurrency: a stale expected_store_version rejects with a conflict; nothing is applied. The advisory lease is NEVER required. Mutation — refused under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "expected_store_version": { "type": "integer", "description": "The store_version you read (OCC key). A mismatch rejects the write with a conflict; nothing is applied." },
                        "by": { "type": "string", "enum": ["owner", "runner"], "default": "owner", "description": "Authoring seat for rename provenance (§1c): owner (the GUI, default) stamps named_by:owner and clears needs_owner_naming; runner (an agent seat) stamps named_by:runner." },
                        "ops": {
                            "type": "array",
                            "description": "The typed edit ops, applied as one atomic preflighted batch.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "op": { "type": "string", "enum": ["rename", "merge", "split", "move_member", "resolve_seam", "assign_unmapped"], "description": "The op tag." },
                                    "block_id": { "type": "string", "description": "rename/split/assign_unmapped target block id." },
                                    "name": { "type": "string", "description": "rename: the new name." },
                                    "purpose": { "type": "string", "description": "rename: the new one-line purpose." },
                                    "into": { "type": "string", "description": "merge: the surviving block id." },
                                    "block_ids": { "type": "array", "items": { "type": "string" }, "description": "merge: the block ids absorbed into `into`." },
                                    "by": { "type": "object", "description": "split: { paths: [[glob, ...], ...] } — the explicit, disjoint, total path groups (o3)." },
                                    "path": { "type": "string", "description": "move_member/resolve_seam/assign_unmapped: the member path." },
                                    "from": { "type": "string", "description": "move_member: the source block id." },
                                    "to": { "type": "string", "description": "move_member: the destination block id." },
                                    "resolution": { "type": "string", "description": "resolve_seam: \"both\" (keep on all owners as shared) or \"primary:<block_id>\" (that block owns it; removed from the others)." }
                                },
                                "required": ["op"]
                            }
                        }
                    },
                    "required": ["agent_id", "expected_store_version", "ops"]
                }
            },
            {
                "name": "candidate_lease",
                "description": "Human View v2 F11-a WRITE verb — the ADVISORY curation lease (o4). A soft, non-blocking lease the F11 screen surfaces (\"a hand is curating candidate vN\"); it NEVER blocks the owner and NEVER bumps store_version, so it cannot invalidate a pending edit or trap the candidate behind a dead agent. `acquire` is an atomic compare-and-set on curating_by + expiry — granted iff the lease is free, expired, or already this agent's; `refresh` extends the TTL for the current holder only; `release` clears it for the current holder (a free release is an idempotent no-op). An expired lease (curating_until < now) is reclaimable by anyone. candidate_edit NEVER requires a held lease — the lease only warns. The owner process is the single serialization point. Mutation — refused under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "The agent holding/refreshing/releasing the lease. The lease is keyed on this identity." },
                        "action": { "type": "string", "enum": ["acquire", "refresh", "release"], "description": "acquire (compare-and-set), refresh (extend, holder only), or release (clear, holder only)." },
                        "ttl_secs": { "type": "integer", "description": "Lease lifetime in seconds for acquire/refresh; omit for the default (900s)." }
                    },
                    "required": ["agent_id", "action"]
                }
            },
            {
                "name": "candidate_naming",
                "description": "Human View v2 F11-c WRITE verb — the in-screen 'Name with runner' path (§2b). The OWNER builds the naming packets for the requested candidate blocks (the same member-paths + dominant-kinds + top-symbols shape the scan sends, never file bodies), calls the announced runner daemon's /name (the shared secret stays owner-side — the browser never holds it), sanitizes the hostile output (o5), and applies the accepted names through ONE candidate_edit batch under the given OCC key with the RUNNER seat — provenance (named_by:runner, needs_owner_naming:false) and OCC hold; the store is never rewritten outside the verb. block_ids absent = every block still needing a name. Returns {store_version, named, fell_back, refusal?}: partial is normal; with no live naming-runner the call returns the honest no_naming_runner refusal and touches nothing. HTTP-ONLY (like mission_spawn): it needs owner-process state (the announce registry + the secret) — the sync MCP dispatch refuses with a redirect to POST /api/tools/candidate_naming. Mutation — refused under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "expected_store_version": { "type": "integer", "description": "The store_version you read (OCC key). A mismatch rejects the call with a conflict BEFORE any runner is invoked; nothing is applied." },
                        "block_ids": { "type": "array", "items": { "type": "string" }, "description": "The blocks to name. Omit to name every block still carrying an untouched provisional name (needs_owner_naming). An unknown id is a hard error." }
                    },
                    "required": ["agent_id", "expected_store_version"]
                }
            },
            {
                "name": "mission_post",
                "description": "Human View v2 F2.5a WRITE verb. Appends one mission letter (schema `m1nd-mission-letter-v0`) to the bound brain's mailbox box as a `kind=mission` line, after the §1 contract gates all pass. Gates, in order: (1) schema + `mission_id` shape (`msn_<12hex>`) + `mission_seq>=1` + the §1f no-absolute-path guard on `brain_ref`; (2) per-phase field gating — `executing` carries NO verdict, `merge_wait` REQUIRES a gate, and the §1d LANDED LAW: `landed` REQUIRES `receipt.imported==true` with a real `store_version` (a zero-exit gate WITHOUT an imported receipt is `merge_wait`, never `landed`); (3) a `receipt_candidate`, when present, is complete (`artifact_hash`+`evidence_refs`); (4) the §1e HEAD CAS — the mission's letters form a content-hash chain: `mission_seq` increments by 1 and `prev_letter_id` names the prior letter's content id; a letter that does not extend the current head is REJECTED with `stale_head` and NOTHING is appended. An identical replay dedups by content id (idempotent). The letter is STATE, not evidence — it NEVER changes a block's color (that is `receipt_import`'s job alone). Returns the appended letter's `letter_id` (set it as the next letter's `prev_letter_id`). Mutation — refused under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "The emitting agent id — stamped into the mailbox line (part of the content id, so an identical replay from the same agent dedups)." },
                        "letter": { "type": "object", "description": "The mission letter (m1nd-mission-letter-v0): schema, mission_id (msn_<12hex>), mission_seq, prev_letter_id (the prior letter's content id, or null for seq 1), block_id, brain_ref (a reference string — NEVER an absolute path), seat (oracle|hand), runner_id, capability (build-runner|naming-runner|loop-runner|hand-runner|review-runner), phase (judging|executing|gate|review|merge_wait|landed|failed), and the phase-gated fields verdict/gate/receipt_candidate/receipt, plus packet_ref, tokens_total, started_at, updated_at." }
                    },
                    "required": ["agent_id", "letter"]
                }
            },
            {
                "name": "mission_spawn",
                "description": "Human View v2 F2.5c WRITE verb (§4b) — HTTP-ONLY. The owner→runner-daemon PROXY that launches a spawn mission. The browser holds no shared secret, so the spawn travels THROUGH the owner: this verb resolves the live runner (from the announce registry), reads the owner-local `runnerd.secret`, resolves the workspace project_root from the `?brain=` selector, and FORWARDS `{runner_id, packet_markdown, block_id, brain_ref, brain}` to the runner daemon's loopback `/run` with the secret in the `x-runnerd-secret` header. The daemon opens the mission chain (judging→executing→merge_wait|failed) and NEVER lands (the landed-law: import is a human act). Returns `{mission_id, accepted:true, runner_id}` on acceptance, or the daemon's honest refusal (`unpinned_runner`, `workspace_not_allowed`, …) verbatim. Served only by `POST /api/tools/mission_spawn` on the owner — the sync MCP dispatch refuses it with a redirect. Mutation — refused under a read-only attach.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "The calling agent id (the UI passes 'gui')." },
                        "runner_id": { "type": "string", "description": "The pinned, LIVE runner id to spawn on (must appear in GET /api/runnerd/status)." },
                        "packet_markdown": { "type": "string", "description": "The composed MissionPacket markdown handed to the runner." },
                        "block_id": { "type": "string", "description": "The SystemBlock the mission extends (sb_...)." },
                        "brain_ref": { "type": "string", "description": "The brain's reference string for the letter (a display name / repo_id — NEVER an absolute path)." }
                    },
                    "required": ["agent_id", "runner_id", "packet_markdown", "block_id", "brain_ref"]
                }
            }
        ]
    });
    let tools = registry["tools"]
        .as_array_mut()
        .expect("static tool registry must be an array");
    tools.retain(|tool| {
        !matches!(
            tool.get("name").and_then(serde_json::Value::as_str),
            Some("mission_post" | "receipt_import")
        )
    });
    tools.push(mission_service_tool_schema());
    tools.push(external_mutation_service_tool_schema());
    tools.push(graph_ingest_preview_tool_schema());
    tools.push(authority_session_challenge_tool_schema());
    tools.push(authority_session_authenticate_tool_schema());
    tools.push(authority_authorize_tool_schema());
    annotate_floor_gated_descriptions(tools);
    registry
}

/// Prefix every floor-gated verb's description with the house POLICY-DISABLED
/// annotation, the sibling of the `ingest` compatibility sweep at scale.
///
/// `tools/list` may not advertise as executable a verb the generic MCP/REST gate
/// refuses: under the M1ND-10 authority floors those calls come back
/// `generic_action_authority_required`, so an un-annotated description is a lie
/// the host repeats to every agent. The verdict is DERIVED from the same floor
/// table [`enforce_generic_action_policy`] enforces, so a future verb cannot be
/// advertised un-annotated by being forgotten.
///
/// Descriptions are the ONLY thing touched. No schema FIELD is added, removed or
/// renamed: one bad `inputSchema` once emptied the whole tool list in strict
/// clients, and honesty is not worth re-running that.
///
/// A description that already carries a hand-curated `POLICY-DISABLED` sweep
/// (`ingest`) is left exactly as it is — the curated text says more than the
/// derived line can.
fn annotate_floor_gated_descriptions(tools: &mut [serde_json::Value]) {
    for tool in tools.iter_mut() {
        let Some(name) = tool.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(annotation) = floor_gate_annotation(name) else {
            continue;
        };
        let Some(description) = tool.get("description").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if description.contains(FLOOR_GATE_MARKER) {
            continue;
        }
        let annotated = format!("{annotation}{description}");
        tool["description"] = serde_json::Value::String(annotated);
    }
}

/// The house marker for a surface that policy refuses. Introduced by the
/// `ingest` compatibility sweep; reused verbatim so hosts, tests and agents keep
/// grepping for exactly one token.
pub(crate) const FLOOR_GATE_MARKER: &str = "POLICY-DISABLED";

/// The typed G2/G3 consumers — the ones the generic floor gate never judges.
///
/// [`enforce_generic_action_policy`] says it in prose ("`mission_service` is not
/// a generic call: the served transports intercept it before invoking this
/// gate"); `mcp_http::run_mission_service_wire` implements it for all six, ahead
/// of the gate call in `route_and_run`. These verbs ARE the exact authority flow
/// the `ingest` sweep tells agents to use, so calling them policy-disabled would
/// be the same lie pointed the other way. They are excluded from the derived
/// annotation and refuse on their own typed terms (an owner-observed session,
/// selected actor and lease), which their descriptions already state.
///
/// Four of the six route only to ORDINARY actions today and would never be
/// annotated anyway; the exclusion is what keeps that true if a floor rises.
pub(crate) const TYPED_CONSUMER_TOOLS: &[&str] = &[
    "mission_service",
    "external_mutation_service",
    "graph_ingest_preview",
    "authority_session_challenge",
    "authority_session_authenticate",
    "authority_authorize",
];

/// Action -> authority floor, read once per process from the canonical M1ND-10
/// catalog.
///
/// `None` when the catalog itself fails to validate; every verb is then
/// annotated UNRESOLVED, which is what the gate really does in that state
/// (`generic_action_policy_unresolved`).
fn catalog_floors_by_action(
) -> Option<&'static std::collections::BTreeMap<String, m1nd_control::AuthorityFloor>> {
    static FLOORS: std::sync::OnceLock<
        Option<std::collections::BTreeMap<String, m1nd_control::AuthorityFloor>>,
    > = std::sync::OnceLock::new();
    FLOORS
        .get_or_init(|| {
            let catalog = m1nd_control::m1nd10_action_catalog().ok()?;
            Some(
                catalog
                    .entries
                    .into_iter()
                    .map(|entry| (entry.action.as_str().to_string(), entry.authority_floor))
                    .collect(),
            )
        })
        .as_ref()
}

/// What the generic gate will do with one advertised verb, derived from the
/// floor table rather than from a hand-kept list.
enum FloorGate {
    /// Every branch is ORDINARY: a plain client really can dispatch it.
    Open,
    /// The floor could not be resolved — the gate refuses with
    /// `generic_action_policy_unresolved`.
    Unresolved,
    /// At least one branch sits above ORDINARY.
    Gated {
        floors: String,
        gated_actions: Vec<&'static str>,
        every_branch: bool,
    },
}

fn floor_gate_verdict(tool: &str) -> FloorGate {
    if TYPED_CONSUMER_TOOLS.contains(&tool) {
        return FloorGate::Open;
    }
    let (Some(floors), Some(actions)) = (
        catalog_floors_by_action(),
        crate::action_routes::possible_mcp_actions(tool),
    ) else {
        return FloorGate::Unresolved;
    };

    let mut gated_floors = std::collections::BTreeSet::new();
    let mut gated_actions = Vec::new();
    let mut open_actions = 0usize;
    for action in actions {
        let Some(floor) = floors.get(action) else {
            return FloorGate::Unresolved;
        };
        if generic_dispatch_floor_is_available(*floor) {
            open_actions += 1;
        } else {
            gated_floors.insert(authority_floor_name(*floor));
            gated_actions.push(action);
        }
    }
    if gated_actions.is_empty() {
        return FloorGate::Open;
    }
    FloorGate::Gated {
        floors: gated_floors.into_iter().collect::<Vec<_>>().join("|"),
        gated_actions,
        every_branch: open_actions == 0,
    }
}

/// The derived honesty annotation for one advertised verb, or `None` when every
/// branch of that verb is ORDINARY and a plain client really can dispatch it.
fn floor_gate_annotation(tool: &str) -> Option<String> {
    match floor_gate_verdict(tool) {
        FloorGate::Open => None,
        FloorGate::Unresolved => Some(format!(
            "{FLOOR_GATE_MARKER} (authority floor UNRESOLVED). Do not call it over generic \
             MCP/REST: dispatch refuses with generic_action_policy_unresolved until the action \
             catalog resolves this verb's floor. "
        )),
        FloorGate::Gated {
            floors,
            every_branch: true,
            ..
        } => Some(format!(
            "{FLOOR_GATE_MARKER} (authority floor {floors}). Do not call it over generic \
             MCP/REST: dispatch refuses with generic_action_authority_required until an exact \
             typed G2/G3 consumer (authority lease) is installed for this action. "
        )),
        FloorGate::Gated {
            floors,
            gated_actions,
            every_branch: false,
        } => Some(format!(
            "{FLOOR_GATE_MARKER} (authority floor {floors}) on {}. Do not call those branches \
             over generic MCP/REST: dispatch refuses with generic_action_authority_required until \
             an exact typed G2/G3 consumer (authority lease) is installed; the ORDINARY branches \
             stay callable. ",
            gated_actions.join(", ")
        )),
    }
}

/// The SHORT form of the same annotation, for a surface that renders its own
/// one-line summary instead of the schema description.
///
/// `help` prefers a curated one-liner from the tool manual over the schema
/// text, so without this the `help` verb would keep telling agents the lie
/// `tools/list` just stopped telling — for the 14 gated verbs that have a
/// manual entry. Same marker, same derived floors, minus the sentence a
/// one-liner cannot carry.
pub(crate) fn annotate_floor_gated_summary(tool: &str, summary: &str) -> String {
    if summary.contains(FLOOR_GATE_MARKER) {
        return summary.to_string();
    }
    let floors = match floor_gate_verdict(tool) {
        FloorGate::Open => return summary.to_string(),
        FloorGate::Unresolved => "UNRESOLVED".to_string(),
        FloorGate::Gated { floors, .. } => floors,
    };
    format!(
        "{FLOOR_GATE_MARKER} (authority floor {floors} — generic dispatch refuses until an exact \
         typed G2/G3 consumer is installed). {summary}"
    )
}

fn authority_session_challenge_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "name": "authority_session_challenge",
        "description": "Start the production G2 owner-session ceremony. Owner time, brain, wire session, and session-context digest are injected by the served owner; no key material is generated or accepted here.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "schema": { "const": crate::authority_transport::AUTHORITY_SESSION_CHALLENGE_REQUEST_SCHEMA },
                "request_id": { "type": "string", "minLength": 1 },
                "subject_id": { "type": "string", "minLength": 1 },
                "key_id": { "type": "string", "minLength": 1 },
                "app_host_identity": { "type": "string", "minLength": 1 },
                "nonce": { "type": "string", "minLength": 1 },
                "requested_ttl_ms": { "type": "integer", "minimum": 1, "maximum": crate::authority_transport::MAX_AUTHORITY_SESSION_CHALLENGE_TTL_MS }
            },
            "required": ["schema", "request_id", "subject_id", "key_id", "app_host_identity", "nonce", "requested_ttl_ms"]
        }
    })
}

fn authority_session_authenticate_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "name": "authority_session_authenticate",
        "description": "Complete one pending G2 session challenge using a cryptographically signed runtime.session.handshake AuthorityCapabilityV1. The owner verifies the pinned key and consumes the challenge exactly once.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "schema": { "const": crate::authority_transport::AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA },
                "request_id": { "type": "string", "minLength": 1 },
                "challenge_id": { "type": "string", "minLength": 1 },
                "capability": authority_capability_tool_schema()
            },
            "required": ["schema", "request_id", "challenge_id", "capability"]
        }
    })
}

fn authority_authorize_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "name": "authority_authorize",
        "description": "M1ND-10 G2 distinct authorization ingress. Verifies a server-pinned authority path and returns one one-shot lease; it never executes the target action. Owner time, wire session, ingress context, brain, and verification keys are injected by the served owner.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "schema": { "const": crate::authority_transport::AUTHORITY_AUTHORIZE_REQUEST_SCHEMA },
                "request_id": { "type": "string", "minLength": 1 },
                "authority_session_id": { "type": ["string", "null"] },
                "authority_session_context_digest": { "type": ["string", "null"] },
                "target_action": { "type": "string", "minLength": 1 },
                "payload_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                "requested_effects": {
                    "type": "array",
                    "minItems": 1,
                    "uniqueItems": true,
                    "items": authority_effect_tool_schema()
                },
                "mission_id": { "type": ["string", "null"] },
                "mission_head_id": { "type": ["string", "null"] },
                "input": authority_authorize_input_tool_schema()
            },
            "required": ["schema", "request_id", "target_action", "payload_digest", "requested_effects", "input"]
        }
    })
}

fn authority_capability_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "schema": { "const": m1nd_control::AUTHORITY_CAPABILITY_SCHEMA },
            "capability_id": { "type": "string", "minLength": 1 },
            "issuer_subject_id": { "type": "string", "minLength": 1 },
            "issuer_key_id": { "type": "string", "minLength": 1 },
            "algorithm": {
                "enum": [
                    m1nd_control::ED25519_ALGORITHM,
                    m1nd_control::ECDSA_P256_SHA256_X962_ALGORITHM
                ]
            },
            "subject_id": { "type": "string", "minLength": 1 },
            "audience": { "type": "string", "minLength": 1 },
            "organism_id": { "type": "string", "minLength": 1 },
            "brain_id": { "type": "string", "minLength": 1 },
            "mission_id": { "type": ["string", "null"], "minLength": 1 },
            "mission_head_id": { "type": ["string", "null"], "minLength": 1 },
            "action": { "type": "string", "minLength": 1 },
            "authority_variant": {
                "enum": ["ORDINARY", "HUMAN", "POLICY", "AGENT_QUORUM", "SAFETY_KERNEL"]
            },
            "active_mode": {
                "enum": ["HUMAN_GATED", "POLICY_AUTONOMOUS", "FULL_AUTONOMY"]
            },
            "payload_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "policy_registry_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "constitution_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "key_registry_epoch": { "type": "integer", "minimum": 0 },
            "issued_at": { "type": "integer", "minimum": 0 },
            "expires_at": { "type": "integer", "minimum": 0 },
            "nonce": { "type": "string", "minLength": 1 },
            "signature": { "type": "string", "minLength": 1 }
        },
        "required": [
            "schema", "capability_id", "issuer_subject_id", "issuer_key_id", "algorithm",
            "subject_id", "audience", "organism_id", "brain_id", "action",
            "authority_variant", "active_mode", "payload_digest", "policy_registry_digest",
            "constitution_digest", "key_registry_epoch", "issued_at", "expires_at", "nonce",
            "signature"
        ]
    })
}

fn authority_effect_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "enum": [
            "READ", "GRAPH_MUTATION", "RUNTIME_STORE_WRITE", "SOURCE_FILESYSTEM_WRITE",
            "HOST_FILESYSTEM_WRITE", "COORDINATION_RECORD", "MISSION_STATE_WRITE",
            "SOVEREIGN_MUTATION", "PROCESS_SPAWN", "PROCESS_SIGNAL",
            "EXECUTABLE_REPLACEMENT", "NETWORK_ACCESS", "NETWORK_EXPOSE",
            "FREEZE_ISSUANCE", "EPOCH_FENCE", "EPOCH_BUMP", "REVOKE_CAPABILITY",
            "ABORT_PREPARED", "DEMOTE_GRANT", "ROLLBACK_SIGNED_CANDIDATE"
        ]
    })
}

fn authority_owner_role_tool_schema() -> serde_json::Value {
    serde_json::json!({ "enum": ["author", "reviewer", "runner"] })
}

fn autonomy_digest_tool_schema() -> serde_json::Value {
    serde_json::json!({ "type": "string", "pattern": "^[0-9a-f]{64}$" })
}

fn autonomy_nullable_string_tool_schema() -> serde_json::Value {
    serde_json::json!({ "type": ["string", "null"], "minLength": 1 })
}

fn autonomy_nullable_digest_tool_schema() -> serde_json::Value {
    serde_json::json!({ "type": ["string", "null"], "pattern": "^[0-9a-f]{64}$" })
}

fn autonomy_intent_ref_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "intent_digest": autonomy_digest_tool_schema(),
            "canonicalization_version": { "type": "string", "minLength": 1 },
            "content_address": { "type": "string", "minLength": 1 }
        },
        "required": ["intent_digest", "canonicalization_version", "content_address"]
    })
}

fn autonomy_decision_binding_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "decision_id": { "type": "string", "minLength": 1 },
            "intent_digest": autonomy_digest_tool_schema(),
            "intent_core_ref": autonomy_intent_ref_tool_schema(),
            "intent_canonicalization_version": { "type": "string", "minLength": 1 },
            "required_authority_variant": { "enum": ["POLICY", "AGENT_QUORUM"] },
            "issuer_subject_id": { "type": "string", "minLength": 1 },
            "decision_subject_id": { "type": "string", "minLength": 1 },
            "caller_subject_id": { "type": "string", "minLength": 1 },
            "audience": { "type": "string", "minLength": 1 },
            "proposer_subject_id": { "type": "string", "minLength": 1 },
            "executor_subject_id": autonomy_nullable_string_tool_schema(),
            "promotion_target_subject_id": autonomy_nullable_string_tool_schema(),
            "ratification_target_subject_id": autonomy_nullable_string_tool_schema(),
            "delegation_grant_digest": autonomy_nullable_digest_tool_schema(),
            "action_policy_registry_digest": autonomy_digest_tool_schema(),
            "classifier_decision_digest": autonomy_digest_tool_schema(),
            "constitution_digest": autonomy_digest_tool_schema(),
            "constitution_epoch": { "type": "integer", "minimum": 0 },
            "autonomy_epoch": { "type": "integer", "minimum": 0 },
            "active_mode": { "enum": ["HUMAN_GATED", "POLICY_AUTONOMOUS", "FULL_AUTONOMY"] },
            "grant_id": autonomy_nullable_string_tool_schema(),
            "effective_tier": {
                "type": ["string", "null"],
                "enum": [null, "A0_OBSERVE", "A1_PROPOSE", "A2_EXECUTE", "A3_AUTONOMOUS_LAND", "A4_AUTONOMOUS_GOVERN", "A5_FULL_AUTONOMY"]
            },
            "action_class": { "type": "string", "minLength": 1 },
            "semantic_action_id": { "type": "string", "pattern": "^[a-z][a-z0-9_]*(?:\\.[a-z][a-z0-9_]*)+$" },
            "risk_class": { "enum": ["LOW", "MEDIUM", "HIGH", "CRITICAL"] },
            "risk_scope_digest": autonomy_digest_tool_schema(),
            "resource_environment_scope_digest": autonomy_digest_tool_schema(),
            "requested_budget": { "type": "integer", "minimum": 0 },
            "sentinel_required": { "type": "boolean" },
            "sentinel_verdict_digest": autonomy_nullable_digest_tool_schema(),
            "action_payload_digest": autonomy_digest_tool_schema()
        },
        "required": [
            "decision_id", "intent_digest", "intent_core_ref", "intent_canonicalization_version",
            "required_authority_variant", "issuer_subject_id", "decision_subject_id",
            "caller_subject_id", "audience", "proposer_subject_id", "action_policy_registry_digest",
            "classifier_decision_digest", "constitution_digest", "constitution_epoch",
            "autonomy_epoch", "active_mode", "action_class", "semantic_action_id", "risk_class",
            "risk_scope_digest", "resource_environment_scope_digest", "requested_budget",
            "sentinel_required", "action_payload_digest"
        ]
    })
}

fn autonomy_independence_spec_tool_schema() -> serde_json::Value {
    let seat = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "principal_id": { "type": "string", "minLength": 1 },
            "key_id": { "type": "string", "minLength": 1 },
            "failure_domain": { "type": "string", "minLength": 1 },
            "parent_session_context_digest": autonomy_digest_tool_schema()
        },
        "required": ["principal_id", "key_id", "failure_domain", "parent_session_context_digest"]
    });
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "schema": { "type": "string", "minLength": 1 },
            "core": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "constitution_epoch": { "type": "integer", "minimum": 0 },
                    "voting_verifiers": { "type": "array", "items": seat },
                    "quorum_threshold": { "type": "integer", "minimum": 0, "maximum": 65535 },
                    "minimum_failure_domains": { "type": "integer", "minimum": 0, "maximum": 65535 },
                    "blind_isolation_policy_digest": autonomy_digest_tool_schema(),
                    "nonvoting_sentinel_id": { "type": "string", "minLength": 1 },
                    "proposer_executor_nonvoting": { "type": "boolean" },
                    "sentinel_nonvoting": { "type": "boolean" }
                },
                "required": [
                    "constitution_epoch", "voting_verifiers", "quorum_threshold",
                    "minimum_failure_domains", "blind_isolation_policy_digest",
                    "nonvoting_sentinel_id", "proposer_executor_nonvoting", "sentinel_nonvoting"
                ]
            },
            "independence_spec_digest": autonomy_digest_tool_schema()
        },
        "required": ["schema", "core", "independence_spec_digest"]
    })
}

fn autonomy_quorum_evidence_tool_schema() -> serde_json::Value {
    let vote = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "verifier_principal_id": { "type": "string", "minLength": 1 },
            "verifier_key_id": { "type": "string", "minLength": 1 },
            "failure_domain": { "type": "string", "minLength": 1 },
            "parent_session_context_digest": autonomy_digest_tool_schema(),
            "intent_digest": autonomy_digest_tool_schema(),
            "constitution_digest": autonomy_digest_tool_schema(),
            "candidate_digest": autonomy_nullable_digest_tool_schema(),
            "evidence_digest": autonomy_digest_tool_schema(),
            "rollout_plan_digest": autonomy_digest_tool_schema(),
            "rollback_plan_digest": autonomy_digest_tool_schema(),
            "disposition": { "enum": ["APPROVE", "DISSENT", "ABSTAIN"] },
            "signature": { "type": "string", "minLength": 1 }
        },
        "required": [
            "verifier_principal_id", "verifier_key_id", "failure_domain",
            "parent_session_context_digest", "intent_digest", "constitution_digest",
            "evidence_digest", "rollout_plan_digest", "rollback_plan_digest",
            "disposition", "signature"
        ]
    });
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "independence_spec": autonomy_independence_spec_tool_schema(),
            "votes": { "type": "array", "items": vote },
            "sentinel_verdict_digest": autonomy_digest_tool_schema()
        },
        "required": ["independence_spec", "votes", "sentinel_verdict_digest"]
    })
}

fn autonomy_authority_decision_tool_schema() -> serde_json::Value {
    let policy = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "authority_kind": { "const": "POLICY" },
            "authority_decision": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "schema": { "type": "string", "minLength": 1 },
                    "core": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "binding": autonomy_decision_binding_tool_schema(),
                            "policy_digest": autonomy_digest_tool_schema(),
                            "matched_clauses_digest": autonomy_digest_tool_schema(),
                            "risk_budget_scope_digest": autonomy_digest_tool_schema(),
                            "proof_receipts_digest": autonomy_digest_tool_schema(),
                            "sentinel_exemption_clause_digest": autonomy_nullable_digest_tool_schema()
                        },
                        "required": [
                            "binding", "policy_digest", "matched_clauses_digest",
                            "risk_budget_scope_digest", "proof_receipts_digest"
                        ]
                    },
                    "decision_digest": autonomy_digest_tool_schema(),
                    "owner_signature": { "type": "string", "minLength": 1 }
                },
                "required": ["schema", "core", "decision_digest", "owner_signature"]
            }
        },
        "required": ["authority_kind", "authority_decision"]
    });
    let quorum = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "authority_kind": { "const": "AGENT_QUORUM" },
            "authority_decision": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "schema": { "type": "string", "minLength": 1 },
                    "core": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "binding": autonomy_decision_binding_tool_schema(),
                            "quorum": autonomy_quorum_evidence_tool_schema(),
                            "evidence_rollout_rollback_digest": autonomy_digest_tool_schema()
                        },
                        "required": ["binding", "quorum", "evidence_rollout_rollback_digest"]
                    },
                    "decision_digest": autonomy_digest_tool_schema(),
                    "owner_signature": { "type": "string", "minLength": 1 }
                },
                "required": ["schema", "core", "decision_digest", "owner_signature"]
            }
        },
        "required": ["authority_kind", "authority_decision"]
    });
    serde_json::json!({ "oneOf": [policy, quorum] })
}

fn autonomy_capability_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "schema": { "type": "string", "minLength": 1 },
            "core": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "capability_id": { "type": "string", "minLength": 1 },
                    "intent_digest": autonomy_digest_tool_schema(),
                    "intent_core_ref": autonomy_intent_ref_tool_schema(),
                    "intent_canonicalization_version": { "type": "string", "minLength": 1 },
                    "decision_digest": autonomy_digest_tool_schema(),
                    "decision_policy_digest": autonomy_digest_tool_schema(),
                    "required_authority_variant": { "enum": ["POLICY", "AGENT_QUORUM"] },
                    "action_policy_registry_digest": autonomy_digest_tool_schema(),
                    "classifier_decision_digest": autonomy_digest_tool_schema(),
                    "constitution_digest": autonomy_digest_tool_schema(),
                    "constitution_epoch": { "type": "integer", "minimum": 0 },
                    "autonomy_epoch": { "type": "integer", "minimum": 0 },
                    "organism_id": { "type": "string", "minLength": 1 },
                    "repo_id": { "type": "string", "minLength": 1 },
                    "issuer_subject_id": { "type": "string", "minLength": 1 },
                    "decision_subject_id": { "type": "string", "minLength": 1 },
                    "caller_subject_id": { "type": "string", "minLength": 1 },
                    "proposer_subject_id": { "type": "string", "minLength": 1 },
                    "executor_subject_id": autonomy_nullable_string_tool_schema(),
                    "promotion_target_subject_id": autonomy_nullable_string_tool_schema(),
                    "ratification_target_subject_id": autonomy_nullable_string_tool_schema(),
                    "delegation_grant_digest": autonomy_nullable_digest_tool_schema(),
                    "audience": { "type": "string", "minLength": 1 },
                    "active_mode": { "enum": ["POLICY_AUTONOMOUS", "FULL_AUTONOMY"] },
                    "activation_receipt_id": autonomy_nullable_string_tool_schema(),
                    "grant_id": { "type": "string", "minLength": 1 },
                    "grant_digest": autonomy_digest_tool_schema(),
                    "effective_tier": { "enum": ["A0_OBSERVE", "A1_PROPOSE", "A2_EXECUTE", "A3_AUTONOMOUS_LAND", "A4_AUTONOMOUS_GOVERN", "A5_FULL_AUTONOMY"] },
                    "action_class": { "type": "string", "minLength": 1 },
                    "semantic_action_id": { "type": "string", "pattern": "^[a-z][a-z0-9_]*(?:\\.[a-z][a-z0-9_]*)+$" },
                    "risk_class": { "enum": ["LOW", "MEDIUM", "HIGH", "CRITICAL"] },
                    "risk_scope_digest": autonomy_digest_tool_schema(),
                    "sentinel_verdict_digest": autonomy_nullable_digest_tool_schema(),
                    "brain_id": { "type": "string", "minLength": 1 },
                    "mission_id": autonomy_nullable_string_tool_schema(),
                    "mission_head_id": autonomy_nullable_string_tool_schema(),
                    "block_id": autonomy_nullable_string_tool_schema(),
                    "candidate_digest": autonomy_nullable_digest_tool_schema(),
                    "promotion_subject_id": autonomy_nullable_string_tool_schema(),
                    "resource_environment_scope_digest": autonomy_digest_tool_schema(),
                    "requested_budget": { "type": "integer", "minimum": 0 },
                    "expected_store_epoch": { "type": "integer", "minimum": 0 },
                    "expected_store_version": { "type": "integer", "minimum": 0 },
                    "expected_boundary_version": { "type": "integer", "minimum": 0 },
                    "expected_contract_version": { "type": "integer", "minimum": 0 },
                    "idempotency_key": { "type": "string", "minLength": 1 },
                    "payload_digest": autonomy_digest_tool_schema(),
                    "nonce": { "type": "string", "minLength": 1 },
                    "issued_at": { "type": "integer", "minimum": 0 },
                    "expires_at": { "type": "integer", "minimum": 0 }
                },
                "required": [
                    "capability_id", "intent_digest", "intent_core_ref",
                    "intent_canonicalization_version", "decision_digest", "decision_policy_digest",
                    "required_authority_variant", "action_policy_registry_digest",
                    "classifier_decision_digest", "constitution_digest", "constitution_epoch",
                    "autonomy_epoch", "organism_id", "repo_id", "issuer_subject_id",
                    "decision_subject_id", "caller_subject_id", "proposer_subject_id", "audience",
                    "active_mode", "grant_id", "grant_digest", "effective_tier", "action_class",
                    "semantic_action_id", "risk_class", "risk_scope_digest", "brain_id",
                    "resource_environment_scope_digest", "requested_budget", "expected_store_epoch",
                    "expected_store_version", "expected_boundary_version", "expected_contract_version",
                    "idempotency_key", "payload_digest", "nonce", "issued_at", "expires_at"
                ]
            },
            "capability_digest": autonomy_digest_tool_schema(),
            "owner_signature": { "type": "string", "minLength": 1 }
        },
        "required": ["schema", "core", "capability_digest", "owner_signature"]
    })
}

fn autonomy_sentinel_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "schema": { "type": "string", "minLength": 1 },
            "core": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "verdict_id": { "type": "string", "minLength": 1 },
                    "sentinel_identity_key_binary_policy_digest": autonomy_digest_tool_schema(),
                    "intent_digest": autonomy_digest_tool_schema(),
                    "intent_core_ref": autonomy_intent_ref_tool_schema(),
                    "intent_canonicalization_version": { "type": "string", "minLength": 1 },
                    "metric_evidence_rollback_digest": autonomy_digest_tool_schema(),
                    "risk_scope_digest": autonomy_digest_tool_schema(),
                    "constitution_epoch": { "type": "integer", "minimum": 0 },
                    "autonomy_epoch": { "type": "integer", "minimum": 0 },
                    "nonce": { "type": "string", "minLength": 1 },
                    "issued_at": { "type": "integer", "minimum": 0 },
                    "expires_at": { "type": "integer", "minimum": 0 },
                    "verdict": { "enum": ["GREEN", "RED"] }
                },
                "required": [
                    "verdict_id", "sentinel_identity_key_binary_policy_digest", "intent_digest",
                    "intent_core_ref", "intent_canonicalization_version",
                    "metric_evidence_rollback_digest", "risk_scope_digest", "constitution_epoch",
                    "autonomy_epoch", "nonce", "issued_at", "expires_at", "verdict"
                ]
            },
            "verdict_digest": autonomy_digest_tool_schema(),
            "signature": { "type": "string", "minLength": 1 }
        },
        "required": ["schema", "core", "verdict_digest", "signature"]
    })
}

fn autonomy_authority_evidence_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "intent_digest": autonomy_digest_tool_schema(),
            "decision": autonomy_authority_decision_tool_schema(),
            "capability": autonomy_capability_tool_schema(),
            "sentinel": { "oneOf": [{ "type": "null" }, autonomy_sentinel_tool_schema()] }
        },
        "required": ["intent_digest", "decision", "capability", "sentinel"]
    })
}

fn authority_authorize_input_tool_schema() -> serde_json::Value {
    let ordinary = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "authority": { "const": "ordinary_session" },
            "role": authority_owner_role_tool_schema()
        },
        "required": ["authority", "role"]
    });
    let positive = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "authority": { "const": "positive_sovereign" },
            "capability": authority_capability_tool_schema(),
            "role": authority_owner_role_tool_schema(),
            "capability_kind": { "enum": ["HUMAN", "AUTONOMY", "SAFETY"] },
            "authority_decision_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "applicable_grant_id": { "type": ["string", "null"], "minLength": 1 },
            "applicable_tier": {
                "type": ["string", "null"],
                "enum": [null, "A0_OBSERVE", "A1_PROPOSE", "A2_EXECUTE", "A3_AUTONOMOUS_LAND", "A4_AUTONOMOUS_GOVERN", "A5_FULL_AUTONOMY"]
            },
            "autonomy_evidence": {
                "oneOf": [{ "type": "null" }, autonomy_authority_evidence_tool_schema()]
            }
        },
        "required": [
            "authority", "capability", "role", "capability_kind",
            "authority_decision_digest"
        ]
    });
    let service = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "authority": { "const": "service_identity" },
            "assertion": service_identity_assertion_tool_schema()
        },
        "required": ["authority", "assertion"]
    });
    let safety = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "authority": { "const": "safety" },
            "attempt": safety_actuator_attempt_tool_schema()
        },
        "required": ["authority", "attempt"]
    });
    serde_json::json!({
        "description": "Closed authority union. Owner-session roles are assertions checked against the owner-pinned subject map.",
        "oneOf": [ordinary, positive, service, safety]
    })
}

fn service_identity_assertion_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "schema": { "const": crate::authority_runtime::SERVICE_IDENTITY_ASSERTION_SCHEMA },
            "core": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "service_id": { "type": "string", "minLength": 1 },
                    "subject_id": { "type": "string", "minLength": 1 },
                    "key_id": { "type": "string", "minLength": 1 },
                    "role": { "enum": ["mission_service", "author", "reviewer", "runner"] },
                    "organism_id": { "type": "string", "minLength": 1 },
                    "brain_id": { "type": "string", "minLength": 1 },
                    "audience": { "type": "string", "minLength": 1 },
                    "identity_key_binary_policy_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "action": { "type": "string", "minLength": 1 },
                    "object_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "mission_id": { "type": ["string", "null"], "minLength": 1 },
                    "mission_head_id": { "type": ["string", "null"], "minLength": 1 },
                    "transport_session_id": { "type": "string", "minLength": 1 },
                    "ingress_context_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "nonce": { "type": "string", "minLength": 1 },
                    "issued_at": { "type": "integer", "minimum": 0 },
                    "expires_at": { "type": "integer", "minimum": 0 }
                },
                "required": [
                    "service_id", "subject_id", "key_id", "role", "organism_id", "brain_id",
                    "audience", "identity_key_binary_policy_digest", "action", "object_digest",
                    "transport_session_id", "ingress_context_digest", "nonce", "issued_at",
                    "expires_at"
                ]
            },
            "signature": { "type": "string", "minLength": 1 }
        },
        "required": ["schema", "core", "signature"]
    })
}

fn safety_actuator_attempt_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "schema": { "const": crate::authority_runtime::SAFETY_ACTUATOR_ATTEMPT_SCHEMA },
            "core": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "attempt_id": { "type": "string", "minLength": 1 },
                    "actuator_subject_id": { "type": "string", "minLength": 1 },
                    "actuator_key_id": { "type": "string", "minLength": 1 },
                    "actuator_identity_key_binary_policy_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "action": { "type": "string", "minLength": 1 },
                    "payload_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "negative_effects": {
                        "type": "array",
                        "minItems": 1,
                        "uniqueItems": true,
                        "items": { "enum": ["FREEZE_ISSUANCE", "EPOCH_FENCE", "EPOCH_BUMP", "REVOKE_CAPABILITY", "ABORT_PREPARED", "DEMOTE_GRANT", "ROLLBACK_SIGNED_CANDIDATE"] }
                    },
                    "constitution_epoch": { "type": "integer", "minimum": 0 },
                    "autonomy_epoch": { "type": "integer", "minimum": 0 },
                    "nonce": { "type": "string", "minLength": 1 },
                    "issued_at": { "type": "integer", "minimum": 0 },
                    "expires_at": { "type": "integer", "minimum": 0 }
                },
                "required": [
                    "attempt_id", "actuator_subject_id", "actuator_key_id",
                    "actuator_identity_key_binary_policy_digest", "action", "payload_digest",
                    "negative_effects", "constitution_epoch", "autonomy_epoch", "nonce",
                    "issued_at", "expires_at"
                ]
            },
            "signature": { "type": "string", "minLength": 1 }
        },
        "required": ["schema", "core", "signature"]
    })
}

fn mission_service_tool_schema() -> serde_json::Value {
    let common = serde_json::json!({
        "schema": { "const": crate::mission_service_transport::MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA },
        "request_id": { "type": "string", "minLength": 1 }
    });
    let variant = |action: &'static str, fields: serde_json::Value, required: &[&str]| {
        let mut properties = common.clone();
        properties["action"] = serde_json::json!({ "const": action });
        if let (Some(target), Some(source)) = (properties.as_object_mut(), fields.as_object()) {
            target.extend(source.clone());
        }
        let mut required_fields = vec!["action", "schema", "request_id"];
        required_fields.extend_from_slice(required);
        serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": properties,
            "required": required_fields,
        })
    };
    serde_json::json!({
        "name": "mission_service",
        "description": "M1ND-10 G3 sole external mission mutation boundary. Accepts the closed v1 action union; authority and owner time are injected by the owner and are forbidden in the body. Legacy mission_post, receipt_import, and raw landed are permanent tombstones.",
        "inputSchema": {
            // MCP requires a top-level "type": "object"; clients (e.g. Claude Code)
            // reject the ENTIRE tools/list if any tool ships a bare oneOf.
            "type": "object",
            "oneOf": [
                variant(
                    "land_intent",
                    serde_json::json!({
                        "mission_id": { "type": "string" },
                        "expected_head_id": { "type": "string" },
                        "candidate_id": { "type": "string" },
                        "expected_candidate_digest": { "type": "string" },
                        "expected_store_version": { "type": "integer", "minimum": 1 },
                        "idempotency_key": { "type": "string" }
                    }),
                    &["mission_id", "expected_head_id", "candidate_id", "expected_candidate_digest", "expected_store_version", "idempotency_key"]
                ),
                variant(
                    "mission_transition",
                    serde_json::json!({ "intent": { "type": "object" }, "payload": { "type": "object" } }),
                    &["intent", "payload"]
                ),
                variant(
                    "execution_dispatch",
                    serde_json::json!({ "intent": { "type": "object" }, "payload": { "type": "object" } }),
                    &["intent", "payload"]
                ),
                variant(
                    "execution_started",
                    serde_json::json!({ "snapshot": { "type": "object" }, "intent": { "type": "object" }, "payload": { "type": "object" } }),
                    &["snapshot", "intent", "payload"]
                ),
                variant(
                    "execution_terminal",
                    serde_json::json!({ "snapshot": { "type": "object" }, "intent": { "type": "object" }, "payload": { "type": "object" } }),
                    &["snapshot", "intent", "payload"]
                ),
                variant(
                    "land",
                    serde_json::json!({ "request": { "type": "object" } }),
                    &["request"]
                )
            ]
        }
    })
}

fn external_mutation_service_tool_schema() -> serde_json::Value {
    let common = serde_json::json!({
        "schema": { "const": crate::external_mutation_service::EXTERNAL_MUTATION_REQUEST_SCHEMA },
        "request_id": { "type": "string", "minLength": 1 }
    });
    let variant = |action: &'static str, fields: serde_json::Value, required: &[&str]| {
        let mut properties = common.clone();
        properties["action"] = serde_json::json!({ "const": action });
        if let (Some(target), Some(source)) = (properties.as_object_mut(), fields.as_object()) {
            target.extend(source.clone());
        }
        let mut required_fields = vec!["action", "schema", "request_id"];
        required_fields.extend_from_slice(required);
        serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": properties,
            "required": required_fields,
        })
    };
    let graph_ingest_parent = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "operation_id": { "type": "string", "minLength": 1 },
            "lease_id": { "type": "string", "minLength": 1 },
            "reservation_id": { "type": "string", "minLength": 1 },
            "operation_object_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "semantic_payload_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "outcome_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "published_result_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" }
        },
        "required": [
            "operation_id", "lease_id", "reservation_id", "operation_object_digest",
            "semantic_payload_digest", "outcome_digest", "published_result_digest"
        ]
    });
    let graph_ingest_request = |parent: serde_json::Value, require_parent: bool| {
        let mut required = vec![
            "preview_id",
            "root",
            "expected_graph_generation",
            "expected_source_projection_digest",
        ];
        if require_parent {
            required.push("parent");
        }
        serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "preview_id": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                "root": { "type": "string", "minLength": 1 },
                "expected_graph_generation": { "type": "integer", "minimum": 0 },
                "expected_source_projection_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                "include_dotfiles": { "type": "boolean", "default": false },
                "dotfile_patterns": {
                    "type": "array",
                    "uniqueItems": true,
                    "items": { "type": "string", "minLength": 1 }
                },
                "parent": parent
            },
            "required": required
        })
    };
    serde_json::json!({
        "name": "external_mutation_service",
        "description": "M1ND-10 closed MCP-only consumer for exact elevated system-block ratification, brain promotion, source-edit commit, and governed full-root code ingestion. Action, effects, authority identity, and owner time are derived or injected owner-side; lease labels never authorize generic tools.",
        "inputSchema": {
            // MCP requires a top-level "type": "object"; clients (e.g. Claude Code)
            // reject the ENTIRE tools/list if any tool ships a bare oneOf.
            "type": "object",
            "oneOf": [
                variant(
                    "system_blocks_ratify",
                    serde_json::json!({
                        "expected_store_version": { "type": "integer", "minimum": 1 },
                        "block_ids": {
                            "type": ["array", "null"],
                            "minItems": 1,
                            "uniqueItems": true,
                            "items": { "type": "string", "minLength": 1 }
                        }
                    }),
                    &["expected_store_version"]
                ),
                variant(
                    "brain_promote",
                    serde_json::json!({
                        "source_brain": { "type": "string", "minLength": 1 },
                        "claim": { "type": "string", "minLength": 1 },
                        "reason": { "type": "string", "minLength": 1 },
                        "expected_source_sha256": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                        "expected_medulla_sha256": {
                            "type": ["string", "null"],
                            "pattern": "^[0-9a-f]{64}$"
                        }
                    }),
                    &["source_brain", "claim", "reason", "expected_source_sha256", "expected_medulla_sha256"]
                ),
                variant(
                    "source_edit_commit",
                    serde_json::json!({
                        "request": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "schema": { "const": crate::external_mutation_service::SOURCE_EDIT_COMMIT_REQUEST_SCHEMA },
                                "preview_id": { "type": "string", "minLength": 1 }
                            },
                            "required": ["schema", "preview_id"]
                        }
                    }),
                    &["request"]
                ),
                variant(
                    "graph_ingest_replace",
                    serde_json::json!({
                        "request": graph_ingest_request(
                            serde_json::json!({ "type": "null" }),
                            false
                        )
                    }),
                    &["request"]
                ),
                variant(
                    "graph_ingest_merge_existing",
                    serde_json::json!({
                        "request": graph_ingest_request(graph_ingest_parent, true)
                    }),
                    &["request"]
                )
            ]
        }
    })
}

fn graph_ingest_preview_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "name": "graph_ingest_preview",
        "description": "Read-only owner-derived preview for governed full-root ingestion of the already selected brain. It returns the exact action/effects, current OCC and candidate bindings, authority object digest, and execute request template. It does not create/rebind a brain, consume a lease, or mutate graph/store state.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "schema": { "const": crate::external_mutation_service::GRAPH_INGEST_PREVIEW_REQUEST_SCHEMA },
                "request_id": { "type": "string", "minLength": 1 },
                "mode": { "enum": ["REPLACE", "MERGE_EXISTING"] },
                "include_dotfiles": { "type": "boolean", "default": false },
                "dotfile_patterns": {
                    "type": "array",
                    "uniqueItems": true,
                    "items": { "type": "string", "minLength": 1 }
                },
                "parent": {
                    "type": ["object", "null"],
                    "additionalProperties": false,
                    "properties": {
                        "operation_id": { "type": "string", "minLength": 1 },
                        "lease_id": { "type": "string", "minLength": 1 },
                        "reservation_id": { "type": "string", "minLength": 1 },
                        "operation_object_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                        "semantic_payload_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                        "outcome_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                        "published_result_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" }
                    },
                    "required": [
                        "operation_id", "lease_id", "reservation_id",
                        "operation_object_digest", "semantic_payload_digest",
                        "outcome_digest", "published_result_digest"
                    ]
                }
            },
            "required": ["schema", "request_id", "mode", "parent"]
        }
    })
}

// ---------------------------------------------------------------------------
// Read-only attach: mutating-tool deny-list
// ---------------------------------------------------------------------------

/// Tools that mutate graph/plasticity/disk state and must be refused when the
/// session is attached read-only. Read-only/analysis tools are NOT listed here
/// and continue to work normally. `persist` is handled specially (see
/// [`read_only_denied`]) because its `status` action is read-only.
const READ_ONLY_DENIED_TOOLS: &[&str] = &[
    "ingest",
    "apply",
    "apply_batch",
    "edit_commit",
    "memorize",
    // promote writes a medulla copy + a witness stamp to disk (MEDULLA M6).
    "promote",
    "learn",
    // antibody_create is the antibody store's writer: create/delete/enable/disable
    // all rewrite `state.antibodies`, which the checkpoint inventory carries as the
    // `antibodies` sidecar. A read-only attach must refuse it on its own merits,
    // and the classification is ALSO what makes the write durable the turn it is
    // acked — the actor's O(1) witness watches graph structure and session
    // generations, so a sidecar-only write is invisible to it. `antibody_scan` and
    // `antibody_list` stay ABSENT: scan's counter drift joins the staged-persist
    // debounce instead (see `handle_antibody_scan`), list is a pure read.
    "antibody_create",
    "daemon_start",
    "auto_ingest_start",
    // xray_retag commits tag mutations to graph_path on disk, so a read-only
    // attach must refuse it (dry_run would also be blocked here — acceptable,
    // since the verb's purpose is to lead to a write).
    "xray_retag",
    // xray_apply physically writes source files to disk, so a read-only attach must refuse it.
    "xray_apply",
    // xray_paint commits proof-state tag mutations to graph_path on disk, so a
    // read-only attach must refuse it (same stance as xray_retag).
    "xray_paint",
    // debrief is the ONLY mutation in the delegation layer: it memorizes findings,
    // teaches via learn, flips the registry record, and appends the outcomes
    // ledger — a read-only attach must refuse it. `delegate` is deliberately ABSENT
    // (it is read-only like north; its omission from this list IS its ambient
    // legality — ORGANISM R6 / NEXTGEN-AGENT-PRD §O.12.3).
    "debrief",
    // runtime_overlay INGESTS spans, applies activation boosts to the live graph
    // and persists overlay state — a mutation, not a render source. A read-only
    // attach must refuse it (HUMAN-VIEW-V2-F0-TECH §6/§12: the render path reads
    // a persisted artifact; it never calls this verb).
    "runtime_overlay",
    // Human View v2 F0a SystemBlock store WRITES (Slice 2): each persists the
    // sidecar store to disk (seed import, ratify, receipt attach), so a read-only
    // attach must refuse them. `system_blocks_snapshot` is deliberately ABSENT —
    // it is a pure read (like xray_ledger), so it stays ambiently legal.
    "skeleton_candidate",
    "system_blocks_seed_import",
    "system_blocks_ratify",
    "receipt_import",
    // Slice 3 WRITES: reconcile persists fingerprints/boundary bumps/unmapped;
    // archive flips block state; delete removes a block — all mutate the store on
    // disk. `receipt_recompute` is deliberately ABSENT — it is a pure read (history
    // is never mutated), so it stays ambiently legal like `system_blocks_snapshot`.
    "system_blocks_reconcile",
    "system_blocks_archive",
    "system_blocks_delete",
    // HUMAN VIEW v2 F11-a: candidate_edit persists a preflighted batch of boundary
    // edits (one store write + one version bump); candidate_lease persists the
    // advisory curation lease. Both mutate the store on disk, so a read-only attach
    // must refuse them.
    "candidate_edit",
    "candidate_lease",
    // HUMAN VIEW v2 F11-c: candidate_naming applies runner names through a
    // candidate_edit batch (a store write). HTTP-only (like mission_spawn), but the
    // read-only law + the tool surface stay consistent here.
    "candidate_naming",
    // HUMAN VIEW v2 F2.5a: mission_post appends a mission letter to the box on
    // disk (a mailbox write), so a read-only attach must refuse it. The
    // `kind=mission` READ is an HTTP route (a pure read), never an MCP verb — it
    // is not gated here (§6-F2.5a: the safety laws land first).
    "mission_post",
    // HUMAN VIEW v2 F2.5c: mission_spawn PROXIES a spawn to the runner daemon — it
    // launches a mission (a write), so a read-only attach must refuse it. It is an
    // HTTP-only proxy handled in `http_server::handle_mission_spawn` (it needs the
    // owner's announce registry + the shared secret + an async forward); the
    // dispatch arm here only surfaces an honest "http-only" message to an MCP-stdio
    // caller. Listing it keeps the read-only law + the tool surface consistent.
    "mission_spawn",
    // transplant writes source/dest/referencer files atomically (through
    // apply_batch), so a read-only attach must refuse it.
    // `transplant_commit` lands a staged plan — the same write under a handle.
    // `transplant_preview` is deliberately ABSENT (it stages in memory and never
    // writes, mirroring the `edit_preview` exemption).
    "transplant",
    "transplant_commit",
];

/// Returns true if `tool_name` must be refused in read-only attach mode.
///
/// Normalizes the optional `m1nd.`/`m1nd_` prefix first so `apply`, `m1nd_apply`
/// and `m1nd.apply` are all caught. `persist` is allowed only for its read-only
/// `action == "status"`; every other persist action (`save`/`checkpoint`/`load`)
/// writes graph/disk state and is denied. `edit_preview` is intentionally
/// allowed: it stages an in-memory preview and never writes to disk; only
/// `edit_commit` performs the write.
pub(crate) fn read_only_denied(tool_name: &str, params: &serde_json::Value) -> bool {
    let bare = tool_name
        .strip_prefix("m1nd.")
        .or_else(|| tool_name.strip_prefix("m1nd_"))
        .unwrap_or(tool_name);
    if READ_ONLY_DENIED_TOOLS.contains(&bare) {
        return true;
    }
    if bare == "persist" {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("status");
        return !action.eq_ignore_ascii_case("status");
    }
    false
}

/// Resolve the complete semantic effect set before proof gating. Exact branches
/// are preferred; when classification depends on a trusted owner fact not yet
/// computed, conservatively union every reachable branch. Catalog/route drift
/// is a refusal, never a weaker default.
fn source_write_effect_required(
    tool_name: &str,
    params: &serde_json::Value,
    normalized_tool: &str,
) -> M1ndResult<bool> {
    use crate::action_routes::TrustedMcpRouteFacts;
    use m1nd_control::Effect;

    match crate::action_routes::classify_mcp_action(
        tool_name,
        params,
        TrustedMcpRouteFacts::default(),
    ) {
        Ok(classified) => Ok(classified.effects.contains(&Effect::SourceFilesystemWrite)),
        Err(exact_error) => crate::action_routes::possible_mcp_effects(tool_name)
            .map(|effects| effects.contains(&Effect::SourceFilesystemWrite))
            .map_err(|union_error| M1ndError::InvalidParams {
                tool: normalized_tool.to_string(),
                detail: format!(
                    "cannot conservatively resolve action effects (exact={exact_error}; union={union_error})"
                ),
            }),
    }
}

/// Extract exact physical targets after the semantic effect union says a source
/// write is reachable. A future source-write action without an extractor fails
/// closed automatically instead of silently escaping a name allow-list.
fn proof_gate_targets(
    bare_tool: &str,
    params: &serde_json::Value,
    state: &SessionState,
) -> M1ndResult<Vec<String>> {
    match bare_tool {
        "apply" => Ok(params
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(|s| vec![s.to_string()])
            .filter(|targets| !targets[0].trim().is_empty())
            .ok_or_else(|| M1ndError::InvalidParams {
                tool: bare_tool.to_string(),
                detail: "source write has no resolvable file_path target".to_string(),
            })?),
        "apply_batch" => Ok(params
            .get("edits")
            .and_then(|v| v.as_array())
            .map(|edits| {
                edits
                    .iter()
                    .filter_map(|e| e.get("file_path").and_then(|v| v.as_str()))
                    .map(|s| s.to_string())
                    .collect()
            })
            .ok_or_else(|| M1ndError::InvalidParams {
                tool: bare_tool.to_string(),
                detail: "source write has no edits array".to_string(),
            })?),
        "edit_commit" => Ok(params
            .get("preview_id")
            .and_then(|v| v.as_str())
            .and_then(|pid| state.edit_previews.get(pid))
            .map(|preview| vec![preview.file_path.clone()])
            .ok_or_else(|| M1ndError::InvalidParams {
                tool: bare_tool.to_string(),
                detail: "edit_commit preview target is missing or expired".to_string(),
            })?),
        // B1: transplant writes source + dest + DERIVED referencer files the
        // caller never named. The full touched set is derived read-only (same
        // discovery the verb itself runs) so the armed gate covers ALL of them —
        // a referencer without a permit refuses the whole call before any write.
        "transplant" => Ok(crate::transplant::proof_gate_touched_files(state, params)),
        // A2: a staged transplant already KNOWS its full touched set — recover it
        // from the preview (mirrors the `edit_commit` arm above), failing closed
        // with an explicit error when the preview is missing or expired.
        "transplant_commit" => Ok(params
            .get("preview_id")
            .and_then(|v| v.as_str())
            .and_then(|pid| state.transplant_previews.get(pid))
            .map(|p| p.planned.iter().map(|f| f.file_path.clone()).collect())
            .ok_or_else(|| M1ndError::InvalidParams {
                tool: bare_tool.to_string(),
                detail: "transplant_commit preview target is missing or expired".to_string(),
            })?),
        "xray_apply" => {
            let input: crate::xray_handlers::XrayApplyInput =
                serde_json::from_value(params.clone()).map_err(|error| M1ndError::InvalidParams {
                    tool: bare_tool.to_string(),
                    detail: error.to_string(),
                })?;
            if input.mode == crate::xray_handlers::XrayMode::Commit
                && input
                    .expect_version
                    .as_deref()
                    .is_none_or(|version| version.trim().is_empty())
            {
                return Err(M1ndError::InvalidParams {
                    tool: bare_tool.to_string(),
                    detail: "xray_apply commit requires expect_version from a fresh dry_run; unconditional source commits are disabled"
                        .to_string(),
                });
            }
            crate::xray_handlers::xray_apply_proof_targets(state, &input)
        }
        _ => Err(M1ndError::InvalidParams {
            tool: bare_tool.to_string(),
            detail: format!(
                "semantic action includes SOURCE_FILESYSTEM_WRITE but no exact target resolver exists for '{bare_tool}'"
            ),
        }),
    }
}

// ---------------------------------------------------------------------------
// L1GHT marker-fragment filter (field-triage batch A / inbox L28)
// ---------------------------------------------------------------------------

/// The l1ght_adapter emits one node per epistemic/declaration MARKER line of a
/// `.light.md` (`[𝔻 confidence: …]`, `[𝔻 evidence: …]`, `[⟁ depends_on: …]`,
/// `[⍂ entity: …]`, `[⍐ state: …]`, `[⍌ event: …]`, …). Those nodes are DATA —
/// they annotate the claim they attach to — but they are NOT slot-worthy: they
/// must never occupy a memory/anchor/focus row in the north packet, where they
/// crowd out the real claim/section rows the agent actually needs. Field-report
/// L28 (live founder SessionStart hook): 2 of 5 memory slots + 4 of 4 anchor
/// slots were spent on rows like `𝔻 confidence: 0.9` and `𝔻 confidence: 0.95`.
///
/// Detection signal, in order of preference (STRUCTURAL over glyph-string):
///   1. The node's external id carries the `::tag::` namespace segment. The
///      adapter mints every marker node id as `light::<ns>::tag::<file>::<line>::…`
///      (l1ght_adapter, the only `::tag::` producer) — section/claim/next/meta
///      nodes never do. This is the deterministic, structural discriminator.
///   2. Fallback for surfaces that expose only the label: the label begins with
///      a L1GHT marker glyph (𝔻 epistemic, ⟁ binding, ⍂/⍐/⍌ declaration). The
///      glyph set is matched in full and honestly — this catches a marker row
///      even when the id is unavailable at the call site.
///
/// Both signals are cheap and side-effect-free; either one firing means "marker".
const MARKER_GLYPHS: [char; 5] = ['𝔻', '⟁', '⍂', '⍐', '⍌'];

/// The `::tag::` id segment the l1ght_adapter stamps on (and only on) marker nodes.
const LIGHT_MARKER_ID_SEGMENT: &str = "::tag::";

/// True when a `(node_id, label)` pair identifies a L1GHT marker/annotation node
/// rather than a real claim/section. Pass an empty `node_id` when the caller only
/// has the label — the glyph fallback still fires. See [`MARKER_GLYPHS`].
fn is_marker_fragment(node_id: &str, label: &str) -> bool {
    if node_id.contains(LIGHT_MARKER_ID_SEGMENT) {
        return true;
    }
    label.trim_start().starts_with(MARKER_GLYPHS)
}

// ---------------------------------------------------------------------------
// Tier 3: memory at point-of-relevance (`_m1nd.memory_nearby`)
// ---------------------------------------------------------------------------

/// Tools whose results carry rankable node ids worth checking for nearby memory.
fn tool_has_memory_anchors(tool: &str) -> bool {
    let bare = tool
        .strip_prefix("m1nd.")
        .or_else(|| tool.strip_prefix("m1nd_"))
        .unwrap_or(tool);
    matches!(
        bare,
        "activate" | "seek" | "focus" | "search" | "surgical_context" | "surgical_context_v2"
    )
}

/// Parse a confidence value out of a memory marker label, if present.
/// Markers authored via `memorize` embed `[𝔻 confidence: 0.9]` in the label text.
fn parse_marker_confidence(label: &str) -> Option<f64> {
    let pos = label.find("confidence:")? + "confidence:".len();
    let tail = &label[pos..];
    let num: String = tail
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    num.parse::<f64>().ok()
}

/// Build `_m1nd.memory_nearby`: for the top result node ids of a query/seek/
/// activate/surgical result, surface any memorized claim that anchors to them
/// via a `grounded_in` edge (marker → code), so the agent sees prior
/// conclusions WITHOUT issuing another query.
///
/// Best-effort and capped at 3. `evidence_fresh` is a cheap signal: true when
/// the cited code file still exists on disk (re-hashing on every query would be
/// too costly here; `audit(checks=["evidence_freshness"])` remains the
/// authoritative hash-level check). Returns `None` when the tool has no anchors
/// or nothing is found.
fn memory_nearby_for_result(
    state: &SessionState,
    tool: &str,
    result: &serde_json::Value,
) -> Option<Vec<serde_json::Value>> {
    if !tool_has_memory_anchors(tool) {
        return None;
    }

    // Collect up to a few top result node ids (labels / external ids).
    let results = result.get("results").and_then(|v| v.as_array())?;
    let top_ids: Vec<String> = results
        .iter()
        .take(3)
        .filter_map(|r| {
            r.get("node_id")
                .or_else(|| r.get("label"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    if top_ids.is_empty() {
        return None;
    }

    let graph = state.graph.read();
    let grounded_in = graph.strings.lookup("grounded_in")?;
    let evidenced_by_tag = graph.strings.lookup("light:evidenced_by");

    // Map each requested top id → its node index (skip ids not in the graph).
    let mut target_idx: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    for id in &top_ids {
        if let Some(nid) = graph.resolve_id(id) {
            target_idx.insert(nid.as_usize(), id.clone());
        }
    }
    if target_idx.is_empty() {
        return None;
    }

    // Walk markers: every node with an outgoing `grounded_in` edge whose target
    // is one of our top result nodes is a memory anchor for that result.
    let node_count = graph.nodes.count as usize;
    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    'outer: for src_idx in 0..node_count {
        // Only consider memory/light markers when the tag is present in the graph.
        if let Some(tag) = evidenced_by_tag {
            let is_marker = graph
                .nodes
                .tags
                .get(src_idx)
                .is_some_and(|tags| tags.contains(&tag));
            if !is_marker {
                continue;
            }
        }
        let src_nid = m1nd_core::types::NodeId::new(src_idx as u32);
        for edge_i in graph.csr.out_range(src_nid) {
            if graph.csr.relations[edge_i] != grounded_in {
                continue;
            }
            let tgt_idx = graph.csr.targets[edge_i].as_usize();
            let Some(anchor_id) = target_idx.get(&tgt_idx) else {
                continue;
            };
            // The `grounded_in` edge starts at the EVIDENCE MARKER node (`𝔻 evidence:
            // <path>`), so the marker's own label is a fragment, not the memorized
            // claim. Confidence is still parsed from the marker label (that's where the
            // value lives), but the surfaced `claim` must be the REAL claim/section the
            // marker annotates. Resolve it via the marker's incoming `evidenced_by`
            // edge (l1ght_adapter mints `claim --evidenced_by--> marker`); when that
            // anchoring claim can't be found, skip rather than surface a marker
            // fragment as a claim (field-triage L28).
            let marker_label = graph
                .strings
                .resolve(graph.nodes.label[src_idx])
                .to_string();
            let confidence = parse_marker_confidence(&marker_label);
            let src_nid_for_claim = m1nd_core::types::NodeId::new(src_idx as u32);
            let claim = if is_marker_fragment("", &marker_label) {
                let resolved = graph
                    .csr
                    .in_range(src_nid_for_claim)
                    .filter_map(|rev_i| {
                        let claim_nid = graph.csr.rev_sources[rev_i];
                        let claim_label = graph
                            .strings
                            .try_resolve(graph.nodes.label[claim_nid.as_usize()])
                            .unwrap_or("");
                        // The parent claim/section is itself never a marker fragment.
                        (!claim_label.is_empty() && !is_marker_fragment("", claim_label))
                            .then(|| claim_label.to_string())
                    })
                    .next();
                match resolved {
                    Some(c) => c,
                    None => continue, // no real claim behind the marker — do not surface it
                }
            } else {
                marker_label
            };
            if !seen.insert(claim.clone()) {
                continue;
            }
            // Cheap freshness: does the cited code file still exist on disk?
            let tgt_ext = graph
                .id_to_node
                .iter()
                .find(|(_, &nid)| nid.as_usize() == tgt_idx)
                .map(|(interned, _)| graph.strings.resolve(*interned).to_string());
            let evidence_fresh = tgt_ext
                .as_deref()
                .and_then(|ext| state.file_inventory.get(ext))
                .map(|inv| std::path::Path::new(&inv.file_path).exists())
                .unwrap_or(true);

            let mut entry = serde_json::json!({
                "claim": claim,
                "anchored_to": anchor_id,
                "evidence_fresh": evidence_fresh,
            });
            if let Some(c) = confidence {
                entry["confidence"] = serde_json::json!(c);
            }
            out.push(entry);
            if out.len() >= 3 {
                break 'outer;
            }
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

// ---------------------------------------------------------------------------
// `orient` — boot into a task in one call (agent-first cold-start aggregation)
// ---------------------------------------------------------------------------

/// Top-N nodes by PageRank as the global "attention backbone".
///
/// Shared helper factored from the `graph_intelligence` block in
/// `handle_session_handshake` (tools.rs). Returns `{node_id, label, pagerank}`
/// objects, descending by score, skipping zero scores. Empty when PageRank has
/// not been computed yet.
fn top_pagerank_anchors(graph: &m1nd_core::graph::Graph, n: usize) -> Vec<serde_json::Value> {
    if !graph.pagerank_computed || graph.nodes.pagerank.is_empty() {
        return vec![];
    }
    // NodeId → external id reverse map (only for the few nodes we return).
    let mut nid_to_ext: std::collections::HashMap<usize, String> =
        std::collections::HashMap::with_capacity(graph.id_to_node.len());
    for (interned, &nid) in &graph.id_to_node {
        nid_to_ext.insert(nid.as_usize(), graph.strings.resolve(*interned).to_string());
    }
    let count = graph.nodes.count as usize;
    let mut ranked: Vec<(f32, usize)> = (0..count)
        .filter_map(|i| {
            let pr = graph.nodes.pagerank[i].get();
            if pr <= 0.0 {
                return None;
            }
            // Skip L1GHT marker fragments: on a memory-heavy graph they can rank into
            // the top-N and waste anchor slots (field-triage L28). The external id and
            // label are already resolvable here, so exclude before truncation — a real
            // code/claim node takes the slot instead of an annotation node.
            let ext = nid_to_ext.get(&i).map(String::as_str).unwrap_or("");
            let label = graph
                .strings
                .try_resolve(graph.nodes.label[i])
                .unwrap_or("");
            if is_marker_fragment(ext, label) {
                return None;
            }
            Some((pr, i))
        })
        .collect();
    ranked.sort_unstable_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(n);
    ranked
        .into_iter()
        .map(|(pr, idx)| {
            let ext_id = nid_to_ext.get(&idx).cloned().unwrap_or_default();
            let label = graph
                .strings
                .try_resolve(graph.nodes.label[idx])
                .unwrap_or("")
                .to_string();
            serde_json::json!({ "node_id": ext_id, "label": label, "pagerank": pr })
        })
        .collect()
}

/// `orient` — pre-pack an agent's STARTING CONTEXT from a free-form task string.
///
/// AGGREGATION handler: it composes existing primitives rather than
/// reimplementing them.
///   * spread-activation on the task text via `handle_activate` (which uses
///     `SessionState::run_query`, so this works in `--read-only` attach too) →
///     `focus_nodes`.
///   * `memory_nearby_for_result` over the focus nodes → prior conclusions.
///   * `top_pagerank_anchors` → global attention backbone.
///   * coverage state from `state.coverage_sessions` → visited/total +
///     high-PageRank unvisited files (or null when the agent has no session).
///   * the top focus node → concrete `suggested_first_calls` (surgical_context,
///     then why) so the agent's very next move is grounded.
///
/// READ-ONLY SAFE: only queries. Not in `read_only_denied`.
fn handle_orient(
    state: &mut SessionState,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "orient".into(),
            detail: "orient requires an `agent_id` string".into(),
        })?
        .to_string();
    let task = params
        .get("task")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "orient".into(),
            detail: "orient requires a `task` string describing what the agent is about to do"
                .into(),
        })?
        .to_string();
    if task.trim().is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "orient".into(),
            detail: "orient `task` must be non-empty".into(),
        });
    }
    let top_k = params
        .get("top_k")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(8)
        .clamp(1, 50);

    // 1. Spread-activate on the task text. handle_activate routes through
    //    run_query, which picks query_readonly in read-only mode — so orient is
    //    safe in --read-only attach. Reuse it wholesale; no reimplementation.
    let activate_input = ActivateInput {
        query: task.clone(),
        agent_id: agent_id.clone(),
        top_k,
        dimensions: vec![
            "structural".into(),
            "semantic".into(),
            "temporal".into(),
            "causal".into(),
        ],
        xlr: true,
        include_ghost_edges: false,
        include_structural_holes: false,
        token_budget: None,
    };
    let activate_out = tools::handle_activate(state, activate_input)?;

    // focus_nodes: compact projection of the activated nodes, ranked by activation.
    // Drop L1GHT marker fragments first (field-triage L28): on a memory-heavy graph a
    // marker node can activate and otherwise take a focus slot — and, as the top focus,
    // drive a nonsense `suggested_first_calls`. Filtering the activated set once keeps
    // both `focus_nodes` and `top_focus_id` grounded in real claim/code nodes.
    let focus_ranked: Vec<&_> = activate_out
        .activated
        .iter()
        .filter(|a| !is_marker_fragment(&a.node_id, &a.label))
        .collect();
    let focus_nodes: Vec<serde_json::Value> = focus_ranked
        .iter()
        .take(top_k)
        .map(|a| {
            let path = a.provenance.as_ref().and_then(|p| p.source_path.clone());
            serde_json::json!({
                "node_id": a.node_id,
                "label": a.label,
                "path": path,
                "pagerank": a.pagerank,
                "activation": a.activation,
                "kind": a.node_type,
            })
        })
        .collect();
    let top_focus_id = focus_ranked.first().map(|a| a.node_id.clone());

    // 2. memory_nearby: reuse memory_nearby_for_result over the focus nodes.
    //    It expects a `results` array of `{node_id|label}` — shape one from the
    //    focus nodes (capped at ~5 prior conclusions).
    let memory_nearby = {
        let pseudo_result = serde_json::json!({
            "results": activate_out
                .activated
                .iter()
                .take(5)
                .map(|a| serde_json::json!({ "node_id": a.node_id }))
                .collect::<Vec<_>>(),
        });
        memory_nearby_for_result(state, "activate", &pseudo_result).unwrap_or_default()
    };

    // 3. anchors: global PageRank attention backbone (cap 5).
    let anchors = {
        let graph = state.graph.read();
        top_pagerank_anchors(&graph, 5)
    };

    // 4. coverage: surface visited/total + a few high-PageRank unvisited files
    //    when the agent has a coverage session; otherwise null.
    let coverage = build_orient_coverage(state, &agent_id);

    // 5. suggested_first_calls: lead with surgical_context on the top focus node
    //    (grounded edit prep), then reuse suggest_next for textual guidance.
    let mut suggested_first_calls: Vec<serde_json::Value> = Vec::new();
    if let Some(ref node_id) = top_focus_id {
        suggested_first_calls.push(serde_json::json!({
            "tool": "surgical_context",
            "arguments": { "agent_id": agent_id, "node_id": node_id },
        }));
    }
    suggested_first_calls.push(serde_json::json!({
        "tool": "why",
        "arguments": { "agent_id": agent_id, "query": task },
    }));

    let summary = if let Some(first) = focus_nodes.first() {
        let label = first
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("the top focus node");
        format!(
            "Load {} first ({} focus node(s) activated for this task); then ground it with surgical_context.",
            label,
            focus_nodes.len()
        )
    } else {
        "No focus nodes activated for this task — try ingesting the relevant area or refining the task description.".to_string()
    };

    Ok(serde_json::json!({
        "task": task,
        "focus_nodes": focus_nodes,
        "memory_nearby": memory_nearby,
        "anchors": anchors,
        "coverage": coverage,
        "suggested_first_calls": suggested_first_calls,
        "proof_state": "triaging",
        "summary": summary,
    }))
}

/// The "north packet" — a single pre-orient handoff so an agent never starts
/// cold (Ω+1 ambient loop, in-repo primitive).
///
/// This is PURE COMPOSITION: it fans out four handlers that already ship and
/// assembles their honest outputs into ONE orientation packet — collapsing what
/// is otherwise 4 separate round-trips (`trust_selftest` → `orient` →
/// `boot_memory` → `focus`) into a SINGLE call. No graph logic, no new traversal.
///
/// The honesty signals pass straight through, never faked:
///   - `binding` carries the `trust_selftest` verdict + fingerprint verbatim; a
///     degraded/unbound binding keeps its attached `recovery_playbook` (the repair).
///   - an EMPTY / unbound graph honestly returns `needs: "needs_ingest"` with the
///     repair, NOT a fabricated orientation — `context`/`sufficiency` stay null.
///   - each durable memory carries a REAL `age_ms` (now − `updated_at_ms`) and its
///     `source_agent`; when a timestamp is somehow absent the age is ABSENT
///     (honest "unknown"), never faked to "now" — mirroring the `seek` provenance rule.
///   - `sufficiency` is the answer-free stop signal lifted from `focus`, or null
///     when the graph can't answer yet.
///   - `honest_gaps` names what m1nd does NOT yet know for this task.
///
/// Freshest-first ordering key for the broad (non-task-scoped) L1GHT recall
/// fallback. `authored_ms_ago` is an AGE (now − Created), so a smaller value is
/// fresher and an absent value is an UNKNOWN age (undated legacy claim). The key
/// `(is_none, age)` sorts dated claims ahead of undated ones (`false` < `true`)
/// and, within the dated claims, ascending age = freshest first. A plain
/// `sort_by_key(|r| r.authored_ms_ago)` is inverted, because Rust orders
/// `None < Some(_)`, which would float every undated claim to the front.
fn light_recall_freshness_key(authored_ms_ago: Option<u64>) -> (bool, Option<u64>) {
    (authored_ms_ago.is_none(), authored_ms_ago)
}

/// Read-only safe: every composed handler is read-only (`trust_selftest`,
/// `orient`→`activate`, `boot_memory action=list`, `focus`→`seek` all route
/// through read-only-safe paths).
fn handle_north(
    state: &mut SessionState,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "north".into(),
            detail: "north requires an `agent_id` string".into(),
        })?
        .to_string();
    let task = params
        .get("task")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "north".into(),
            detail: "north requires a `task` string describing what the agent is about to do"
                .into(),
        })?
        .to_string();
    if task.trim().is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "north".into(),
            detail: "north `task` must be non-empty".into(),
        });
    }
    let top_k = params
        .get("top_k")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(8)
        .clamp(1, 50);
    let scope = params
        .get("scope")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // 1. BINDING — one trust_selftest gives the verdict + fingerprint + graph
    //    state + (when not full) the attached recovery_playbook. Reuse wholesale;
    //    it internally composes session_handshake + recovery_playbook already.
    let trust = tools::handle_trust_selftest(
        state,
        TrustSelftestInput {
            agent_id: agent_id.clone(),
            observed_tool_count: None,
            available_tools: Vec::new(),
            missing_tools: Vec::new(),
            observed_tool: None,
            observed_proof_state: None,
            observed_candidates: None,
            scope: scope.clone(),
            error_text: None,
        },
    )?;
    let verdict = trust
        .get("verdict")
        .and_then(|v| v.as_str())
        .unwrap_or("orientation_only")
        .to_string();
    let binding_ok = trust.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    let graph_populated = trust
        .get("checks")
        .and_then(|c| c.get("graph_populated"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // The graph can't ground context yet when it is empty/unbound. In that case
    // we return the repair honestly rather than a fabricated orientation.
    let needs_ingest = verdict == "needs_ingest"
        || (!graph_populated && matches!(verdict.as_str(), "needs_ingest" | "orientation_only"));
    let binding = serde_json::json!({
        "trust_mode": verdict,
        "fingerprint": trust.get("binding_fingerprint").cloned().unwrap_or(serde_json::Value::Null),
        "ok": binding_ok,
        "graph_populated": graph_populated,
        "graph_state": trust.get("graph_state").cloned().unwrap_or(serde_json::Value::Null),
    });
    // The repair path travels with the packet whenever the binding is not full
    // trust, so the agent gets the fix, not just the diagnosis.
    let recovery_playbook = trust
        .get("recovery_playbook")
        .cloned()
        .filter(|v| !v.is_null());

    // 2. MEMORY — durable cross-session recall from BOTH memory systems, merged:
    //    (a) boot_memory KV (action=list), and (b) L1GHT agent-memory written by
    //    `memorize` (the primary memory system). Each entry carries a REAL age and
    //    its authoring agent when known. If a timestamp is ever absent, the age is
    //    ABSENT (honest "unknown"), never faked to "now" — the same rule seek's
    //    recall provenance follows. The two feeds are concatenated so a cold agent
    //    sees the durable KV facts AND the memorized L1GHT claims for its task.
    let now = now_ms();
    let stale_after_ms: u64 = 30 * 24 * 60 * 60 * 1000; // 30 days
                                                        // This store's own memory tier
                                                        // (project brain vs medulla) — every
                                                        // row from THIS store's beat carries it.
    let own_beat_tier = if state.is_medulla_store() {
        "medulla"
    } else {
        "project"
    };
    // (a) boot_memory KV entries.
    let boot_entries: Vec<serde_json::Value> = {
        let list = crate::boot_memory_handlers::handle_boot_memory(
            state,
            crate::boot_memory_handlers::BootMemoryInput {
                agent_id: agent_id.clone(),
                action: "list".into(),
                key: None,
                value: None,
                tags: Vec::new(),
                source_refs: Vec::new(),
            },
        )?;
        list.get("entries")
            .and_then(|e| e.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| {
                        let updated_at_ms = entry.get("updated_at_ms").and_then(|v| v.as_u64());
                        // age_ms: Some(now − updated) when the stamp is present and
                        // sane; None (absent) when unknown — never fabricated.
                        let age_ms = updated_at_ms
                            .filter(|&ts| ts > 0 && ts <= now)
                            .map(|ts| now.saturating_sub(ts));
                        let stale = age_ms.map(|age| age > stale_after_ms);
                        let mut obj = serde_json::Map::new();
                        obj.insert("kind".into(), serde_json::json!("boot_memory"));
                        obj.insert(
                            "claim".into(),
                            entry.get("key").cloned().unwrap_or(serde_json::Value::Null),
                        );
                        if let Some(age) = age_ms {
                            obj.insert("age_ms".into(), serde_json::json!(age));
                        }
                        obj.insert(
                            "source_agent".into(),
                            entry
                                .get("updated_by_agent")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null),
                        );
                        if let Some(stale) = stale {
                            obj.insert("stale".into(), serde_json::json!(stale));
                        }
                        // Tier label so a composed beat can tell project KV from
                        // medulla KV (MEDULLA-PRD §10.4). boot KV has no per-row
                        // origin brain; the tier is the store's own tier.
                        obj.insert("tier".into(), serde_json::json!(own_beat_tier));
                        obj.insert(
                            "tags".into(),
                            entry.get("tags").cloned().unwrap_or(serde_json::json!([])),
                        );
                        obj.insert(
                            "source_refs".into(),
                            entry
                                .get("source_refs")
                                .cloned()
                                .unwrap_or(serde_json::json!([])),
                        );
                        serde_json::Value::Object(obj)
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    // (b) L1GHT agent-memory recall — the fix. `memorize` (the primary memory
    //     system) writes graph-native `.light.md` claims that the runtime auto-loads
    //     and that `seek` already surfaces WITH provenance (`source_agent`,
    //     `authored_ms_ago` — stamped only on `.light.md` hits, never on code nodes).
    //     Compose that recall INTO the packet so a memorized-at-close claim compounds
    //     into the next agent's north.
    //
    //     ROBUST ON A MIXED GRAPH (field-triage #6). Every L1GHT node's external id
    //     is `light::<namespace>::…` (l1ght_adapter), so we scope the recall seek to
    //     the `light::` id prefix. seek's own scope filter matches the node-id prefix
    //     (layer_handlers, `ext.starts_with(scope)`), so CODE nodes are structurally
    //     excluded from the recall pass BEFORE scoring — the note surfaces regardless
    //     of how a large code corpus ranks. This replaces the previous approach of
    //     post-filtering a task-scoped top-K to light provenance, which silently
    //     returned empty once code nodes dominated the window (the live bug: north.
    //     memory=[] / "No durable memory yet" while a direct seek found the note at
    //     rank #2). We reuse seek's existing `scope` parameter wholesale — no new
    //     retrieval, and seek's behavior for normal callers is unchanged.
    //
    //     Note the caller's own `scope` (a CODE-context filter) is deliberately NOT
    //     applied here: memory is cross-cutting, so a north scoped to one code area
    //     must still recall the memories relevant to its task. We still keep only the
    //     light-provenance hits as a belt-and-suspenders guard, and when the task-
    //     scoped recall is empty we fall back to a broad recall so a cold agent still
    //     sees that institutional memory EXISTS rather than implying there is none.
    const LIGHT_RECALL_SCOPE: &str = "light::";
    let light_limit = 5usize;
    // A recall hit is a real memory only if it carries authorship provenance AND is
    // not a L1GHT marker fragment. Marker nodes (`[𝔻 confidence: …]`, `[𝔻 evidence: …]`,
    // …) inherit their file's `source_agent`/`authored_ms_ago` prov-tags, so provenance
    // alone is NOT enough to keep them — they would occupy memory slots that belong to
    // the claim/section rows (field-triage L28). Excluding them here means markers never
    // enter `hits`, so a claim always takes the slot instead of its own annotation.
    let is_light_hit = |r: &layers::SeekResultEntry| -> bool {
        (r.source_agent.is_some() || r.authored_ms_ago.is_some())
            && !is_marker_fragment(&r.node_id, &r.label)
    };
    // The tier this brain's OWN memory feed carries (MEDULLA-PRD §5.1 · §10.4): a
    // routed project brain's own recall is `project`; the medulla/owner store's own
    // recall is `medulla`. The cross-store compose (adding the medulla feed to a
    // project beat, and the `all-brains` fan-out) is done by the routing layer AROUND
    // north — here we only honestly label the single store north ran on.
    let own_tier = if state.is_medulla_store() {
        "medulla"
    } else {
        "project"
    };
    let map_light = |r: &layers::SeekResultEntry| -> serde_json::Value {
        // age_ms is the authored age seek already computed (now − Created); absent
        // stays absent — never fabricated. staleness uses the same 30-day rule.
        let age_ms = r.authored_ms_ago;
        let stale = age_ms.map(|age| age > stale_after_ms);
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!("light"));
        obj.insert("claim".into(), serde_json::json!(r.label));
        if let Some(age) = age_ms {
            obj.insert("age_ms".into(), serde_json::json!(age));
        }
        obj.insert(
            "source_agent".into(),
            r.source_agent
                .clone()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
        // Provenance-in-recall (MEDULLA-PRD §6 · MED-INV-4): which brain the claim was
        // born in + its tier. `origin_brain` is absent (unknown) on legacy files —
        // rendered so, never faked. `tier` is this store's own tier (see above).
        obj.insert("tier".into(), serde_json::json!(own_tier));
        if let Some(origin) = &r.origin_brain {
            obj.insert("origin_brain".into(), serde_json::json!(origin));
        }
        if let Some(stale) = stale {
            obj.insert("stale".into(), serde_json::json!(stale));
        }
        obj.insert("node_id".into(), serde_json::json!(r.node_id));
        serde_json::Value::Object(obj)
    };
    let light_entries: Vec<serde_json::Value> = if graph_populated {
        let mut seek_light = |query: &str, k: usize| -> Vec<layers::SeekResultEntry> {
            layer_handlers::handle_seek(
                state,
                layers::SeekInput {
                    query: query.to_string(),
                    agent_id: agent_id.clone(),
                    top_k: k,
                    // Scope the recall to the L1GHT id namespace so code nodes never
                    // compete for the window — deterministic on any graph mix.
                    scope: Some(LIGHT_RECALL_SCOPE.to_string()),
                    node_types: Vec::new(),
                    min_score: 0.1,
                    graph_rerank: true,
                    conformance_aware: true,
                    token_budget: None,
                },
            )
            .map(|o| o.results.into_iter().filter(|r| is_light_hit(r)).collect())
            .unwrap_or_default()
        };
        // Task-scoped recall first: the memories most relevant to what the agent is
        // about to do. The scope above already excludes code, so this ranks light
        // nodes against each other only.
        let mut hits = seek_light(&task, 24);
        if hits.is_empty() {
            // No task-relevant memory surfaced — fall back to a broad memory recall so
            // a cold agent still sees that institutional memory EXISTS (honest: this is
            // "memory exists, not necessarily about your task", surfaced most-recent).
            hits = seek_light("memory decision finding note claim", 24);
            // Prefer the freshest few when the recall is not task-scoped. See
            // `light_recall_freshness_key`: dated claims first, freshest within
            // them, undated (unknown-age) claims last — NOT the inverted
            // `None`-first order a bare `sort_by_key(authored_ms_ago)` gives.
            hits.sort_by_key(|r| light_recall_freshness_key(r.authored_ms_ago));
        }
        // De-dup by node_id (a memory can surface under both label and evidence path)
        // and cap at light_limit.
        let mut seen = std::collections::HashSet::new();
        hits.into_iter()
            .filter(|r| seen.insert(r.node_id.clone()))
            .take(light_limit)
            .map(|r| map_light(&r))
            .collect()
    } else {
        Vec::new()
    };

    // Merge both feeds: durable KV facts first, then the memorized L1GHT claims.
    let memory: Vec<serde_json::Value> = boot_entries
        .iter()
        .cloned()
        .chain(light_entries.iter().cloned())
        .collect();

    // Honest gaps — what m1nd does NOT yet know for this task. A pre-orient MUST
    // say what it can't see rather than imply omniscience.
    let mut honest_gaps: Vec<String> = Vec::new();

    // P1 medulla-only read fallback (TWO-TIER-BRAIN-PRD §9.5 · §10.4 rung 3 ·
    // TT-INV-2). The caller's root is KNOWN, no project brain covers it, and THIS
    // store is the medulla. The medulla's cross-project doctrine + promoted memory
    // (composed above, tier/origin-labeled) is a LEGITIMATE feed — but its own
    // CODE graph maps a DIFFERENT repo, so handing back its focus_nodes/anchors as
    // "your context" is context poisoning. Under the fallback the honest story is
    // `project_brain_absent`, NOT an unfinished ingest: suppress the needs_ingest
    // (empty/unbound) narrative so a served-medulla beat is never mislabeled as
    // "nothing ingested" (requirement 4).
    let brainless_caller = state.caller_root_is_brainless();
    let needs_ingest = needs_ingest && !brainless_caller;

    // 3 + 4. CONTEXT + SUFFICIENCY — only meaningful once the graph is bound and
    //    populated. When it isn't, we say so honestly (needs_ingest) instead of
    //    running orient/focus over an empty graph and returning a fake packet.
    let (context, sufficiency, next_move) = if brainless_caller {
        // Cut the poison: NO code anchors from the foreign graph cross to the
        // caller. Context is null, the gap is the canonical project_brain_absent,
        // and the next move is the honest recovery (the same closed-bootstrap
        // language the write path uses — never an invented `m1nd init` birth).
        honest_gaps.push(crate::session::PROJECT_BRAIN_ABSENT_GAP.to_string());
        (
            serde_json::Value::Null,
            serde_json::Value::Null,
            "No project brain covers your repo — the medulla's cross-project doctrine is served as memory, but its code graph does not map your repo. Creating a project brain is unavailable until the typed bootstrap consumer is installed; see `reception` for the honest options.".to_string(),
        )
    } else if needs_ingest || !graph_populated {
        // ONE authoring site for this fact (human_view amendment 5): the same
        // constant the S4 voice card wraps verbatim — byte-equal by construction.
        honest_gaps.push(crate::human_view::NEEDS_INGEST_GAP.into());
        (
            serde_json::Value::Null,
            serde_json::Value::Null,
            "Run ingest for the intended repo, then call north again to get grounded context."
                .to_string(),
        )
    } else {
        // CONTEXT — reuse orient wholesale (focus_nodes + anchors + coverage +
        // memory_nearby + suggested_first_calls). Zero reimplementation.
        let orient = handle_orient(
            state,
            &serde_json::json!({
                "agent_id": agent_id,
                "task": task,
                "top_k": top_k,
            }),
        )?;
        let focus_nodes = orient
            .get("focus_nodes")
            .cloned()
            .unwrap_or(serde_json::json!([]));
        let anchors = orient
            .get("anchors")
            .cloned()
            .unwrap_or(serde_json::json!([]));
        let focus_empty = focus_nodes.as_array().map(|a| a.is_empty()).unwrap_or(true);
        if focus_empty {
            honest_gaps.push(
                "No focus nodes activated for this task — the relevant area may not be ingested, or the task text may not match the graph."
                    .into(),
            );
        }
        let context = serde_json::json!({
            "focus_nodes": focus_nodes,
            "anchors": anchors,
            "coverage": orient.get("coverage").cloned().unwrap_or(serde_json::Value::Null),
            "memory_nearby": orient.get("memory_nearby").cloned().unwrap_or(serde_json::Value::Null),
        });

        // SUFFICIENCY — the answer-free stop signal, lifted from focus. focus wraps
        // seek with budget packing and reports sufficient | gathering | saturated.
        let focus_out = layer_handlers::handle_focus(
            state,
            layers::FocusInput {
                goal: task.clone(),
                agent_id: agent_id.clone(),
                token_budget: 2000,
                top_k: 60,
                scope: scope.clone(),
                node_types: Vec::new(),
                min_score: 0.1,
            },
        )?;
        let sufficiency =
            serde_json::to_value(&focus_out.sufficiency).unwrap_or(serde_json::Value::Null);

        // next_move — one honest suggested first action. Lead with the concrete
        // grounded call orient already proposes (surgical_context on the top
        // focus node); fall back to a re-scope hint when nothing activated.
        let next_move = orient
            .get("suggested_first_calls")
            .and_then(|c| c.as_array())
            .and_then(|calls| calls.first())
            .map(|call| {
                let tool = call.get("tool").and_then(|v| v.as_str()).unwrap_or("view");
                format!("Call `{tool}` on the top focus node to ground the task before editing.")
            })
            .unwrap_or_else(|| {
                "No focus nodes activated — refine the task text or ingest the relevant area, then call north again.".to_string()
            });

        (context, sufficiency, next_move)
    };

    if !binding_ok {
        honest_gaps.push(format!(
            "Binding is not full trust ({verdict}) — treat retrieval as orientation only and verify final truth against local files; see recovery_playbook for the repair."
        ));
    }
    // Ground-truth count of the durable L1GHT store on disk. A beat that surfaced
    // no memory must NOT claim "no durable memory yet" when the store is non-empty
    // (MED-INV-6 false-absence): recall missing a task-relevant hit is not the same
    // as an empty store. When N>0 we stamp `memory_exists: N` and say so honestly.
    let light_memory_on_disk = state.light_memory_count();
    if memory.is_empty() {
        if light_memory_on_disk > 0 {
            honest_gaps.push(format!(
                "The memory store holds {light_memory_on_disk} durable L1GHT claim(s), but none surfaced for this task — recall found no task-relevant match, not an empty store. Broaden the task text or seek the store directly."
            ));
        } else {
            honest_gaps.push(
                "No durable memory yet — neither boot_memory nor L1GHT agent-memory holds a prior cross-session claim to carry.".into(),
            );
        }
    } else if light_entries.is_empty() && graph_populated {
        // Boot KV facts exist but no memorized L1GHT claim surfaced — say so, so the
        // agent knows the primary (memorize) memory had nothing to add for this task.
        honest_gaps.push(
            "No L1GHT agent-memory claim surfaced for this task — only durable boot_memory facts are carried; `memorize` findings, if any, did not match.".into(),
        );
    }

    // Skeleton coherence is a vital sign, never a gate. Reuse the snapshot read
    // surface and fail open if its sidecar cannot be read so `north` itself never
    // becomes unavailable because of this signal. The composed line is captured
    // so the human_view card can reuse it VERBATIM (amendment 5: one sentence
    // per fact — never a second wording).
    let mut coherence_line: Option<String> = None;
    // Slice-2 map fact (HUMAN-VIEW voice): the SERVED brain's ratified
    // SystemBlock count + its coherence status. Lifted from the SAME snapshot
    // read as the coherence signal (one disk touch, no second read). PER-BRAIN
    // by construction — `handle_system_blocks_snapshot` reads only the bound
    // brain's store; never a cross-brain total. The structured `map` field
    // rides ONLY when a store exists (absent — not null — otherwise, mirroring
    // `landing_bell`: no empty ornament).
    let mut ratified_blocks: usize = 0;
    let mut map_field: Option<serde_json::Value> = None;
    if let Ok(snapshot) = crate::system_blocks_handlers::handle_system_blocks_snapshot(
        state,
        crate::system_blocks_handlers::SnapshotInput {
            agent_id: Some(agent_id.clone()),
        },
    ) {
        if snapshot["skeleton_coherence"]["status"] == "mismatch" {
            let expected_slug = snapshot["skeleton_coherence"]["expected_slug"]
                .as_str()
                .unwrap_or("unknown");
            let found_slug = snapshot["skeleton_coherence"]["found_slug"]
                .as_str()
                .unwrap_or("unknown");
            let line = format!(
                "Skeleton coherence sickness: serving brain expects slug `{expected_slug}`, but the SystemBlock store carries `{found_slug}` — signal only; reads and writes remain available."
            );
            honest_gaps.push(line.clone());
            coherence_line = Some(line);
        }
        if snapshot["present"] == serde_json::Value::Bool(true) {
            ratified_blocks = snapshot["store"]["blocks"]
                .as_array()
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter(|b| b["state"] == serde_json::Value::String("ratified".into()))
                        .count()
                })
                .unwrap_or(0);
            let coherence_status = snapshot["skeleton_coherence"]["status"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            map_field = Some(serde_json::json!({
                "ratified_blocks": ratified_blocks,
                "coherence": coherence_status,
            }));
        }
    }

    // The landing bell is a vital sign, never a gate. Reuse the mission-box read
    // surface (the same box the tray and `mission_post` speak) and fail OPEN if the
    // box cannot be read, so `north` itself never becomes unavailable over this
    // signal. It counts the missions whose CURRENT head is `merge_wait`:
    // `heads_by_mission` resolves each chain's tip, so a mission that later landed
    // or failed has moved its head and never rings — only one still waiting on the
    // human landing does. Deliberately NO tail cap: a `merge_wait` head can sit far
    // back in the append-only box while newer letters for other missions push it
    // down, so reading only the tail would risk a false silence (a waiting mission
    // missed) — worse than the read the tray already pays on every poll.
    let mut landing_bell: Option<serde_json::Value> = None;
    let mut bell_line: Option<String> = None;
    let mut bell_merge_wait: usize = 0;
    let mission_box = crate::mission_letter_handlers::mission_box_path(state);
    if let Ok(letters) = crate::mailbox::read_letters(&mission_box) {
        let merge_wait = crate::mission_letter::heads_by_mission(&letters)
            .into_values()
            .filter(|h| h.head.phase == crate::mission_letter::Phase::MergeWait)
            .count();
        if merge_wait > 0 {
            // Composed ONCE; the human_view card reuses this exact string
            // (amendment 5) — byte-equal to the honest_gaps entry by construction.
            let line = format!(
                "{merge_wait} mission(s) in merge_wait await the human landing — the tray is the door"
            );
            honest_gaps.push(line.clone());
            bell_line = Some(line);
            bell_merge_wait = merge_wait;
            landing_bell = Some(serde_json::json!({ "merge_wait": merge_wait }));
        }
    }

    // P1 (ORGANISM-INSIDE): the collision gap — present ONLY when THIS session
    // collides with another live mutating hand on the SAME brain whose work
    // co-locates (same caller_root/worktree or overlapping working set). It rides
    // the EXISTING honest_gaps mechanism (no new north schema field), and the P1
    // gate requires it on BOTH colliding sessions' packets — each session derives
    // its own here. Advisory, never blocking (the same posture as reception).
    // Fail-open + read-only: an unreadable registry yields no line, never breaks
    // north; presence is witness tissue that verifies nothing.
    {
        let (roster, _) = state.presence_roster();
        let now = crate::util::now_ms();
        let collisions = crate::presence::collisions_for_agent(&roster, &agent_id, now);
        if !collisions.is_empty() {
            let others: Vec<String> = collisions
                .iter()
                .map(|c| format!("{} ({})", c.other_agent, c.overlap.join(", ")))
                .collect();
            honest_gaps.push(format!(
                "COLLISION: another mutating session shares this brain and your work — {}. Advisory only (m1nd sees activity visible to it, not git); coordinate before you both write.",
                others.join("; ")
            ));
        }
    }

    // First-Contact Reception (TWO-TIER-BRAIN-PRD §9.5.5): on a caller_root
    // mismatch this carries the honest front-desk block; on match/unknown it is
    // null (absent-ish). Computed here, after every prior `state` borrow has
    // returned, so it never conflicts with the earlier `&mut state` uses.
    let reception = state.reception_verdict();

    // The m1nd voice (`human_view`, m1nd-human-view-v0) — the server-composed,
    // already-mounted card for the HUMAN in the conversation. Composed AFTER
    // reception (amendment 3: a card composed before it would describe the
    // wrong brain — under `caller_root_mismatch` the card IS the warning and
    // carries zero statistics). Every line is a measured fact already in this
    // packet or an honest_gaps string reused VERBATIM (amendments 5 + 8/G1).
    // Fail-open: the composer is pure and total; `north` never becomes
    // unavailable over its own voice.
    let human_view = {
        let reception_mismatch = reception.as_ref().and_then(|r| {
            (r.get("match").and_then(|m| m.as_str()) == Some("caller_root_mismatch")).then(|| {
                crate::human_view::ReceptionMismatch {
                    honest: r
                        .get("honest")
                        .and_then(|v| v.as_str())
                        .unwrap_or("this graph does NOT cover your repo"),
                    caller_root: r
                        .get("caller_root")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown"),
                    bound_workspace: r
                        .get("bound_workspace")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown"),
                }
            })
        });
        let node_count = state.graph.read().num_nodes() as u64;
        // The pulse's `focus` cell reads whether orient activated any focus node
        // for this task — lifted from the context slice already composed above
        // (null under needs_ingest, where the pulse ignores this cell anyway).
        let focus_activated = context
            .get("focus_nodes")
            .and_then(|f| f.as_array())
            .map(|nodes| !nodes.is_empty())
            .unwrap_or(false);
        crate::human_view::compose_human_view(&crate::human_view::HumanViewInput {
            trust_mode: binding
                .get("trust_mode")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown"),
            node_count,
            memory_count: light_memory_on_disk,
            ratified_blocks,
            focus_activated,
            merge_wait: bell_merge_wait,
            bell_line: bell_line.as_deref(),
            coherence_line: coherence_line.as_deref(),
            needs_ingest,
            reception_mismatch,
            next_move: &next_move,
        })
    };

    let mut packet = serde_json::json!({
        "schema": "m1nd-north-packet-v0",
        "task": task,
        "binding": binding,
        "context": context,
        "memory": memory,
        // Ground-truth size of the durable L1GHT store on disk (MED-INV-6). A
        // consumer never has to infer "is the store empty?" from `memory: []` —
        // an empty beat over a non-empty store carries `memory_exists > 0` and the
        // honest gap says the store has claims that just did not match this task.
        "memory_exists": light_memory_on_disk,
        "sufficiency": sufficiency,
        "next_move": next_move,
        "honest_gaps": honest_gaps,
        "reception": reception.unwrap_or(serde_json::Value::Null),
        "needs": if needs_ingest { serde_json::json!("needs_ingest") } else { serde_json::Value::Null },
        "recovery_playbook": recovery_playbook.unwrap_or(serde_json::Value::Null),
        "proof_state": "triaging",
        "non_claims": [
            "north composes existing read-only verbs; it does not ingest, mutate, or repair the graph.",
            "north does not refresh the host MCP binding.",
            "north does not replace compiler, tests, or local file truth.",
            "an absent memory age means unknown authored time, never freshly authored."
        ],
    });
    // The landing bell rides ONLY when it rings: a zero-`merge_wait` beat carries no
    // decorative field (design point 1 — no empty ornament). Present iff N>0.
    if let Some(bell) = landing_bell {
        if let Some(obj) = packet.as_object_mut() {
            obj.insert("landing_bell".to_string(), bell);
        }
    }
    // The map fact rides ONLY when the served brain has a SystemBlock store
    // (slice-2 voice: ratified-block count + coherence, per-brain). Absent — not
    // null — when no store exists (no empty ornament, mirroring landing_bell).
    if let Some(map) = map_field {
        if let Some(obj) = packet.as_object_mut() {
            obj.insert("map".to_string(), map);
        }
    }
    // The voice card rides ONLY when composed (fail-open: a compose miss omits
    // the field — never a null ornament, and never an error on the packet).
    if let Some(card) = human_view {
        if let Some(obj) = packet.as_object_mut() {
            obj.insert("human_view".to_string(), card);
        }
    }
    Ok(packet)
}

/// Thin wrapper so the delegation layer can reuse `orient`'s composition
/// (anchors + focus_nodes) without re-implementing it. `orient` is private to
/// this module; `delegate` is in `north`'s class and composes the same pieces.
pub(crate) fn handle_orient_for_delegate(
    state: &mut SessionState,
    agent_id: &str,
    task: &str,
    top_k: u64,
) -> M1ndResult<serde_json::Value> {
    handle_orient(
        state,
        &serde_json::json!({
            "agent_id": agent_id,
            "task": task,
            "top_k": top_k,
        }),
    )
}

/// Resolve a node id to its absolute on-disk file path for `am_i_stale`.
///
/// Two paths, both grounded in already-recorded state — no new traversal logic:
///   1. The node id is itself a `file::…` inventory key → return its recorded
///      `file_path` directly.
///   2. Otherwise look the node up in the graph and use its provenance
///      `source_path` (the file the node was ingested from).
///
/// Returns `None` when the node is unknown / has no file provenance.
fn resolve_node_file_path(state: &SessionState, node_id: &str) -> Option<String> {
    // Fast path: the node id is already a file inventory key.
    if let Some(entry) = state.file_inventory.get(node_id) {
        return Some(entry.file_path.clone());
    }
    // Otherwise resolve via the graph's provenance source_path.
    let graph = state.graph.read();
    let interned = graph.strings.lookup(node_id)?;
    let nid = *graph.id_to_node.get(&interned)?;
    let prov = graph.resolve_node_provenance(nid);
    prov.source_path
}

/// `am_i_stale` — tell a long-running agent which files in its working set have
/// changed on disk SINCE m1nd ingested them, so it knows to re-read before
/// acting. The perception an agent structurally lacks.
///
/// AGGREGATION handler: it composes existing primitives and reimplements
/// nothing.
///   * `state.file_inventory` is the "what m1nd last saw" baseline — each entry
///     records the absolute `file_path` and the `sha256` captured at ingest.
///   * `audit_handlers::content_sha256` recomputes the current on-disk SHA-256
///     with the SAME routine the ingest path used, so a recomputed hash is
///     directly comparable to the stored one (shared fn — no second hasher).
///   * `state.coverage_sessions[agent_id].visited_files` is the DEFAULT working
///     set when the caller passes neither `files` nor `nodes`: "you don't even
///     have to tell me what you're holding; I'll check what you've touched".
///
/// READ-ONLY SAFE: only reads disk + inventory. Not in `read_only_denied`.
fn handle_am_i_stale(
    state: &mut SessionState,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "am_i_stale".into(),
            detail: "am_i_stale requires an `agent_id` string".into(),
        })?
        .to_string();

    let explicit_files: Vec<String> = params
        .get("files")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let explicit_nodes: Vec<String> = params
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Decide the working set + its provenance. Each target is (path, node_id?).
    // `note` is a caller-facing explanation when a target couldn't be turned
    // into a checkable path (e.g. an unresolvable node id).
    let mut targets: Vec<(String, Option<String>)> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let source: &str;

    if !explicit_files.is_empty() {
        source = "explicit_files";
        for path in explicit_files {
            targets.push((path, None));
        }
    } else if !explicit_nodes.is_empty() {
        source = "explicit_nodes";
        for node_id in explicit_nodes {
            match resolve_node_file_path(state, &node_id) {
                Some(path) => targets.push((path, Some(node_id))),
                None => notes.push(format!(
                    "node `{}` could not be resolved to a file path (unknown or not file-backed)",
                    node_id
                )),
            }
        }
    } else if let Some(session) = state.coverage_sessions.get(&agent_id) {
        source = "coverage_session";
        for path in &session.visited_files {
            targets.push((path.clone(), None));
        }
    } else {
        source = "empty";
    }

    // Build a quick lookup from absolute file_path → inventory entry. The
    // inventory is keyed by external_id, but visited_files / explicit files are
    // disk paths, so index by file_path (mirrors audit_handlers usage). We also
    // index by the canonicalized form so a caller passing a non-canonical path
    // (e.g. /var/… on macOS where the inventory recorded /private/var/…) still
    // matches the same baseline.
    let mut inventory_by_path: std::collections::HashMap<
        String,
        &crate::session::FileInventoryEntry,
    > = std::collections::HashMap::with_capacity(state.file_inventory.len() * 2);
    for entry in state.file_inventory.values() {
        inventory_by_path.insert(entry.file_path.clone(), entry);
        if let Ok(canon) = std::fs::canonicalize(&entry.file_path) {
            inventory_by_path.insert(canon.to_string_lossy().to_string(), entry);
        }
    }

    let mut stale: Vec<serde_json::Value> = Vec::new();
    let mut fresh: Vec<String> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();

    for (path, node_id) in &targets {
        let entry = inventory_by_path.get(path.as_str()).copied().or_else(|| {
            std::fs::canonicalize(path)
                .ok()
                .and_then(|c| inventory_by_path.get(c.to_string_lossy().as_ref()).copied())
        });
        let Some(entry) = entry else {
            // Never ingested → m1nd has no baseline for this path.
            unknown.push(path.clone());
            continue;
        };
        let disk_path = std::path::Path::new(path.as_str());
        if !disk_path.exists() {
            let mut item = serde_json::json!({ "path": path, "reason": "missing" });
            if let Some(nid) = node_id {
                item["node_id"] = serde_json::Value::String(nid.clone());
            }
            stale.push(item);
            continue;
        }
        let current_hash = crate::audit_handlers::content_sha256(disk_path);
        match (&entry.sha256, current_hash) {
            (Some(known), Some(now)) if known != &now => {
                let mut item = serde_json::json!({ "path": path, "reason": "changed" });
                if let Some(nid) = node_id {
                    item["node_id"] = serde_json::Value::String(nid.clone());
                }
                stale.push(item);
            }
            (Some(_), Some(_)) => {
                // Hash matches — fresh.
                fresh.push(path.clone());
            }
            _ => {
                // No recorded hash, or the file couldn't be re-read: we can't
                // make a confident staleness judgement, so report as unknown
                // rather than silently calling it fresh.
                unknown.push(path.clone());
            }
        }
    }

    let checked = stale.len() + fresh.len() + unknown.len();

    let summary = if source == "empty" {
        notes.push(
            "no `files`/`nodes` given and agent has no coverage session — nothing to check".into(),
        );
        format!(
            "0 files checked: agent `{}` has no coverage session and you passed no files or nodes.",
            agent_id
        )
    } else if checked == 0 {
        format!(
            "0 files checked ({}): nothing in your working set was tracked in m1nd's file inventory.",
            source
        )
    } else if stale.is_empty() {
        format!(
            "All {} file(s) checked ({}) are fresh — nothing changed on disk since ingest.",
            checked, source
        )
    } else {
        let stale_paths: Vec<&str> = stale
            .iter()
            .filter_map(|s| s.get("path").and_then(|p| p.as_str()))
            .collect();
        let preview: Vec<&str> = stale_paths.iter().take(3).copied().collect();
        let suffix = if stale_paths.len() > preview.len() {
            format!(
                "{}, +{} more",
                preview.join(", "),
                stale_paths.len() - preview.len()
            )
        } else {
            preview.join(", ")
        };
        let touched = if source == "coverage_session" {
            "you've touched"
        } else {
            "you're checking"
        };
        format!(
            "{} of {} files {} changed since ingest — re-read {} before editing.",
            stale.len(),
            checked,
            touched,
            suffix
        )
    };

    let mut out = serde_json::json!({
        "checked": checked,
        "stale": stale,
        "fresh": fresh,
        "unknown": unknown,
        "source": source,
        "summary": summary,
    });
    if !notes.is_empty() {
        out["notes"] =
            serde_json::Value::Array(notes.into_iter().map(serde_json::Value::String).collect());
    }
    Ok(out)
}

/// Build the `coverage` block for `orient` from `state.coverage_sessions`.
///
/// Returns `{visited, total, unvisited_high_value:[paths]}` when the agent has a
/// coverage session, otherwise `null`. `unvisited_high_value` lists up to 5 file
/// paths with the highest PageRank that the agent has not yet visited.
fn build_orient_coverage(state: &SessionState, agent_id: &str) -> serde_json::Value {
    let Some(session) = state.coverage_sessions.get(agent_id) else {
        return serde_json::Value::Null;
    };
    let graph = state.graph.read();
    let total = graph.nodes.count as usize;
    let visited = session.visited_nodes.len();

    // High-PageRank file nodes the agent has not visited yet.
    let mut unvisited: Vec<(f32, String)> = Vec::new();
    if graph.pagerank_computed && !graph.nodes.pagerank.is_empty() {
        for (interned, &nid) in &graph.id_to_node {
            let ext = graph.strings.resolve(*interned).to_string();
            if session.visited_nodes.contains(&ext) || session.visited_files.contains(&ext) {
                continue;
            }
            // Prefer file-level nodes for the "what to read next" hint.
            if !ext.starts_with("file::") {
                continue;
            }
            let idx = nid.as_usize();
            let pr = graph
                .nodes
                .pagerank
                .get(idx)
                .map(|p| p.get())
                .unwrap_or(0.0);
            if pr > 0.0 {
                let path = ext.strip_prefix("file::").unwrap_or(&ext).to_string();
                unvisited.push((pr, path));
            }
        }
        unvisited
            .sort_unstable_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        unvisited.truncate(5);
    }
    serde_json::json!({
        "visited": visited,
        "total": total,
        "unvisited_high_value": unvisited.into_iter().map(|(_, p)| p).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------
// Free dispatch functions — used by both JSON-RPC stdio and HTTP API.
// Zero duplication: McpServer::dispatch_tool() delegates to these.
// ---------------------------------------------------------------------------

/// FAIL-OPEN guard for a background VIGIL (gardener v1). Runs `vigil`, and if it
/// errs, LOGS and SWALLOWS the failure so a watcher can never propagate its error
/// into the agent's unrelated tool call. Returns `true` on success — surfaced so
/// tests can pin the fail-open contract directly, and callers may branch on it.
pub(crate) fn vigil_fail_open<F>(label: &str, tool: &str, vigil: F) -> bool
where
    F: FnOnce() -> M1ndResult<()>,
{
    match vigil() {
        Ok(()) => true,
        Err(err) => {
            eprintln!(
                "[m1nd] gardener: {label} failed for tool '{tool}' \
                 (fail-open — the tool call continues): {err}"
            );
            false
        }
    }
}

fn generic_dispatch_floor_is_available(floor: m1nd_control::AuthorityFloor) -> bool {
    use m1nd_control::AuthorityFloor;

    match floor {
        AuthorityFloor::Ordinary => true,
        AuthorityFloor::ScopedGrantA2
        | AuthorityFloor::PositiveSovereign
        | AuthorityFloor::ServiceIdentity
        | AuthorityFloor::SafetyOnly => false,
    }
}

fn authority_floor_name(floor: m1nd_control::AuthorityFloor) -> &'static str {
    use m1nd_control::AuthorityFloor;

    match floor {
        AuthorityFloor::Ordinary => "ORDINARY",
        AuthorityFloor::ScopedGrantA2 => "SCOPED_GRANT_A2",
        AuthorityFloor::PositiveSovereign => "POSITIVE_SOVEREIGN",
        AuthorityFloor::ServiceIdentity => "SERVICE_IDENTITY",
        AuthorityFloor::SafetyOnly => "SAFETY_ONLY",
    }
}

/// Mandatory generic REST/MCP action-policy boundary.
///
/// This decision is pure and must run before brain resolution, presence
/// tracking, freshness ticks, proof-permit consumption, or handler dispatch.
/// Only ORDINARY actions can currently use the existing authenticated generic
/// ingress. Every stronger floor stays closed until that exact semantic action
/// has a typed, action-bound G2/G3 lease consumer. `mission_service` is not a
/// generic call: the served transports intercept it before invoking this gate.
pub(crate) fn enforce_generic_action_policy(
    tool_name: &str,
    params: &serde_json::Value,
) -> M1ndResult<()> {
    use crate::action_routes::{McpActionRouteError, TrustedMcpRouteFacts};

    let bare = tool_name
        .strip_prefix("m1nd.")
        .or_else(|| tool_name.strip_prefix("m1nd_"))
        .unwrap_or(tool_name);

    // Preserve permanent G3 tombstone precedence. Payload shape and authority
    // claims cannot revive the retired raw mutation primitives.
    if let Some(refusal) = crate::mission_service_transport::legacy_mutation_refusal(bare) {
        return Err(M1ndError::InvalidParams {
            tool: bare.to_string(),
            detail: format!("{}: {}", refusal.code, refusal.detail),
        });
    }

    let (action, floors) = match crate::action_routes::classify_mcp_action(
        bare,
        params,
        TrustedMcpRouteFacts::default(),
    ) {
        Ok(classified) => (
            Some(classified.action.to_string()),
            std::collections::BTreeSet::from([classified.authority_floor]),
        ),
        Err(McpActionRouteError::MissingTrustedFact { .. }) => {
            let floors = crate::action_routes::possible_mcp_authority_floors(bare).map_err(
                |error| M1ndError::InvalidParams {
                    tool: bare.to_string(),
                    detail: format!(
                        "generic_action_policy_unresolved: cannot conservatively resolve authority floor: {error}"
                    ),
                },
            )?;
            (None, floors)
        }
        Err(error) => {
            return Err(M1ndError::InvalidParams {
                tool: bare.to_string(),
                detail: format!(
                "generic_action_policy_unresolved: semantic action classification refused: {error}"
            ),
            })
        }
    };

    // The ONE opening in the wall, keyed BY ACTION and never by floor
    // (GENESIS-INGEST-CONSUMERS-SPEC.md §1.1, owner-ratified 2026-07-29).
    //
    // It is deliberately reachable ONLY from the exactly-classified branch. The
    // `MissingTrustedFact` arm above resolves a UNION of floors with no exact
    // action, so it can never land here — an unresolved classification stays
    // fail-closed, as it must.
    //
    // Admission here admits the CATEGORY only. Every authority-relevant fact
    // about the refresh is enforced inside the handler, after brain resolution.
    if action.as_deref().is_some_and(|exact| {
        crate::action_consumers::GENERIC_A2_LOCAL_ADMITTED_ACTIONS.contains(&exact)
    }) {
        return Ok(());
    }

    let unavailable: Vec<&'static str> = floors
        .iter()
        .copied()
        .filter(|floor| !generic_dispatch_floor_is_available(*floor))
        .map(authority_floor_name)
        .collect();
    if unavailable.is_empty() {
        return Ok(());
    }

    Err(M1ndError::InvalidParams {
        tool: bare.to_string(),
        detail: format!(
            "generic_action_authority_required: semantic_action={} authority_floor={} cannot use generic REST/MCP dispatch; no exact typed G2/G3 lease consumer is installed for this action",
            action.as_deref().unwrap_or("<owner-fact-dependent>"),
            unavailable.join("|")
        ),
    })
}

/// Defense-in-depth wrapper for generic transport calls. Transport seams still
/// invoke the pure policy gate before any of their own tracking/routing effects.
pub(crate) fn dispatch_generic_tool(
    state: &mut SessionState,
    tool_name: &str,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    enforce_generic_action_policy(tool_name, params)?;
    dispatch_tool(state, tool_name, params)
}

/// Dispatch a tool call by name. Normalizes underscores to dots.
/// Used by both JSON-RPC stdio and HTTP API -- zero duplication.
///
/// Public so the transplant proof-harness integration suites (tests/) can drive
/// the SAME gated dispatch path a live agent takes — the M1ND_PROOF_GATE / catalog
/// gating lives inside this function, so an in-process caller gets it unchanged.
///
/// v0.4.0: wraps all responses with _m1nd metadata.
pub fn dispatch_tool(
    state: &mut SessionState,
    tool_name: &str,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    let normalized = tool_name.to_string();
    let start = std::time::Instant::now();
    let bare = normalized
        .strip_prefix("m1nd.")
        .or_else(|| normalized.strip_prefix("m1nd_"))
        .unwrap_or(&normalized);

    // G3 tombstones are selected by the wire action name alone. This precedes
    // read-only, proof, authority, and payload classification: no capability or
    // body shape can revive the former raw mission/receipt write primitives.
    if let Some(refusal) = crate::mission_service_transport::legacy_mutation_refusal(bare) {
        return Err(M1ndError::InvalidParams {
            tool: bare.to_string(),
            detail: format!("{}: {}", refusal.code, refusal.detail),
        });
    }
    if bare == "mission_service" {
        return Err(M1ndError::InvalidParams {
            tool: bare.to_string(),
            detail: "mission_service_unavailable: the stdio-only dispatcher has no sovereign authority provider; use the served owner's authenticated REST or Streamable-HTTP MCP ingress"
                .to_string(),
        });
    }
    if matches!(bare, "external_mutation_service" | "graph_ingest_preview") {
        return Err(M1ndError::InvalidParams {
            tool: bare.to_string(),
            detail: "external_mutation_service_policy_disabled: stdio has no owner-observed Streamable-HTTP session, selected actor, or typed consumer; use the served owner's authenticated MCP ingress"
                .to_string(),
        });
    }
    if matches!(
        bare,
        "authority_session_challenge" | "authority_session_authenticate" | "authority_authorize"
    ) {
        return Err(M1ndError::InvalidParams {
            tool: bare.to_string(),
            detail: "authority_service_unavailable: the stdio dispatcher has no owner-observed wire correlation/ingress context; use the served owner's authenticated REST or Streamable-HTTP MCP ingress"
                .to_string(),
        });
    }

    // Read-only attach gate: refuse mutating tools BEFORE any dispatch or
    // side-effecting tick so the writer's on-disk state can never be touched.
    if state.read_only && read_only_denied(&normalized, params) {
        return Err(M1ndError::InvalidParams {
            tool: normalized.clone(),
            detail: format!(
                "m1nd is attached read-only (--read-only); mutation tool '{}' is disabled. Detach or run a read-write instance to modify state.",
                normalized
            ),
        });
    }

    if skeleton_write_needs_root_gate(&normalized, params) {
        if let (Some(caller_root), Some(brain_root)) =
            (state.caller_root.clone(), state.workspace_root.clone())
        {
            if !state.covers_root(&caller_root) {
                return Ok(serde_json::json!({
                    "ok": false,
                    "schema": "m1nd-system-block-write-v0",
                    "refused": "brainless_root",
                    "caller_root": caller_root,
                    "brain_root": brain_root,
                    "reason": format!(
                        "this session's caller root '{caller_root}' does not resolve to the brain being written at '{brain_root}' — implicit routing would write the wrong brain"
                    ),
                    "fix": {
                        "action": "bootstrap_unavailable",
                        "code": "brain_bootstrap_consumer_not_installed",
                        "note": "no public bootstrap mutation was attempted",
                        "explicit_rest_selector": format!(
                            "for a deliberate cross-brain write, use the explicit REST ?brain={brain_root} selector"
                        ),
                    },
                }));
            }
        }
    }

    // Extract agent_id for tracking
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // P1 presence (ORGANISM-INSIDE): stamp the OBSERVED mutation level when this
    // dispatch is a mutating verb — `read_only_denied` is the single pure
    // classifier (verdict c). In-memory only; the throttled beat in `track_agent`
    // carries it to the sidecar. Never a write here, never able to break dispatch.
    if read_only_denied(&normalized, params) {
        state.note_mutation_observed(&agent_id);
    }

    let query_preview = params
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            params
                .get("claim")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| params.get("node_id").and_then(|v| v.as_str()).unwrap_or(""))
        })
        .to_string();

    // FRESHNESS-BY-TRAFFIC (gardener v1, G1). The daemon's re-ingest tick used to
    // live ONLY in handle_mcp_method (the MCP-wire seam), leaving the REST, stdio
    // side-loop, and mcp_http seams deaf to it — a file changed under a served
    // owner stayed stale until an MCP-wire call happened by. It now rides
    // dispatch_tool, the ONE path every seam funnels through, mirroring the
    // auto-ingest vigil just below. The full condition is checked INLINE (not
    // delegated to run_daemon_tick's own tick_in_flight guard) so a dispatch nested
    // INSIDE a running tick — the project-brain bootstrap ingest (project_brains.rs),
    // an auto-ingest flow — never enters run_daemon_tick and never inflates
    // pending_rerun. Heavy re-ingest/scan entry tools are skipped
    // (daemon_autotick_entry_too_heavy): their own work supersedes the tick, and
    // stacking a tick ahead of them risks holding the brain lock past the REST 30s
    // timeout. Fail-open: a background vigil never breaks the agent's tool call.
    if state.daemon_state.active
        && !state.daemon_state.tick_in_flight
        && should_autotick_daemon(&normalized)
        && !daemon_autotick_entry_too_heavy(&normalized)
        && state.daemon_state.last_tick_ms.is_some_and(|last| {
            now_ms().saturating_sub(last) >= state.daemon_state.poll_interval_ms
        })
    {
        vigil_fail_open("daemon tick", &normalized, || {
            run_daemon_tick(state, "traffic");
            Ok(())
        });
    }

    if !matches!(
        normalized.as_str(),
        "recovery_playbook"
            | "trust_selftest"
            | "mission_start"
            | "mission_event"
            | "mission_next"
            | "mission_verify"
            | "mission_handoff"
            | "mission_close"
            | "evidence_query"
    ) {
        // FAIL-OPEN (gardener v1). The inline auto-ingest tick is a background
        // VIGIL, not part of the agent's request. Before, the `?` here turned a
        // vigil failure into the agent's unrelated tool-call error. A watcher must
        // never be able to break a tool call: run it fail-open (log + swallow). The
        // code daemon's own tick already fails open (`run_daemon_tick` discards the
        // handle_daemon_tick Result); this closes the matching hole for the
        // document watcher's inline tick.
        vigil_fail_open("auto_ingest tick", &normalized, || {
            auto_ingest::maybe_tick_auto_ingest(state, &normalized)
        });
    }

    // G5 proof spine: after every background freshness tick and immediately
    // before the handler, classify the semantic action by its complete effects.
    // Source writes atomically consume agent+scope+generation+digest+TTL marks.
    // Empty exact plans (apply_batch/xray idempotent no-op) need no permit.
    let mut proof_cleanup_identities = Vec::new();
    if proof_gate_enabled() && source_write_effect_required(&normalized, params, &normalized)? {
        let bare_tool = normalized
            .strip_prefix("m1nd.")
            .or_else(|| normalized.strip_prefix("m1nd_"))
            .unwrap_or(&normalized);
        let targets = proof_gate_targets(bare_tool, params, state)?;
        if !targets.is_empty() {
            proof_cleanup_identities = state
                .consume_proof_ready_targets(&agent_id, &targets)
                .map_err(|detail| M1ndError::InvalidParams {
                    tool: normalized.clone(),
                    detail: format!(
                        "M1ND_PROOF_GATE refused SOURCE_FILESYSTEM_WRITE for agent_id='{agent_id}': {detail}. Run surgical_context_v2 for each exact target against the current graph/disk state, then retry once."
                    ),
                })?;
        }
    }

    let result = match normalized.as_str() {
        name if name.starts_with("perspective_") => dispatch_perspective_tool(state, name, params),
        name if name.starts_with("lock_") => dispatch_lock_tool(state, name, params),
        _ => dispatch_core_tool(state, &normalized, params),
    };
    if !proof_cleanup_identities.is_empty() {
        state.clear_active_proof_permits(&agent_id, &proof_cleanup_identities);
    }
    let result = result.map_err(|error| match error {
        M1ndError::Serde(detail) => M1ndError::InvalidParams {
            tool: normalized.clone(),
            detail: help_guidance::runtime_error_guidance_hint(&normalized, &detail.to_string()),
        },
        M1ndError::InvalidParams { tool, detail } => {
            let normalized_tool = tool
                .strip_prefix("m1nd.")
                .or_else(|| tool.strip_prefix("m1nd_"))
                .unwrap_or(&tool)
                .to_string();
            M1ndError::InvalidParams {
                tool: tool.clone(),
                detail: help_guidance::runtime_error_guidance_hint(&normalized_tool, &detail),
            }
        }
        other => other,
    });

    // Post-dispatch: log query + add _m1nd metadata.
    // Brand gate G1.5: the savings tracker was removed — nothing tallies
    // unmeasured tokens-saved here anymore.
    let mut result = result;
    if let Ok(ref mut value) = result {
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let result_count = value
            .get("results")
            .and_then(|v| v.as_array())
            .map_or(0, |a| a.len());

        // Log query
        state.log_query(
            &normalized,
            &agent_id,
            elapsed_ms,
            result_count,
            &query_preview,
        );

        // Additive `_m1nd` response envelope (Tier 2). Gated behind
        // M1ND_RESPONSE_ENVELOPE (default ON; set to "0"/"false" to disable).
        // ADDITIVE ONLY: attaches a `_m1nd` object to JSON-object results;
        // never removes or renames existing fields. Non-object results (rare)
        // are left untouched.
        if response_envelope_enabled() && value.is_object() {
            // Builders read the result; snapshot it once to avoid a borrow
            // conflict with the mutable insert below.
            let snapshot = value.clone();
            let mut meta = personality::build_m1nd_meta(&normalized, &snapshot);
            // Promote the headline summary so agents get it at the top level.
            let summary = personality::personality_line(&normalized, &snapshot);
            if !summary.is_empty() {
                meta["summary"] = serde_json::Value::String(summary);
            }
            meta["read_only"] = serde_json::json!(state.read_only);

            // Tier 3: memory at point-of-relevance (additive, capped, best-effort).
            if let Some(nearby) = memory_nearby_for_result(state, &normalized, &snapshot) {
                if !nearby.is_empty() {
                    meta["memory_nearby"] = serde_json::Value::Array(nearby);
                }
            }

            if let Some(obj) = value.as_object_mut() {
                obj.insert("_m1nd".to_string(), meta);
            }
        }
    }

    result
}

pub(crate) fn skeleton_write_needs_root_gate(tool_name: &str, params: &serde_json::Value) -> bool {
    match tool_name {
        "system_blocks_seed_import"
        | "skeleton_candidate"
        | "candidate_edit"
        | "system_blocks_ratify"
        | "system_blocks_reconcile"
        | "system_blocks_archive"
        | "system_blocks_delete" => true,
        "candidate_lease" => params.get("action").and_then(|v| v.as_str()) == Some("acquire"),
        _ => false,
    }
}

/// SPEC-1b's ingress canonicalization for the REST tools seam.
///
/// Returns the canonicalized `M1nd-Caller-Root` to stamp on the session for THIS
/// dispatch, or `None` to leave the session's own value alone.
///
/// Deliberately narrow. `/api/tools/*` has never stamped a caller root, and
/// making it do so for every verb would newly switch on reception verdicts and
/// brainless-root routing for every REST caller that happens to send the header
/// — a behaviour change far outside this door. So it stamps for exactly the
/// action that needs a caller root to decide anything: the refresh. Both
/// transports then reach ONE predicate with a canonical value, which is what
/// makes their refusals byte-identical (SPEC-1g) rather than merely similar.
pub(crate) fn refresh_caller_root_from_header(
    tool_name: &str,
    params: &serde_json::Value,
    header: &Option<String>,
) -> Option<String> {
    let bare = tool_name
        .strip_prefix("m1nd.")
        .or_else(|| tool_name.strip_prefix("m1nd_"))
        .unwrap_or(tool_name);
    if bare != "ingest" || params.get("mode").and_then(|v| v.as_str()) != Some("refresh") {
        return None;
    }
    let raw = header.as_deref()?.trim();
    if raw.is_empty() {
        return None;
    }
    Some(crate::project_brains::ProjectBrainRegistry::canonical_key(
        raw,
    ))
}

/// Dispatch core + superpowers tools (35 tools).
fn dispatch_core_tool(
    state: &mut SessionState,
    tool_name: &str,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    match tool_name {
        "orient" => handle_orient(state, params),
        "north" => handle_north(state, params),
        "cockpit" => crate::cockpit::handle_cockpit(state, params),
        "delegate" => crate::delegation_handlers::handle_delegate(state, params),
        "debrief" => crate::delegation_handlers::handle_debrief(state, params),
        "evidence_query" => crate::evidence_spine_owner::handle_evidence_query(state, params),
        "am_i_stale" => handle_am_i_stale(state, params),
        "activate" => {
            let input: ActivateInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = tools::handle_activate(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "xray_retag" => {
            let input: crate::xray_handlers::XrayRetagInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::xray_handlers::handle_xray_retag(state, input)
        }
        "xray_apply" => {
            let input: crate::xray_handlers::XrayApplyInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            if proof_gate_enabled() {
                crate::xray_handlers::handle_xray_apply_authorized(state, input)
            } else {
                crate::xray_handlers::handle_xray_apply(state, input)
            }
        }
        "xray_orient" => {
            let input: crate::xray_handlers::XrayOrientInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::xray_handlers::handle_xray_orient(state, input)
        }
        "xray_gate" => {
            let input: crate::xray_handlers::XrayGateInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::xray_handlers::handle_xray_gate(state, input)
        }
        "xray_paint" => {
            let input: crate::xray_handlers::XrayPaintInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::xray_handlers::handle_xray_paint(state, input)
        }
        "xray_ledger" => {
            let input: crate::xray_handlers::XrayLedgerInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::xray_handlers::handle_xray_ledger(state, input)
        }
        "system_blocks_snapshot" => {
            let input: crate::system_blocks_handlers::SnapshotInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::system_blocks_handlers::handle_system_blocks_snapshot(state, input)
        }
        "skeleton_candidate" => {
            let input: crate::system_blocks_handlers::SkeletonCandidateInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::system_blocks_handlers::handle_skeleton_candidate(state, input)
        }
        "system_blocks_seed_import" => {
            let input: crate::system_blocks_handlers::SeedImportInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::system_blocks_handlers::handle_system_blocks_seed_import(state, input)
        }
        "system_blocks_ratify" => {
            let input: crate::system_blocks_handlers::RatifyInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::system_blocks_handlers::handle_system_blocks_ratify(state, input)
        }
        "receipt_import" => {
            let input: crate::system_blocks_handlers::ReceiptImportInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::system_blocks_handlers::handle_receipt_import(state, input)
        }
        "system_blocks_reconcile" => {
            let input: crate::system_blocks_handlers::ReconcileInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::system_blocks_handlers::handle_system_blocks_reconcile(state, input)
        }
        "receipt_recompute" => {
            let input: crate::system_blocks_handlers::ReceiptRecomputeInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::system_blocks_handlers::handle_receipt_recompute(state, input)
        }
        "system_blocks_archive" => {
            let input: crate::system_blocks_handlers::ArchiveInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::system_blocks_handlers::handle_system_blocks_archive(state, input)
        }
        "system_blocks_delete" => {
            let input: crate::system_blocks_handlers::DeleteInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::system_blocks_handlers::handle_system_blocks_delete(state, input)
        }
        "candidate_edit" => {
            let input: crate::system_blocks_handlers::CandidateEditInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::system_blocks_handlers::handle_candidate_edit(state, input)
        }
        "candidate_lease" => {
            let input: crate::system_blocks_handlers::CandidateLeaseInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::system_blocks_handlers::handle_candidate_lease(state, input)
        }
        // HUMAN VIEW v2 F2.5a — the mission-letter write verb (§2c).
        "mission_post" => {
            let input: crate::mission_letter_handlers::MissionPostInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::mission_letter_handlers::handle_mission_post(state, input)
        }
        // HUMAN VIEW v2 F2.5c — mission_spawn (§4b) is an OWNER→runnerd PROXY that
        // needs owner-process state (the announce registry + the shared secret) and
        // an async HTTP forward; it is served ONLY by the HTTP route
        // (`http_server::handle_mission_spawn`), never through this sync dispatch. An
        // MCP-stdio caller reaching here gets an honest redirect, never a silent
        // failure or a fake spawn.
        "mission_spawn" => Err(M1ndError::InvalidParams {
            tool: "mission_spawn".to_string(),
            detail:
                "mission_spawn is an HTTP-only proxy verb — call it via `POST /api/tools/mission_spawn` on the owner (it forwards to the runner daemon with the shared secret the browser never holds)"
                    .to_string(),
        }),
        // HUMAN VIEW v2 F11-c — candidate_naming (§2b) is likewise HTTP-only: it
        // needs the owner-process announce registry + the shared secret + the
        // /name forward, none of which this sync dispatch sees. An MCP-stdio
        // caller gets an honest redirect, never a silent failure.
        "candidate_naming" => Err(M1ndError::InvalidParams {
            tool: "candidate_naming".to_string(),
            detail:
                "candidate_naming is an HTTP-only verb — call it via `POST /api/tools/candidate_naming` on the owner (it calls the runner daemon's /name with the shared secret the browser never holds, then applies the names through candidate_edit)"
                    .to_string(),
        }),
        "impact" => {
            let input: ImpactInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = tools::handle_impact(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "missing" => {
            let input: MissingInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_missing(state, input)
        }
        "why" => {
            let input: WhyInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_why(state, input)
        }
        "warmup" => {
            let input: WarmupInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_warmup(state, input)
        }
        "counterfactual" => {
            let input: CounterfactualInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_counterfactual(state, input)
        }
        "predict" => {
            let input: PredictInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_predict(state, input)
        }
        "fingerprint" => {
            let input: FingerprintInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_fingerprint(state, input)
        }
        "drift" => {
            let input: DriftInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_drift(state, input)
        }
        "learn" => {
            let input: LearnInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_learn(state, input)
        }
        "ingest" => {
            let input: IngestInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_ingest(state, input)
        }
        // SPEC-2's birth verb, at the DISPATCHER — which is exactly where it can
        // never succeed. The dispatcher holds no `HumanOrigin` and cannot
        // construct one (the type has no public constructor), so every route
        // that reaches this arm is by definition a route with no owner stamp.
        // The generic policy gate refuses first and harder; this is the second
        // layer, for any in-process seam that reaches `dispatch_tool` directly.
        "brain_birth" => Ok(crate::brain_birth::birth_refusal_without_stamp()),
        "document_resolve" => {
            let input: DocumentResolveInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            universal_docs::resolve_document(state, input)
        }
        "document_provider_health" => {
            let input: DocumentProviderHealthInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            universal_docs::provider_health(input)
        }
        "document_bindings" => {
            let input: DocumentBindingsInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            universal_docs::document_bindings(state, input)
        }
        "document_drift" => {
            let input: DocumentDriftInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            universal_docs::document_drift(state, input)
        }
        "auto_ingest_start" => {
            let input: AutoIngestStartInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            auto_ingest::handle_auto_ingest_start(state, input)
        }
        "auto_ingest_stop" => {
            let input: AutoIngestStopInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            auto_ingest::handle_auto_ingest_stop(state, input)
        }
        "auto_ingest_status" => {
            let input: AutoIngestStatusInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            auto_ingest::handle_auto_ingest_status(state, input)
        }
        "auto_ingest_tick" => {
            let input: AutoIngestTickInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            auto_ingest::handle_auto_ingest_tick(state, input)
        }
        "resonate" => {
            let input: ResonateInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_resonate(state, input)
        }
        "health" => {
            let input: HealthInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = tools::handle_health(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "session_handshake" => {
            let input: SessionHandshakeInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_session_handshake(state, input)
        }
        "trust_selftest" => {
            let input: TrustSelftestInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_trust_selftest(state, input)
        }
        "recovery_playbook" => {
            let input: RecoveryPlaybookInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_recovery_playbook(state, input)
        }
        "doctor" => {
            let input: DoctorInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_doctor(state, input)
        }
        "mission_start" => {
            let input: layers::MissionStartInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            mission_handlers::handle_mission_start(state, input)
        }
        "mission_event" => {
            let input: layers::MissionEventInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            mission_handlers::handle_mission_event(state, input)
        }
        "mission_next" => {
            let input: layers::MissionNextInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            mission_handlers::handle_mission_next(state, input)
        }
        "mission_verify" => {
            let input: layers::MissionVerifyInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            mission_handlers::handle_mission_verify(state, input)
        }
        "mission_handoff" => {
            let input: layers::MissionHandoffInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            mission_handlers::handle_mission_handoff(state, input)
        }
        "mission_close" => {
            let input: layers::MissionCloseInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            mission_handlers::handle_mission_close(state, input)
        }
        // L2-L7: Superpowers layer tools
        "seek" => {
            let input: layers::SeekInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_seek(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "focus" => {
            let input: layers::FocusInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_focus(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "scan" => {
            let input: layers::ScanInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_scan(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "timeline" => {
            let input: layers::TimelineInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_timeline(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "diverge" => {
            let input: layers::DivergeInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_diverge(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "trail_save" => {
            let input: layers::TrailSaveInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_trail_save(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "trail_resume" => {
            let input: layers::TrailResumeInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_trail_resume(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "trail_merge" => {
            let input: layers::TrailMergeInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_trail_merge(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "trail_list" => {
            let input: layers::TrailListInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_trail_list(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "hypothesize" => {
            let input: layers::HypothesizeInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_hypothesize(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "differential" => {
            let input: layers::DifferentialInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_differential(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "trace" => {
            let input: layers::TraceInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "trace".into(),
                    detail: e.to_string(),
                })?;
            let output = layer_handlers::handle_trace(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "validate_plan" => {
            let input: layers::ValidatePlanInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_validate_plan(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "federate" => {
            let input: layers::FederateInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_federate(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "antibody_scan" => {
            let input: layers::AntibodyScanInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_antibody_scan(state, input)
        }
        "antibody_list" => {
            let input: layers::AntibodyListInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_antibody_list(state, input)
        }
        "antibody_create" => {
            let input: layers::AntibodyCreateInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_antibody_create(state, input)
        }
        "flow_simulate" => {
            let input: layers::FlowSimulateInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_flow_simulate(state, input)
        }
        "epidemic" => {
            let input: layers::EpidemicInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_epidemic(state, input)
        }
        "tremor" => {
            let input: layers::TremorInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_tremor(state, input)
        }
        "trust" => {
            let input: layers::TrustInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_trust(state, input)
        }
        "heuristics_surface" => {
            let input: surgical::HeuristicsSurfaceInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = surgical_handlers::handle_heuristics_surface(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "layers" => {
            let input: layers::LayersInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_layers(state, input)
        }
        "layer_inspect" => {
            let input: layers::LayerInspectInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_layer_inspect(state, input)
        }
        "ghost_edges" => {
            let input: layers::GhostEdgesInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_ghost_edges(state, input)
        }
        "calibrate_predict" => {
            let input: layers::CalibratePredictInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_calibrate_predict(state, input)
        }
        "calibrate_envelope" => {
            let input: layers::CalibrateEnvelopeInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_calibrate_envelope(state, input)
        }
        "taint_trace" => {
            let input: layers::TaintTraceInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_taint_trace(state, input)
        }
        "twins" => {
            let input: layers::TwinsInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_twins(state, input)
        }
        "refactor_plan" => {
            let input: layers::RefactorPlanInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_refactor_plan(state, input)
        }
        "runtime_overlay" => {
            let input: layers::RuntimeOverlayInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_runtime_overlay(state, input)
        }
        // -----------------------------------------------------------------
        // v0.4.0: search, help, panoramic, report
        // -----------------------------------------------------------------
        "search" => {
            let input: layers::SearchInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = search_handlers::handle_search(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "scan_all" => {
            let input: layers::ScanAllInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::audit_handlers::handle_scan_all(state, input)
        }
        "cross_verify" => {
            let input: layers::CrossVerifyInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::audit_handlers::handle_cross_verify(state, input)
        }
        "coverage_session" => {
            let input: layers::CoverageSessionInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::audit_handlers::handle_coverage_session(state, input)
        }
        "external_references" => {
            let input: layers::ExternalReferencesInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::audit_handlers::handle_external_references(state, input)
        }
        "federate_auto" => {
            let input: layers::FederateAutoInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::audit_handlers::handle_federate_auto(state, input)
        }
        "glob" => {
            let input: layers::GlobInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = search_handlers::handle_glob(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "help" => {
            let input: layers::HelpInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = search_handlers::handle_help(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "report" => {
            let input: layers::ReportInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = report_handlers::handle_report(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "audit" => {
            let input: layers::AuditInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::audit_handlers::handle_audit(state, input)
        }
        "daemon_start" => {
            let input: layers::DaemonStartInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::daemon_handlers::handle_daemon_start(state, input)
        }
        "daemon_stop" => {
            let input: layers::DaemonStopInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::daemon_handlers::handle_daemon_stop(state, input)
        }
        "daemon_status" => {
            let input: layers::DaemonStatusInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::daemon_handlers::handle_daemon_status(state, input)
        }
        "daemon_tick" => {
            let input: layers::DaemonTickInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::daemon_handlers::handle_daemon_tick(state, input)
        }
        "alerts_list" => {
            let input: layers::AlertsListInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::daemon_handlers::handle_alerts_list(state, input)
        }
        "alerts_ack" => {
            let input: layers::AlertsAckInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::daemon_handlers::handle_alerts_ack(state, input)
        }
        "panoramic" => {
            let input: layers::PanoramicInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = report_handlers::handle_panoramic(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        // Brand gate G1.5: the `savings` tool (unmeasured token-economy claims)
        // was removed — it falls through to the unknown-tool arm below.
        // -----------------------------------------------------------------
        // Surgical: context + apply
        // -----------------------------------------------------------------
        "surgical_context" => {
            let input: crate::protocol::surgical::SurgicalContextInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "surgical_context".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_surgical_context(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "apply" => {
            let input: crate::protocol::surgical::ApplyInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "apply".into(),
                    detail: e.to_string(),
                })?;
            let output = if proof_gate_enabled() {
                surgical_handlers::handle_apply_authorized(state, input)?
            } else {
                surgical_handlers::handle_apply(state, input)?
            };
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        // -----------------------------------------------------------------
        // Surgical V2: context_v2 + apply_batch
        // -----------------------------------------------------------------
        "surgical_context_v2" => {
            let input: crate::protocol::surgical::SurgicalContextV2Input =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "surgical_context_v2".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_surgical_context_v2(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "apply_batch" => {
            let input: crate::protocol::surgical::ApplyBatchInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "apply_batch".into(),
                    detail: e.to_string(),
                })?;
            let output = if proof_gate_enabled() {
                surgical_handlers::handle_apply_batch_authorized(state, input)?
            } else {
                surgical_handlers::handle_apply_batch(state, input)?
            };
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "transplant" => {
            let input: crate::protocol::surgical::TransplantInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "transplant".into(),
                    detail: e.to_string(),
                })?;
            let output = crate::transplant::handle_transplant(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "transplant_preview" => {
            let input: crate::protocol::surgical::TransplantInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "transplant_preview".into(),
                    detail: e.to_string(),
                })?;
            let output = crate::transplant::handle_transplant_preview(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "transplant_commit" => {
            let input: crate::protocol::surgical::TransplantCommitInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "transplant_commit".into(),
                    detail: e.to_string(),
                })?;
            let output = crate::transplant::handle_transplant_commit(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "edit_preview" => {
            let input: crate::protocol::surgical::EditPreviewInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "edit_preview".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_edit_preview(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "edit_commit" => {
            let input: crate::protocol::surgical::EditCommitInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "edit_commit".into(),
                    detail: e.to_string(),
                })?;
            let output = if proof_gate_enabled() {
                surgical_handlers::handle_edit_commit_authorized(state, input)?
            } else {
                surgical_handlers::handle_edit_commit(state, input)?
            };
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        // -----------------------------------------------------------------
        // View: lightweight file reader
        // -----------------------------------------------------------------
        "view" => {
            let input: crate::protocol::surgical::ViewInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "view".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_view(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "batch_view" => {
            let input: crate::protocol::surgical::BatchViewInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "batch_view".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_batch_view(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "persist" => {
            let input: crate::persist_handlers::PersistInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::persist_handlers::handle_persist(state, input)
        }
        "boot_memory" => {
            let input: crate::boot_memory_handlers::BootMemoryInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::boot_memory_handlers::handle_boot_memory(state, input)
        }
        "memorize" => {
            let input: crate::light_author_handlers::LightAuthorInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::light_author_handlers::handle_light_author(state, input)
        }
        // MEDULLA M6: `promote` is an OWNER-LEVEL cross-store verb — it reads a
        // project brain's store and writes the medulla. A single-store dispatch
        // (`&mut SessionState`) cannot reach two stores, so it is served at the
        // routed HTTP seam (`mcp_http::run_promote`), never here. Door-coverage
        // honesty (§C9.4): reaching this arm means the call came through the
        // paramless/stdio door, which cannot host the crossing yet.
        "promote" => Err(M1ndError::InvalidParams {
            tool: "promote".into(),
            detail: "promote is served at the routed HTTP door (it crosses two stores: a project \
                     brain → the medulla). Call it over the served owner's /mcp endpoint; the \
                     stdio/paramless door does not host the crossing (door-coverage honesty, §C9.4)."
                .into(),
        }),
        // -----------------------------------------------------------------
        // ORGANISM R16: the SOUL — PATHOS native and verified (SOUL-PRD).
        // Read-only over a git-tracked document; writes NOTHING (S0).
        // -----------------------------------------------------------------
        "soul_check" => {
            let input: layers::SoulCheckInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::soul_handlers::handle_soul_check(state, input)
        }
        "soul_read" => {
            let input: layers::SoulReadInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::soul_handlers::handle_soul_read(state, input)
        }
        // -----------------------------------------------------------------
        // v0.7.0: Diagnostic tools
        // -----------------------------------------------------------------
        "metrics" => {
            let input: layers::MetricsInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_metrics(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "type_trace" => {
            let input: layers::TypeTraceInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_type_trace(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "diagram" => {
            let input: layers::DiagramInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_diagram(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        _ => Err(M1ndError::UnknownTool {
            name: tool_name.to_string(),
        }),
    }
}

/// MCP protocol version this server implements/prefers.
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Older MCP protocol versions we remain compatible with. If a client offers one
/// of these we echo it back (per the MCP spec's version-negotiation handshake);
/// otherwise we reply with our preferred [`MCP_PROTOCOL_VERSION`].
const MCP_SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// Presence is a durable coordination side effect, so the strictly read-only
/// EvidenceQuery must not inherit per-seam `agent_id` tracking before its
/// deny-unknown-fields payload is decoded. This also covers prefixed tool names.
pub(crate) fn tool_tracks_agent_presence(tool_name: &str) -> bool {
    let bare = tool_name
        .strip_prefix("m1nd.")
        .or_else(|| tool_name.strip_prefix("m1nd_"))
        .unwrap_or(tool_name);
    bare != "evidence_query"
}

/// Negotiate the protocol version: honor the client's requested version when we
/// support it, otherwise fall back to our preferred version.
fn negotiate_protocol_version(requested: Option<&str>) -> &'static str {
    if let Some(req) = requested {
        if let Some(v) = MCP_SUPPORTED_PROTOCOL_VERSIONS
            .iter()
            .copied()
            .find(|v| *v == req)
        {
            return v;
        }
    }
    MCP_PROTOCOL_VERSION
}

/// Transport-agnostic MCP method dispatch.
///
/// Handles the JSON-RPC MCP protocol methods (`initialize`,
/// `notifications/initialized`, `tools/list`, `tools/call`, method-not-found)
/// against a borrowed [`SessionState`]. Used by both the stdio transport
/// (via [`McpServer::dispatch`]) and the Streamable-HTTP transport
/// (`mcp_http::handle_mcp_post`), so both bind to the same shared graph.
///
/// Note: the stdio-only live FS watcher refresh for `daemon_start`/`daemon_stop`
/// is NOT performed here — the caller (`McpServer::dispatch`) handles that after
/// this returns, since it requires `&mut McpServer`, not just `&mut SessionState`.
pub(crate) fn handle_mcp_method(
    state: &mut SessionState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let method = request.method.as_str();

    match method {
        "initialize" => {
            let requested = request
                .params
                .get("protocolVersion")
                .and_then(|v| v.as_str());
            let protocol_version = negotiate_protocol_version(requested);
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id.clone(),
                result: Some(serde_json::json!({
                    "protocolVersion": protocol_version,
                    "serverInfo": {
                        "name": "m1nd-mcp",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": {
                        "tools": {},
                    },
                    "instructions": M1ND_INSTRUCTIONS,
                })),
                error: None,
            }
        }
        "notifications/initialized" => {
            // No response needed for notifications, but we return one since caller expects it
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id.clone(),
                result: Some(serde_json::Value::Null),
                error: None,
            }
        }
        "ping" => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: request.id.clone(),
            result: Some(serde_json::json!({})),
            error: None,
        },
        "tools/list" => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: request.id.clone(),
            result: Some(tool_schemas()),
            error: None,
        },
        "tools/call" => handle_mcp_method_transactional(state, request)
            .unwrap_or_else(|error| mcp_tool_error_response(request.id.clone(), error.to_string())),
        _ => {
            // Method not found — JSON-RPC protocol error
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id.clone(),
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", method),
                    data: None,
                }),
            }
        }
    }
}

/// Execute one MCP method while leaving a `tools/call` domain failure as a real
/// callback error. Actor-backed transports MUST use this seam: the actor can
/// then roll back a partial mutation before the transport converts the error to
/// MCP `isError` content. Pure protocol methods delegate to the wire handler.
pub(crate) fn handle_mcp_method_transactional(
    state: &mut SessionState,
    request: &JsonRpcRequest,
) -> M1ndResult<JsonRpcResponse> {
    if request.method != "tools/call" {
        return Ok(handle_mcp_method(state, request));
    }

    let tool_name = request
        .params
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let arguments = request
        .params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

    // Authority is decided before presence, freshness, proof consumption, or
    // handler effects.
    enforce_generic_action_policy(tool_name, &arguments)?;
    if tool_tracks_agent_presence(tool_name) {
        if let Some(agent_id) = arguments.get("agent_id").and_then(|value| value.as_str()) {
            state.track_agent(agent_id);
        }
    }

    let payload = dispatch_generic_tool(state, tool_name, &arguments)?;
    Ok(mcp_tool_result_response(request.id.clone(), &payload))
}

pub(crate) fn mcp_tool_result_response(
    id: serde_json::Value,
    payload: &serde_json::Value,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(serde_json::json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(payload).unwrap_or_default(),
            }]
        })),
        error: None,
    }
}

pub(crate) fn mcp_tool_error_response(id: serde_json::Value, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(serde_json::json!({
            "content": [{ "type": "text", "text": format!("Error: {message}") }],
            "isError": true
        })),
        error: None,
    }
}

fn should_autotick_daemon(tool_name: &str) -> bool {
    !matches!(
        tool_name,
        "daemon_start"
            | "daemon_stop"
            | "daemon_status"
            | "daemon_tick"
            | "alerts_list"
            | "alerts_ack"
            | "session_handshake"
            | "trust_selftest"
            | "recovery_playbook"
            | "mission_start"
            | "mission_event"
            | "mission_next"
            | "mission_verify"
            | "mission_handoff"
            | "mission_close"
            | "evidence_query"
    )
}

/// Heavy entry tools whose OWN work already re-ingests or re-scans the graph — a
/// freshness-by-traffic tick fired just AHEAD of them is redundant, and stacking
/// the tick's wall-clock (measured ~3.7s for 8 changed files on the 901-file m1nd
/// brain, growing toward the 32-file tick budget) on top of theirs risks holding
/// the brain lock past the REST 30s timeout — `spawn_blocking` is NOT cancelled
/// when `tokio::time::timeout` fires (http_server.rs), so a tool that overruns the
/// window keeps the lock and wedges the brain for every waiting request. Kept
/// SEPARATE from `should_autotick_daemon` (whose skip-list is pinned by regression
/// test to the daemon-control verbs): these tools are eligible to autotick in
/// principle; the tick is deliberately skipped here as a cost/redundancy guard.
fn daemon_autotick_entry_too_heavy(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "ingest" | "scan" | "scan_all" | "skeleton_candidate"
    )
}

fn background_tick_if_due(state: &mut SessionState) {
    if !state.daemon_state.active || state.daemon_state.poll_interval_ms == 0 {
        return;
    }
    let due = state
        .daemon_state
        .last_tick_ms
        .is_none_or(|last| now_ms().saturating_sub(last) >= state.daemon_state.poll_interval_ms);
    if !due {
        return;
    }

    let _ = crate::daemon_handlers::handle_daemon_tick(
        state,
        layers::DaemonTickInput {
            agent_id: "daemon".into(),
            max_files: 32,
        },
    );
}

fn run_daemon_tick(state: &mut SessionState, trigger: &str) {
    if state.daemon_state.tick_in_flight {
        state.daemon_state.pending_rerun = true;
        return;
    }

    state.daemon_state.tick_in_flight = true;
    state.daemon_state.last_tick_trigger = Some(trigger.to_string());
    let _ = crate::daemon_handlers::handle_daemon_tick(
        state,
        layers::DaemonTickInput {
            agent_id: "daemon".into(),
            max_files: 32,
        },
    );
    state.daemon_state.tick_in_flight = false;

    if state.daemon_state.pending_rerun {
        state.daemon_state.pending_rerun = false;
        state.daemon_state.last_tick_trigger = Some("reconciliation".into());
        state.daemon_state.tick_in_flight = true;
        let _ = crate::daemon_handlers::handle_daemon_tick(
            state,
            layers::DaemonTickInput {
                agent_id: "daemon".into(),
                max_files: 32,
            },
        );
        state.daemon_state.tick_in_flight = false;
    }
}

fn daemon_wait_duration_ms(state: &SessionState) -> u64 {
    if !state.daemon_state.active {
        return 1000;
    }
    if state.daemon_state.poll_interval_ms == 0 {
        return 1000;
    }

    let exponent = state
        .daemon_state
        .idle_streak
        .min(state.daemon_state.max_backoff_multiplier.saturating_sub(1));
    let effective_poll_interval_ms = state
        .daemon_state
        .poll_interval_ms
        .saturating_mul(2u64.pow(exponent))
        .clamp(25, 10_000);
    let scheduler_interval_ms = if state.daemon_state.watch_backend == "native_fs" {
        effective_poll_interval_ms.max(5_000)
    } else {
        effective_poll_interval_ms
    };

    match state.daemon_state.last_tick_ms {
        Some(last_tick_ms) => {
            let elapsed = now_ms().saturating_sub(last_tick_ms);
            if elapsed >= scheduler_interval_ms {
                25
            } else {
                scheduler_interval_ms
                    .saturating_sub(elapsed)
                    .clamp(25, 1000)
            }
        }
        None => 25,
    }
}

impl LiveDaemonWatcher {
    fn start(
        watch_paths: &[String],
        event_tx: mpsc::SyncSender<ServerEvent>,
    ) -> Result<Self, String> {
        let dropped_counter = Arc::new(AtomicU64::new(0));
        let dropped_for_cb = dropped_counter.clone();
        let tx_for_cb = event_tx.clone();

        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                let event = match result {
                    Ok(_) => ServerEvent::WatchNotice,
                    Err(error) => ServerEvent::WatchError(error.to_string()),
                };
                match tx_for_cb.try_send(event) {
                    Ok(_) => {}
                    Err(mpsc::TrySendError::Full(_)) | Err(mpsc::TrySendError::Disconnected(_)) => {
                        dropped_for_cb.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
            .map_err(|error| error.to_string())?;

        for raw_path in watch_paths {
            let path = PathBuf::from(raw_path);
            let mode = if path.is_dir() {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            watcher
                .watch(path.as_path(), mode)
                .map_err(|error| error.to_string())?;
        }

        Ok(Self {
            _watcher: watcher,
            dropped_counter,
        })
    }
}

/// Dispatch perspective tools (12 tools).
fn dispatch_perspective_tool(
    state: &mut SessionState,
    tool_name: &str,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    use crate::perspective_handlers;
    use crate::protocol::perspective::*;

    match tool_name {
        "perspective_start" => {
            let input: PerspectiveStartInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_start".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_start(state, input)
        }
        "perspective_routes" => {
            let input: PerspectiveRoutesInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_routes".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_routes(state, input)
        }
        "perspective_inspect" => {
            let input: PerspectiveInspectInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_inspect".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_inspect(state, input)
        }
        "perspective_peek" => {
            let input: PerspectivePeekInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_peek".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_peek(state, input)
        }
        "perspective_follow" => {
            let input: PerspectiveFollowInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_follow".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_follow(state, input)
        }
        "perspective_suggest" => {
            let input: PerspectiveSuggestInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_suggest".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_suggest(state, input)
        }
        "perspective_affinity" => {
            let input: PerspectiveAffinityInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_affinity".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_affinity(state, input)
        }
        "perspective_branch" => {
            let input: PerspectiveBranchInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_branch".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_branch(state, input)
        }
        "perspective_back" => {
            let input: PerspectiveBackInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_back".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_back(state, input)
        }
        "perspective_compare" => {
            let input: PerspectiveCompareInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_compare".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_compare(state, input)
        }
        "perspective_list" => {
            let input: PerspectiveListInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_list".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_list(state, input)
        }
        "perspective_close" => {
            let input: PerspectiveCloseInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_close".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_close(state, input)
        }
        _ => Err(M1ndError::UnknownTool {
            name: tool_name.to_string(),
        }),
    }
}

/// Dispatch lock tools (5 tools).
fn dispatch_lock_tool(
    state: &mut SessionState,
    tool_name: &str,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    use crate::lock_handlers;
    use crate::protocol::lock::*;

    match tool_name {
        "lock_create" => {
            let input: LockCreateInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "lock_create".into(),
                    detail: e.to_string(),
                })?;
            lock_handlers::handle_lock_create(state, input)
        }
        "lock_watch" => {
            let input: LockWatchInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "lock_watch".into(),
                    detail: e.to_string(),
                })?;
            lock_handlers::handle_lock_watch(state, input)
        }
        "lock_diff" => {
            let input: LockDiffInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "lock_diff".into(),
                    detail: e.to_string(),
                })?;
            lock_handlers::handle_lock_diff(state, input)
        }
        "lock_rebase" => {
            let input: LockRebaseInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "lock_rebase".into(),
                    detail: e.to_string(),
                })?;
            lock_handlers::handle_lock_rebase(state, input)
        }
        "lock_release" => {
            let input: LockReleaseInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "lock_release".into(),
                    detail: e.to_string(),
                })?;
            lock_handlers::handle_lock_release(state, input)
        }
        _ => Err(M1ndError::UnknownTool {
            name: tool_name.to_string(),
        }),
    }
}

impl McpServer {
    fn actor_runtime(&self) -> M1ndResult<&StdioActorRuntime> {
        self.actor_runtime.as_ref().ok_or_else(|| {
            M1ndError::PersistenceFailed(
                "stdio brain actor is unavailable; call McpServer::start first".to_string(),
            )
        })
    }

    fn actor_execute<R, Execute>(&self, mutating: bool, execute: Execute) -> M1ndResult<R>
    where
        R: Send + 'static,
        Execute: FnOnce(&mut SessionState) -> Result<R, RuntimeJobFailure> + Send + 'static,
    {
        let runtime = self.actor_runtime()?;
        runtime.project_brains.execute_target_runtime(
            Arc::clone(&runtime.session),
            None,
            true,
            mutating,
            execute,
        )
    }

    fn actor_execute_m1nd<R, Execute>(&self, mutating: bool, execute: Execute) -> M1ndResult<R>
    where
        R: Send + 'static,
        Execute: FnOnce(&mut SessionState) -> M1ndResult<R> + Send + 'static,
    {
        let runtime = self.actor_runtime()?;
        runtime.project_brains.execute_target_m1nd(
            Arc::clone(&runtime.session),
            None,
            true,
            mutating,
            execute,
        )
    }

    fn daemon_loop_view(&self) -> M1ndResult<DaemonLoopView> {
        let dropped = self
            .daemon_runtime
            .as_ref()
            .and_then(|runtime| runtime.watcher.as_ref())
            .map(|watcher| watcher.dropped_counter.load(Ordering::Relaxed));
        self.actor_execute(false, move |state| {
            if !state.read_only {
                if let Some(dropped) = dropped {
                    let previous = state.daemon_state.watch_events_dropped;
                    state.daemon_state.watch_events_dropped = previous.max(dropped);
                    if state.daemon_state.watch_events_dropped != previous {
                        // `daemon_state` is a durable checkpoint file and this turn
                        // is read-classified, so the actor's witness cannot see the
                        // write. Join the staged-persist debounce.
                        state.note_durable_sidecar_drift();
                    }
                }
            }
            Ok(DaemonLoopView {
                active: state.daemon_state.active,
                read_only: state.read_only,
                watch_paths: state.daemon_state.watch_paths.clone(),
                git_root_present: state.daemon_state.git_root.is_some(),
                watch_backend: state.daemon_state.watch_backend.clone(),
                watch_backend_error: state.daemon_state.watch_backend_error.clone(),
                coalesce_window_ms: state.daemon_state.coalesce_window_ms,
                wait_duration_ms: daemon_wait_duration_ms(state),
            })
        })
    }

    fn refresh_daemon_watcher(&mut self) -> M1ndResult<()> {
        let Some(event_tx) = self
            .daemon_runtime
            .as_ref()
            .map(|runtime| runtime.event_tx.clone())
        else {
            return Ok(());
        };
        let view = self.daemon_loop_view()?;
        if let Some(runtime) = self.daemon_runtime.as_mut() {
            runtime.watcher = None;
        }

        if view.read_only {
            return Ok(());
        }

        if !view.active {
            if view.watch_backend != "polling" || view.watch_backend_error.is_some() {
                self.actor_execute(true, |state| {
                    state.daemon_state.watch_backend = "polling".into();
                    state.daemon_state.watch_backend_error = None;
                    Ok(())
                })?;
            }
            return Ok(());
        }

        let (watcher, backend, backend_error) =
            match LiveDaemonWatcher::start(&view.watch_paths, event_tx) {
                Ok(watcher) => {
                    let backend = if view.git_root_present {
                        "git_native_fs"
                    } else {
                        "native_fs"
                    };
                    (Some(watcher), backend.to_string(), None)
                }
                Err(error) => (None, "polling".to_string(), Some(error)),
            };

        let published_backend = backend.clone();
        let published_error = backend_error.clone();
        if let Err(error) = self.actor_execute(true, move |state| {
            state.daemon_state.watch_backend = published_backend;
            state.daemon_state.watch_backend_error = published_error;
            Ok(())
        }) {
            drop(watcher);
            return Err(error);
        }
        if let Some(runtime) = self.daemon_runtime.as_mut() {
            runtime.watcher = watcher;
        }
        Ok(())
    }

    /// Create server with config. Does not start serving yet.
    ///
    /// Startup sequence:
    /// 1. Try to load graph snapshot from disk
    /// 2. If loaded, finalize (PageRank + CSR) if needed
    /// 3. Build all engines from graph
    /// 4. Try to load plasticity state and import into graph
    /// 5. Fall back gracefully to empty graph on any failure
    pub fn new(config: McpConfig) -> M1ndResult<Self> {
        // Build domain config from config.domain
        let domain_config = match config.domain.as_deref() {
            Some("music") => DomainConfig::music(),
            Some("memory") => DomainConfig::memory(),
            Some("generic") => DomainConfig::generic(),
            Some("code") | None => DomainConfig::code(),
            Some(other) => {
                eprintln!("[m1nd] Unknown domain '{}', falling back to 'code'", other);
                DomainConfig::code()
            }
        };
        eprintln!("[m1nd] Domain: {}", domain_config.name);

        // The one-time legacy-snapshot adoption (upgrade-path repair for a
        // pre-1.5 layout that kept its populated snapshot at `./graph_snapshot.json`)
        // used to run HERE, writing the legacy bytes into the runtime root before
        // the load below found them. It cannot: no actor exists yet, so
        // `BrainActorHandle::start` reverted the adopted files to a CURRENT that
        // predated them, on the same boot. It now runs once the bound actor is up
        // — see `legacy_snapshot_adoption::maybe_adopt_legacy_snapshot`, invoked
        // from `ProjectBrainRegistry::runtime_for_target`.

        // Step 1: Try to load graph snapshot
        let (mut graph, graph_loaded) = if config.graph_source.exists() {
            match m1nd_core::snapshot::load_graph(&config.graph_source) {
                Ok(g) => {
                    eprintln!(
                        "[m1nd] Loaded graph snapshot: {} nodes, {} edges",
                        g.num_nodes(),
                        g.num_edges(),
                    );
                    (g, true)
                }
                Err(e) => {
                    eprintln!(
                        "[m1nd] Failed to load graph snapshot ({}), starting fresh",
                        e,
                    );
                    (m1nd_core::graph::Graph::new(), false)
                }
            }
        } else {
            eprintln!("[m1nd] No graph snapshot found, starting fresh");
            (m1nd_core::graph::Graph::new(), false)
        };

        // Step 2: Finalize loaded graph if needed
        if graph_loaded && !graph.finalized && graph.num_nodes() > 0 {
            if let Err(e) = graph.finalize() {
                eprintln!(
                    "[m1nd] Failed to finalize loaded graph ({}), starting fresh",
                    e,
                );
                graph = m1nd_core::graph::Graph::new();
            }
        }

        // Step 3: Build all engines (handled by SessionState::initialize)
        let mut state = SessionState::initialize(graph, &config, domain_config)?;

        // Step 4: Try to load plasticity state
        if graph_loaded && config.plasticity_state.exists() {
            match m1nd_core::snapshot::load_plasticity_state(&config.plasticity_state) {
                Ok(states) => {
                    let mut g = state.graph.write();
                    // BOTH engines restore from the sidecar, exactly as strict
                    // recovery does (`SessionState::recover_from_checkpoint`).
                    // `state.orchestrator.plasticity` is the engine `activate`/
                    // `query` actually update (query.rs `query()` step 8), and it
                    // stamps its own `query_count` into `last_used_query`. Left at
                    // zero while the shared graph carries the restored counts, the
                    // first strengthen marks a just-used edge 1 — i.e. OLDER than
                    // every edge untouched since the previous boot, skewing every
                    // recency consumer. Re-applying the same validated plan to the
                    // same topology is idempotent and cannot fail where the first
                    // import succeeded, so the two share one report below.
                    let imported = state
                        .plasticity
                        .import_state(&mut g, &states)
                        .and_then(|_| state.orchestrator.plasticity.import_state(&mut g, &states));
                    match imported {
                        Ok(_) => {
                            eprintln!(
                                "[m1nd] Loaded plasticity state: {} synaptic records",
                                states.len(),
                            );
                        }
                        Err(e) => {
                            eprintln!(
                                "[m1nd] Failed to import plasticity state ({}), continuing without it",
                                e,
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[m1nd] Failed to load plasticity state ({}), continuing without it",
                        e,
                    );
                }
            }
        }

        // Step 5: Auto-load agent-authored memory.
        // On boot, ingest <runtime_root>/agent-memory/*.light.md (adapter=light,
        // mode=merge) so knowledge the agent authored via `memorize` in prior
        // sessions is loaded automatically. Gated by M1ND_AUTO_LOAD_AGENT_MEMORY
        // (default ON; "0"/"false" disables). The result is stashed on the
        // session and surfaced verbatim in session_handshake — never hidden.
        state.agent_memory_boot = crate::tools::reload_agent_memory(&mut state);

        let offline_context = (state.runtime_root.clone(), state.project_root_display());

        Ok(Self {
            config,
            boot_state: Some(state),
            actor_runtime: None,
            daemon_runtime: None,
            offline_context,
            shutdown_requested: Arc::new(AtomicBool::new(false)),
            shutdown_wake: Arc::new(std::sync::Mutex::new(None)),
            stopped: false,
        })
    }

    /// Internal transport handoff into the brain actor boundary.
    ///
    /// External callers must never obtain the raw mutable session or its
    /// process-lifecycle capability. Transports inside this crate consume the
    /// server and immediately install the state into a [`BrainSessionCell`].
    ///
    /// ```compile_fail
    /// use m1nd_mcp::server::{McpConfig, McpServer};
    ///
    /// let _raw = McpServer::new(McpConfig::default())
    ///     .unwrap()
    ///     .into_session_state();
    /// ```
    pub(crate) fn into_session_state(mut self) -> SessionState {
        self.boot_state
            .take()
            .expect("raw SessionState is available only before McpServer::start")
    }

    /// Start the stdio owner's revocable heartbeat without exposing the unique
    /// instance handle to the binary crate or any library consumer.
    pub fn spawn_instance_heartbeat(&self) -> M1ndResult<tokio::task::JoinHandle<()>> {
        let permit = self.actor_execute(false, |state| Ok(state.instance.heartbeat_permit()))?;
        Ok(crate::instance_registry::spawn_heartbeat(permit))
    }

    /// The configured global registry dir (the shared instance phonebook), for an
    /// offline one-shot that has to build a `ProjectBrainRegistry` of its own and
    /// must land its instances in the SAME phonebook the owner uses.
    pub(crate) fn config_registry_dir(&self) -> Option<PathBuf> {
        self.config.registry_dir.clone()
    }

    /// Return an opaque cooperative stop handle for a running stdio loop.
    pub fn shutdown_handle(&self) -> McpShutdownHandle {
        McpShutdownHandle {
            requested: Arc::clone(&self.shutdown_requested),
            wake: Arc::clone(&self.shutdown_wake),
        }
    }

    /// Return the safe in-process tool facade after actor startup.
    pub fn tool_client(&self) -> M1ndResult<McpToolClient> {
        let runtime = self.actor_runtime()?;
        Ok(McpToolClient {
            session: Arc::downgrade(&runtime.session),
            project_brains: Arc::downgrade(&runtime.project_brains),
        })
    }

    /// Narrow, copy-only projection used by offline operator subcommands. It
    /// deliberately returns no mutable session, lock, actor, or lifecycle
    /// capability.
    pub fn offline_operator_context(&self) -> (PathBuf, Option<String>) {
        self.offline_context.clone()
    }

    /// Startup sequence (03-MCP Section 1.2):
    /// 1. Load graph snapshot       (done in new())
    /// 2. Load plasticity state     (done in new())
    /// 3. Compute PageRank          (done in new() via finalize)
    /// 4. Build CSR (finalize)      (done in new() via finalize)
    /// 5. Warm up engines           (engines built in new())
    /// 6. Register MCP tools (13 tools)
    /// 7. Ready for connections
    pub fn start(&mut self) -> M1ndResult<()> {
        if self.actor_runtime.is_none() {
            let state = self.boot_state.take().ok_or_else(|| {
                M1ndError::PersistenceFailed(
                    "stdio server has no construction state to install".to_string(),
                )
            })?;
            let runtime_root = state.runtime_root.clone();
            let session = Arc::new(BrainSessionCell::new(state));
            let project_brains = Arc::new(crate::project_brains::ProjectBrainRegistry::new(
                runtime_root.join(crate::project_brains::PROJECT_BRAINS_DIR),
                self.config.registry_dir.clone(),
            ));
            self.actor_runtime = Some(StdioActorRuntime {
                session,
                project_brains,
            });
        }

        let runtime = self.actor_runtime()?;
        let snapshot = runtime.project_brains.read_target_runtime_snapshot(
            Arc::clone(&runtime.session),
            None,
            true,
            |state| {
                Ok((
                    state.graph.read().num_nodes(),
                    state.graph.read().num_edges(),
                ))
            },
        )?;
        eprintln!(
            "[m1nd-mcp] Server ready. {} nodes, {} edges",
            snapshot.value.0, snapshot.value.1,
        );

        Ok(())
    }

    /// Main event loop: read JSON-RPC from stdin, dispatch, write response to stdout.
    /// Blocks until EOF or shutdown signal.
    pub fn serve(&mut self) -> M1ndResult<()> {
        self.actor_runtime()?;
        if self.shutdown_requested.load(Ordering::Acquire) {
            return Ok(());
        }
        let stdout = std::io::stdout();
        let mut writer = stdout.lock();
        let (tx, rx) = mpsc::sync_channel(1024);
        self.daemon_runtime = Some(DaemonRuntimeControl {
            event_tx: tx.clone(),
            watcher: None,
        });
        if let Ok(mut wake) = self.shutdown_wake.lock() {
            *wake = Some(tx.clone());
        }
        let _wake_registration = ShutdownWakeRegistration {
            wake: Arc::clone(&self.shutdown_wake),
        };
        self.refresh_daemon_watcher()?;

        if self.shutdown_requested.load(Ordering::Acquire) {
            return Ok(());
        }

        thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut reader = stdin.lock();
            loop {
                let next = read_request_payload(&mut reader);
                match next {
                    Ok(Some(value)) => {
                        if tx.send(ServerEvent::Request(value.0, value.1)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = tx.send(ServerEvent::StdinClosed);
                        break;
                    }
                    Err(_) => {
                        let _ = tx.send(ServerEvent::StdinClosed);
                        break;
                    }
                }
            }
        });

        let mut pending_request: Option<(String, TransportMode)> = None;
        'serve: loop {
            if self.shutdown_requested.load(Ordering::Acquire) {
                break;
            }
            let daemon_view = self.daemon_loop_view()?;

            let next_event = if let Some((payload, mode)) = pending_request.take() {
                Ok(ServerEvent::Request(payload, mode))
            } else {
                rx.recv_timeout(Duration::from_millis(daemon_view.wait_duration_ms))
            };

            let (payload, transport_mode) = match next_event {
                Ok(ServerEvent::Request(payload, mode)) => (payload, mode),
                Ok(ServerEvent::StdinClosed) => break,
                Ok(ServerEvent::Shutdown) => break,
                Ok(ServerEvent::WatchNotice) => {
                    let burst = coalesce_watch_burst(&rx, daemon_view.coalesce_window_ms);
                    let watch_events_seen = burst.watch_events_seen;
                    let coalesced_at_ms = burst.coalesced_at_ms;
                    let coalesced_error = burst.backend_error;
                    let coalesced_watch_errors = burst.watch_errors;
                    pending_request = burst.pending_request;
                    let stop_after_coalesce = burst.stdin_closed || burst.shutdown;
                    let dropped = self
                        .daemon_runtime
                        .as_ref()
                        .and_then(|runtime| runtime.watcher.as_ref())
                        .map(|watcher| watcher.dropped_counter.load(Ordering::Relaxed));
                    self.actor_execute(true, move |state| {
                        if state.read_only {
                            return Ok(());
                        }
                        state.daemon_state.last_watch_event_ms = Some(coalesced_at_ms);
                        state.daemon_state.watch_events_seen = state
                            .daemon_state
                            .watch_events_seen
                            .saturating_add(watch_events_seen);
                        state.daemon_state.last_coalesced_event_ms = Some(coalesced_at_ms);
                        state.daemon_state.coalesced_event_count = state
                            .daemon_state
                            .coalesced_event_count
                            .saturating_add(watch_events_seen);
                        state.daemon_state.watch_events_dropped = state
                            .daemon_state
                            .watch_events_dropped
                            .max(dropped.unwrap_or(0))
                            .saturating_add(coalesced_watch_errors);
                        if coalesced_error.is_some() {
                            state.daemon_state.watch_backend_error = coalesced_error;
                        }
                        run_daemon_tick(state, "watch_event");
                        Ok(())
                    })?;
                    if stop_after_coalesce {
                        break 'serve;
                    }
                    continue;
                }
                Ok(ServerEvent::WatchError(error)) => {
                    let observed_at = now_ms();
                    self.actor_execute(true, move |state| {
                        if state.read_only {
                            return Ok(());
                        }
                        state.daemon_state.watch_events_dropped =
                            state.daemon_state.watch_events_dropped.saturating_add(1);
                        state.daemon_state.watch_backend_error = Some(error);
                        state.daemon_state.last_watch_event_ms = Some(observed_at);
                        run_daemon_tick(state, "reconciliation");
                        Ok(())
                    })?;
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Drain auto-ingest on the idle clock, independent of the code
                    // daemon: an idle session may have auto-ingest running while the
                    // daemon is stopped, and the notify callback only enqueues.
                    // maybe_tick short-circuits when read-only / not running / empty,
                    // so this stays cheap and must run BEFORE the daemon-inactive
                    // early continue below.
                    self.actor_execute(false, |state| {
                        if state.read_only {
                            return Ok(());
                        }
                        let _ = auto_ingest::pump_auto_ingest_if_due(state);
                        let trigger = if state.daemon_state.watch_backend == "native_fs" {
                            "reconciliation"
                        } else {
                            "idle_timeout"
                        };
                        if !state.daemon_state.active || state.daemon_state.poll_interval_ms == 0 {
                            return Ok(());
                        }
                        let due = state.daemon_state.last_tick_ms.is_none_or(|last| {
                            now_ms().saturating_sub(last) >= daemon_wait_duration_ms(state)
                        });
                        if due {
                            run_daemon_tick(state, trigger);
                        }
                        Ok(())
                    })?;
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            let trimmed = payload.trim();
            if trimmed.is_empty() {
                continue;
            }

            // MCP notifications (no "id" field) must be silently ignored per spec.
            // Check for notification before attempting full request parse.
            if let Ok(raw) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if raw.get("id").is_none() {
                    // This is a notification — no response required.
                    continue;
                }
            }

            // Parse JSON-RPC request
            let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
                Ok(r) => r,
                Err(e) => {
                    let err_resp = JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: serde_json::Value::Null,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {}", e),
                            data: None,
                        }),
                    };
                    let _ = write_response(&mut writer, &err_resp, transport_mode);
                    continue;
                }
            };

            // Dispatch and get response
            let response = self.dispatch(&request);

            let resp = match response {
                Ok(r) => r,
                Err(e) => JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: request.id.clone(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32603,
                        message: format!("{}", e),
                        data: None,
                    }),
                },
            };

            if write_response(&mut writer, &resp, transport_mode).is_err() {
                break; // stdout closed
            }
        }

        Ok(())
    }

    /// Graceful shutdown: persist state, flush writes, close connections.
    pub fn shutdown(&mut self) -> M1ndResult<()> {
        if self.stopped {
            return Ok(());
        }
        eprintln!("[m1nd-mcp] Shutting down...");
        self.daemon_runtime = None;

        if let Some(runtime) = self.actor_runtime.as_ref() {
            // A failed checkpoint/actor stop is NOT a release condition. Keep
            // the unique process lease alive so an unacked postimage can never
            // race a replacement writer.
            let acks = runtime.project_brains.shutdown(Duration::from_secs(5))?;
            {
                let mut state = runtime.session.lock_mut_before_actor().map_err(|error| {
                    M1ndError::PersistenceFailed(format!(
                        "stdio actor stopped but session ownership was not restored: {error}"
                    ))
                })?;
                state.instance.release()?;
            }
            self.actor_runtime = None;
            self.stopped = true;
            eprintln!(
                "[m1nd-mcp] {} actor checkpoint ACK(s); owner released. Goodbye.",
                acks.len()
            );
            return Ok(());
        }

        if let Some(state) = self.boot_state.as_mut() {
            // Construction-only shutdown still obeys persist-before-release.
            state.persist()?;
            state.instance.release()?;
        }
        self.boot_state = None;
        self.stopped = true;
        eprintln!("[m1nd-mcp] State persisted; owner released. Goodbye.");
        Ok(())
    }

    /// Dispatch a single JSON-RPC request to the appropriate tool handler.
    ///
    /// Thin wrapper over the transport-agnostic [`handle_mcp_method`] free fn.
    /// The only stdio-specific concern kept here is refreshing the live FS
    /// watcher after a successful `daemon_start`/`daemon_stop`, which requires
    /// `&mut self` (the watcher lives on `McpServer`, not `SessionState`).
    fn dispatch(&mut self, request: &JsonRpcRequest) -> M1ndResult<JsonRpcResponse> {
        let mutating = if request.method == "tools/call" {
            let tool_name = request
                .params
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
            read_only_denied(tool_name, &arguments)
        } else {
            false
        };
        let actor_request = request.clone();
        let response = match self.actor_execute_m1nd(mutating, move |state| {
            handle_mcp_method_transactional(state, &actor_request)
        }) {
            Ok(response) => response,
            Err(error) if request.method == "tools/call" => {
                mcp_tool_error_response(request.id.clone(), error.to_string())
            }
            Err(error) => return Err(error),
        };

        // stdio-only: if this was a successful daemon_start/daemon_stop tool call,
        // rebind the live FS watcher to match the new daemon state.
        if request.method == "tools/call" {
            let tool_name = request
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if matches!(tool_name, "daemon_start" | "daemon_stop") {
                // The MCP wrapper reports tool execution errors via isError content,
                // not JSON-RPC errors, so only refresh when the call did not error.
                let is_error = response
                    .result
                    .as_ref()
                    .and_then(|r| r.get("isError"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if !is_error {
                    self.refresh_daemon_watcher()?;
                }
            }
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        all_tool_schemas, background_tick_if_due, daemon_wait_duration_ms, handle_mcp_method,
        handle_mcp_method_transactional, light_recall_freshness_key, read_request_payload,
        run_daemon_tick, should_autotick_daemon, tool_schemas, tool_schemas_for_tier,
        write_response, DaemonRuntimeControl, McpServer, TransportMode, ESSENTIAL_TOOLS,
    };
    use crate::server::McpConfig;
    use crate::session::SessionState;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use std::sync::mpsc;

    fn build_state() -> (tempfile::TempDir, SessionState) {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            registry_dir: Some(runtime_dir.join("registry")),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");
        (temp, state)
    }

    fn build_server() -> (tempfile::TempDir, McpServer) {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            registry_dir: Some(runtime_dir.join("registry")),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let server = McpServer::new(config).expect("server");
        (temp, server)
    }

    #[test]
    fn stdio_start_moves_raw_state_behind_the_bound_actor() {
        let (_temp, mut server) = build_server();
        assert!(server.boot_state.is_some());

        server.start().expect("start stdio actor");

        assert!(server.boot_state.is_none());
        let runtime = server.actor_runtime.as_ref().expect("actor runtime");
        assert!(
            runtime.session.try_lock().is_none(),
            "raw SessionState access must be fenced while the actor is active"
        );
        server.shutdown().expect("actor checkpoint shutdown");
    }

    #[test]
    fn stdio_protocol_dispatch_runs_initialize_and_tools_list_through_actor() {
        let (_temp, mut server) = build_server();
        server.start().expect("start stdio actor");

        let initialize = server
            .dispatch(&crate::protocol::JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: serde_json::json!(1),
                method: "initialize".into(),
                params: serde_json::json!({"protocolVersion": super::MCP_PROTOCOL_VERSION}),
            })
            .expect("initialize dispatch");
        assert_eq!(
            initialize
                .result
                .as_ref()
                .and_then(|value| value.get("serverInfo"))
                .and_then(|value| value.get("name")),
            Some(&serde_json::json!("m1nd-mcp"))
        );

        let tools = server
            .dispatch(&crate::protocol::JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: serde_json::json!(2),
                method: "tools/list".into(),
                params: serde_json::json!({}),
            })
            .expect("tools/list dispatch");
        assert!(tools.result.as_ref().is_some_and(|result| result["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty())));

        let ping = server
            .dispatch(&crate::protocol::JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: serde_json::json!(3),
                method: "ping".into(),
                params: serde_json::json!({}),
            })
            .expect("ping dispatch");
        assert_eq!(ping.result, Some(serde_json::json!({})));
        server.shutdown().expect("shutdown");
    }

    #[test]
    fn transactional_mcp_handler_keeps_tool_failure_as_callback_error() {
        let (_temp, mut state) = build_state();
        let request = crate::protocol::JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::json!(9),
            method: "tools/call".into(),
            params: serde_json::json!({
                "name": "definitely_unknown_tool",
                "arguments": {"agent_id": "transactional-error-test"}
            }),
        };
        let error = handle_mcp_method_transactional(&mut state, &request)
            .expect_err("actor seam must observe the domain error");
        assert!(!error.to_string().is_empty());

        let wire = handle_mcp_method(&mut state, &request);
        assert_eq!(
            wire.result
                .as_ref()
                .and_then(|value| value.get("isError"))
                .and_then(|value| value.as_bool()),
            Some(true),
            "only the wire compatibility wrapper converts the error to MCP content"
        );
    }

    #[test]
    fn stdio_framing_round_trips_line_and_content_length_modes() {
        let payload = r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#;
        let framed = format!("Content-Length: {}\r\n\r\n{}", payload.len(), payload);
        let mut framed_reader = std::io::Cursor::new(framed.into_bytes());
        let (decoded, mode) = read_request_payload(&mut framed_reader)
            .expect("read framed request")
            .expect("framed payload");
        assert_eq!(decoded, payload);
        assert!(matches!(mode, TransportMode::Framed));

        let response = crate::protocol::JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: serde_json::json!(1),
            result: Some(serde_json::json!({})),
            error: None,
        };
        let mut framed_output = Vec::new();
        write_response(&mut framed_output, &response, mode).expect("write framed response");
        let framed_output = String::from_utf8(framed_output).expect("framed UTF-8");
        assert!(framed_output.starts_with("Content-Length: "));
        assert!(framed_output.contains("\r\n\r\n{"));

        let mut line_reader = std::io::Cursor::new(format!("{payload}\n").into_bytes());
        let (decoded, mode) = read_request_payload(&mut line_reader)
            .expect("read line request")
            .expect("line payload");
        assert_eq!(decoded, payload);
        assert!(matches!(mode, TransportMode::Line));
        let mut line_output = Vec::new();
        write_response(&mut line_output, &response, mode).expect("write line response");
        assert!(line_output.ends_with(b"\n"));
    }

    #[test]
    fn safe_tool_client_is_available_only_after_start_and_uses_actor_dispatch() {
        let (_temp, mut server) = build_server();
        assert!(server.tool_client().is_err());
        server.start().expect("start stdio actor");

        let client = server.tool_client().expect("safe tool client");
        let health = client
            .call_tool("health", &serde_json::json!({"agent_id": "embed-test"}))
            .expect("actor-backed health");
        assert!(health.is_object());
        server.shutdown().expect("shutdown");
        assert!(client
            .call_tool("health", &serde_json::json!({"agent_id": "after-stop"}))
            .is_err());
    }

    #[test]
    fn actor_mutation_classification_preserves_persist_status_exception() {
        assert!(!super::read_only_denied(
            "persist",
            &serde_json::json!({"action": "status"})
        ));
        assert!(super::read_only_denied(
            "persist",
            &serde_json::json!({"action": "save"})
        ));
        assert!(super::read_only_denied(
            "m1nd.persist",
            &serde_json::json!({"action": "checkpoint"})
        ));
    }

    #[test]
    fn stdin_closed_consumed_during_watch_coalescing_is_terminal() {
        let (tx, rx) = mpsc::sync_channel(2);
        tx.send(super::ServerEvent::StdinClosed)
            .expect("queue stdin close behind the already-consumed watch notice");

        let burst = super::coalesce_watch_burst(&rx, 1);

        assert!(burst.stdin_closed);
        assert!(burst.pending_request.is_none());
        assert_eq!(burst.watch_events_seen, 1);
    }

    #[test]
    fn failed_prestart_persist_keeps_instance_registered() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        let registry_dir = runtime_dir.join("registry");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let poison = temp.path().join("not-a-directory");
        std::fs::write(&poison, "poison").expect("poison file");
        let config = McpConfig {
            graph_source: poison.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            registry_dir: Some(registry_dir.clone()),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let mut server = McpServer::new(config).expect("server");

        assert!(server.shutdown().is_err(), "poisoned persist must fail");
        assert!(
            !crate::instance_registry::list_instances(Some(&registry_dir))
                .expect("list live owner")
                .is_empty(),
            "persist failure must not release the owner lease"
        );
    }

    fn build_state_read_only() -> (tempfile::TempDir, SessionState) {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            registry_dir: Some(runtime_dir.join("registry")),
            runtime_dir: Some(runtime_dir),
            read_only: true,
            ..McpConfig::default()
        };
        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");
        (temp, state)
    }

    fn digest_bytes(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};

        format!("sha256:{}", crate::util::hex_lower(&Sha256::digest(bytes)))
    }

    fn directory_digest(root: &std::path::Path) -> String {
        fn collect(
            root: &std::path::Path,
            current: &std::path::Path,
            records: &mut Vec<(String, u8, Vec<u8>)>,
        ) {
            let Ok(entries) = std::fs::read_dir(current) else {
                return;
            };
            let mut paths: Vec<std::path::PathBuf> = entries
                .map(|entry| entry.expect("read directory entry").path())
                .collect();
            paths.sort();
            for path in paths {
                let relative = path
                    .strip_prefix(root)
                    .expect("path under digest root")
                    .to_string_lossy()
                    .to_string();
                let metadata = std::fs::symlink_metadata(&path).expect("digest metadata");
                if metadata.file_type().is_symlink() {
                    records.push((
                        relative,
                        b'L',
                        std::fs::read_link(&path)
                            .expect("digest symlink")
                            .to_string_lossy()
                            .as_bytes()
                            .to_vec(),
                    ));
                } else if metadata.is_dir() {
                    records.push((relative, b'D', Vec::new()));
                    collect(root, &path, records);
                } else {
                    records.push((relative, b'F', std::fs::read(&path).expect("digest file")));
                }
            }
        }

        use sha2::{Digest, Sha256};
        let mut records = Vec::new();
        if root.exists() {
            collect(root, root, &mut records);
        }
        let mut hasher = Sha256::new();
        for (relative, kind, bytes) in records {
            hasher.update((relative.len() as u64).to_le_bytes());
            hasher.update(relative.as_bytes());
            hasher.update([kind]);
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        }
        format!("sha256:{}", crate::util::hex_lower(&hasher.finalize()))
    }

    fn graph_digest(state: &SessionState) -> String {
        let scratch = tempfile::tempdir().expect("graph digest scratch");
        let path = scratch.path().join("graph.json");
        m1nd_core::snapshot::save_graph(&state.graph.read(), &path)
            .expect("serialize graph for digest");
        digest_bytes(&std::fs::read(path).expect("read graph digest snapshot"))
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DeniedActionStateDigest {
        graph: String,
        store: String,
        filesystem: String,
        graph_generation: u64,
        plasticity_generation: u64,
        cache_generation: u64,
        queries_processed: u64,
        tracked_agents: Vec<String>,
    }

    fn denied_action_state_digest(
        state: &SessionState,
        filesystem_root: &std::path::Path,
    ) -> DeniedActionStateDigest {
        let mut tracked_agents: Vec<String> = state.sessions.keys().cloned().collect();
        tracked_agents.sort();
        DeniedActionStateDigest {
            graph: graph_digest(state),
            store: directory_digest(&crate::system_blocks_handlers::store_dir(state)),
            filesystem: directory_digest(filesystem_root),
            graph_generation: state.graph_generation,
            plasticity_generation: state.plasticity_generation,
            cache_generation: state.cache_generation,
            queries_processed: state.queries_processed,
            tracked_agents,
        }
    }

    /// Exercise the retired receipt primitive's domain laws without reopening
    /// it on any external dispatcher. External ingress tests must continue to
    /// call `dispatch_tool` and observe the permanent G3 tombstone.
    fn call_receipt_import_handler(
        state: &mut SessionState,
        params: &serde_json::Value,
    ) -> super::M1ndResult<serde_json::Value> {
        let input: crate::system_blocks_handlers::ReceiptImportInput =
            serde_json::from_value(params.clone()).map_err(super::M1ndError::Serde)?;
        crate::system_blocks_handlers::handle_receipt_import(state, input)
    }

    /// Exercise mission-letter validation/persistence as a domain unit. The
    /// helper is test-only and deliberately bypasses no production ingress.
    fn call_mission_post_handler(
        state: &mut SessionState,
        params: &serde_json::Value,
    ) -> super::M1ndResult<serde_json::Value> {
        let input: crate::mission_letter_handlers::MissionPostInput =
            serde_json::from_value(params.clone()).map_err(super::M1ndError::Serde)?;
        crate::mission_letter_handlers::handle_mission_post(state, input)
    }

    #[test]
    fn generic_dispatch_floor_table_is_exhaustive_and_fail_closed() {
        use m1nd_control::AuthorityFloor;

        assert!(super::generic_dispatch_floor_is_available(
            AuthorityFloor::Ordinary
        ));
        for floor in [
            AuthorityFloor::ScopedGrantA2,
            AuthorityFloor::PositiveSovereign,
            AuthorityFloor::ServiceIdentity,
            AuthorityFloor::SafetyOnly,
        ] {
            assert!(
                !super::generic_dispatch_floor_is_available(floor),
                "{floor:?} must never be available on generic dispatch"
            );
        }

        // Every registered MCP tool has a catalog-backed floor union. Routes
        // whose every branch is elevated must refuse even with an empty or
        // otherwise incomplete body; malformed selectors cannot weaken them.
        for tool in crate::action_routes::MCP_TOOL_ROUTE_NAMES {
            let floors =
                crate::action_routes::possible_mcp_authority_floors(tool).unwrap_or_else(|error| {
                    panic!("authority-floor route missing for {tool}: {error}")
                });
            assert!(!floors.is_empty(), "empty floor union for {tool}");
            if floors
                .iter()
                .all(|floor| !super::generic_dispatch_floor_is_available(*floor))
            {
                assert!(
                    super::enforce_generic_action_policy(tool, &serde_json::json!({})).is_err(),
                    "all-elevated route {tool} admitted an incomplete generic call"
                );
            }
        }
    }

    #[test]
    fn generic_dispatch_allows_exact_ordinary_branches_and_denies_elevated_branches() {
        for (tool, params) in [
            ("health", serde_json::json!({"agent_id": "ordinary"})),
            (
                "persist",
                serde_json::json!({"agent_id": "ordinary", "action": "status"}),
            ),
            (
                "boot_memory",
                serde_json::json!({"agent_id": "ordinary", "action": "get"}),
            ),
            (
                "memorize",
                serde_json::json!({"agent_id": "ordinary", "node_label": "default path", "claims": []}),
            ),
        ] {
            super::enforce_generic_action_policy(tool, &params)
                .unwrap_or_else(|error| panic!("ORDINARY branch {tool} refused: {error}"));
        }

        let elevated = [
            (
                "apply",
                serde_json::json!({"agent_id": "a", "file_path": "/tmp/never-write.rs", "new_content": "denied", "reingest": false}),
                "SCOPED_GRANT_A2",
            ),
            (
                "system_blocks_ratify",
                serde_json::json!({"agent_id": "a", "expected_store_version": 1, "ratifier": "a", "ratified_via": "human-ui"}),
                "POSITIVE_SOVEREIGN",
            ),
            (
                "daemon_tick",
                serde_json::json!({"agent_id": "a"}),
                "SERVICE_IDENTITY",
            ),
            (
                "ingest",
                serde_json::json!({"agent_id": "a", "mode": "merge", "paths": ["."]}),
                "SCOPED_GRANT_A2",
            ),
            (
                "ingest",
                serde_json::json!({
                    "agent_id": "a",
                    "path": "/tmp/never-bootstrap",
                    "project_root": "/tmp/never-bootstrap"
                }),
                "POSITIVE_SOVEREIGN",
            ),
        ];
        for (tool, params, expected_floor) in elevated {
            for routed_name in [
                tool.to_string(),
                format!("m1nd.{tool}"),
                format!("m1nd_{tool}"),
            ] {
                let error = super::enforce_generic_action_policy(&routed_name, &params)
                    .expect_err("elevated generic action must refuse");
                let rendered = error.to_string();
                assert!(
                    rendered.contains("generic_action_authority_required")
                        && rendered.contains(expected_floor),
                    "unexpected {routed_name} refusal: {rendered}"
                );
            }
        }
    }

    #[test]
    fn denied_generic_actions_and_prefixed_aliases_change_no_state_digest() {
        use crate::protocol::JsonRpcRequest;

        let (temp, mut state) = build_state();
        super::dispatch_tool(
            &mut state,
            "system_blocks_seed_import",
            &serde_json::json!({
                "agent_id": "fixture",
                "seed_json": include_str!("../../docs/system-blocks/m1nd.seed.v0.json")
            }),
        )
        .expect("seed fixture through the internal domain dispatch");

        let denied_source = temp.path().join("must-not-change.rs");
        std::fs::write(&denied_source, "pub const ORIGINAL: bool = true;\n")
            .expect("seed denied source target");
        let denied = [
            (
                "apply",
                serde_json::json!({
                    "agent_id": "attacker",
                    "file_path": denied_source.to_string_lossy(),
                    "new_content": "pub const COMPROMISED: bool = true;\n",
                    "reingest": false
                }),
                "SCOPED_GRANT_A2",
            ),
            (
                "system_blocks_ratify",
                serde_json::json!({
                    "agent_id": "attacker",
                    "expected_store_version": 1,
                    "ratifier": "attacker",
                    "ratified_via": "human-ui"
                }),
                "POSITIVE_SOVEREIGN",
            ),
            (
                "system_blocks_delete",
                serde_json::json!({
                    "agent_id": "attacker",
                    "expected_store_version": 1,
                    "block_id": "sb_m1nd_core_graph_kernel",
                    "force": true
                }),
                "POSITIVE_SOVEREIGN",
            ),
            (
                "daemon_tick",
                serde_json::json!({"agent_id": "attacker"}),
                "SERVICE_IDENTITY",
            ),
        ];
        let baseline = denied_action_state_digest(&state, temp.path());

        for (tool, arguments, expected_floor) in denied {
            for name in [
                tool.to_string(),
                format!("m1nd.{tool}"),
                format!("m1nd_{tool}"),
            ] {
                let response = super::handle_mcp_method(
                    &mut state,
                    &JsonRpcRequest {
                        jsonrpc: "2.0".to_string(),
                        id: serde_json::json!(name.clone()),
                        method: "tools/call".to_string(),
                        params: serde_json::json!({
                            "name": name.clone(),
                            "arguments": arguments.clone(),
                        }),
                    },
                );
                let rendered = response.result.expect("tool refusal result").to_string();
                assert!(
                    rendered.contains("generic_action_authority_required")
                        && rendered.contains(expected_floor),
                    "unexpected refusal: {rendered}"
                );
                assert_eq!(
                    denied_action_state_digest(&state, temp.path()),
                    baseline,
                    "denied {name} changed graph/store/filesystem or tracking state"
                );
            }
        }
        assert_eq!(
            std::fs::read_to_string(denied_source).expect("read denied source target"),
            "pub const ORIGINAL: bool = true;\n"
        );
        assert!(!state.sessions.contains_key("attacker"));
    }

    // === Gardener v1: FAIL-OPEN — a background vigil never breaks a tool call ===

    /// The fail-open contract (gardener v1): a vigil that ERRORS is swallowed
    /// (logged, reported `false`), never propagated. RED against the old `?` at the
    /// auto-ingest tick seam — a `?` would have surfaced the error into the tool call.
    #[test]
    fn vigil_fail_open_swallows_a_failing_vigil() {
        use super::vigil_fail_open;
        let ok = vigil_fail_open("test vigil", "search", || {
            Err(m1nd_core::error::M1ndError::CorruptState {
                reason: "boom".into(),
            })
        });
        assert!(!ok, "a failing vigil is swallowed and reports false");
        let ok2 = vigil_fail_open("test vigil", "search", || Ok(()));
        assert!(ok2, "a succeeding vigil reports true");
    }

    /// End-to-end through the real traffic-autotick seam: a daemon whose tick CANNOT
    /// persist (its state path's parent is a regular file, so `save_json_atomic`
    /// fails) must NOT turn the agent's unrelated tool call into an error. "tick com
    /// persist quebrado → o tool call do agente SUCEDE."
    #[test]
    fn broken_background_tick_never_fails_the_agents_tool_call() {
        use crate::protocol::core::JsonRpcRequest;
        let (temp, mut state) = build_state();

        // Arm the daemon (this persists once to the good path), then break its
        // persist target so the NEXT tick's persist errors.
        crate::daemon_handlers::handle_daemon_start(
            &mut state,
            crate::protocol::layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec![temp.path().to_string_lossy().to_string()],
                poll_interval_ms: 1,
            },
        )
        .expect("daemon start");

        let poison_file = temp.path().join("poison");
        std::fs::write(&poison_file, b"i am a file, not a dir").expect("write poison file");
        // A path whose PARENT is a regular file → create_dir_all fails → persist Err.
        state.daemon_state_path = poison_file.join("daemon_state.json");
        // Force the traffic autotick to be due.
        state.daemon_state.last_tick_ms = Some(0);

        // Sanity: the poisoned persist really does error now.
        assert!(
            state.persist_daemon_state().is_err(),
            "precondition: the poisoned daemon_state_path must make persist fail"
        );

        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::json!(1),
            method: "tools/call".into(),
            params: serde_json::json!({
                "name": "health",
                "arguments": { "agent_id": "test" }
            }),
        };
        let response = super::handle_mcp_method(&mut state, &request);
        assert!(
            response.error.is_none(),
            "a broken background tick must not surface a JSON-RPC error: {:?}",
            response.error
        );
        let is_error = response
            .result
            .as_ref()
            .and_then(|r| r.get("isError"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(
            !is_error,
            "the agent's tool call must SUCCEED despite the failing background vigil"
        );
    }

    // === Gardener v1: RESTART RESUME — an armed daemon survives a boot and ticks ===

    /// The resume law (gardener v1): `active` survives a restart, and the daemon
    /// actually TICKS again on the first traffic. RED without the load-time
    /// sanitization: every traffic tick persists MID-tick, so the disk carries
    /// `tick_in_flight: true` (proven below as a precondition) — a verbatim resume
    /// wedges `run_daemon_tick` forever (it sees a phantom in-flight tick and
    /// refuses every new one).
    #[test]
    fn armed_daemon_resumes_across_restart_and_ticks_again() {
        use crate::protocol::core::JsonRpcRequest;
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).expect("repo dir");
        std::fs::write(repo.join("core.py"), "def core():\n    return 1\n").expect("seed file");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            registry_dir: Some(runtime_dir.join("registry")),
            runtime_dir: Some(runtime_dir.clone()),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        crate::daemon_handlers::handle_daemon_start(
            &mut state,
            crate::protocol::layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec![repo.to_string_lossy().to_string()],
                poll_interval_ms: 1,
            },
        )
        .expect("daemon start");

        // One REAL traffic tick through the transport seam (handle_mcp_method →
        // run_daemon_tick), exactly what a served owner does per routed call.
        state.daemon_state.last_tick_ms = Some(0);
        let health_call = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::json!(1),
            method: "tools/call".into(),
            params: serde_json::json!({
                "name": "health",
                "arguments": { "agent_id": "test" }
            }),
        };
        let response = super::handle_mcp_method(&mut state, &health_call);
        assert!(response.error.is_none(), "traffic call must succeed");
        assert!(
            state.daemon_state.tick_count >= 1,
            "the traffic autotick must have run at least one tick"
        );

        // THE WEDGE SHAPE, pinned as a precondition: the tick persisted itself
        // while `tick_in_flight` was true, so that is what the disk carries.
        let disk: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(runtime_dir.join("daemon_state.json"))
                .expect("read persisted daemon state"),
        )
        .expect("parse persisted daemon state");
        assert_eq!(
            disk["tick_in_flight"], true,
            "precondition: the post-traffic-tick disk carries the in-flight flag \
             (this is the exact shape a restart resumes from)"
        );
        assert_eq!(
            disk["active"], true,
            "precondition: the daemon is armed on disk"
        );
        drop(state);

        // RESTART: a fresh SessionState over the SAME runtime dir.
        let mut resumed = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("re-init session");
        assert!(
            resumed.daemon_state.active,
            "the armed daemon must resume active across a restart"
        );
        assert!(
            !resumed.daemon_state.tick_in_flight && !resumed.daemon_state.pending_rerun,
            "transient reentrancy flags must be sanitized on load"
        );

        // And it TICKS again on the first traffic (RED: wedged without the fix).
        resumed.daemon_state.last_tick_ms = Some(0);
        let before = resumed.daemon_state.tick_count;
        let response = super::handle_mcp_method(&mut resumed, &health_call);
        assert!(
            response.error.is_none(),
            "post-restart traffic call must succeed"
        );
        assert!(
            resumed.daemon_state.tick_count > before,
            "the resumed daemon must tick on traffic — a wedged resume is the bug"
        );
        assert_eq!(
            resumed.daemon_state.last_tick_trigger.as_deref(),
            Some("traffic"),
            "the resumed tick is the traffic tick (freshness-by-traffic, v1)"
        );
    }

    // === Gardener v1 / G1: FRESHNESS-BY-TRAFFIC ON EVERY SEAM ===
    // The REST, stdio side-loop, and mcp_http seams call `dispatch_tool` DIRECTLY,
    // never `handle_mcp_method`. The traffic autotick used to live only in
    // `handle_mcp_method`, so those three seams were deaf to it — a file changed
    // under a served owner stayed stale until an MCP-wire call happened by. The
    // tick now rides `dispatch_tool`, the one path every seam funnels through.

    /// Arms a daemon over a one-file repo and returns the session (its own runtime
    /// dir under `temp`). The `TempDir` is returned so the caller keeps it alive.
    fn build_state_with_armed_daemon() -> (tempfile::TempDir, SessionState) {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).expect("repo dir");
        std::fs::write(repo.join("core.py"), "def core():\n    return 1\n").expect("seed file");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            registry_dir: Some(runtime_dir.join("registry")),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");
        crate::daemon_handlers::handle_daemon_start(
            &mut state,
            crate::protocol::layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec![repo.to_string_lossy().to_string()],
                poll_interval_ms: 1,
            },
        )
        .expect("daemon start");
        (temp, state)
    }

    /// RED-first at the exact core of the REST seam: a NON-skip verb dispatched
    /// STRAIGHT through `dispatch_tool` (no `handle_mcp_method` in the path) must
    /// run the freshness-by-traffic tick. Before the tick moved into
    /// `dispatch_tool`, a direct dispatch left the daemon deaf and `tick_count`
    /// unchanged — that is the three-deaf-seams bug this pins.
    #[test]
    fn dispatch_tool_ticks_the_daemon_on_traffic() {
        let (_temp, mut state) = build_state_with_armed_daemon();
        state.daemon_state.last_tick_ms = Some(0); // force the autotick due
        let before = state.daemon_state.tick_count;
        super::dispatch_tool(
            &mut state,
            "health",
            &serde_json::json!({ "agent_id": "test" }),
        )
        .expect("health dispatch");
        assert!(
            state.daemon_state.tick_count > before,
            "dispatch_tool must run the freshness-by-traffic tick (the REST seam core)"
        );
        assert_eq!(
            state.daemon_state.last_tick_trigger.as_deref(),
            Some("traffic"),
            "the dispatch_tool tick is the traffic tick"
        );
    }

    /// The skip-list holds at the `dispatch_tool` seam too: a daemon-control verb
    /// (the REAL list — `should_autotick_daemon`) must NOT tick even with an armed,
    /// due daemon, while a normal verb on the SAME daemon WOULD — proving the
    /// no-tick is the skip-list, not a dead daemon.
    #[test]
    fn dispatch_tool_respects_the_autotick_skip_list() {
        let (_temp, mut state) = build_state_with_armed_daemon();
        state.daemon_state.last_tick_ms = Some(0);
        let before = state.daemon_state.tick_count;
        super::dispatch_tool(
            &mut state,
            "daemon_status",
            &serde_json::json!({ "agent_id": "test" }),
        )
        .expect("daemon_status dispatch");
        assert_eq!(
            state.daemon_state.tick_count, before,
            "a skip-list verb must NOT autotick, even through dispatch_tool"
        );

        state.daemon_state.last_tick_ms = Some(0);
        super::dispatch_tool(
            &mut state,
            "health",
            &serde_json::json!({ "agent_id": "test" }),
        )
        .expect("health dispatch");
        assert!(
            state.daemon_state.tick_count > before,
            "a normal verb on the same armed+due daemon must tick (skip-list is verb-specific)"
        );
    }

    /// Heavy re-ingest/scan entry tools are skipped by the traffic autotick: their
    /// own work supersedes the tick, and stacking the tick's wall-clock ahead of
    /// theirs risks holding the brain lock past the REST 30s timeout. Pinned as a
    /// predicate so the honest skip is explicit and cannot silently regress.
    #[test]
    fn heavy_entry_tools_skip_the_traffic_autotick() {
        use super::daemon_autotick_entry_too_heavy;
        for heavy in ["ingest", "scan", "scan_all", "skeleton_candidate"] {
            assert!(
                daemon_autotick_entry_too_heavy(heavy),
                "{heavy} is a heavy re-ingest/scan entry tool and must skip the tick"
            );
        }
        for light in ["health", "search", "seek", "why", "impact", "apply"] {
            assert!(
                !daemon_autotick_entry_too_heavy(light),
                "{light} is a normal verb and must remain tick-eligible"
            );
        }
    }

    #[test]
    fn read_only_deny_list_is_precise() {
        use super::read_only_denied;
        let empty = serde_json::json!({});
        // Mutating tools are denied (bare and prefixed).
        for t in [
            "ingest",
            "apply",
            "apply_batch",
            "edit_commit",
            "memorize",
            "learn",
            "daemon_start",
            "auto_ingest_start",
            "runtime_overlay",
            // Human View v2 F0a/F0c SystemBlock store writes.
            "skeleton_candidate",
            "system_blocks_seed_import",
            "system_blocks_ratify",
            "receipt_import",
            // Slice 3 SystemBlock store writes.
            "system_blocks_reconcile",
            "system_blocks_archive",
            "system_blocks_delete",
            // F11-a: the candidate_edit batch verb + the advisory lease verb.
            "candidate_edit",
            "candidate_lease",
            // F11-c: the Name-with-runner route (a candidate_edit write inside).
            "candidate_naming",
            // F2.5a: the mission-letter write verb.
            "mission_post",
            // F2.5c: the mission_spawn proxy (a write — it launches a mission).
            "mission_spawn",
            "m1nd_apply",
            "m1nd.ingest",
        ] {
            assert!(read_only_denied(t, &empty), "{t} should be denied");
        }
        // Read-only / analysis tools are allowed.
        for t in [
            "seek",
            "search",
            "activate",
            "why",
            "impact",
            "audit",
            "surgical_context_v2",
            "session_handshake",
            "trust_selftest",
            "doctor",
            "health",
            "view",
            "scan",
            "trace",
            "edit_preview",
            // The SystemBlock store READs are pure reads — never denied.
            "system_blocks_snapshot",
            "receipt_recompute",
        ] {
            assert!(!read_only_denied(t, &empty), "{t} should be allowed");
        }
        // persist: status is allowed; save/checkpoint/load are denied.
        assert!(!read_only_denied(
            "persist",
            &serde_json::json!({"action": "status"})
        ));
        assert!(read_only_denied(
            "persist",
            &serde_json::json!({"action": "save"})
        ));
        assert!(read_only_denied(
            "persist",
            &serde_json::json!({"action": "load"})
        ));
        // persist with no action defaults to status (allowed).
        assert!(!read_only_denied("persist", &empty));
    }

    #[test]
    fn skeleton_write_root_gate_covers_only_the_requested_mutations() {
        let empty = serde_json::json!({});
        for tool in [
            "system_blocks_seed_import",
            "skeleton_candidate",
            "candidate_edit",
            "system_blocks_ratify",
            "system_blocks_reconcile",
            "system_blocks_archive",
            "system_blocks_delete",
        ] {
            assert!(super::skeleton_write_needs_root_gate(tool, &empty));
        }
        assert!(super::skeleton_write_needs_root_gate(
            "candidate_lease",
            &serde_json::json!({"action": "acquire"})
        ));
        assert!(!super::skeleton_write_needs_root_gate(
            "candidate_lease",
            &serde_json::json!({"action": "release"})
        ));
        assert!(!super::skeleton_write_needs_root_gate("memorize", &empty));
        assert!(!super::skeleton_write_needs_root_gate(
            "system_blocks_snapshot",
            &empty
        ));
    }

    #[test]
    fn skeleton_write_refuses_foreign_caller_root_and_names_both_roots() {
        let (temp, mut state) = build_state();
        let brain_root = temp.path().join("repo-alpha");
        let caller_root = temp.path().join("repo-beta");
        std::fs::create_dir_all(&brain_root).expect("brain root");
        std::fs::create_dir_all(&caller_root).expect("caller root");
        state.workspace_root = Some(brain_root.to_string_lossy().to_string());
        state.ingest_roots = vec![brain_root.to_string_lossy().to_string()];
        state.caller_root = Some(caller_root.to_string_lossy().to_string());

        let result = super::dispatch_tool(
            &mut state,
            "system_blocks_seed_import",
            &serde_json::json!({"agent_id": "t", "seed_json": "{}"}),
        )
        .expect("root mismatch is an honest refusal");

        assert_eq!(result["refused"], "brainless_root");
        assert_eq!(
            result["caller_root"],
            caller_root.to_string_lossy().as_ref()
        );
        assert_eq!(result["brain_root"], brain_root.to_string_lossy().as_ref());
        let rendered = result.to_string();
        assert!(rendered.contains("brain_bootstrap_consumer_not_installed"));
        assert!(!rendered.contains("ingest project_root="));
        assert!(rendered.contains("explicit REST ?brain="));
    }

    #[test]
    fn skeleton_write_matching_or_absent_caller_root_reaches_handler() {
        for caller_matches in [true, false] {
            let (temp, mut state) = build_state();
            let brain_root = temp.path().join("repo-alpha");
            std::fs::create_dir_all(&brain_root).expect("brain root");
            state.workspace_root = Some(brain_root.to_string_lossy().to_string());
            state.ingest_roots = vec![brain_root.to_string_lossy().to_string()];
            state.caller_root = caller_matches.then(|| brain_root.to_string_lossy().to_string());

            let error = super::dispatch_tool(
                &mut state,
                "system_blocks_seed_import",
                &serde_json::json!({"agent_id": "t", "seed_json": "{}"}),
            )
            .expect_err("invalid seed proves dispatch reached the handler");
            assert!(!error.to_string().contains("brainless_root"));
        }
    }

    #[test]
    fn system_blocks_verbs_are_wired_end_to_end() {
        let (_temp, mut state) = build_state();
        // Fresh brain: snapshot is an honest "no skeleton".
        let snap = super::dispatch_tool(
            &mut state,
            "system_blocks_snapshot",
            &serde_json::json!({"agent_id": "t"}),
        )
        .expect("snapshot ok");
        assert_eq!(snap["present"], false);

        // Import the real seed inline -> twelve blocks at store_version 1.
        let seed = include_str!("../../docs/system-blocks/m1nd.seed.v0.json");
        let imp = super::dispatch_tool(
            &mut state,
            "system_blocks_seed_import",
            &serde_json::json!({"agent_id": "t", "seed_json": seed}),
        )
        .expect("seed_import ok");
        assert_eq!(imp["store_version"], 1);
        assert_eq!(imp["block_count"], 12);

        // Snapshot now reports the twelve blocks and the live store.
        let snap2 = super::dispatch_tool(
            &mut state,
            "system_blocks_snapshot",
            &serde_json::json!({"agent_id": "t"}),
        )
        .expect("snapshot ok");
        assert_eq!(snap2["present"], true);
        assert_eq!(snap2["block_count"], 12);

        // Direct ratification is closed before OCC: a forgeable origin string
        // cannot reach the store transition.
        let err = super::dispatch_tool(
            &mut state,
            "system_blocks_ratify",
            &serde_json::json!({"agent_id": "t", "expected_store_version": 99, "ratifier": "owner"}),
        )
        .expect_err("direct ratify must require sovereign authority");
        assert!(
            err.to_string().contains("sovereign_authority_required"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn forgeable_human_ui_token_never_authorizes_direct_ratification() {
        let (_temp, mut state) = build_state();
        let seed = include_str!("../../docs/system-blocks/m1nd.seed.v0.json");
        let imp = super::dispatch_tool(
            &mut state,
            "system_blocks_seed_import",
            &serde_json::json!({"agent_id": "t", "seed_json": seed}),
        )
        .expect("seed_import ok");
        assert_eq!(imp["store_version"], 1);
        let before = crate::system_blocks::SystemBlockStore::load(
            &crate::system_blocks_handlers::store_dir(&state),
        )
        .expect("store readable before refusal")
        .expect("seeded store present before refusal");

        for params in [
            serde_json::json!({"agent_id": "runner", "expected_store_version": 1, "ratifier": "runner"}),
            serde_json::json!({"agent_id": "gui", "expected_store_version": 1, "ratifier": "gui", "ratified_via": "human-ui"}),
        ] {
            let error = super::dispatch_tool(&mut state, "system_blocks_ratify", &params)
                .expect_err("client-authored origin values grant no authority");
            let rendered = error.to_string();
            assert!(
                rendered.contains("sovereign_authority_required")
                    || rendered.contains("unknown field `ratified_via`"),
                "unexpected: {rendered}"
            );
        }

        let store = crate::system_blocks::SystemBlockStore::load(
            &crate::system_blocks_handlers::store_dir(&state),
        )
        .expect("store remains readable")
        .expect("seeded store remains present");
        assert_eq!(store, before);
    }

    /// A valid `spec` receipt for a real ratified seed block (boundary 1 / contract 1).
    /// Anchor-only evidence clears the evidence contract for a non-`test` type, so the
    /// scope + OCC are the only remaining gates — leaving the ORIGIN gate as the thing
    /// under test.
    fn human_ui_seed_receipt() -> serde_json::Value {
        serde_json::json!({
            "type": "spec",
            "emitter": { "kind": "verb", "id": "human-ui-landing" },
            "scope": {
                "block_id": "sb_m1nd_core_graph_kernel",
                "boundary_version": 1,
                "contract_version": 1,
                "resolution_hash": "sha256:res"
            },
            "evidence": { "artifact_hash": "sha256:art", "evidence_refs": ["artifacts/x.txt"] },
            "validity": { "expires_on": null, "stales_on": [] }
        })
    }

    #[test]
    fn receipt_import_refuses_without_the_human_origin_and_lands_with_it() {
        // Sovereign-stamp step 0: `receipt_import` is a human write exactly like ratify,
        // and until now it carried NO origin gate. This proves the mirror on the SHARED
        // receipt handler: a call with no origin and one with an off-list origin are BOTH
        // refused and touch nothing; the `human-ui` call lands and bumps the OCC counter
        // exactly once.
        let (_temp, mut state) = build_state();
        let seed = include_str!("../../docs/system-blocks/m1nd.seed.v0.json");
        let imp = super::dispatch_tool(
            &mut state,
            "system_blocks_seed_import",
            &serde_json::json!({"agent_id": "t", "seed_json": seed}),
        )
        .expect("seed_import ok");
        assert_eq!(imp["store_version"], 1);
        let receipt = human_ui_seed_receipt();

        // No origin token → soft refusal naming the field + the allow-list, nothing landed.
        let refused = call_receipt_import_handler(
            &mut state,
            &serde_json::json!({
                "agent_id": "runner",
                "expected_store_version": 1,
                "block_id": "sb_m1nd_core_graph_kernel",
                "receipt": receipt.clone(),
            }),
        )
        .expect("the origin gate is a soft refusal, not an error");
        assert_eq!(refused["refused"], "human_gesture_required");
        assert_eq!(refused["field"], "imported_via");
        assert_eq!(
            refused["allowed_origins"],
            serde_json::json!(["human-ui", "human-touchid"])
        );
        assert_eq!(
            refused["lesson"],
            "landing a receipt is the human gesture — the owner's screen sends it; agents never do"
        );

        // An off-list (invented) origin is refused identically — the allow-list is closed.
        let refused2 = call_receipt_import_handler(
            &mut state,
            &serde_json::json!({
                "agent_id": "runner",
                "expected_store_version": 1,
                "block_id": "sb_m1nd_core_graph_kernel",
                "receipt": receipt.clone(),
                "imported_via": "human-tray",
            }),
        )
        .expect("an unknown origin is a soft refusal too");
        assert_eq!(refused2["refused"], "human_gesture_required");

        // The SAME call carrying the owner screen's origin token lands. Its OCC key is
        // still 1 — proof both refusals above bumped nothing.
        let landed = call_receipt_import_handler(
            &mut state,
            &serde_json::json!({
                "agent_id": "gui",
                "expected_store_version": 1,
                "block_id": "sb_m1nd_core_graph_kernel",
                "receipt": receipt,
                "imported_via": "human-ui",
            }),
        )
        .expect("human-origin receipt import lands");
        assert_eq!(landed["store_version"], 2);
        assert_eq!(landed["receipt_count"], 1);
    }

    #[test]
    fn legacy_receipt_import_is_tombstoned_before_origin_or_body_parsing() {
        // G3 owns the external boundary now. Neither a malformed body nor a formerly
        // accepted human-origin token may revive the retired raw receipt primitive.
        let (_temp, mut state) = build_state();
        let seed = include_str!("../../docs/system-blocks/m1nd.seed.v0.json");
        super::dispatch_tool(
            &mut state,
            "system_blocks_seed_import",
            &serde_json::json!({"agent_id": "t", "seed_json": seed}),
        )
        .expect("seed_import ok");
        let receipt = human_ui_seed_receipt();

        for params in [
            serde_json::json!({}),
            serde_json::json!({"imported_via": "human-ui"}),
            serde_json::json!({
                "agent_id": "gui",
                "expected_store_version": 1,
                "block_id": "sb_m1nd_core_graph_kernel",
                "receipt": receipt.clone(),
                "imported_via": "human-ui",
            }),
            serde_json::json!({
                "agent_id": "h4nd-tray-touchid",
                "expected_store_version": 1,
                "block_id": "sb_m1nd_core_graph_kernel",
                "receipt": receipt.clone(),
                "imported_via": "human-touchid",
            }),
        ] {
            let error = super::dispatch_tool(&mut state, "receipt_import", &params)
                .expect_err("raw receipt_import is a permanent external tombstone");
            assert!(
                error.to_string().contains("legacy_direct_mutation_refused"),
                "unexpected refusal: {error}"
            );
        }

        let store = crate::system_blocks::SystemBlockStore::load(
            &crate::system_blocks_handlers::store_dir(&state),
        )
        .expect("store remains readable")
        .expect("seeded store remains present");
        assert_eq!(store.store_version, 1, "tombstones mutate nothing");
        assert!(store.blocks.iter().all(|block| block.receipts.is_empty()));
    }

    #[test]
    fn mission_post_refuses_an_unknown_block_unless_synthetic() {
        // The block guard: a real letter naming a block no skeleton holds is
        // refused; the same letter with synthetic:true (a smoke probe) posts.
        let (temp, mut state) = build_state();
        let repo = temp.path().join("m1nd");
        std::fs::create_dir_all(&repo).expect("repo");
        state.workspace_root = Some(repo.to_string_lossy().to_string());
        state.ingest_roots = vec![repo.to_string_lossy().to_string()];

        // Seed the real 12-block skeleton (holds sb_m1nd_ingest, no sb_ghost).
        let real_seed = include_str!("../../docs/system-blocks/m1nd.seed.v0.json");
        super::dispatch_tool(
            &mut state,
            "system_blocks_seed_import",
            &serde_json::json!({"agent_id": "t", "seed_json": real_seed}),
        )
        .expect("seed imports");

        let letter = |block: &str, synthetic: bool| {
            serde_json::json!({
                "schema": "m1nd-mission-letter-v0", "mission_id": "msn_0123456789ab", "mission_seq": 1,
                "block_id": block, "brain_ref": "m1nd", "seat": "hand", "capability": "build-runner",
                "phase": "judging", "packet_ref": "sha256:x", "tokens_total": 0, "synthetic": synthetic,
                "started_at": "2026-07-10T00:00:00Z", "updated_at": "2026-07-10T00:00:00Z",
            })
        };

        let err = call_mission_post_handler(
            &mut state,
            &serde_json::json!({"agent_id": "t", "letter": letter("sb_ghost", false)}),
        )
        .expect_err("an unknown block is refused");
        assert!(err.to_string().contains("unknown_block"), "got: {err}");

        call_mission_post_handler(
            &mut state,
            &serde_json::json!({"agent_id": "t", "letter": letter("sb_ghost", true)}),
        )
        .expect("a synthetic probe posts despite the ghost block");

        let mut real = letter("sb_m1nd_ingest", false);
        real["mission_id"] = serde_json::json!("msn_abcdef012345");
        call_mission_post_handler(
            &mut state,
            &serde_json::json!({"agent_id": "t", "letter": real}),
        )
        .expect("a real block posts");
        let _ = &temp;
    }

    #[test]
    fn mission_post_refuses_receipt_candidate_with_stale_boundary_version() {
        // The boundary-staleness guard (field bug: the orphan letter
        // msn_17a1d1f9b013). A mission letter may carry a `receipt_candidate` — a
        // one-click import the tray offers. gate #3 only checked the candidate's
        // evidence was COMPLETE, never that its `scope.boundary_version` still
        // matched the LIVE block. A candidate proving a boundary the block has
        // moved past is dead evidence: `receipt_import` would reject it with
        // `stale_scope`, but the letter was already appended. Declare the staleness
        // at POST instead, naming both versions. Mirrors the import law
        // (`system_blocks`: receipt.scope.boundary_version != block.boundary_version).
        let (temp, mut state) = build_state();
        let repo = temp.path().join("m1nd");
        std::fs::create_dir_all(&repo).expect("repo");
        state.workspace_root = Some(repo.to_string_lossy().to_string());
        state.ingest_roots = vec![repo.to_string_lossy().to_string()];

        // Seed the real skeleton: sb_m1nd_core_graph_kernel lives at boundary 1.
        let real_seed = include_str!("../../docs/system-blocks/m1nd.seed.v0.json");
        super::dispatch_tool(
            &mut state,
            "system_blocks_seed_import",
            &serde_json::json!({"agent_id": "t", "seed_json": real_seed}),
        )
        .expect("seed imports");

        // A merge_wait letter (requires a gate) carrying a candidate whose scope
        // boundary is the parameter under test.
        let letter = |cand_boundary: u32, mission_id: &str| {
            serde_json::json!({
                "schema": "m1nd-mission-letter-v0",
                "mission_id": mission_id,
                "mission_seq": 1,
                "block_id": "sb_m1nd_core_graph_kernel",
                "brain_ref": "m1nd",
                "seat": "hand",
                "capability": "build-runner",
                "phase": "merge_wait",
                "gate": {
                    "command": "cargo test -p m1nd-mcp",
                    "exit_status": 0,
                    "artifact_hash": "sha256:gatelog"
                },
                "receipt_candidate": {
                    "block_id": "sb_m1nd_core_graph_kernel",
                    "type": "spec",
                    "scope": {"boundary_version": cand_boundary, "contract_version": 1},
                    "evidence": {
                        "artifact_hash": "sha256:art",
                        "evidence_refs": ["artifacts/x.txt"]
                    }
                },
                "packet_ref": "sha256:x",
                "tokens_total": 0,
                "started_at": "2026-07-10T00:00:00Z",
                "updated_at": "2026-07-10T00:00:00Z",
            })
        };

        // A candidate proving the LIVE boundary (1) posts cleanly — the control.
        call_mission_post_handler(
            &mut state,
            &serde_json::json!({"agent_id": "t", "letter": letter(1, "msn_0123456789ab")}),
        )
        .expect("a candidate proving the live boundary posts");

        // A candidate proving a boundary the block is NOT at (3 != live 1) is
        // refused, naming both versions. On main (no gate) this posts silently —
        // the exact vector that birthed the orphan letter.
        let err = call_mission_post_handler(
            &mut state,
            &serde_json::json!({"agent_id": "t", "letter": letter(3, "msn_abcdef012345")}),
        )
        .expect_err("a stale-boundary candidate must be refused at post");
        let msg = err.to_string();
        assert!(
            msg.contains("stale_scope"),
            "refusal names the staleness: {msg}"
        );
        assert!(
            msg.contains('3') && msg.contains('1'),
            "refusal names both boundary versions (candidate 3 vs live 1): {msg}"
        );
        let _ = &temp;
    }

    /// THE CURATION-DISPATCH BATTERY (field bug 2026-07-10, seen twice in human
    /// dogfood): the map's "Send to an agent for curation" letter was refused on a
    /// HOSTED brain. Two composed causes: (1) the UI derived `brain_ref` from the
    /// skeleton id — `null` on candidate-form ids (so the letter said "brain"), a
    /// lowercase sanitized slug otherwise — while the brain guard compares the
    /// DISPATCHING brain's display name (the basename of its project root,
    /// case-sensitive); (2) behind it, the letter anchors `block_id` at the
    /// skeleton id, which the block guard did not recognize. This drives the EXACT
    /// letter the FIXED UI composes against a REAL hosted brain (registry
    /// bootstrap + the real `skeleton_candidate` scan) and it must POST — and the
    /// letter must PERSIST in THAT brain's repo-side box, never anywhere else
    /// (the 2026-07-09 field report: letters "evaporated" after a reconnect
    /// because the session collapsed to the bound brain and they mis-routed into
    /// its box; the guard now refuses that shape honestly, and this pins the
    /// happy path landing in the right box). The old wrong refs still refuse —
    /// the guard is recognized-identity-only, never loosened.
    #[test]
    fn curation_letter_posts_to_a_hosted_brain_and_lands_in_its_box() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // A repo whose basename is NOT a lowercase slug — pinning that the
        // display-name compare is exact (the sanitized slug "repo_b1" must fail).
        let repo = tmp.path().join("Repo-B1");
        std::fs::create_dir_all(repo.join("src")).expect("mk repo");
        std::fs::write(
            repo.join("Cargo.toml"),
            "[package]\nname = \"repob1\"\nversion = \"0.0.0\"\n",
        )
        .expect("Cargo.toml");
        std::fs::write(
            repo.join("src/lib.rs"),
            "pub fn repo_b1_probe() -> i64 { 1 }\n",
        )
        .expect("lib.rs");

        // A REAL hosted brain, through the same registry bootstrap the routing
        // seams use (its own SessionState: ingest_roots = the repo, runtime_root =
        // its owner-side store dir).
        let reg = crate::project_brains::ProjectBrainRegistry::with_capacity(
            tmp.path().join("project-brains"),
            None,
            4,
        );
        let (brain, _ingest, _reused) = reg
            .bootstrap(
                &repo.to_string_lossy(),
                &serde_json::json!({"agent_id": "t"}),
            )
            .expect("bootstrap the hosted brain");
        let repo_key = repo.to_string_lossy().to_string();

        // The store the dogfood had: the REAL candidate scan on the hosted brain
        // (skeleton id in the candidate form the UI must recognize).
        reg.execute_target_runtime(
            std::sync::Arc::clone(&brain),
            Some(&repo_key),
            false,
            true,
            |state| {
                super::dispatch_tool(
                    state,
                    "skeleton_candidate",
                    &serde_json::json!({"agent_id": "t", "naming": "heuristic"}),
                )
                .map_err(|error| {
                    crate::runtime_jobs::RuntimeJobFailure::new(
                        "test_dispatch_failed",
                        error.to_string(),
                    )
                })
            },
        )
        .expect("the hosted brain actor scans a candidate skeleton");
        let runtime_root = reg
            .read_target_runtime_snapshot(
                std::sync::Arc::clone(&brain),
                Some(&repo_key),
                false,
                |state| Ok::<_, crate::runtime_jobs::RuntimeJobFailure>(state.runtime_root.clone()),
            )
            .expect("read hosted runtime root through actor")
            .value;
        let store =
            crate::system_blocks::SystemBlockStore::load(std::path::Path::new(&runtime_root))
                .expect("store loads")
                .expect("store exists after the scan");
        let skeleton_id = store.skeleton.skeleton_id.clone();
        assert_eq!(
            skeleton_id, "sk_repo_b1_candidate",
            "the scan mints the candidate-form skeleton id (sanitized slug)"
        );

        // The EXACT letter the fixed UI composes (BuildMapView handleSendCuration →
        // composeSeq1Letter): brain_ref = the brain's display name (basename of the
        // project root), block_id = the skeleton id, seat oracle, hand-runner.
        let ui_letter = |brain_ref: &str, block_id: &str, mission_id: &str| {
            serde_json::json!({
                "schema": "m1nd-mission-letter-v0",
                "mission_id": mission_id,
                "mission_seq": 1,
                "block_id": block_id,
                "brain_ref": brain_ref,
                "seat": "oracle",
                "capability": "hand-runner",
                "phase": "judging",
                "packet_ref": "sha256:cafef00dcafe",
                "tokens_total": 0,
                "started_at": "2026-07-10T00:00:00Z",
                "updated_at": "2026-07-10T00:00:00Z",
            })
        };

        // (1) THE TEST THAT WOULD HAVE CAUGHT THE BUG: the fixed letter posts.
        let posted_params = serde_json::json!({
            "agent_id": "gui",
            "letter": ui_letter("Repo-B1", &skeleton_id, "msn_0123456789ab")
        });
        let posted = reg
            .execute_target_runtime(
                std::sync::Arc::clone(&brain),
                Some(&repo_key),
                false,
                true,
                move |state| {
                    call_mission_post_handler(state, &posted_params).map_err(|error| {
                        crate::runtime_jobs::RuntimeJobFailure::new(
                            "test_mission_post_failed",
                            error.to_string(),
                        )
                    })
                },
            )
            .expect("the curation letter posts through the hosted brain actor");
        assert_eq!(posted["mission_seq"], 1);
        assert_eq!(posted["deduped"], false);

        // (2) THE h4nd PIN: the letter PERSISTED in THIS brain's repo-side box —
        // never in the owner-side store dir (the mis-route family: letters landing
        // in another brain's box read as "evaporated" to the poster).
        let hosted_box = repo.join(crate::mailbox::BOX_REL_PATH);
        let box_text = std::fs::read_to_string(&hosted_box)
            .expect("the hosted brain's repo-side box exists after the post");
        assert!(
            box_text.contains("msn_0123456789ab"),
            "the letter lives in the hosted brain's own box: {box_text}"
        );
        let owner_side_box = crate::mailbox::medulla_box_path(std::path::Path::new(&runtime_root));
        assert!(
            !owner_side_box.exists(),
            "nothing may land in the owner-side store box for a code-rooted brain"
        );

        // (3) NO LOOSENING: both OLD UI-composed refs still refuse with the honest
        // brain_mismatch — the null-repoId fallback ("brain") and the sanitized
        // slug ("repo_b1", the case/sanitization drift).
        for wrong_ref in ["brain", "repo_b1"] {
            let params = serde_json::json!({
                "agent_id": "gui",
                "letter": ui_letter(wrong_ref, &skeleton_id, "msn_00000000dead")
            });
            let err = reg
                .execute_target_runtime(
                    std::sync::Arc::clone(&brain),
                    Some(&repo_key),
                    false,
                    true,
                    move |state| {
                        call_mission_post_handler(state, &params).map_err(|error| {
                            crate::runtime_jobs::RuntimeJobFailure::new(
                                "test_mission_post_refused",
                                error.to_string(),
                            )
                        })
                    },
                )
                .expect_err("a wrong brain_ref must still refuse");
            let msg = err.to_string();
            assert!(
                msg.contains("brain_mismatch") && msg.contains("Repo-B1"),
                "the refusal names the mismatch and the bound display: {msg}"
            );
        }

        // (4) The skeleton anchor recognizes ONLY this store's skeleton id: a
        // foreign skeleton id still refuses as unknown_block.
        let foreign_params = serde_json::json!({
            "agent_id": "gui",
            "letter": ui_letter("Repo-B1", "sk_other_candidate", "msn_00000000beef")
        });
        let err = reg
            .execute_target_runtime(
                std::sync::Arc::clone(&brain),
                Some(&repo_key),
                false,
                true,
                move |state| {
                    call_mission_post_handler(state, &foreign_params).map_err(|error| {
                        crate::runtime_jobs::RuntimeJobFailure::new(
                            "test_mission_post_refused",
                            error.to_string(),
                        )
                    })
                },
            )
            .expect_err("a foreign skeleton id must refuse");
        assert!(err.to_string().contains("unknown_block"), "got: {err}");
    }

    #[test]
    fn candidate_edit_and_lease_are_wired_through_dispatch() {
        let (_temp, mut state) = build_state();
        // Import the real (ratified) seed -> store_version 1.
        let seed = include_str!("../../docs/system-blocks/m1nd.seed.v0.json");
        let imp = super::dispatch_tool(
            &mut state,
            "system_blocks_seed_import",
            &serde_json::json!({"agent_id": "t", "seed_json": seed}),
        )
        .expect("seed_import ok");
        assert_eq!(imp["store_version"], 1);

        // candidate_edit routes through dispatch, the ops parse from tagged JSON, and
        // the §1a candidate-only gate fires on the ratified skeleton.
        let err = super::dispatch_tool(
            &mut state,
            "candidate_edit",
            &serde_json::json!({
                "agent_id": "t",
                "expected_store_version": 1,
                "ops": [{"op": "rename", "block_id": "sb_any", "name": "X"}]
            }),
        )
        .expect_err("a ratified skeleton refuses candidate_edit");
        assert!(
            err.to_string().contains("skeleton_not_candidate"),
            "unexpected: {err}"
        );

        // candidate_lease acquire is wired and returns the advisory lease state (the
        // lease is independent of skeleton state and never bumps store_version).
        let lease = super::dispatch_tool(
            &mut state,
            "candidate_lease",
            &serde_json::json!({"agent_id": "agentA", "action": "acquire"}),
        )
        .expect("lease acquires");
        assert_eq!(lease["state"], "acquired");
        assert_eq!(lease["curating_by"], "agentA");
        assert_eq!(
            lease["store_version"], 1,
            "the lease does not bump the OCC counter"
        );
    }

    #[test]
    fn mission_post_refuses_a_brain_ref_that_is_not_the_bound_brain() {
        // The brain guard: a session bound to a real code root refuses a letter
        // naming a DIFFERENT brain_ref — the silent mis-route becomes an honest
        // refusal. A matching ref posts; the medulla fallback (no root) stays
        // permissive (covered by the end-to-end test above, whose state has no
        // bound root).
        let (temp, mut state) = build_state();
        let repo = temp.path().join("project-a");
        std::fs::create_dir_all(repo.join(".git")).expect("repo dir with .git");
        state.workspace_root = Some(repo.to_string_lossy().to_string());

        let letter = serde_json::json!({
            "schema": "m1nd-mission-letter-v0",
            "mission_id": "msn_0123456789ab",
            "mission_seq": 1,
            "block_id": "sb_x",
            "brain_ref": "some-other-brain",
            "seat": "hand",
            "capability": "build-runner",
            "phase": "judging",
            "packet_ref": "sha256:abc",
            "tokens_total": 0,
            "started_at": "2026-07-09T00:00:00Z",
            "updated_at": "2026-07-09T00:00:00Z",
        });
        let err = call_mission_post_handler(
            &mut state,
            &serde_json::json!({"agent_id": "t", "letter": letter}),
        )
        .expect_err("a mismatched brain_ref must refuse");
        let msg = err.to_string();
        assert!(msg.contains("brain_mismatch"), "got: {msg}");
        assert!(
            msg.contains("project-a"),
            "the refusal names the bound brain: {msg}"
        );

        // The same letter naming the BOUND brain posts cleanly.
        let mut ok_letter = letter;
        ok_letter["brain_ref"] = serde_json::json!("project-a");
        let out = call_mission_post_handler(
            &mut state,
            &serde_json::json!({"agent_id": "t", "letter": ok_letter}),
        )
        .expect("a matching brain_ref posts");
        assert_eq!(out["mission_seq"], 1);
    }

    #[test]
    fn mission_post_handler_enforces_head_cas_and_landed_law() {
        let (_temp, mut state) = build_state();

        // A helper to build a mission-letter JSON value at a given seq/phase.
        let letter = |seq: u64, phase: &str, prev: Option<&str>| {
            let mut m = serde_json::json!({
                "schema": "m1nd-mission-letter-v0",
                "mission_id": "msn_0123456789ab",
                "mission_seq": seq,
                "block_id": "sb_x",
                "brain_ref": "repo-a",
                "seat": "hand",
                "capability": "build-runner",
                "phase": phase,
                "started_at": "2026-07-09T00:00:00Z",
                "updated_at": "2026-07-09T00:00:00Z"
            });
            if let Some(p) = prev {
                m["prev_letter_id"] = serde_json::json!(p);
            }
            m
        };

        // seq 1 (judging) posts cleanly and returns a letter_id.
        let out1 = call_mission_post_handler(
            &mut state,
            &serde_json::json!({"agent_id": "hand-a", "letter": letter(1, "judging", None)}),
        )
        .expect("seq 1 posts");
        let id1 = out1["letter_id"].as_str().expect("letter_id").to_string();
        assert_eq!(out1["mission_seq"], 1);
        assert_eq!(out1["deduped"], false);

        // seq 2 chained on seq 1's id posts cleanly.
        let out2 = call_mission_post_handler(
            &mut state,
            &serde_json::json!({"agent_id": "hand-a", "letter": letter(2, "executing", Some(&id1))}),
        )
        .expect("seq 2 extends the head");
        assert_eq!(out2["mission_seq"], 2);

        // seq 2 with the WRONG prev → stale_head surfaces through dispatch, nothing appended.
        let err = call_mission_post_handler(
            &mut state,
            &serde_json::json!({"agent_id": "hand-a", "letter": letter(2, "executing", Some("deadbeefdead"))}),
        )
        .expect_err("a stale head must be refused");
        assert!(err.to_string().contains("stale_head"), "unexpected: {err}");

        // The §1d landed law surfaces too: landed without an imported receipt.
        let err2 = call_mission_post_handler(
            &mut state,
            &serde_json::json!({"agent_id": "hand-a", "letter": letter(3, "landed", Some(&id1))}),
        )
        .expect_err("gate-zero cannot land");
        assert!(err2.to_string().contains("landed"), "unexpected: {err2}");
    }

    #[test]
    fn agent_composed_archive_dies_at_the_door() {
        // F2.5e forged-origin gate (binding change 1), mirroring
        // `agent_composed_receipt_import_dies_at_the_door`: an agent that tries to ARCHIVE
        // a superseded receipt — sending NO origin, an EMPTY one, or INVENTING a plausible
        // one — dies at the door before anything is appended. Only the owner screen's
        // `archived_via:"human-ui"` supersedes the merge_wait head. The box is INTACT after
        // every forged attempt (the anti-lie proof: an agent may not silently bury its own
        // unproven work — the product's first silent-burial verb stays human-only).
        let (temp, mut state) = build_state();
        let repo = temp.path().join("project-a");
        std::fs::create_dir_all(repo.join(".git")).expect("repo dir with .git");
        state.workspace_root = Some(repo.to_string_lossy().to_string());

        // Seq 1 = a merge_wait head (a green gate); a merge_wait letter needs no origin
        // token — only `archived` is gated.
        let merge_wait = serde_json::json!({
            "schema": "m1nd-mission-letter-v0", "mission_id": "msn_0123456789ab", "mission_seq": 1,
            "block_id": "sb_x", "brain_ref": "project-a", "seat": "hand", "capability": "build-runner",
            "phase": "merge_wait",
            "gate": {"command": "cargo test -p m1nd-mcp", "exit_status": 0, "artifact_hash": "sha256:gatelog"},
            "started_at": "2026-07-13T00:00:00Z", "updated_at": "2026-07-13T00:00:00Z",
        });
        let posted = call_mission_post_handler(
            &mut state,
            &serde_json::json!({"agent_id": "hand", "letter": merge_wait}),
        )
        .expect("the merge_wait head posts");
        let head_id = posted["letter_id"].as_str().expect("letter_id").to_string();

        let box_path = crate::mission_letter_handlers::mission_box_path(&state);
        let box_before =
            std::fs::read_to_string(&box_path).expect("box exists after the merge_wait post");

        // The archived seq-2 letter (extends the merge_wait head), with a variable origin.
        let archived = |via: Option<serde_json::Value>| {
            let mut params = serde_json::json!({
                "agent_id": "runner",
                "letter": {
                    "schema": "m1nd-mission-letter-v0", "mission_id": "msn_0123456789ab", "mission_seq": 2,
                    "prev_letter_id": head_id.clone(), "block_id": "sb_x", "brain_ref": "project-a",
                    "seat": "hand", "capability": "build-runner", "phase": "archived",
                    "started_at": "2026-07-13T00:01:00Z", "updated_at": "2026-07-13T00:01:00Z",
                }
            });
            if let Some(v) = via {
                params["archived_via"] = v;
            }
            params
        };

        for forged in [
            None,                                     // absent field
            Some(serde_json::json!("")),              // empty string
            Some(serde_json::json!("runnerd")),       // an agent origin
            Some(serde_json::json!("human-touchid")), // a future origin that does not exist yet
        ] {
            let refused = call_mission_post_handler(&mut state, &archived(forged.clone()))
                .expect("the archive gate is a soft refusal, not an error");
            assert_eq!(
                refused["refused"], "human_gesture_required",
                "an agent-composed archive must die at the door: archived_via={forged:?}"
            );
            assert_eq!(refused["field"], "archived_via");
            let box_now = std::fs::read_to_string(&box_path).expect("box still readable");
            assert_eq!(box_before, box_now, "a forged archive appends NOTHING");
        }

        // The owner screen's origin token supersedes the merge_wait head.
        let done =
            call_mission_post_handler(&mut state, &archived(Some(serde_json::json!("human-ui"))))
                .expect("a human-origin archive posts");
        assert_eq!(done["mission_seq"], 2);
        assert_eq!(done["phase"], "archived");
        let _ = &temp;
    }

    #[test]
    fn legacy_mission_post_tombstone_precedes_read_only_and_body_validation() {
        let (_temp, mut state) = build_state_read_only();
        let err = super::dispatch_tool(
            &mut state,
            "mission_post",
            &serde_json::json!({"body": "deliberately not a mission letter"}),
        )
        .expect_err("raw mission_post must hit the permanent G3 tombstone");
        let message = err.to_string();
        assert!(
            message.contains("legacy_direct_mutation_refused")
                && message.contains("mission_post")
                && !message.contains("attached read-only"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn mission_spawn_dispatch_is_http_only_redirect() {
        // The sync MCP dispatch never runs the proxy (it needs owner-process state +
        // an async forward); an MCP-stdio caller gets an honest redirect, not a fake
        // spawn. The real proxy is the HTTP route (http_server::handle_mission_spawn).
        let (_temp, mut state) = build_state();
        let err = super::dispatch_tool(
            &mut state,
            "mission_spawn",
            &serde_json::json!({"agent_id": "gui", "runner_id": "build-1",
                "packet_markdown": "# p", "block_id": "sb_x", "brain_ref": "repo-a"}),
        )
        .expect_err("mission_spawn is HTTP-only over MCP dispatch");
        assert!(
            err.to_string().contains("HTTP-only")
                && err.to_string().contains("/api/tools/mission_spawn"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn candidate_naming_dispatch_is_http_only_redirect() {
        // F11-c mirrors the mission_spawn law: the sync MCP dispatch never runs the
        // naming route (it needs the announce registry + the shared secret + the
        // /name forward); an MCP-stdio caller gets an honest redirect.
        let (_temp, mut state) = build_state();
        let err = super::dispatch_tool(
            &mut state,
            "candidate_naming",
            &serde_json::json!({"agent_id": "gui", "expected_store_version": 1}),
        )
        .expect_err("candidate_naming is HTTP-only over MCP dispatch");
        assert!(
            err.to_string().contains("HTTP-only")
                && err.to_string().contains("/api/tools/candidate_naming"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn read_only_dispatch_refuses_mutation_but_allows_query() {
        let (_temp, mut state) = build_state_read_only();
        assert!(state.read_only);

        // A mutation tool is refused with the contract error message.
        let err = super::dispatch_tool(
            &mut state,
            "ingest",
            &serde_json::json!({"agent_id": "t", "path": "/tmp/x"}),
        )
        .expect_err("ingest must be refused in read-only");
        let msg = err.to_string();
        assert!(
            msg.contains("attached read-only") && msg.contains("ingest"),
            "unexpected error: {msg}"
        );

        // A read-only tool still works (health needs no graph).
        let ok = super::dispatch_tool(&mut state, "health", &serde_json::json!({"agent_id": "t"}));
        assert!(ok.is_ok(), "health should work read-only: {ok:?}");
    }

    #[test]
    fn read_only_persist_is_a_noop() {
        let (_temp, mut state) = build_state_read_only();
        // persist() must early-return Ok without creating the graph file.
        state.persist().expect("read-only persist returns Ok");
        assert!(
            !state.graph_path.exists(),
            "read-only persist must not write the graph snapshot"
        );
        // should_persist is always false even after many queries.
        state.queries_processed = state.auto_persist_interval as u64;
        assert!(!state.should_persist());
    }

    #[test]
    fn response_envelope_attaches_additively() {
        let (_temp, mut state) = build_state();
        // seek on an empty graph returns a results-shaped object; the envelope
        // must be attached without removing existing fields.
        let out = super::dispatch_tool(
            &mut state,
            "seek",
            &serde_json::json!({"agent_id": "t", "query": "anything"}),
        )
        .expect("seek ok");
        let obj = out.as_object().expect("object result");
        assert!(obj.contains_key("_m1nd"), "_m1nd envelope must be present");
        let meta = &obj["_m1nd"];
        assert!(meta.get("suggest_next").is_some(), "suggest_next kept");
        assert!(meta.get("read_only").is_some(), "read_only kept");
        // Brand gate G1: the unmeasured savings envelope is removed. An
        // uncalibrated `tokens_saved` guessed on every response is the
        // confident guess — honesty is the product.
        assert!(
            meta.get("tokens_saved").is_none(),
            "tokens_saved must be gone (unmeasured claim)"
        );
        assert!(meta.get("savings").is_none(), "savings block must be gone");
        assert!(meta.get("gaia").is_none(), "gaia block must be gone");
        // Additive: the original results field is still there.
        assert!(obj.contains_key("results"), "results field preserved");
    }

    #[test]
    fn savings_tool_is_removed_and_report_carries_no_unmeasured_claims() {
        // Brand gate G1.5 (founder decision 2026-07-03, mailbox L16): the opt-in
        // `savings`/`report` unmeasured-claims surface is killed. G1 removed the
        // per-response envelope; the standalone tools were "a living remnant of the
        // confident guess behind an explicit call". The brand cannot say it killed
        // the unmeasured claim while a tool named `savings` still emits it.
        //
        // Verdict: `savings` is savings-flavored throughout -> removed entirely.
        // `report` keeps its honest content (queries, elapsed, heuristic hotspots,
        // graph counts) but is STRIPPED of every tokens-saved / CO2 field.

        // (a) `savings` no longer appears in the full tool registry (any tier).
        let schema = all_tool_schemas();
        let names: Vec<String> = schema["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(|value| value.as_str()))
            .map(|value| value.to_string())
            .collect();
        assert!(
            !names.contains(&"savings".to_string()),
            "the `savings` tool must be removed from the registry (unmeasured token-claims surface)"
        );
        // `report` survives (honest remainder), so it stays advertised.
        assert!(
            names.contains(&"report".to_string()),
            "`report` should remain — it keeps honest counts after the savings strip"
        );

        // (b) `savings` is not dispatchable either — handler is gone.
        let (_temp, mut state) = build_state();
        let savings =
            super::dispatch_tool(&mut state, "savings", &serde_json::json!({"agent_id": "t"}));
        assert!(
            savings.is_err(),
            "the `savings` tool must no longer dispatch to a handler"
        );

        // (c) `report`'s output carries NO unmeasured token/CO2 keys anywhere.
        let out = super::dispatch_tool(&mut state, "report", &serde_json::json!({"agent_id": "t"}))
            .expect("report ok");
        let json = serde_json::to_string(&out).expect("report serializes");
        for banned in [
            "tokens_saved_session",
            "tokens_saved_global",
            "co2_saved_grams",
            "tokens_saved",
            "global_tokens_never_burned",
        ] {
            assert!(
                !json.contains(banned),
                "report output must not contain the unmeasured claim `{banned}`: {json}"
            );
        }
        // The honest remainder is still there.
        let obj = out.as_object().expect("report object");
        assert!(
            obj.contains_key("heuristic_hotspots"),
            "report must keep its honest heuristic_hotspots"
        );
        assert!(
            obj.contains_key("session_queries"),
            "report must keep its honest session_queries count"
        );
    }

    #[test]
    fn server_instructions_document_the_agent_native_loop() {
        // Host-agnostic contract: every MCP host injects M1ND_INSTRUCTIONS, so the
        // OMEGA operating loop must be documented here (not in a host-specific skill).
        // The doctrine is: pre-orient (north-first) -> act on calibrated verdicts ->
        // post-capture (memorize with evidence). Guard every load-bearing beat.
        let s = super::M1ND_INSTRUCTIONS;
        assert!(
            s.contains("agent_id"),
            "instructions must state the agent_id requirement"
        );
        // 1. PRE-ORIENT: north is called first and needs_ingest is the honest
        // empty-graph answer; it must not invent a public bootstrap repair.
        assert!(
            s.contains("north"),
            "instructions must lead with north (pre-orient)"
        );
        assert!(
            s.contains("needs_ingest"),
            "instructions must document the needs_ingest state"
        );
        // 2. ACT ON VERDICTS: the calibrated gate and its honest answers.
        assert!(
            s.contains("abstain"),
            "instructions must document the act/reverify/abstain verdict"
        );
        assert!(
            s.contains("calibrate_predict"),
            "instructions must document arming the gate with calibrate_predict"
        );
        assert!(
            s.contains("insufficient_evidence"),
            "instructions must distinguish insufficient_evidence from a risk band"
        );
        // 3. POST-CAPTURE: the compounding memory habit with staleness self-flagging.
        assert!(s.contains("memorize"), "instructions must mention memorize");
        assert!(
            s.contains("evidence"),
            "instructions must require evidence paths on memorized claims"
        );
        assert!(
            s.contains("evidence_freshness"),
            "instructions must mention the staleness check"
        );
        assert!(
            s.contains("write_light_memory"),
            "instructions must mention the mission_close memory option"
        );
        // Field telemetry: every agent is a sensor — learn on retrieval verdicts,
        // local-only field-reports.jsonl when m1nd itself misbehaves (report, never detour).
        assert!(
            s.contains("learn") && s.contains("field-reports.jsonl"),
            "instructions must document the field-telemetry loop (learn + local field-reports)"
        );
        // 7. THE M1ND VOICE: the human_view card law — the negative-default
        // cadence verbatim, the render duties, and the attribution test. The
        // instructions are the only anti-spam line on instructions-only hosts.
        assert!(
            s.contains("human_view") && s.contains("m1nd-human-view-v0"),
            "instructions must document the human_view card"
        );
        assert!(
            s.contains(
                "Do NOT render the card unless m1nd contributed structurally to the mission"
            ),
            "instructions must carry the negative-default cadence verbatim"
        );
        assert!(
            s.contains("never the same state_sig twice in a session"),
            "instructions must carry the anti-repetition clause"
        );
        assert!(
            s.contains("`│`→`|`"),
            "instructions must carry the 1:1 ASCII fallback map"
        );
        assert!(
            s.contains("counterfactual test"),
            "instructions must gate attribution on the counterfactual test"
        );
    }

    #[test]
    fn public_surface_does_not_advertise_the_unreachable_brain_bootstrap() {
        let registry = all_tool_schemas();
        let tools = registry["tools"].as_array().expect("tools array");
        let ingest = tools
            .iter()
            .find(|tool| tool["name"] == "ingest")
            .expect("compatibility ingest name remains in the registry");
        assert!(
            ingest["description"]
                .as_str()
                .unwrap_or_default()
                .contains("POLICY-DISABLED"),
            "the compatibility schema must not claim generic ingest is executable"
        );
        let properties = ingest["inputSchema"]["properties"]
            .as_object()
            .expect("ingest properties");
        assert!(properties.contains_key("path"));
        assert!(!properties.contains_key("project_root"));
        assert!(!properties.contains_key("allow_overlap"));
        let rendered = serde_json::to_string(&registry).expect("serialize public registry");
        assert!(
            !rendered.contains("\"project_root\"") && !rendered.contains("allow_overlap"),
            "public schemas must not expose the unreachable ingest/project_root bootstrap"
        );

        let instructions = super::M1ND_INSTRUCTIONS;
        for stale_call in [
            "ingest project_root=",
            "`ingest` with `project_root=",
            "brain.bootstrap",
        ] {
            assert!(
                !instructions.contains(stale_call),
                "instructions still advertise the unreachable call {stale_call:?}"
            );
        }
        assert!(
            instructions.contains("brain_bootstrap_consumer_not_installed"),
            "instructions must name the fail-honest bootstrap state"
        );
    }

    /// The `ingest` sweep above, at registry scale. `tools/list` advertises the
    /// full verb surface, but under the M1ND-10 authority floors a plain MCP
    /// client is refused (`generic_action_authority_required`) on every verb
    /// whose action floor sits above ORDINARY. The gated set is DERIVED here
    /// from the same floor table the gate reads, so a verb cannot be advertised
    /// un-annotated by being forgotten, and the lie cannot come back silently.
    ///
    /// Both directions are asserted: a floor-gated verb must carry the marker
    /// AND its floor names, and a plainly dispatchable verb must NOT claim to be
    /// policy-disabled. The typed G2/G3 consumers are excluded (they bypass this
    /// gate entirely) and are held to the second rule.
    #[test]
    fn public_surface_annotates_every_floor_gated_verb() {
        use m1nd_control::AuthorityFloor;

        let catalog = m1nd_control::m1nd10_action_catalog().expect("canonical action catalog");
        let floors: std::collections::BTreeMap<&str, AuthorityFloor> = catalog
            .entries
            .iter()
            .map(|entry| (entry.action.as_str(), entry.authority_floor))
            .collect();

        let registry = all_tool_schemas();
        let tools = registry["tools"].as_array().expect("tools array");
        let advertised: std::collections::BTreeSet<&str> = tools
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect();

        // The exclusion may not become a hiding place: every typed consumer it
        // names must really be on the advertised surface.
        for typed in super::TYPED_CONSUMER_TOOLS {
            assert!(
                advertised.contains(typed),
                "TYPED_CONSUMER_TOOLS names {typed}, which tools/list does not advertise"
            );
        }

        let mut gated = 0usize;
        let mut unannotated: Vec<String> = Vec::new();
        let mut over_claimed: Vec<String> = Vec::new();
        for tool in tools {
            let name = tool["name"].as_str().expect("tool name");
            let description = tool["description"].as_str().unwrap_or_default();
            let actions = crate::action_routes::possible_mcp_actions(name)
                .unwrap_or_else(|| panic!("unrouted advertised tool {name}"));

            let mut gated_floors: std::collections::BTreeSet<&'static str> = Default::default();
            for action in &actions {
                let floor = floors
                    .get(action)
                    .unwrap_or_else(|| panic!("{name} routes to absent action {action}"));
                if !super::generic_dispatch_floor_is_available(*floor) {
                    gated_floors.insert(super::authority_floor_name(*floor));
                }
            }

            if gated_floors.is_empty() || super::TYPED_CONSUMER_TOOLS.contains(&name) {
                if description.contains(super::FLOOR_GATE_MARKER) {
                    over_claimed.push(name.to_string());
                }
                continue;
            }
            gated += 1;
            let floor_list = gated_floors.into_iter().collect::<Vec<_>>().join("|");
            // The curated `ingest` sweep names its floors in prose, so only the
            // house marker is required of it; every derived line must also carry
            // the floor names an agent needs to know what to ask for.
            let names_its_floors = name == "ingest"
                || floor_list
                    .split('|')
                    .all(|floor| description.contains(floor));
            if !description.contains(super::FLOOR_GATE_MARKER) || !names_its_floors {
                unannotated.push(format!("{name} (authority floor {floor_list})"));
            }
        }

        assert!(
            gated >= 30,
            "derivation found only {gated} floor-gated verbs — the floor table moved or the \
             derivation broke; a vacuous pass would hide the lie"
        );
        assert!(
            unannotated.is_empty(),
            "tools/list advertises {} floor-gated verbs a plain MCP client CANNOT dispatch, \
             with no {} annotation:\n  {}",
            unannotated.len(),
            super::FLOOR_GATE_MARKER,
            unannotated.join("\n  ")
        );
        assert!(
            over_claimed.is_empty(),
            "plainly dispatchable verbs must NOT claim to be policy-disabled: {}",
            over_claimed.join(", ")
        );

        // The `help` verb is the OTHER advertisement surface: it prefers the
        // tool manual's curated one-liner over the schema description, so the
        // annotation has to reach it too or the lie just moves house.
        let gated_names: std::collections::BTreeSet<&str> = tools
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .filter(|name| {
                !super::TYPED_CONSUMER_TOOLS.contains(name)
                    && crate::action_routes::possible_mcp_actions(name)
                        .unwrap_or_default()
                        .iter()
                        .any(|action| {
                            floors.get(action).is_some_and(|floor| {
                                !super::generic_dispatch_floor_is_available(*floor)
                            })
                        })
            })
            .collect();
        let help_catalog = crate::help_guidance::catalog_entries();
        let help_gated = help_catalog
            .iter()
            .filter(|entry| gated_names.contains(entry.name.as_str()))
            .count();
        assert!(
            help_gated > 0,
            "the help catalog carries no floor-gated verb — this assertion went vacuous"
        );
        let unannotated_help: Vec<String> = help_catalog
            .into_iter()
            .filter(|entry| {
                gated_names.contains(entry.name.as_str())
                    && !entry.one_liner.contains(super::FLOOR_GATE_MARKER)
            })
            .map(|entry| entry.name)
            .collect();
        assert!(
            unannotated_help.is_empty(),
            "the `help` catalog still summarizes floor-gated verbs as callable: {}",
            unannotated_help.join(", ")
        );
    }

    #[test]
    fn boot_auto_loads_agent_memory_and_reports_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        let mem_dir = runtime_dir.join("agent-memory");
        std::fs::create_dir_all(&mem_dir).expect("mem dir");
        // A prior session's authored memory.
        std::fs::write(
            mem_dir.join("prior.light.md"),
            "---\nProtocol: L1GHT/1.0\nNode: PriorKnowledge\n---\n\n## Recall\n\nThe [⍂ entity: PriorFinding] was learned last session.\n[𝔻 confidence: high]\n",
        )
        .expect("write light memory");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            registry_dir: Some(runtime_dir.join("registry")),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let server = McpServer::new(config).expect("server");
        let state = server.into_session_state();

        let report = state
            .agent_memory_boot
            .as_ref()
            .expect("agent_memory_boot should be Some when the dir exists with files");
        assert_eq!(
            report["loaded"], true,
            "memory should auto-load: {:?}",
            report
        );
        assert_eq!(report["file_count"], 1);
        assert!(
            report["nodes_added"].as_u64().unwrap_or(0) >= 1,
            "expected nodes added from the .light.md, got {:?}",
            report["nodes_added"]
        );
        // The prior knowledge must now be in the live graph.
        assert!(
            state.graph.read().num_nodes() >= 1,
            "graph should contain the loaded memory nodes"
        );
    }

    /// Regression: friendly boot must restore the plasticity query counter into
    /// the ORCHESTRATOR's engine too, not only `state.plasticity`.
    ///
    /// `activate`/`query` strengthen through `state.orchestrator.plasticity`
    /// (query.rs `query()` -> `plasticity.update`), which stamps its own
    /// `query_count` into `graph.edge_plasticity.last_used_query`. Importing the
    /// sidecar into `state.plasticity` alone left that counter at zero while the
    /// shared graph carried the restored counts, so the first strengthen after a
    /// warm boot stamped a just-used edge with 1 — making it look OLDER than
    /// every edge untouched since the previous boot. Strict recovery
    /// (`SessionState::recover_from_checkpoint`) already imports into both.
    #[test]
    fn friendly_boot_restores_plasticity_counter_into_orchestrator_engine() {
        use m1nd_core::plasticity::{PlasticityConfig, PlasticityEngine};
        use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};

        /// Warm queries the previous boot is pretended to have run. Well above
        /// the 0/1/2 a fresh orchestrator engine would stamp.
        const WARM_QUERIES: u32 = 41;

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let graph_path = runtime_dir.join("graph.json");
        let plasticity_path = runtime_dir.join("plasticity.json");

        // A previous boot's snapshot + sidecar.
        let mut graph = Graph::new();
        let lib = graph
            .add_node("file::src/lib.rs", "lib.rs", NodeType::File, &[], 0.0, 0.0)
            .expect("add lib node");
        let core = graph
            .add_node(
                "file::src/core.rs",
                "core.rs",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add core node");
        graph
            .add_edge(
                lib,
                core,
                "imports",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.8),
            )
            .expect("add edge");
        graph.finalize().expect("finalize graph");
        m1nd_core::snapshot::save_graph(&graph, &graph_path).expect("save graph snapshot");

        // Age the sidecar through a real engine: WARM_QUERIES strengthen cycles
        // leave `last_used_query = WARM_QUERIES` on the co-activated edge.
        let mut warm = PlasticityEngine::new(&graph, PlasticityConfig::default());
        let activated = vec![(lib, FiniteF32::new(0.9)), (core, FiniteF32::new(0.8))];
        for _ in 0..WARM_QUERIES {
            warm.update(&mut graph, &activated, &activated, "warm")
                .expect("warm plasticity cycle");
        }
        let warm_states = warm.export_state(&graph).expect("export warm state");
        let restored_max = warm_states
            .iter()
            .map(|state| state.last_used_query)
            .max()
            .expect("sidecar carries at least one synapse");
        assert_eq!(
            restored_max, WARM_QUERIES,
            "fixture must carry a non-zero restored query counter"
        );
        m1nd_core::snapshot::save_plasticity_state(&warm_states, &plasticity_path)
            .expect("save plasticity sidecar");

        // Friendly boot over that pair.
        let config = McpConfig {
            graph_source: graph_path,
            plasticity_state: plasticity_path,
            registry_dir: Some(runtime_dir.join("registry")),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let mut state = McpServer::new(config).expect("server").into_session_state();
        state.ingest_roots = vec![temp.path().to_string_lossy().to_string()];
        state.workspace_root = Some(temp.path().to_string_lossy().to_string());

        let before: Vec<u32> = state.graph.read().edge_plasticity.last_used_query.clone();
        assert!(
            before.contains(&restored_max),
            "boot must import the sidecar into the shared graph, got {before:?}"
        );

        // One orchestrator-driven query (the production `activate` path).
        let output = crate::tools::handle_activate(
            &mut state,
            crate::protocol::core::ActivateInput {
                query: "lib core".into(),
                agent_id: "plasticity-parity-test".into(),
                top_k: 5,
                dimensions: vec!["structural".into(), "semantic".into()],
                xlr: false,
                include_ghost_edges: false,
                include_structural_holes: false,
                token_budget: None,
            },
        )
        .expect("activate");
        assert!(
            output.plasticity.edges_strengthened >= 1,
            "the query must strengthen at least one edge for this test to mean anything"
        );

        let after: Vec<u32> = state.graph.read().edge_plasticity.last_used_query.clone();
        let touched: Vec<(usize, u32)> = after
            .iter()
            .enumerate()
            .filter(|(slot, value)| before.get(*slot) != Some(*value))
            .map(|(slot, value)| (slot, *value))
            .collect();
        assert!(
            !touched.is_empty(),
            "a strengthened edge must restamp last_used_query"
        );
        for (slot, value) in touched {
            assert!(
                value >= restored_max,
                "just-used edge at CSR slot {slot} was stamped {value}, older than the restored \
                 maximum {restored_max}: the orchestrator's plasticity engine booted with a zeroed \
                 query counter"
            );
        }
    }

    #[test]
    fn tool_schemas_expose_new_audit_surface_and_retrobuilder_tools() {
        // Use all_tool_schemas() — the full registry regardless of tier — to verify
        // that all advanced tool handlers are registered in the binary.
        // The tier gate only affects tools/list advertisement, not handler existence.
        let schema = all_tool_schemas();
        let names: Vec<String> = schema["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(|value| value.as_str()))
            .map(|value| value.to_string())
            .collect();

        for expected in [
            "ghost_edges",
            "taint_trace",
            "twins",
            "refactor_plan",
            "runtime_overlay",
            "batch_view",
            "scan_all",
            "cross_verify",
            "coverage_session",
            "external_references",
            "federate_auto",
            "audit",
            "session_handshake",
            "trust_selftest",
            "recovery_playbook",
            "doctor",
            "daemon_start",
            "daemon_stop",
            "daemon_status",
            "daemon_tick",
            "alerts_list",
            "alerts_ack",
            "mission_start",
            "mission_event",
            "mission_next",
            "mission_verify",
            "mission_handoff",
            "mission_close",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "all_tool_schemas should expose {expected} (handler registered in binary)"
            );
        }
    }

    #[test]
    fn authority_tool_schemas_close_capability_and_authority_unions() {
        let registry = all_tool_schemas();
        let tools = registry["tools"].as_array().expect("tools array");
        let tool = |name: &str| {
            tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("missing authority tool {name}"))
        };

        let authenticate = tool("authority_session_authenticate");
        let capability = &authenticate["inputSchema"]["properties"]["capability"];
        assert_eq!(capability["additionalProperties"], false);
        assert_eq!(
            capability["properties"]["schema"]["const"],
            m1nd_control::AUTHORITY_CAPABILITY_SCHEMA
        );
        assert!(capability["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "signature"));
        assert_eq!(
            capability["properties"]["payload_digest"]["pattern"],
            "^[0-9a-f]{64}$"
        );

        let authorize = tool("authority_authorize");
        let schema = &authorize["inputSchema"];
        assert_eq!(schema["additionalProperties"], false);
        assert!(schema["properties"]["requested_effects"]["items"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect == "SOVEREIGN_MUTATION"));
        let variants = schema["properties"]["input"]["oneOf"]
            .as_array()
            .expect("closed authority oneOf");
        assert_eq!(variants.len(), 4);
        let tags: std::collections::BTreeSet<&str> = variants
            .iter()
            .map(|variant| {
                assert_eq!(variant["additionalProperties"], false);
                variant["properties"]["authority"]["const"]
                    .as_str()
                    .expect("authority tag")
            })
            .collect();
        assert_eq!(
            tags,
            std::collections::BTreeSet::from([
                "ordinary_session",
                "positive_sovereign",
                "safety",
                "service_identity",
            ])
        );
        let positive = variants
            .iter()
            .find(|variant| variant["properties"]["authority"]["const"] == "positive_sovereign")
            .unwrap();
        assert_eq!(
            positive["properties"]["capability"]["additionalProperties"],
            false
        );
        let autonomy = &positive["properties"]["autonomy_evidence"]["oneOf"][1];
        assert_eq!(autonomy["additionalProperties"], false);
        assert!(autonomy["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "sentinel"));
        assert_eq!(
            autonomy["properties"]["capability"]["additionalProperties"],
            false
        );
        assert_eq!(
            autonomy["properties"]["capability"]["properties"]["core"]["additionalProperties"],
            false
        );
        assert_eq!(
            autonomy["properties"]["capability"]["properties"]["core"]["properties"]["repo_id"]
                ["type"],
            "string"
        );
        assert_eq!(
            autonomy["properties"]["capability"]["properties"]["core"]["properties"]
                ["semantic_action_id"]["pattern"],
            "^[a-z][a-z0-9_]*(?:\\.[a-z][a-z0-9_]*)+$"
        );
        let decision_variants = autonomy["properties"]["decision"]["oneOf"]
            .as_array()
            .expect("closed constitutional decision oneOf");
        assert_eq!(decision_variants.len(), 2);
        assert_eq!(
            decision_variants
                .iter()
                .map(|variant| variant["properties"]["authority_kind"]["const"]
                    .as_str()
                    .unwrap())
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from(["AGENT_QUORUM", "POLICY"])
        );
        assert_eq!(
            autonomy["properties"]["sentinel"]["oneOf"][1]["properties"]["core"]
                ["additionalProperties"],
            false
        );
    }

    #[test]
    fn autotick_skips_daemon_control_tools() {
        for skipped in [
            "daemon_start",
            "daemon_stop",
            "daemon_status",
            "daemon_tick",
            "alerts_list",
            "alerts_ack",
            "session_handshake",
            "trust_selftest",
            "recovery_playbook",
            "mission_start",
            "mission_event",
            "mission_next",
            "mission_verify",
            "mission_handoff",
            "mission_close",
        ] {
            assert!(
                !should_autotick_daemon(skipped),
                "autotick should skip {skipped}"
            );
        }
        assert!(should_autotick_daemon("search"));
        assert!(should_autotick_daemon("apply"));
    }

    #[test]
    fn mission_control_records_guardrails_and_proof_packet() {
        let (_temp, mut state) = build_state();

        // The agent's direct evidence must point at a path that actually exists
        // under a verify root (workspace_root == runtime_root in this fixture) so
        // `mission_verify` grades it `direct` on a verifiable signal, not on the
        // label alone. Write the real source the mission claims to have read.
        let auth_src = state.runtime_root.join("src").join("auth.rs");
        std::fs::create_dir_all(auth_src.parent().expect("src dir")).expect("create src dir");
        std::fs::write(&auth_src, "// logout route clears the session cookie\n")
            .expect("write auth.rs evidence");

        let start = super::dispatch_tool(
            &mut state,
            "mission_start",
            &serde_json::json!({
                "agent_id": "jimi",
                "repo": "/tmp/project",
                "task": "audit auth/session boundary",
                "mode": "review",
                "budget": "short",
                "risk": "medium"
            }),
        )
        .expect("mission_start");
        assert_eq!(start["schema"], "m1nd-mission-start-v0");
        let mission_id = start["mission_id"]
            .as_str()
            .expect("mission id")
            .to_string();
        assert!(
            state
                .runtime_root
                .join("mission-control")
                .join(format!("{mission_id}.json"))
                .exists(),
            "mission_start should persist mission state under runtime_root"
        );

        let next = super::dispatch_tool(
            &mut state,
            "mission_next",
            &serde_json::json!({
                "agent_id": "jimi",
                "mission_id": mission_id,
                "last_event": {
                    "event": "graph_query",
                    "tool": "seek",
                    "outcome": "inconclusive"
                }
            }),
        )
        .expect("mission_next");
        assert_eq!(next["schema"], "m1nd-mission-next-v0");
        assert_eq!(next["move"]["type"], "read_file");
        assert!(next["do_not"]
            .as_array()
            .expect("do_not")
            .iter()
            .any(|value| value == "seek"));

        let event = super::dispatch_tool(
            &mut state,
            "mission_event",
            &serde_json::json!({
                "agent_id": "jimi",
                "mission_id": mission_id,
                "event": "file_read",
                "payload": {
                    "path": "src/auth.rs",
                    "lines": [42, 55]
                },
                "outcome": "read direct source",
                "agent_confidence": 0.82
            }),
        )
        .expect("mission_event");
        assert_eq!(event["schema"], "m1nd-mission-event-v1");
        assert_eq!(event["event"]["event"], "file_read");
        assert_eq!(event["event"]["payload"]["path"], "src/auth.rs");
        assert_eq!(event["event"]["outcome"], "read direct source");
        assert_eq!(event["event"]["evidence_class"], "direct");
        assert!(event["event_digest"]
            .as_str()
            .expect("event digest")
            .starts_with("hash64:"));

        let graph_only = super::dispatch_tool(
            &mut state,
            "mission_verify",
            &serde_json::json!({
                "agent_id": "jimi",
                "mission_id": mission_id,
                "claim": "logout clears session",
                "evidence_refs": ["seek:auth flow"]
            }),
        )
        .expect("mission_verify graph-only");
        assert_eq!(graph_only["verdict"], "insufficient_evidence");
        assert_eq!(graph_only["evidence_grade"], "graph_only");

        let direct = super::dispatch_tool(
            &mut state,
            "mission_verify",
            &serde_json::json!({
                "agent_id": "jimi",
                "mission_id": mission_id,
                "claim": "logout route clears the session cookie",
                "evidence_refs": ["file_read:src/auth.rs:42"]
            }),
        )
        .expect("mission_verify direct");
        // `src/auth.rs` exists under the verify root and was read this turn, so the
        // direct label carries a verifiable signal and legitimately closes the claim.
        assert_eq!(direct["verdict"], "verified_for_mission");
        assert_eq!(direct["evidence_grade"], "direct");

        let handoff = super::dispatch_tool(
            &mut state,
            "mission_handoff",
            &serde_json::json!({
                "agent_id": "jimi",
                "mission_id": mission_id,
                "summary": "handoff after direct source proof",
                "recipient_agent_id": "reviewer"
            }),
        )
        .expect("mission_handoff");
        assert_eq!(handoff["schema"], "m1nd-mission-handoff-v1");
        assert_eq!(handoff["verified_claims"].as_array().unwrap().len(), 1);
        assert!(handoff["files_read"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "src/auth.rs"));

        let close = super::dispatch_tool(
            &mut state,
            "mission_close",
            &serde_json::json!({
                "agent_id": "jimi",
                "mission_id": mission_id,
                "summary": "checked the auth/session boundary",
                "gaps": ["did not run browser smoke"]
            }),
        )
        .expect("mission_close");
        assert_eq!(close["schema"], "m1nd-mission-proof-packet-v1");
        assert_eq!(close["verified_claims"].as_array().unwrap().len(), 1);
        assert_eq!(close["rejected_claims"].as_array().unwrap().len(), 1);
        assert_eq!(close["handoff_count"], 1);
        assert_eq!(
            close["context_guard_at_start"]["schema"],
            "m1nd-mission-context-guard-v1"
        );
        assert!(close["event_digest"]
            .as_str()
            .expect("event digest")
            .starts_with("hash64:"));
        assert!(close["non_claims"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value
                .as_str()
                .unwrap_or("")
                .contains("does not prove graph contents")));
    }

    #[test]
    fn health_exposes_host_binding_contract() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "health",
            &serde_json::json!({
                "agent_id": "jimi"
            }),
        )
        .expect("health output");

        assert_eq!(
            output["tool_surface_contract"]["schema"],
            "m1nd-tool-surface-contract-v0"
        );
        assert!(
            output["tool_surface_contract"]["required_host_visible_tools"]
                .as_array()
                .expect("required tools")
                .iter()
                .any(|tool| tool.as_str() == Some("trust_selftest")),
            "health should tell partial hosts that trust_selftest is required"
        );
        assert_eq!(
            output["host_binding_alignment"]["schema"],
            "m1nd-host-binding-alignment-v0"
        );
        assert_eq!(
            output["binding_fingerprint"]["schema"],
            "m1nd-binding-fingerprint-v0"
        );
    }

    #[test]
    fn session_handshake_marks_empty_graph_as_needing_ingest() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "session_handshake",
            &serde_json::json!({
                "agent_id": "jimi"
            }),
        )
        .expect("session handshake output");

        assert_eq!(output["schema"], "m1nd-session-handshake-v0");
        assert_eq!(output["trust_mode"], "needs_ingest");
        assert_eq!(output["can_ingest"], true);
        assert_eq!(output["tool_surface"]["degraded_host_tool_surface"], false);
        assert_eq!(
            output["doctor_recovery"]["suggested_tool"],
            "recovery_playbook"
        );
    }

    #[test]
    fn session_handshake_includes_binding_fingerprint() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "session_handshake",
            &serde_json::json!({
                "agent_id": "jimi"
            }),
        )
        .expect("session handshake output");

        assert_eq!(
            output["binding_fingerprint"]["schema"],
            "m1nd-binding-fingerprint-v0"
        );
        assert!(
            output["binding_fingerprint"]["process_id"]
                .as_u64()
                .unwrap_or_default()
                > 0
        );
        assert_eq!(
            output["binding_fingerprint"]["graph_finalized"],
            output["health"]["graph_finalized"]
        );
    }

    #[test]
    fn session_handshake_flags_degraded_host_tool_surface() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "session_handshake",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool_count": 3,
                "available_tools": ["seek", "audit", "doctor"],
                "missing_tools": ["ingest"]
            }),
        )
        .expect("session handshake output");

        assert_eq!(output["schema"], "m1nd-session-handshake-v0");
        assert_eq!(output["trust_mode"], "degraded_host_tool_surface");
        assert_eq!(output["can_ingest"], false);
        assert_eq!(output["can_recover"], false);
        assert!(
            output["tool_surface"]["missing_required_tools"]
                .as_array()
                .expect("missing tools")
                .iter()
                .any(|tool| tool.as_str() == Some("ingest")),
            "handshake should preserve the missing ingest diagnosis"
        );
        assert_eq!(
            output["doctor_recovery"]["arguments"]["observed_tool"],
            "tools/list"
        );
    }

    #[test]
    fn session_handshake_does_not_invent_missing_tools_from_count_only() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "session_handshake",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool_count": 94
            }),
        )
        .expect("session handshake output");

        assert_eq!(output["schema"], "m1nd-session-handshake-v0");
        assert_eq!(output["trust_mode"], "needs_ingest");
        assert_eq!(output["tool_surface"]["tool_count"], 94);
        assert_eq!(output["tool_surface"]["degraded_host_tool_surface"], false);
        assert!(
            output["tool_surface"]["missing_required_tools"]
                .as_array()
                .expect("missing tools")
                .is_empty(),
            "count-only evidence should not invent missing tool names"
        );
    }

    #[test]
    fn session_handshake_returns_full_trust_after_ingest() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(
            repo.join("src/core.py"),
            "def session_handshake_target():\n    return 'trusted graph'\n",
        )
        .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
                project_root: None,
            },
        )
        .expect("ingest");

        let output = super::dispatch_tool(
            &mut state,
            "session_handshake",
            &serde_json::json!({
                "agent_id": "jimi",
                "available_tools": ["health", "trust_selftest", "recovery_playbook", "doctor", "ingest", "seek", "help", "session_handshake"]
            }),
        )
        .expect("session handshake output");

        assert_eq!(output["schema"], "m1nd-session-handshake-v0");
        assert_eq!(output["trust_mode"], "full_trust");
        assert_eq!(output["doctor_recovery"], serde_json::Value::Null);
        assert_eq!(
            output["tool_surface"]["required_tools_present"]["trust_selftest"],
            true
        );
        assert!(
            output["health"]["node_count"].as_u64().unwrap_or_default() > 0,
            "handshake should report the populated graph"
        );
    }

    #[test]
    fn trust_selftest_empty_graph_returns_needs_ingest_with_playbook() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "trust_selftest",
            &serde_json::json!({
                "agent_id": "jimi"
            }),
        )
        .expect("trust selftest output");

        assert_eq!(output["schema"], "m1nd-trust-selftest-v0");
        assert_eq!(output["ok"], false);
        assert_eq!(output["status"], "blocked");
        assert_eq!(output["verdict"], "needs_ingest");
        assert_eq!(output["checks"]["graph_populated"], false);
        assert_eq!(
            output["recovery_playbook"]["schema"],
            "m1nd-recovery-playbook-v0"
        );
        assert_eq!(
            output["session_handshake"]["schema"],
            "m1nd-session-handshake-v0"
        );
    }

    #[test]
    fn trust_selftest_prioritizes_wrong_workspace_over_empty_graph() {
        let (temp, mut state) = build_state();
        let active_repo = temp.path().join("active-repo");
        let other_repo = temp.path().join("other-repo");
        std::fs::create_dir_all(active_repo.join("src")).expect("active src");
        std::fs::create_dir_all(other_repo.join("src")).expect("other src");
        state.workspace_root = Some(active_repo.to_string_lossy().to_string());

        let output = super::dispatch_tool(
            &mut state,
            "trust_selftest",
            &serde_json::json!({
                "agent_id": "jimi",
                "scope": other_repo.join("src").to_string_lossy(),
            }),
        )
        .expect("trust selftest output");

        assert_eq!(output["schema"], "m1nd-trust-selftest-v0");
        assert_eq!(output["ok"], false);
        assert_eq!(output["status"], "blocked");
        assert_eq!(output["verdict"], "wrong_workspace_binding");
        assert_eq!(output["checks"]["needs_ingest"], false);
        assert_eq!(output["checks"]["wrong_workspace_binding"], true);
        assert_eq!(
            output["session_handshake"]["trust_mode"],
            "wrong_workspace_binding"
        );
        assert_eq!(
            output["recovery_playbook"]["trust_mode"],
            "wrong_workspace_binding"
        );
        assert_eq!(output["next_action"], "select_or_bind_workspace");
    }

    #[test]
    fn trust_selftest_flags_degraded_host_tool_surface() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "trust_selftest",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool_count": 3,
                "available_tools": ["seek", "audit", "doctor"],
                "missing_tools": ["ingest", "trust_selftest"]
            }),
        )
        .expect("trust selftest output");

        assert_eq!(output["schema"], "m1nd-trust-selftest-v0");
        assert_eq!(output["ok"], false);
        assert_eq!(output["status"], "warn");
        assert_eq!(output["verdict"], "degraded_host_tool_surface");
        assert_eq!(output["checks"]["host_surface_complete"], false);
        assert!(
            output["session_handshake"]["tool_surface"]["missing_required_tools"]
                .as_array()
                .expect("missing tools")
                .iter()
                .any(|tool| tool.as_str() == Some("trust_selftest")),
            "selftest should preserve missing trust_selftest evidence"
        );
    }

    #[test]
    fn trust_selftest_returns_full_trust_after_ingest() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(
            repo.join("src/core.py"),
            "def trust_selftest_target():\n    return 'trusted graph'\n",
        )
        .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
                project_root: None,
            },
        )
        .expect("ingest");

        let output = super::dispatch_tool(
            &mut state,
            "trust_selftest",
            &serde_json::json!({
                "agent_id": "jimi",
                "available_tools": ["health", "trust_selftest", "recovery_playbook", "doctor", "ingest", "seek", "help", "session_handshake"]
            }),
        )
        .expect("trust selftest output");

        assert_eq!(output["schema"], "m1nd-trust-selftest-v0");
        assert_eq!(output["ok"], true);
        assert_eq!(output["status"], "ok");
        assert_eq!(output["verdict"], "full_trust");
        assert_eq!(output["next_action"], "proceed_with_m1nd_first");
        assert_eq!(output["checks"]["recovery_playbook_attached"], false);
        assert_eq!(output["recovery_playbook"], serde_json::Value::Null);
    }

    #[test]
    fn trust_selftest_flags_stale_binding_from_suspicious_retrieval() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(
            repo.join("src/core.py"),
            "def trust_selftest_stale_binding_target():\n    return 'split brain?'\n",
        )
        .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
                project_root: None,
            },
        )
        .expect("ingest");

        let output = super::dispatch_tool(
            &mut state,
            "trust_selftest",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool": "seek",
                "observed_proof_state": "blocked",
                "observed_candidates": 0
            }),
        )
        .expect("trust selftest output");

        assert_eq!(output["schema"], "m1nd-trust-selftest-v0");
        assert_eq!(output["ok"], false);
        assert_eq!(output["status"], "warn");
        assert_eq!(output["verdict"], "stale_binding_suspected");
        assert_eq!(output["checks"]["graph_populated"], true);
        assert_eq!(output["checks"]["suspicious_retrieval_evidence"], true);
        assert_eq!(
            output["recovery_playbook"]["trust_mode"],
            "stale_binding_suspected"
        );
    }

    #[test]
    fn doctor_blocks_empty_graph_with_recovery_guidance() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "doctor",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool": "seek",
                "observed_proof_state": "blocked",
                "observed_candidates": 0
            }),
        )
        .expect("doctor output");

        assert_eq!(output["schema"], "m1nd-doctor-v0");
        assert_eq!(output["status"], "blocked");
        assert_eq!(output["diagnostics"]["graph_has_nodes"], false);
        assert_eq!(output["diagnostics"]["stale_binding_suspected"], false);
        assert!(
            output["next_actions"]
                .as_array()
                .expect("next actions")
                .iter()
                .any(|action| action.as_str().unwrap_or_default().contains("ingest")),
            "doctor should tell the agent how to recover an empty graph"
        );
    }

    #[test]
    fn doctor_flags_stale_binding_when_retrieval_blocks_on_populated_graph() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(
            repo.join("src/core.py"),
            "def schema_registry():\n    return 'm1nd doctor'\n",
        )
        .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
                project_root: None,
            },
        )
        .expect("ingest");
        state.track_agent("jimi");

        let output = super::dispatch_tool(
            &mut state,
            "doctor",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool": "seek",
                "observed_proof_state": "blocked",
                "observed_candidates": 0
            }),
        )
        .expect("doctor output");

        assert_eq!(output["schema"], "m1nd-doctor-v0");
        assert_eq!(output["status"], "warn");
        assert_eq!(output["diagnostics"]["graph_has_nodes"], true);
        assert_eq!(output["diagnostics"]["stale_binding_suspected"], true);
        assert_eq!(output["diagnostics"]["agent_session_known"], true);
        assert!(
            output["transport_clues"]["split_brain_rule"]
                .as_str()
                .unwrap_or_default()
                .contains("host binding"),
            "doctor should name the host binding split-brain risk"
        );
    }

    #[test]
    fn doctor_flags_degraded_host_tool_surface_when_required_tools_are_missing() {
        let (_temp, mut state) = build_state();
        state.track_agent("jimi");

        let output = super::dispatch_tool(
            &mut state,
            "doctor",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool": "tools/list",
                "observed_proof_state": "blocked",
                "observed_tool_count": 3,
                "available_tools": ["seek", "audit", "doctor"],
                "missing_tools": ["ingest"]
            }),
        )
        .expect("doctor output");

        assert_eq!(output["schema"], "m1nd-doctor-v0");
        assert_eq!(output["diagnostics"]["degraded_host_tool_surface"], true);
        assert!(
            output["tool_surface"]["missing_tools"]
                .as_array()
                .expect("missing tools")
                .iter()
                .any(|tool| tool.as_str() == Some("ingest")),
            "doctor should name ingest as a missing recovery tool"
        );
        assert!(
            output["next_actions"]
                .as_array()
                .expect("next actions")
                .iter()
                .any(|action| action
                    .as_str()
                    .unwrap_or_default()
                    .contains("direct repo reads")),
            "doctor should tell the agent to fall back to file truth when ingest is unavailable"
        );
    }

    #[test]
    fn recovery_playbook_tool_schema_is_exposed() {
        let schema = tool_schemas();
        let names: Vec<String> = schema["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(|value| value.as_str()))
            .map(|value| value.to_string())
            .collect();

        assert!(
            names.iter().any(|name| name == "recovery_playbook"),
            "tool_schemas should expose recovery_playbook"
        );
    }

    #[test]
    fn recovery_playbook_empty_graph_refuses_policy_disabled_ingest() {
        // Field-triage 2026-07-22: an empty brain's needs_ingest playbook used to
        // recommend `{tool:"ingest"}` — a verb the server's OWN policy refuses on
        // this binding (graph.ingest.replace = POSITIVE_SOVEREIGN). Recommending a
        // refused verb sends the agent into a refusal loop. The honest recovery is
        // still `blocked` + `needs_ingest`, but it names the real gap instead.
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "recovery_playbook",
            &serde_json::json!({
                "agent_id": "jimi"
            }),
        )
        .expect("recovery playbook output");

        assert_eq!(output["schema"], "m1nd-recovery-playbook-v0");
        assert_eq!(output["status"], "blocked");
        assert_eq!(output["trust_mode"], "needs_ingest");
        assert!(
            !output["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .any(|step| step["tool"] == "ingest"),
            "playbook must not recommend the policy-disabled generic ingest verb, got {}",
            output["steps"]
        );
        // The honest gap is named in the existing G2/G3 consumer language.
        let rendered = serde_json::to_string(&output).expect("serialize playbook");
        assert!(
            rendered.contains("brain_bootstrap_consumer_not_installed"),
            "playbook must name the honest bootstrap gap, got {rendered}"
        );
    }

    #[test]
    fn recovery_playbook_never_recommends_a_policy_refused_tool() {
        // Property guard: across every recovery scenario, no emitted step may name
        // a tool whose generic dispatch the server would refuse with the step's own
        // arguments. The assertion consults the REAL policy gate, so a future
        // playbook step that recommends a sovereign verb fails this test.
        fn assert_no_policy_refused_step(output: &serde_json::Value, label: &str) {
            for step in output["steps"].as_array().expect("steps") {
                let Some(tool) = step["tool"].as_str() else {
                    continue;
                };
                let args = step
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                assert!(
                    super::enforce_generic_action_policy(tool, &args).is_ok(),
                    "playbook step {} recommends policy-refused tool '{tool}' with args {args} \
                     (scenario {label})",
                    step["id"],
                );
            }
        }

        // needs_ingest — an empty brain must not name the sovereign ingest verb.
        {
            let (_temp, mut state) = build_state();
            let output = super::dispatch_tool(
                &mut state,
                "recovery_playbook",
                &serde_json::json!({ "agent_id": "jimi" }),
            )
            .expect("needs_ingest playbook");
            assert_eq!(output["trust_mode"], "needs_ingest");
            assert_no_policy_refused_step(&output, "needs_ingest");
        }

        // wrong_workspace_binding — a populated brain + an absolute scope outside it.
        {
            let (temp, mut state) = build_state();
            let active_repo = temp.path().join("active-repo");
            std::fs::create_dir_all(active_repo.join("src")).expect("active src");
            std::fs::write(active_repo.join("src/lib.rs"), "pub fn active() {}\n")
                .expect("active file");
            crate::tools::handle_ingest(
                &mut state,
                crate::protocol::IngestInput {
                    path: active_repo.to_string_lossy().to_string(),
                    agent_id: "jimi".into(),
                    mode: "replace".into(),
                    incremental: false,
                    adapter: "code".into(),
                    namespace: None,
                    include_dotfiles: false,
                    dotfile_patterns: Vec::new(),
                    project_root: None,
                },
            )
            .expect("ingest active repo");
            let other_repo = temp.path().join("other-repo");
            std::fs::create_dir_all(other_repo.join("src")).expect("other src");
            let output = super::dispatch_tool(
                &mut state,
                "recovery_playbook",
                &serde_json::json!({
                    "agent_id": "jimi",
                    "observed_tool": "seek",
                    "observed_proof_state": "blocked",
                    "scope": other_repo.join("src").to_string_lossy(),
                }),
            )
            .expect("wrong_workspace playbook");
            assert_eq!(output["trust_mode"], "wrong_workspace_binding");
            assert_no_policy_refused_step(&output, "wrong_workspace_binding");
        }

        // degraded_host_tool_surface — doctor available, ingest missing.
        {
            let (_temp, mut state) = build_state();
            let output = super::dispatch_tool(
                &mut state,
                "recovery_playbook",
                &serde_json::json!({
                    "agent_id": "jimi",
                    "observed_tool_count": 3,
                    "available_tools": ["seek", "audit", "doctor"],
                    "missing_tools": ["ingest"],
                }),
            )
            .expect("degraded playbook");
            assert_no_policy_refused_step(&output, "degraded_host_tool_surface");
        }
    }

    #[test]
    fn ingest_project_root_hint_never_points_at_the_runtime_dir() {
        // Field-triage 2026-07-22: on a fresh brain the needs_ingest step pointed
        // ingest at the RUNTIME dir (.m1nd), not the corpus, because `workspace_root`
        // can be demoted onto the runtime dir. The hint must resolve the real repo.
        let (_temp, mut state) = build_state();
        let runtime_dir = state.runtime_root.to_string_lossy().to_string();
        // Reproduce the demotion: workspace_root sitting on the runtime dir.
        state.workspace_root = Some(runtime_dir.clone());
        state.caller_root = Some("/repo/corpus".to_string());

        let hint = crate::tools::ingest_project_root_hint(&state, None);
        assert_ne!(
            hint, runtime_dir,
            "ingest hint must never be the runtime dir, got {hint}"
        );
        assert_eq!(
            hint, "/repo/corpus",
            "ingest hint must resolve the caller's real repo root"
        );
    }

    #[test]
    fn recovery_playbook_flags_degraded_host_tool_surface() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "recovery_playbook",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool_count": 3,
                "available_tools": ["seek", "audit", "doctor"],
                "missing_tools": ["ingest"]
            }),
        )
        .expect("recovery playbook output");

        assert_eq!(output["trust_mode"], "degraded_host_tool_surface");
        assert_eq!(output["status"], "warn");
        assert!(
            output["tool_surface"]["missing_required_tools"]
                .as_array()
                .expect("missing tools")
                .iter()
                .any(|tool| tool.as_str() == Some("ingest")),
            "recovery playbook should preserve the missing ingest diagnosis"
        );
        assert!(
            output["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .any(|step| step["id"] == "call_doctor" && step["tool"] == "doctor"),
            "recovery playbook should include doctor guidance when doctor is available"
        );
    }

    #[test]
    fn recovery_playbook_flags_stale_binding_on_populated_graph() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(
            repo.join("src/core.py"),
            "def recovery_playbook_target():\n    return 'split brain?'\n",
        )
        .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
                project_root: None,
            },
        )
        .expect("ingest");
        state.track_agent("jimi");

        let output = super::dispatch_tool(
            &mut state,
            "recovery_playbook",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool": "seek",
                "observed_proof_state": "blocked",
                "observed_candidates": 0
            }),
        )
        .expect("recovery playbook output");

        assert_eq!(output["trust_mode"], "stale_binding_suspected");
        assert_eq!(output["status"], "warn");
        assert!(
            output["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .any(|step| step["id"] == "call_doctor" && step["tool"] == "doctor"),
            "stale binding playbook should tell the agent to call doctor"
        );
        assert!(
            output["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .any(|step| step["id"] == "compare_binding_fingerprint"),
            "stale binding playbook should compare binding fingerprints"
        );
        assert!(
            output["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .any(|step| step["action"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("mcp_agent_smoke.py")),
            "stale binding playbook should include repo-local smoke commands"
        );
    }

    #[test]
    fn session_handshake_flags_wrong_workspace_binding_for_absolute_scope() {
        let (temp, mut state) = build_state();
        let active_repo = temp.path().join("active-repo");
        let other_repo = temp.path().join("other-repo");
        std::fs::create_dir_all(active_repo.join("src")).expect("active src");
        std::fs::create_dir_all(other_repo.join("src")).expect("other src");
        std::fs::write(active_repo.join("src/lib.rs"), "pub fn active() {}\n")
            .expect("active file");
        std::fs::write(other_repo.join("Cargo.toml"), "[package]\nname='other'\n")
            .expect("other manifest");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: active_repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
                project_root: None,
            },
        )
        .expect("ingest active repo");

        let output = super::dispatch_tool(
            &mut state,
            "session_handshake",
            &serde_json::json!({
                "agent_id": "jimi",
                "scope": other_repo.join("src").to_string_lossy(),
            }),
        )
        .expect("session handshake output");

        assert_eq!(output["trust_mode"], "wrong_workspace_binding");
        assert_eq!(
            output["context_guard"]["workspace_binding_mismatch"]["code"],
            "wrong_workspace_binding"
        );
        assert_eq!(
            output["doctor_recovery"]["suggested_tool"],
            "recovery_playbook"
        );
    }

    #[test]
    fn recovery_playbook_count_only_host_evidence_does_not_invent_missing_tools() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "recovery_playbook",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool_count": 94
            }),
        )
        .expect("recovery playbook output");

        assert_eq!(output["trust_mode"], "needs_ingest");
        assert_eq!(output["tool_surface"]["tool_count"], 94);
        assert!(
            output["tool_surface"]["missing_required_tools"]
                .as_array()
                .expect("missing tools")
                .is_empty(),
            "count-only evidence should not invent missing tool names"
        );
    }

    #[test]
    fn recovery_playbook_routes_wrong_workspace_binding_before_stale_binding() {
        let (temp, mut state) = build_state();
        let active_repo = temp.path().join("active-repo");
        let other_repo = temp.path().join("other-repo");
        std::fs::create_dir_all(active_repo.join("src")).expect("active src");
        std::fs::create_dir_all(other_repo.join("src")).expect("other src");
        std::fs::write(active_repo.join("src/lib.rs"), "pub fn active() {}\n")
            .expect("active file");
        std::fs::write(other_repo.join("package.json"), "{\"name\":\"other\"}\n")
            .expect("other package");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: active_repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
                project_root: None,
            },
        )
        .expect("ingest active repo");

        let output = super::dispatch_tool(
            &mut state,
            "recovery_playbook",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool": "seek",
                "observed_proof_state": "blocked",
                "observed_candidates": 0,
                "scope": other_repo.join("src").to_string_lossy(),
            }),
        )
        .expect("recovery playbook output");

        assert_eq!(output["trust_mode"], "wrong_workspace_binding");
        assert_eq!(output["next_action"], "select_or_bind_workspace");
        assert_eq!(
            output["context_guard"]["workspace_binding_mismatch"]["requested_workspace_hint"],
            other_repo.to_string_lossy().as_ref()
        );
        assert!(output["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .any(|step| step["id"] == "rebind_with_workspace_root"));
    }

    #[test]
    fn activate_blocked_response_points_to_recovery_playbook_with_graph_state() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(repo.join("src/core.py"), "def core():\n    return 1\n")
            .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
                project_root: None,
            },
        )
        .expect("ingest");

        let output = super::dispatch_tool(
            &mut state,
            "activate",
            &serde_json::json!({
                "agent_id": "jimi",
                "query": "   ",
                "top_k": 5
            }),
        )
        .expect("activate output");

        assert_eq!(output["proof_state"], "blocked");
        assert_eq!(output["next_suggested_tool"], "recovery_playbook");
        assert!(output["graph_state"]["node_count"].as_u64().is_some());
        assert_eq!(
            output["recovery"]["suggested_tool"].as_str(),
            Some("recovery_playbook")
        );
        assert_eq!(
            output["recovery"]["arguments"]["observed_tool"].as_str(),
            Some("activate")
        );
    }

    #[test]
    fn activate_zero_results_without_blocked_proof_does_not_suggest_recovery() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(repo.join("src/core.py"), "def core():\n    return 1\n")
            .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
                project_root: None,
            },
        )
        .expect("ingest");

        let output = super::dispatch_tool(
            &mut state,
            "activate",
            &serde_json::json!({
                "agent_id": "jimi",
                "query": "core",
                "top_k": 0
            }),
        )
        .expect("activate output");

        assert_eq!(output["proof_state"], "triaging");
        assert_eq!(output["activated"].as_array().expect("activated").len(), 0);
        assert_ne!(output["next_suggested_tool"], "recovery_playbook");
        assert_eq!(output["recovery"], serde_json::Value::Null);
        assert_eq!(output["agent_runtime_contract"]["trust_mode"], "full_trust");
    }

    #[test]
    fn background_tick_runs_when_daemon_is_due() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        let file_path = repo.join("src/core.py");
        std::fs::write(&file_path, "def core():\n    return 1\n").expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "test".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
                project_root: None,
            },
        )
        .expect("initial ingest");

        crate::daemon_handlers::handle_daemon_start(
            &mut state,
            crate::protocol::layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec![repo.to_string_lossy().to_string()],
                poll_interval_ms: 25,
            },
        )
        .expect("daemon start");

        std::fs::write(&file_path, "def core():\n    return 9\n").expect("rewrite file");
        state.daemon_state.last_tick_ms = Some(0);

        background_tick_if_due(&mut state);

        let hit = crate::search_handlers::handle_search(
            &mut state,
            crate::protocol::layers::SearchInput {
                query: "return 9".into(),
                agent_id: "test".into(),
                mode: crate::protocol::layers::SearchMode::Literal,
                scope: None,
                filename_pattern: None,
                top_k: 5,
                case_sensitive: false,
                context_lines: 0,
                invert: false,
                count_only: false,
                multiline: false,
                auto_ingest: false,
                max_output_chars: None,
                token_budget: None,
            },
        )
        .expect("search after background tick");

        assert!(
            hit.results
                .iter()
                .any(|result| { result.matched_line.contains("return 9") }),
            "background tick should refresh the graph before the next explicit tool call"
        );
    }

    #[test]
    fn daemon_wait_duration_uses_remaining_time_until_next_tick() {
        let (_temp, mut state) = build_state();
        state.daemon_state.active = true;
        state.daemon_state.poll_interval_ms = 500;
        state.daemon_state.last_tick_ms = Some(super::now_ms().saturating_sub(125));

        let wait_ms = daemon_wait_duration_ms(&state);
        assert!(
            (300..=400).contains(&wait_ms),
            "remaining wait should be close to the poll interval remainder"
        );

        state.daemon_state.last_tick_ms = Some(0);
        let overdue_wait_ms = daemon_wait_duration_ms(&state);
        assert_eq!(overdue_wait_ms, 25);
    }

    #[test]
    fn daemon_wait_duration_expands_with_idle_backoff() {
        let (_temp, mut state) = build_state();
        state.daemon_state.active = true;
        state.daemon_state.poll_interval_ms = 200;
        state.daemon_state.last_tick_ms = Some(super::now_ms());
        state.daemon_state.idle_streak = 2;
        state.daemon_state.max_backoff_multiplier = 8;

        let wait_ms = daemon_wait_duration_ms(&state);
        assert!(
            (700..=800).contains(&wait_ms),
            "idle streak should expand effective wait close to 4x the base interval"
        );
    }

    #[test]
    fn run_daemon_tick_marks_pending_rerun_when_already_in_flight() {
        let (_temp, mut state) = build_state();
        state.daemon_state.tick_in_flight = true;
        state.daemon_state.pending_rerun = false;

        run_daemon_tick(&mut state, "traffic");

        assert!(state.daemon_state.pending_rerun);
        assert!(state.daemon_state.tick_in_flight);
    }

    #[test]
    fn native_watcher_refresh_falls_back_to_polling_for_invalid_path() {
        let (_temp, mut server) = build_server();
        server.start().expect("start actor");
        let (tx, _rx) = mpsc::sync_channel(8);
        server.daemon_runtime = Some(DaemonRuntimeControl {
            event_tx: tx,
            watcher: None,
        });
        server
            .actor_execute(true, |state| {
                state.daemon_state.active = true;
                state.daemon_state.watch_paths = vec!["/definitely/not/present".into()];
                Ok(())
            })
            .expect("configure daemon through actor");

        server.refresh_daemon_watcher().expect("refresh watcher");

        let view = server.daemon_loop_view().expect("daemon view");
        assert_eq!(view.watch_backend, "polling");
        assert!(view.watch_backend_error.is_some());
        server.shutdown().expect("shutdown");
    }

    #[test]
    fn native_watcher_refresh_uses_native_fs_for_real_root() {
        let (temp, mut server) = build_server();
        server.start().expect("start actor");
        let watch_root = temp.path().join("watch-root");
        std::fs::create_dir_all(&watch_root).expect("watch-root");
        let (tx, _rx) = mpsc::sync_channel(8);
        server.daemon_runtime = Some(DaemonRuntimeControl {
            event_tx: tx,
            watcher: None,
        });
        server
            .actor_execute(true, move |state| {
                state.daemon_state.active = true;
                state.daemon_state.watch_paths = vec![watch_root.to_string_lossy().to_string()];
                Ok(())
            })
            .expect("configure daemon through actor");

        server.refresh_daemon_watcher().expect("refresh watcher");

        let view = server.daemon_loop_view().expect("daemon view");
        assert_eq!(view.watch_backend, "native_fs");
        assert!(view.watch_backend_error.is_none());
        server.shutdown().expect("shutdown");
    }

    #[test]
    fn native_backend_uses_coarse_reconciliation_interval() {
        let (_temp, mut state) = build_state();
        state.daemon_state.active = true;
        state.daemon_state.poll_interval_ms = 200;
        state.daemon_state.watch_backend = "native_fs".into();
        state.daemon_state.last_tick_ms = Some(super::now_ms());

        let wait_ms = daemon_wait_duration_ms(&state);
        assert_eq!(wait_ms, 1000);
    }

    // -------------------------------------------------------------------------
    // Tier gate tests (Step 4)
    // These tests use tool_schemas_for_tier() directly to avoid env-var races
    // in parallel test execution. The runtime path (tool_schemas() reading
    // M1ND_TOOL_TIER) is also validated where safe to do so.
    // -------------------------------------------------------------------------

    /// The "essential" tier must advertise exactly the ESSENTIAL_TOOLS set:
    /// all required trust tools present, advanced tools absent.
    #[test]
    fn tier_gate_essential_advertises_only_essential_tools() {
        let schema = tool_schemas_for_tier("essential");
        let names: Vec<&str> = schema["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();

        // Size must equal ESSENTIAL_TOOLS
        assert_eq!(
            names.len(),
            ESSENTIAL_TOOLS.len(),
            "essential tier should advertise exactly {} tools, got {}: {:?}",
            ESSENTIAL_TOOLS.len(),
            names.len(),
            names
        );

        // All required trust tools must be present in the essential set
        for required in crate::tools::HOST_BINDING_REQUIRED_TOOLS {
            assert!(
                names.contains(&required),
                "essential tier must include required trust tool '{}'",
                required
            );
        }

        // Advanced tools that are NOT in ESSENTIAL_TOOLS must be absent
        assert!(
            !names.contains(&"resonate"),
            "advanced tool 'resonate' must be absent from essential tier (schema removed)"
        );
        assert!(
            !names.contains(&"ghost_edges"),
            "advanced tool 'ghost_edges' must be absent from essential tier"
        );
        assert!(
            !names.contains(&"twins"),
            "advanced tool 'twins' must be absent from essential tier"
        );
    }

    /// The "full" tier must advertise all registered tools (same as all_tool_schemas).
    #[test]
    fn tier_gate_full_advertises_all_tools() {
        let full_schema = all_tool_schemas();
        let gated_schema = tool_schemas_for_tier("full");

        let full_count = full_schema["tools"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        let gated_count = gated_schema["tools"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);

        assert_eq!(
            full_count, gated_count,
            "full tier must advertise all {} tools, got {}",
            full_count, gated_count
        );

        // Advanced tools must be present in the full tier
        let gated_names: Vec<&str> = gated_schema["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        // resonate schema was removed from the advertised surface (handler kept for back-compat)
        assert!(
            !gated_names.contains(&"resonate"),
            "resonate schema must be absent from full tier after surface removal"
        );
        assert!(
            gated_names.contains(&"ghost_edges"),
            "advanced tool 'ghost_edges' must be present in full tier"
        );
    }

    /// Explicit "essential" string matches what an empty/unset tier produces.
    #[test]
    fn tier_gate_essential_explicit_matches_unset() {
        // "essential" and "" (unset/default) must yield same count
        let essential_count = tool_schemas_for_tier("essential")["tools"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        let unset_count = tool_schemas_for_tier("")["tools"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);

        assert_eq!(
            essential_count, unset_count,
            "tier=essential must equal tier='' (unset)"
        );
        assert_eq!(
            essential_count,
            ESSENTIAL_TOOLS.len(),
            "essential count must match ESSENTIAL_TOOLS const"
        );
    }

    /// Unrecognized tier values must fall back to essential (not full).
    #[test]
    fn tier_gate_unrecognized_value_falls_back_to_essential() {
        let count = tool_schemas_for_tier("bogus_value_xyz")["tools"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);

        assert_eq!(
            count,
            ESSENTIAL_TOOLS.len(),
            "unrecognized tier value must fall back to essential ({} tools), got {}",
            ESSENTIAL_TOOLS.len(),
            count
        );
    }

    /// Every advertised tool must declare a top-level `"type": "object"` on its
    /// inputSchema. The MCP spec requires it and Claude Code validates it
    /// strictly: ONE offending tool makes the client reject the ENTIRE
    /// tools/list, silently unregistering every m1nd tool (2026-07-22 incident:
    /// mission_service/external_mutation_service shipped a bare top-level oneOf).
    #[test]
    fn every_tool_input_schema_is_top_level_object() {
        let full = all_tool_schemas();
        let tools = full["tools"].as_array().expect("tools array");
        assert!(!tools.is_empty(), "registry must not be empty");
        for tool in tools {
            let name = tool["name"].as_str().unwrap_or("<unnamed>");
            let ty = tool["inputSchema"]["type"].as_str();
            assert_eq!(
                ty,
                Some("object"),
                "tool '{name}' inputSchema.type must be \"object\" (got {ty:?}); a single \
                 violation makes MCP clients drop the whole tools/list"
            );
        }
    }

    /// all_tool_schemas() must always return the full registry regardless of tier,
    /// and the essential set must be a strict subset. This proves that hidden tools
    /// remain registered (their handlers exist) even when tier=essential.
    #[test]
    fn all_tool_schemas_always_contains_all_tools_regardless_of_tier() {
        let full = all_tool_schemas();
        let full_count = full["tools"].as_array().map(|a| a.len()).unwrap_or(0);
        let essential = tool_schemas_for_tier("essential");
        let essential_count = essential["tools"].as_array().map(|a| a.len()).unwrap_or(0);

        // Full registry is strictly larger than the essential set
        assert!(
            full_count > essential_count,
            "full registry ({}) must be larger than essential set ({})",
            full_count,
            essential_count
        );

        // A known advanced tool exists in the full registry (handler registered)
        let full_names: Vec<&str> = full["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        // resonate schema removed from advertised surface — handler remains but schema is gone
        assert!(
            !full_names.contains(&"resonate"),
            "'resonate' schema must be absent from all_tool_schemas after surface removal"
        );
        assert!(
            full_names.contains(&"ghost_edges"),
            "'ghost_edges' handler must remain registered even when tier=essential"
        );
        assert!(
            full_names.contains(&"daemon_start"),
            "'daemon_start' handler must remain registered even when tier=essential"
        );
    }

    #[test]
    fn public_memorize_and_persist_schemas_expose_no_filesystem_override() {
        let schemas = all_tool_schemas();
        for (tool_name, forbidden_field) in [("memorize", "output_path"), ("persist", "path")] {
            let schema = schemas["tools"]
                .as_array()
                .expect("tool registry")
                .iter()
                .find(|tool| tool["name"] == tool_name)
                .unwrap_or_else(|| panic!("missing {tool_name} schema"));
            assert_eq!(
                schema["inputSchema"]["additionalProperties"], false,
                "{tool_name} schema must advertise its fail-closed unknown-field contract"
            );
            assert!(
                schema["inputSchema"]["properties"]
                    .get(forbidden_field)
                    .is_none(),
                "{tool_name} must not advertise caller-controlled `{forbidden_field}`"
            );
        }
    }

    #[test]
    fn legacy_public_path_fields_fail_before_handler_io() {
        let (temp, mut state) = build_state();
        let sentinel = temp.path().join("outside-sentinel");
        std::fs::write(&sentinel, "sentinel\n").expect("sentinel");

        for (tool, arguments, field) in [
            (
                "memorize",
                serde_json::json!({
                    "agent_id": "attacker",
                    "node_label": "Escape",
                    "claims": [{"label": "Escape"}],
                    "output_path": sentinel,
                    "ingest_after": false
                }),
                "output_path",
            ),
            (
                "persist",
                serde_json::json!({
                    "agent_id": "attacker",
                    "action": "load",
                    "path": sentinel
                }),
                "path",
            ),
        ] {
            let error = super::dispatch_tool(&mut state, tool, &arguments)
                .expect_err("legacy path field must fail before its handler");
            assert!(
                error
                    .to_string()
                    .contains(&format!("unknown field `{field}`")),
                "unexpected {tool} refusal: {error}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(&sentinel).expect("read sentinel"),
            "sentinel\n"
        );
    }

    /// resonate schema must be absent from ALL tiers and all_tool_schemas after surface removal.
    /// The dispatch handler is kept for back-compat but is not advertised.
    #[test]
    fn resonate_schema_absent_from_all_tiers_after_surface_removal() {
        let full = all_tool_schemas();
        let essential = tool_schemas_for_tier("essential");
        let full_gated = tool_schemas_for_tier("full");

        for (label, schema) in [
            ("all_tool_schemas", &full),
            ("essential tier", &essential),
            ("full tier", &full_gated),
        ] {
            let names: Vec<&str> = schema["tools"]
                .as_array()
                .expect("tools array")
                .iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
                .collect();
            assert!(
                !names.contains(&"resonate"),
                "'resonate' schema must be absent from {} (handler kept, schema removed)",
                label
            );
        }
    }

    // -----------------------------------------------------------------------
    // orient — agent-first cold-start aggregation tool
    // -----------------------------------------------------------------------

    /// Build a SessionState backed by a small populated, finalized graph so
    /// PageRank is computed and spread-activation has nodes to land on.
    fn build_state_populated(read_only: bool) -> (tempfile::TempDir, SessionState) {
        build_state_populated_with_legacy_boot_memory(read_only, None)
    }

    fn build_state_populated_with_legacy_boot_memory(
        read_only: bool,
        legacy_boot_memory: Option<crate::session::BootMemoryState>,
    ) -> (tempfile::TempDir, SessionState) {
        use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        if let Some(legacy_boot_memory) = legacy_boot_memory {
            std::fs::write(
                runtime_dir.join("boot_memory_state.json"),
                serde_json::to_vec_pretty(&legacy_boot_memory)
                    .expect("serialize legacy boot memory fixture"),
            )
            .expect("seed legacy boot memory before migration");
        }
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            read_only,
            ..McpConfig::default()
        };

        let mut graph = Graph::new();
        // A tiny "lease" cluster so a task about leases activates something.
        let lease = graph
            .add_node(
                "file::src/lease.rs",
                "lease enforcement",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add lease node");
        let registry = graph
            .add_node(
                "file::src/registry.rs",
                "instance registry lease",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add registry node");
        let other = graph
            .add_node(
                "file::src/unrelated.rs",
                "unrelated helper",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add other node");
        graph
            .add_edge(
                lease,
                registry,
                "imports",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.9),
            )
            .expect("edge lease->registry");
        graph
            .add_edge(
                registry,
                other,
                "imports",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.3),
            )
            .expect("edge registry->other");
        graph.finalize().expect("finalize graph");

        let state = SessionState::initialize(graph, &config, DomainConfig::code())
            .expect("init populated session");
        (temp, state)
    }

    /// Build one valid mission letter for the landing-bell tests. `merge_wait`
    /// carries the gate the §1 contract demands; `landed` carries an imported
    /// receipt (the §1d landed law); every other phase carries neither.
    fn bell_letter(
        seq: u64,
        prev: Option<String>,
        phase: crate::mission_letter::Phase,
    ) -> crate::mission_letter::MissionLetter {
        use crate::mission_letter::{
            Capability, GateEvidence, Phase, ReceiptAnchor, Seat, MISSION_LETTER_SCHEMA,
        };
        crate::mission_letter::MissionLetter {
            schema: MISSION_LETTER_SCHEMA.to_string(),
            mission_id: "msn_0123456789ab".to_string(),
            mission_seq: seq,
            prev_letter_id: prev,
            block_id: "sb_alpha".to_string(),
            brain_ref: "repo-a".to_string(),
            seat: Seat::Hand,
            runner_id: None,
            capability: Capability::BuildRunner,
            phase,
            verdict: None,
            gate: matches!(phase, Phase::MergeWait).then(|| GateEvidence {
                command: "cargo test -p m1nd-mcp".to_string(),
                exit_status: 0,
                artifact_hash: "sha256:gatelog".to_string(),
            }),
            receipt_candidate: None,
            receipt: matches!(phase, Phase::Landed).then(|| ReceiptAnchor {
                imported: true,
                store_version: 1,
            }),
            packet_ref: None,
            tokens_total: 0,
            started_at: "2026-07-11T00:00:00Z".to_string(),
            updated_at: "2026-07-11T00:00:00Z".to_string(),
            synthetic: false,
        }
    }

    /// Append a mission-letter chain into the box `north` will read, through the
    /// real post engine (a valid head CAS), so a test proves the true read path.
    fn post_bell_chain(state: &SessionState, phases: &[crate::mission_letter::Phase]) {
        let box_path = crate::mission_letter_handlers::mission_box_path(state);
        if let Some(parent) = box_path.parent() {
            std::fs::create_dir_all(parent).expect("box parent dir");
        }
        let mut prev: Option<String> = None;
        for (i, phase) in phases.iter().enumerate() {
            let letter = bell_letter((i + 1) as u64, prev.clone(), *phase);
            let out = crate::mission_letter::post_mission_letter(&box_path, "hand", &letter)
                .expect("post mission letter");
            prev = Some(out.letter_id);
        }
    }

    fn bell_north_call(state: &mut SessionState) -> serde_json::Value {
        super::dispatch_tool(
            state,
            "north",
            &serde_json::json!({
                "agent_id": "northerner",
                "task": "lease enforcement in the instance registry",
            }),
        )
        .expect("north should succeed")
    }

    /// A mission whose current head is `merge_wait` rings the bell: the exact
    /// honest line (with the real N) AND the structured `landing_bell` field.
    #[test]
    fn north_rings_landing_bell_for_merge_wait_head() {
        use crate::mission_letter::Phase;
        let (_temp, mut state) = build_state_populated(false);
        post_bell_chain(&state, &[Phase::Judging, Phase::MergeWait]);

        let out = bell_north_call(&mut state);

        assert_eq!(
            out["landing_bell"]["merge_wait"], 1,
            "the structured bell must carry the real merge_wait count"
        );
        let gaps = out["honest_gaps"].as_array().expect("honest_gaps array");
        assert!(
            gaps.iter().any(|g| g.as_str()
                == Some(
                    "1 mission(s) in merge_wait await the human landing — the tray is the door"
                )),
            "north must carry the exact landing-bell line, got {gaps:?}"
        );
    }

    /// Once the head leaves `merge_wait` (the human landed it) the bell goes
    /// SILENT — a historical `merge_wait` letter is never counted.
    #[test]
    fn north_landing_bell_silent_when_head_landed() {
        use crate::mission_letter::Phase;
        let (_temp, mut state) = build_state_populated(false);
        post_bell_chain(&state, &[Phase::Judging, Phase::MergeWait, Phase::Landed]);

        let out = bell_north_call(&mut state);

        assert!(
            out.get("landing_bell").is_none(),
            "a landed head must not ring — the field is absent, not null"
        );
        let gaps = out["honest_gaps"].as_array().expect("honest_gaps array");
        assert!(
            !gaps.iter().any(|g| g
                .as_str()
                .is_some_and(|s| s.contains("await the human landing"))),
            "no landing-bell line once the head has landed, got {gaps:?}"
        );
    }

    /// F2.5e: once the head is `archived` (the human SET ASIDE a superseded receipt) the
    /// bell goes SILENT — exactly as a `landed` head does. The archived letter moved the
    /// head off `merge_wait`, so `heads_by_mission` no longer counts it: the bell drops
    /// itself, reusing the same read surface (no bell logic changed).
    #[test]
    fn north_landing_bell_silent_when_head_archived() {
        use crate::mission_letter::Phase;
        let (_temp, mut state) = build_state_populated(false);
        post_bell_chain(&state, &[Phase::Judging, Phase::MergeWait, Phase::Archived]);

        let out = bell_north_call(&mut state);

        assert!(
            out.get("landing_bell").is_none(),
            "an archived head must not ring — the field is absent, not null"
        );
        let gaps = out["honest_gaps"].as_array().expect("honest_gaps array");
        assert!(
            !gaps.iter().any(|g| g
                .as_str()
                .is_some_and(|s| s.contains("await the human landing"))),
            "no landing-bell line once the head is archived, got {gaps:?}"
        );
    }

    /// An empty/absent box — and equally a brain with no code workspace, whose box
    /// is the owner medulla box that was never written — rings nothing.
    #[test]
    fn north_landing_bell_silent_on_absent_box() {
        let (_temp, mut state) = build_state_populated(false);
        // No mission letters written at all.
        let out = bell_north_call(&mut state);
        assert!(
            out.get("landing_bell").is_none(),
            "an absent box must not ring"
        );
        assert_eq!(out["schema"], "m1nd-north-packet-v0");
    }

    /// An unreadable/corrupt box FAILS OPEN: `north` still composes a full packet,
    /// simply without the bell — the signal never takes the packet down with it.
    #[test]
    fn north_landing_bell_fails_open_on_unreadable_box() {
        let (_temp, mut state) = build_state_populated(false);
        let box_path = crate::mission_letter_handlers::mission_box_path(&state);
        if let Some(parent) = box_path.parent() {
            std::fs::create_dir_all(parent).expect("box parent dir");
        }
        // Invalid UTF-8 → read_letters' read_to_string errors → the reader fails open.
        std::fs::write(&box_path, [0xff, 0xfe, 0x00, 0x9f]).expect("write corrupt box");

        let out = bell_north_call(&mut state);

        assert_eq!(
            out["schema"], "m1nd-north-packet-v0",
            "north composes even over an unreadable box"
        );
        assert!(
            out.get("landing_bell").is_none(),
            "an unreadable box rings no bell (fail-open)"
        );
    }

    // === human_view — the m1nd voice card (m1nd-human-view-v0) =============

    /// Assert the mechanical cap on a served card: ≤4 lines, ≤80 chars/line
    /// (human_view amendment 2 — the Budget Law's card-level clause).
    fn assert_human_view_cap(card: &serde_json::Value) {
        let lines = card["lines"].as_array().expect("human_view.lines array");
        assert!(
            lines.len() <= 4,
            "cap law: never more than 4 lines, got {}",
            lines.len()
        );
        for line in lines {
            let s = line.as_str().expect("line is a string");
            assert!(
                s.chars().count() <= 80,
                "cap law: no line over 80 chars, got {} in {s:?}",
                s.chars().count()
            );
        }
    }

    /// A clean populated beat serves a ONE-line card: the identity signature,
    /// already mounted (wordmark + spine), with the mechanical state_sig.
    #[test]
    fn north_human_view_clean_is_one_line_card() {
        let (_temp, mut state) = build_state_populated(false);
        let out = bell_north_call(&mut state);

        let card = &out["human_view"];
        assert_eq!(card["schema"], "m1nd-human-view-v0");
        assert_eq!(card["state"], "clean");
        let lines = card["lines"].as_array().expect("lines array");
        assert_eq!(lines.len(), 1, "clean state = one line (the whisper)");
        let line = lines[0].as_str().unwrap();
        assert!(
            line.starts_with("m1nd "),
            "the signature hangs the wordmark on the margin, got {line:?}"
        );
        let cell6 = line.chars().nth(5).unwrap();
        assert!(
            cell6 == '╷' || cell6 == '│',
            "the pulse row hangs at column 6 — the lombada is born from it, got {line:?}"
        );
        assert!(
            line.contains("3 nodes"),
            "the identity line carries the measured node count, got {line:?}"
        );
        assert_human_view_cap(card);
        // The sig is mechanical: same state on a second beat ⇒ same sig.
        let again = bell_north_call(&mut state);
        assert_eq!(
            card["state_sig"], again["human_view"]["state_sig"],
            "equal state must serve an equal state_sig (the anti-repetition key)"
        );
    }

    /// The bell card's line 2 is the honest_gaps bell string VERBATIM —
    /// byte-equal, never a second wording (amendment 5).
    #[test]
    fn north_human_view_bell_line_byte_equal_to_honest_gaps() {
        use crate::mission_letter::Phase;
        let (_temp, mut state) = build_state_populated(false);
        post_bell_chain(&state, &[Phase::Judging, Phase::MergeWait]);

        let out = bell_north_call(&mut state);

        let card = &out["human_view"];
        assert_eq!(card["state"], "bell");
        let lines = card["lines"].as_array().expect("lines array");
        assert_eq!(lines.len(), 2, "bell card = identity + the bell line");
        let card_bell = lines[1]
            .as_str()
            .unwrap()
            .strip_prefix("     │ ")
            .expect("line 2 rides behind the gutter");
        let gaps = out["honest_gaps"].as_array().expect("honest_gaps array");
        let gap_bell = gaps
            .iter()
            .filter_map(|g| g.as_str())
            .find(|g| g.contains("await the human landing"))
            .expect("the bell gap must be present");
        assert_eq!(
            card_bell, gap_bell,
            "the card's bell line must be byte-equal to the honest_gaps string"
        );
        assert_eq!(
            out["landing_bell"]["merge_wait"], 1,
            "the structured bell and the voice card describe the same fact"
        );
        assert_human_view_cap(card);
    }

    /// MANDATORY shape (amendment 3): under `caller_root_mismatch` the card IS
    /// the warning — reception strings verbatim, the literal repair call, and
    /// ZERO statistics (they would describe the wrong brain). Ringing the bound
    /// brain's bell first proves the card is composed AFTER reception.
    #[test]
    fn north_human_view_mismatch_is_the_warning_without_statistics() {
        use crate::mission_letter::Phase;
        let (_temp, mut state) = build_state_populated(false);
        // The bound brain's bell rings — but the caller is somewhere else.
        post_bell_chain(&state, &[Phase::Judging, Phase::MergeWait]);
        let bound = state
            .workspace_root
            .clone()
            .expect("populated state must have a bound workspace_root");
        state.caller_root = Some("/some/other/repo".into());

        let out = bell_north_call(&mut state);

        assert_eq!(out["reception"]["match"], "caller_root_mismatch");
        let card = &out["human_view"];
        assert_eq!(card["state"], "mismatch");
        let lines = card["lines"].as_array().expect("lines array");
        assert_eq!(
            lines[0], "m1nd │ this graph does NOT cover your repo",
            "line 1 IS the warning, the reception's honest string verbatim"
        );
        // The bound/yours facts ride verbatim across wraps (the tempdir bound
        // path is long, so the line may wrap; the byte-exact 3-line form is
        // pinned in the human_view unit tests with short roots).
        let mut rebuilt = String::new();
        for line in lines.iter().skip(1) {
            let content = line
                .as_str()
                .unwrap()
                .trim_start_matches("     │")
                .trim_start();
            if !rebuilt.is_empty() {
                rebuilt.push(' ');
            }
            rebuilt.push_str(content);
        }
        // Space-stripped containment: a root longer than one line hard-breaks
        // mid-word, so the space-free stream is the wrap-proof witness.
        let flat = rebuilt.replace(' ', "");
        assert!(
            flat.contains(&format!("bound:{bound}").replace(' ', ""))
                && flat.contains("yours:/some/other/repo"),
            "the card names both roots, got {rebuilt:?}"
        );
        for line in lines {
            let s = line.as_str().unwrap();
            assert!(
                !s.contains("nodes") && !s.contains("memories") && !s.contains("merge_wait"),
                "ZERO statistics under mismatch — the card never describes the wrong brain: {s:?}"
            );
        }
        assert_human_view_cap(card);
    }

    /// P1 — the medulla-only read fallback (TWO-TIER-BRAIN-PRD §9.5 · §10.4 rung 3
    /// · TT-INV-2). A caller whose resolved root NO project brain covers must
    /// receive the medulla's cross-project DOCTRINE as a legitimate feed, but
    /// NEVER the medulla's own CODE anchors as "its context" — that leak is
    /// context poisoning (a foreign graph's focus_nodes passed off as the
    /// caller's). The packet carries the canonical `project_brain_absent` label.
    ///
    /// RED before P1: north ran orient over the bound graph and returned its
    /// focus_nodes/anchors as `context`, and carried no project_brain_absent
    /// label — the foreign graph's anchors leaked to the brainless caller.
    #[test]
    fn north_brainless_caller_serves_medulla_not_foreign_anchors() {
        let (temp, mut state) = build_state_populated(false);

        // Seed a legitimate medulla doctrine claim WHILE the session is the plain
        // owner (no foreign caller yet) — an owner doctrine write is legal.
        super::dispatch_tool(
            &mut state,
            "memorize",
            &serde_json::json!({
                "agent_id": "owner",
                "node_label": "CrossProjectDoctrine",
                "claims": [{
                    "label": "trust-first",
                    "text": "north before acting is the standing doctrine",
                    "confidence": 0.8
                }]
            }),
        )
        .expect("owner medulla doctrine write");

        // Now a foreign caller arrives: its root is covered by NO project brain,
        // and the bound store is the medulla.
        let brain_root = temp.path().join("repo-alpha");
        let caller_root = temp.path().join("repo-beta");
        std::fs::create_dir_all(&brain_root).expect("brain root");
        std::fs::create_dir_all(&caller_root).expect("caller root");
        state.workspace_root = Some(brain_root.to_string_lossy().to_string());
        state.ingest_roots = vec![brain_root.to_string_lossy().to_string()];
        state.caller_root = Some(caller_root.to_string_lossy().to_string());
        assert!(
            state.is_medulla_store(),
            "precondition: the bound store is the medulla"
        );
        assert!(
            !state.covers_root(&caller_root.to_string_lossy()),
            "precondition: no project brain covers the caller"
        );

        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "foreign-caller",
                "task": "lease enforcement in the instance registry",
            }),
        )
        .expect("north should succeed");

        // (c) POISON CUT — the bound graph's code anchors/focus_nodes NEVER
        //     surface as the caller's context.
        assert!(
            out["context"].is_null(),
            "brainless caller must not receive the foreign graph's context, got: {}",
            out["context"]
        );

        // (a) the canonical project_brain_absent label rides the reception + gaps.
        assert_eq!(
            out["reception"]["project_brain_absent"], true,
            "reception must carry the canonical project_brain_absent label"
        );
        assert_eq!(
            out["reception"]["match"], "caller_root_mismatch",
            "the mismatch code is preserved (roster-enrich + human_view depend on it)"
        );
        let gaps = out["honest_gaps"].as_array().expect("honest_gaps array");
        assert!(
            gaps.iter().any(|g| g
                .as_str()
                .map(|s| s.contains("project_brain_absent"))
                .unwrap_or(false)),
            "honest_gaps must name project_brain_absent, got: {gaps:?}"
        );

        // MEDULLA SERVED — the doctrine store is intact and served (not wiped by
        // the cut): the on-disk count is honest, and every served memory row is
        // medulla-tier (its light recall is scoped to `light::`, so it
        // structurally cannot carry code anchors).
        assert!(
            out["memory_exists"].as_u64().unwrap_or(0) >= 1,
            "the medulla doctrine store is served, not wiped: {}",
            out["memory_exists"]
        );
        for row in out["memory"].as_array().expect("memory array") {
            assert_eq!(
                row["tier"], "medulla",
                "every served memory row is medulla-tier under the fallback: {row}"
            );
        }

        // (4) needs_ingest must NOT lie as "empty of known brain" — the honest
        //     story is project_brain_absent, not an unfinished ingest.
        assert!(
            out["needs"].is_null(),
            "the served-medulla beat is not an empty-graph needs_ingest, got: {}",
            out["needs"]
        );
    }

    /// P1 no-regression: a caller whose root IS covered by the bound brain gets
    /// the normal grounded context — the fallback must never fire for a home
    /// caller, and no project_brain_absent gap appears.
    #[test]
    fn north_covered_caller_keeps_grounded_context() {
        let (temp, mut state) = build_state_populated(false);
        let root = temp.path().join("repo-home");
        std::fs::create_dir_all(&root).expect("home root");
        state.workspace_root = Some(root.to_string_lossy().to_string());
        state.ingest_roots = vec![root.to_string_lossy().to_string()];
        // The caller IS the bound brain's root — a covered, home caller.
        state.caller_root = Some(root.to_string_lossy().to_string());
        assert!(
            state.covers_root(&root.to_string_lossy()),
            "precondition: the caller is covered"
        );

        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "home-caller",
                "task": "lease enforcement in the instance registry",
            }),
        )
        .expect("north should succeed");

        // Covered → no reception block, real context with activated focus nodes.
        assert!(
            out["reception"].is_null(),
            "covered caller gets no reception packet, got: {}",
            out["reception"]
        );
        assert!(
            !out["context"].is_null(),
            "covered caller gets grounded context"
        );
        let focus = out["context"]["focus_nodes"]
            .as_array()
            .expect("focus_nodes array");
        assert!(
            !focus.is_empty(),
            "the lease task activates focus nodes for a covered caller"
        );
        // The project_brain_absent label never fires for a home caller.
        let gaps = out["honest_gaps"].as_array().expect("honest_gaps array");
        assert!(
            gaps.iter()
                .all(|g| !g.as_str().unwrap_or("").contains("project_brain_absent")),
            "no project_brain_absent gap for a covered caller: {gaps:?}"
        );
    }

    /// MANDATORY shape (amendment 4): the empty/unbound graph serves the honest
    /// needs_ingest card — the zero IS the message and the gap string rides
    /// verbatim (wrapped whole, never truncated).
    #[test]
    fn north_human_view_needs_ingest_form() {
        let (_temp, mut state) = build_state();

        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "voice-needs-ingest",
                "task": "lease enforcement in the instance registry",
            }),
        )
        .expect("north should succeed even on an empty graph");

        let card = &out["human_view"];
        assert_eq!(card["state"], "needs_ingest");
        let lines = card["lines"].as_array().expect("lines array");
        assert_eq!(
            lines[0], "m1nd ╷│╷╷╷  needs_ingest · 0 nodes",
            "S4 line 1: the graph cell alone is raised, the zero is the message"
        );
        // The wrapped card content reassembles to the exact honest_gaps string.
        let mut rebuilt = String::new();
        for line in lines.iter().skip(1) {
            let content = line
                .as_str()
                .unwrap()
                .trim_start_matches("     │")
                .trim_start();
            if !rebuilt.is_empty() {
                rebuilt.push(' ');
            }
            rebuilt.push_str(content);
        }
        assert_eq!(
            rebuilt,
            crate::human_view::NEEDS_INGEST_GAP,
            "the gap rides verbatim — wrapped, never reworded"
        );
        assert_human_view_cap(card);
    }

    /// Fail-open: an unreadable mission box mutes the bell, and the voice card
    /// still rides HONEST (no bell state claimed) — the packet never errors
    /// over its own voice.
    #[test]
    fn north_human_view_rides_honest_when_bell_source_is_unreadable() {
        let (_temp, mut state) = build_state_populated(false);
        let box_path = crate::mission_letter_handlers::mission_box_path(&state);
        if let Some(parent) = box_path.parent() {
            std::fs::create_dir_all(parent).expect("box parent dir");
        }
        std::fs::write(&box_path, [0xff, 0xfe, 0x00, 0x9f]).expect("write corrupt box");

        let out = bell_north_call(&mut state);

        assert_eq!(out["schema"], "m1nd-north-packet-v0");
        let card = &out["human_view"];
        assert_eq!(
            card["state"], "clean",
            "an unreadable bell source serves the honest clean card, never a fabricated bell"
        );
        assert_human_view_cap(card);
    }

    #[test]
    fn orient_returns_focus_nodes_on_populated_graph() {
        let (_temp, mut state) = build_state_populated(false);
        let out = super::dispatch_tool(
            &mut state,
            "orient",
            &serde_json::json!({
                "agent_id": "orienter",
                "task": "lease enforcement in the instance registry",
            }),
        )
        .expect("orient should succeed on a populated graph");

        // Contract shape.
        assert_eq!(
            out["task"], "lease enforcement in the instance registry",
            "task must be echoed"
        );
        assert_eq!(out["proof_state"], "triaging");
        assert!(out["summary"].is_string(), "summary must be a string");

        let focus = out["focus_nodes"].as_array().expect("focus_nodes array");
        assert!(
            !focus.is_empty(),
            "focus_nodes must be non-empty on a populated graph"
        );
        // Each focus node carries the contract fields.
        for f in focus {
            assert!(f["node_id"].is_string(), "focus node needs node_id");
            assert!(f["label"].is_string(), "focus node needs label");
            assert!(f.get("pagerank").is_some(), "focus node needs pagerank");
            assert!(f.get("activation").is_some(), "focus node needs activation");
            assert!(f.get("kind").is_some(), "focus node needs kind");
            assert!(f.get("path").is_some(), "focus node needs path key");
        }

        // anchors are the global PageRank backbone (non-empty on a finalized graph).
        let anchors = out["anchors"].as_array().expect("anchors array");
        assert!(
            !anchors.is_empty(),
            "anchors must be non-empty once PageRank is computed"
        );
        for a in anchors {
            assert!(a["node_id"].is_string());
            assert!(a["pagerank"].is_number());
        }

        // The activation inside orient records a coverage session for this agent,
        // so coverage is populated with visited/total and a high-value shortlist.
        let cov = &out["coverage"];
        assert!(
            cov.is_object(),
            "coverage must be populated after activation"
        );
        assert!(cov["visited"].is_number(), "coverage.visited present");
        assert_eq!(cov["total"], serde_json::json!(3), "graph has 3 nodes");
        assert!(
            cov["unvisited_high_value"].is_array(),
            "coverage.unvisited_high_value is an array"
        );

        // suggested_first_calls leads with surgical_context on the top focus node.
        let calls = out["suggested_first_calls"]
            .as_array()
            .expect("suggested_first_calls array");
        assert!(!calls.is_empty(), "must suggest at least one first call");
        assert_eq!(calls[0]["tool"], "surgical_context");
        assert!(calls[0]["arguments"]["node_id"].is_string());

        // The _m1nd envelope is attached by dispatch (additive).
        assert!(
            out.as_object().unwrap().contains_key("_m1nd"),
            "_m1nd envelope must wrap orient too"
        );
    }

    #[test]
    fn orient_works_in_read_only_mode() {
        let (_temp, mut state) = build_state_populated(true);
        assert!(state.read_only, "state must be read-only");

        // orient must NOT be caught by the mutation deny-list.
        use super::read_only_denied;
        assert!(
            !read_only_denied("orient", &serde_json::json!({})),
            "orient must be allowed in read-only mode"
        );

        // It dispatches successfully through the read-only path (query_readonly).
        let out = super::dispatch_tool(
            &mut state,
            "orient",
            &serde_json::json!({
                "agent_id": "ro-agent",
                "task": "lease enforcement",
            }),
        )
        .expect("orient must succeed in read-only attach");
        assert_eq!(out["task"], "lease enforcement");
        assert!(out["focus_nodes"].is_array());
        // read_only flag is surfaced via the envelope.
        assert_eq!(out["_m1nd"]["read_only"], serde_json::json!(true));

        // A read-only attach must never write the graph snapshot.
        assert!(
            !state.graph_path.exists(),
            "orient must not persist anything in read-only mode"
        );
    }

    /// REAL PROBE: load the repo's actual graph_snapshot.json (~5540 nodes) and
    /// run `orient` on a real task, printing the focus nodes + summary.
    ///
    /// Run with: `cargo test -p m1nd-mcp orient_real_snapshot_probe -- --nocapture`
    /// Skips gracefully (printing a note) if the snapshot is not present.
    #[test]
    fn orient_real_snapshot_probe() {
        // Locate the repo-root snapshot relative to the crate dir.
        let snapshot = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("graph_snapshot.json"))
            .filter(|p| p.exists());
        let Some(snapshot_path) = snapshot else {
            eprintln!("[orient_real_snapshot_probe] graph_snapshot.json not found — skipping");
            return;
        };

        let graph =
            m1nd_core::snapshot::load_graph(&snapshot_path).expect("load real graph_snapshot.json");
        eprintln!(
            "[orient_real_snapshot_probe] loaded {} nodes from {}",
            graph.nodes.count,
            snapshot_path.display()
        );

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            read_only: true, // attach-style: prove orient works without mutating
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(graph, &config, DomainConfig::code())
            .expect("init session from real snapshot");

        let out = super::dispatch_tool(
            &mut state,
            "orient",
            &serde_json::json!({
                "agent_id": "probe",
                "task": "read-only attach lease enforcement",
                "top_k": 8,
            }),
        )
        .expect("orient on real snapshot");

        eprintln!("\n=== orient(task=\"read-only attach lease enforcement\") on REAL graph ===");
        eprintln!("summary: {}", out["summary"].as_str().unwrap_or(""));
        eprintln!("focus_nodes:");
        for (i, f) in out["focus_nodes"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            eprintln!(
                "  {:>2}. {:<55} act={:.4} pr={:.6} kind={} path={}",
                i + 1,
                f["label"].as_str().unwrap_or(""),
                f["activation"].as_f64().unwrap_or(0.0),
                f["pagerank"].as_f64().unwrap_or(0.0),
                f["kind"].as_str().unwrap_or(""),
                f["path"].as_str().unwrap_or("·"),
            );
        }
        eprintln!("anchors (global PageRank backbone):");
        for a in out["anchors"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
        {
            eprintln!(
                "  - {:<55} pr={:.6}",
                a["label"].as_str().unwrap_or(""),
                a["pagerank"].as_f64().unwrap_or(0.0)
            );
        }
        eprintln!(
            "memory_nearby: {} | coverage: {}",
            out["memory_nearby"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0),
            out["coverage"]
        );
        eprintln!("=== end probe ===\n");

        // Real data must produce a non-empty starting context.
        assert!(
            !out["focus_nodes"].as_array().unwrap().is_empty(),
            "orient must surface focus nodes on the real graph"
        );
    }

    // === north packet (pre-orient) ========================================

    /// On an EMPTY / unbound graph, north must HONESTLY return needs_ingest plus
    /// the repair — never a fabricated orientation. Mirrors
    /// `trust_selftest_empty_graph_returns_needs_ingest_with_playbook`.
    #[test]
    fn north_empty_graph_returns_needs_ingest_not_fake_packet() {
        let (_temp, mut state) = build_state();

        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "northerner",
                "task": "lease enforcement in the instance registry",
            }),
        )
        .expect("north should succeed even on an empty graph");

        assert_eq!(out["schema"], "m1nd-north-packet-v0");
        // Honest empty-graph signal, not a fake context.
        assert_eq!(
            out["needs"], "needs_ingest",
            "empty graph must say needs_ingest"
        );
        assert_eq!(out["binding"]["trust_mode"], "needs_ingest");
        assert_eq!(out["binding"]["graph_populated"], false);
        assert_eq!(out["binding"]["ok"], false);
        assert!(
            out["context"].is_null(),
            "context must be null (no fabricated orientation) on an empty graph, got {}",
            out["context"]
        );
        assert!(
            out["sufficiency"].is_null(),
            "sufficiency must be null when the graph cannot answer yet"
        );
        // The repair travels with the packet.
        assert_eq!(
            out["recovery_playbook"]["schema"], "m1nd-recovery-playbook-v0",
            "the repair (recovery_playbook) must accompany a degraded binding"
        );
        // honest_gaps must name the missing graph.
        let gaps = out["honest_gaps"].as_array().expect("honest_gaps array");
        assert!(
            gaps.iter()
                .any(|g| g.as_str().unwrap_or("").contains("empty or unbound")),
            "honest_gaps must state the graph is empty/unbound, got {gaps:?}"
        );
    }

    /// First-Contact Reception degraded mode (TWO-TIER-BRAIN-PRD §9.5.5).
    /// When the caller's resolved root (hop-2 `M1nd-Caller-Root`) is KNOWN and
    /// falls OUTSIDE the bound workspace, north must carry a `reception` block
    /// flagging the mismatch honestly. THIS FAILS BEFORE THE FIX (no `reception`
    /// key) — that silence is exactly the live Antigravity/project-b failure this
    /// slice kills.
    #[test]
    fn north_reception_flags_caller_root_mismatch() {
        let (_temp, mut state) = build_state_populated(false);
        // The bound workspace (SessionState::initialize binds it to the runtime dir).
        let bound = state
            .workspace_root
            .clone()
            .expect("populated state must have a bound workspace_root");
        // A caller root guaranteed NOT under the bound workspace.
        state.caller_root = Some("/some/other/repo".into());

        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "reception-mismatch",
                "task": "lease enforcement in the instance registry",
            }),
        )
        .expect("north should succeed on a populated graph");

        assert_eq!(
            out["reception"]["match"], "caller_root_mismatch",
            "north must flag a mismatched caller_root (the silence is the bug)"
        );
        assert_eq!(
            out["reception"]["caller_root"], "/some/other/repo",
            "reception must echo the caller's resolved root verbatim"
        );
        assert_eq!(
            out["reception"]["bound_workspace"], bound,
            "reception must name the bound workspace the caller is NOT under"
        );
        let options = out["reception"]["options"]
            .as_array()
            .expect("reception.options must be an array");
        assert!(
            !options.is_empty(),
            "reception.options must be a non-empty, machine-actionable list"
        );
    }

    /// TT-INV-12 silence-when-matched: when the caller's resolved root falls
    /// UNDER the bound workspace, silent binding is legal — north carries NO
    /// `reception` block. Guards against a false-positive front desk.
    #[test]
    fn north_reception_absent_when_caller_root_matches() {
        let (_temp, mut state) = build_state_populated(false);
        // Caller root == the bound workspace → a match, silence is correct.
        state.caller_root = state.workspace_root.clone();

        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "reception-match",
                "task": "lease enforcement in the instance registry",
            }),
        )
        .expect("north should succeed on a populated graph");

        assert!(
            out.get("reception").is_none() || out["reception"].is_null(),
            "a matched caller_root must NOT raise reception (TT-INV-12)"
        );
    }

    /// Honesty-by-omission (§9.5.4 absent≠wrong): when the caller root is
    /// UNKNOWN (direct-HTTP / legacy bridge sent no header), the match cannot be
    /// computed, so north raises NO `reception` block — no false alarm.
    #[test]
    fn north_reception_absent_when_caller_root_unknown() {
        let (_temp, mut state) = build_state_populated(false);
        // Leave caller_root = None (the default) → unknown caller.
        assert!(state.caller_root.is_none());

        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "reception-unknown",
                "task": "lease enforcement in the instance registry",
            }),
        )
        .expect("north should succeed on a populated graph");

        assert!(
            out.get("reception").is_none() || out["reception"].is_null(),
            "an unknown caller_root must NOT raise reception (honesty by omission)"
        );
    }

    /// On a bound + populated graph, north returns a full packet: binding
    /// trust_mode, context focus nodes, an anchors backbone, and a sufficiency
    /// signal. Mirrors `orient_returns_focus_nodes_on_populated_graph`.
    #[test]
    fn north_populated_graph_returns_full_packet() {
        let (_temp, mut state) = build_state_populated(false);

        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "northerner",
                "task": "lease enforcement in the instance registry",
            }),
        )
        .expect("north should succeed on a populated graph");

        assert_eq!(out["schema"], "m1nd-north-packet-v0");
        assert_eq!(out["task"], "lease enforcement in the instance registry");
        // Binding is full trust on a populated, bound graph.
        assert_eq!(out["binding"]["trust_mode"], "full_trust");
        assert_eq!(out["binding"]["graph_populated"], true);
        assert_eq!(out["binding"]["ok"], true);
        assert!(
            out["needs"].is_null(),
            "a populated graph must not signal needs_ingest"
        );
        // Fingerprint passes through verbatim from trust_selftest.
        assert_eq!(
            out["binding"]["fingerprint"]["schema"], "m1nd-binding-fingerprint-v0",
            "binding fingerprint must pass through"
        );
        // Context is real, not null.
        let focus = out["context"]["focus_nodes"]
            .as_array()
            .expect("context.focus_nodes array");
        assert!(
            !focus.is_empty(),
            "focus_nodes must be non-empty on a populated graph"
        );
        assert!(
            out["context"]["anchors"].is_array(),
            "context.anchors must be present"
        );
        // Sufficiency is the answer-free stop signal lifted from focus.
        let state_str = out["sufficiency"]["state"].as_str();
        assert!(
            matches!(state_str, Some("sufficient" | "gathering" | "saturated")),
            "sufficiency.state must be one of sufficient|gathering|saturated, got {state_str:?}"
        );
        assert!(
            out["next_move"].is_string() && !out["next_move"].as_str().unwrap().is_empty(),
            "next_move must be a non-empty honest suggestion"
        );
    }

    /// north carries durable boot memory with its REAL age (now − updated_at_ms)
    /// and authoring agent — proving the provenance mapping and the honesty rule
    /// (age present because the timestamp is present; source_agent carried).
    #[test]
    fn north_carries_boot_memory_with_age_and_author() {
        let legacy = crate::session::BootMemoryState {
            entries: std::collections::HashMap::from([(
                "lease_doctrine".to_string(),
                crate::session::BootMemoryEntry {
                    key: "lease_doctrine".to_string(),
                    value: serde_json::json!({"rule": "leases expire after 30s"}),
                    tags: vec!["lease".to_string(), "doctrine".to_string()],
                    source_refs: vec!["src/lease.rs".to_string()],
                    updated_at_ms: crate::util::now_ms(),
                    updated_by_agent: "jimi".to_string(),
                },
            )]),
        };
        // The compatibility source must exist before initialize so the one-way
        // migration conserves it into Boot Config/L1GHT and retires the writer.
        let (_temp, mut state) = build_state_populated_with_legacy_boot_memory(false, Some(legacy));

        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "northerner",
                "task": "lease enforcement",
            }),
        )
        .expect("north with seeded memory");

        let memory = out["memory"].as_array().expect("memory array");
        assert_eq!(memory.len(), 1, "the one seeded claim must be recalled");
        let entry = &memory[0];
        assert_eq!(entry["claim"], "lease_doctrine", "the claim key is carried");
        assert_eq!(
            entry["source_agent"], "jimi",
            "the authoring agent is carried as source_agent"
        );
        // Age is PRESENT and honest (just-authored → small, non-negative).
        let age = entry["age_ms"]
            .as_u64()
            .expect("age_ms present because updated_at_ms is present");
        assert!(
            age < 60_000,
            "just-authored memory should have a small age, got {age}ms"
        );
        // Freshly authored → not stale.
        assert_eq!(
            entry["stale"], false,
            "a fresh memory must not be flagged stale"
        );
        assert_eq!(entry["tags"], serde_json::json!(["lease", "doctrine"]));
    }

    /// R0 — MED-INV-6 packet honesty (RED→GREEN): a north beat over a NON-EMPTY
    /// memory store must NEVER emit the false "No durable memory yet" line. Recall
    /// missing a task-relevant hit (the task does not match any stored claim) is a
    /// no-match, not an empty store. The packet must instead carry `memory_exists`
    /// = the on-disk store count (>0) and an honest gap that says the store HAS
    /// claims that just did not match this task.
    #[test]
    fn north_over_nonempty_store_never_claims_no_durable_memory() {
        let (_temp, mut state) = build_state_populated(false);

        // Seed the durable L1GHT store on disk with claims that have NOTHING to do
        // with the query task, so recall returns empty for this task.
        let store = state.runtime_root.join("agent-memory");
        std::fs::create_dir_all(&store).expect("agent-memory store dir");
        let now_ms = super::now_ms();
        for (i, node) in ["AlphaDoctrine", "BetaDoctrine", "GammaDoctrine"]
            .iter()
            .enumerate()
        {
            let md = format!(
                "---\nProtocol: L1GHT/1.0\nNode: {node}\nState: verified\n\
                 Created: {now_ms}\nSource-Agent: seeder\n---\n\n\
                 # {node}\n\n## {node}\n\n\
                 A durable claim about widget calibration cadence number {i}.\n\n\
                 [⍂ entity: widget calibration cadence]\n[𝔻 confidence: 0.9]\n"
            );
            std::fs::write(store.join(format!("mem_{i}.light.md")), md).expect("write memory");
        }

        // The store now holds 3 claims on disk — ground truth, before any recall.
        assert_eq!(
            state.light_memory_count(),
            3,
            "the store must hold 3 durable claims on disk"
        );

        // A task that matches NONE of the stored claims → recall returns empty.
        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "northerner",
                "task": "quantum flux capacitor tachyon inversion protocol",
            }),
        )
        .expect("north over a non-empty store with an unmatched task");

        // The recalled memory block may be empty (no task match) — that is fine.
        // What is NOT fine is the false-absence line over a non-empty store.
        let gaps = out["honest_gaps"]
            .as_array()
            .expect("honest_gaps array")
            .iter()
            .filter_map(|g| g.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            !gaps.contains("No durable memory yet"),
            "MED-INV-6: a beat over a non-empty store must NOT claim 'No durable memory yet'; gaps were: {gaps}"
        );
        // The packet stamps the ground-truth store size so a consumer never has to
        // infer emptiness from `memory: []`.
        assert_eq!(
            out["memory_exists"].as_u64(),
            Some(3),
            "memory_exists must carry the on-disk store count (3)"
        );
        // And when memory did not surface, the gap tells the honest story.
        let memory_empty = out["memory"]
            .as_array()
            .map(|m| m.is_empty())
            .unwrap_or(true);
        if memory_empty {
            assert!(
                gaps.contains("memory store holds"),
                "an empty beat over a non-empty store must say the store HAS claims that did not match; gaps: {gaps}"
            );
        }
    }

    /// R0 companion: over a TRULY empty store the honest "No durable memory yet"
    /// line is still correct and `memory_exists` is 0 — the fix must not suppress
    /// the true absence, only the FALSE one.
    #[test]
    fn north_over_empty_store_still_says_no_durable_memory() {
        let (_temp, mut state) = build_state_populated(false);
        assert_eq!(
            state.light_memory_count(),
            0,
            "no store seeded → count is 0"
        );
        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "northerner",
                "task": "anything at all",
            }),
        )
        .expect("north over an empty store");
        assert_eq!(out["memory_exists"].as_u64(), Some(0));
        let gaps = out["honest_gaps"]
            .as_array()
            .expect("honest_gaps array")
            .iter()
            .filter_map(|g| g.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            gaps.contains("No durable memory yet"),
            "a truly empty store must still honestly say there is no durable memory; gaps: {gaps}"
        );
    }

    /// R1(a) — Budget Law "no duplicate serialization" (RED→GREEN): a north packet
    /// embeds BOTH `binding.fingerprint` and `binding.graph_state`. The roots must
    /// never be serialized byte-identically in both blocks — `graph_state` carries
    /// only the COUNT. Before the fix the same array was serialized in both.
    /// The fingerprint's own block is now bounded too (a head of
    /// `FINGERPRINT_INGEST_ROOTS_HEAD` with the omission declared); below that
    /// bound — as here — it still lists every root and says it is untruncated.
    #[test]
    fn north_binding_serializes_ingest_roots_once_not_duplicated() {
        let (_temp, mut state) = build_state_populated(false);
        // Give the binding several roots so a duplicated array is unmistakable.
        state.ingest_roots = vec![
            "/path/to/repo".into(),
            "/path/to/other".into(),
            "/path/to/third".into(),
        ];

        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "northerner",
                "task": "lease enforcement",
            }),
        )
        .expect("north on a multi-root binding");

        // Three roots is under the head bound, so the fingerprint still lists them
        // all — and says so instead of leaving the reader to guess.
        let fp = &out["binding"]["fingerprint"];
        let fp_roots = fp["ingest_roots"]
            .as_array()
            .expect("fingerprint carries the ingest_roots head");
        assert_eq!(fp_roots.len(), 3, "fingerprint lists all three roots");
        assert_eq!(fp["ingest_root_count"].as_u64(), Some(3));
        assert_eq!(fp["ingest_roots_truncated"], serde_json::json!(false));
        assert_eq!(fp["ingest_roots_omitted"].as_u64(), Some(0));

        // graph_state must NOT re-serialize the full array — only the count.
        assert!(
            out["binding"]["graph_state"]["ingest_roots"].is_null(),
            "graph_state must NOT duplicate the full ingest_roots array; it was: {}",
            out["binding"]["graph_state"]["ingest_roots"]
        );
        assert_eq!(
            out["binding"]["graph_state"]["ingest_root_count"].as_u64(),
            Some(3),
            "graph_state carries the COUNT instead of the duplicated array"
        );
    }

    /// Field-triage #1: north must COMPOSE L1GHT agent-memory (written by
    /// `memorize`, the primary memory system) into its `memory` block — not only
    /// boot_memory. We plant a `.light.md` with the exact frontmatter the memorize
    /// writer stamps, ingest it (light adapter) into the same graph, then call north
    /// with a task matching that memory. The memorized claim must surface in
    /// `packet.memory` tagged `kind:"light"`, carrying its `source_agent` and a
    /// concrete authored `age_ms` (both lifted from seek's light provenance).
    #[test]
    fn north_composes_light_memory_recall() {
        let (temp, mut state) = build_state_populated(false);

        // A memorized claim about lease leadership, authored just now by agent-mem,
        // with the frontmatter shape `memorize` renders (Created + Source-Agent).
        let now_ms = super::now_ms();
        let mem_dir = temp.path().join("light-mem");
        std::fs::create_dir_all(&mem_dir).expect("light mem dir");
        let md = format!(
            "---\nProtocol: L1GHT/1.0\nNode: LeaseLeadership\nState: verified\n\
             Created: {now_ms}\nSource-Agent: agent-mem\n---\n\n\
             # LeaseLeadership\n\n## LeaseLeadership\n\n\
             The lease leadership handoff must renew the registry lease before takeover.\n\n\
             [⍂ entity: lease enforcement leadership handoff]\n[𝔻 confidence: 0.9]\n"
        );
        std::fs::write(mem_dir.join("lease_leadership.light.md"), md).expect("write memory");

        // Merge the light memory INTO the populated code graph (adapter=light).
        super::dispatch_tool(
            &mut state,
            "ingest",
            &serde_json::json!({
                "agent_id": "agent-mem",
                "path": mem_dir.to_string_lossy(),
                "adapter": "light",
                "mode": "merge",
                "namespace": "light",
            }),
        )
        .expect("ingest light memory");

        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "northerner",
                "task": "lease enforcement leadership handoff",
            }),
        )
        .expect("north with a memorized L1GHT claim present");

        let memory = out["memory"].as_array().expect("memory array");
        let light: Vec<&serde_json::Value> = memory
            .iter()
            .filter(|e| e.get("kind").and_then(|k| k.as_str()) == Some("light"))
            .collect();
        assert!(
            !light.is_empty(),
            "north.memory must compose at least one L1GHT claim, got {memory:?}"
        );
        // The memorized claim's provenance is carried honestly.
        let hit = light
            .iter()
            .find(|e| {
                e["source_agent"].as_str() == Some("agent-mem")
                    || e["claim"]
                        .as_str()
                        .map(|c| c.to_lowercase().contains("lease"))
                        .unwrap_or(false)
            })
            .unwrap_or(&light[0]);
        assert_eq!(
            hit["source_agent"], "agent-mem",
            "the memorized claim carries its authoring agent as source_agent"
        );
        let age = hit["age_ms"]
            .as_u64()
            .expect("age_ms present — the light hit carries an authored age");
        assert!(
            age < 60_000,
            "a just-authored memory should have a small age, got {age}ms"
        );
        assert_eq!(
            hit["stale"], false,
            "a fresh memory must not be flagged stale"
        );
    }

    /// FIELD-TRIAGE #6 (RED→GREEN): L1GHT recall must be robust on a MIXED graph.
    ///
    /// The live failure: on a graph carrying agent-memory L1GHT notes AND a large
    /// code corpus, `north.memory` returned empty and `honest_gaps` said "No durable
    /// memory yet" — while a direct `seek` found the note at rank #2 WITH full
    /// provenance. Root cause: the memory beat asked `seek` for a task-scoped top-K
    /// and then POST-FILTERED to light-provenance hits; once code nodes dominate
    /// ranking, the top-K contains no light hit and the filter yields empty.
    ///
    /// This test reproduces that exactly: a synthetic code corpus whose labels are
    /// saturated with the SAME task keywords (so code outranks the note across the
    /// whole top-K), plus one memorized `.light.md` claim. On the pre-fix code the
    /// note is crowded past the recall window and `memory` is empty (RED). The fix
    /// scopes the recall seek to the `light::` node-id namespace so code nodes are
    /// structurally invisible to the recall pass — the note surfaces regardless of
    /// how the mixed graph ranks (GREEN).
    #[test]
    fn north_recalls_light_memory_on_mixed_graph() {
        use m1nd_core::types::{EdgeDirection, FiniteF32, NodeId, NodeType};

        let (temp, mut state) = build_state_populated(false);

        // 1. Flood the graph with a large code corpus whose labels carry the exact
        //    task tokens, so on a keyword query these code File nodes all score high
        //    and crowd any single memory node out of the top-K. This is what drowns
        //    the light note on a real repo (6k+ nodes) — reproduced deterministically.
        {
            let mut graph = state.graph.write();
            for i in 0..400 {
                let ext = format!("file::src/generated/handoff_{i}.rs");
                let label = format!("lease enforcement leadership handoff registry impl {i}");
                let node = graph
                    .add_node(&ext, &label, NodeType::File, &[], 0.0, 0.0)
                    .expect("add synthetic code node");
                // A little connectivity so PageRank/graph-rerank treats them as real.
                if i > 0 {
                    let prev = NodeId::new(node.as_usize() as u32 - 1);
                    let _ = graph.add_edge(
                        prev,
                        node,
                        "imports",
                        FiniteF32::new(1.0),
                        EdgeDirection::Forward,
                        false,
                        FiniteF32::new(0.5),
                    );
                }
            }
            graph.finalize().expect("re-finalize mixed graph");
        }

        // 2. Plant one memorized claim (exact `memorize` frontmatter) and merge it
        //    into the SAME graph via the light adapter under the `light` namespace —
        //    the identical path `reload_agent_memory` uses for real agent-memory.
        let now_ms = super::now_ms();
        let mem_dir = temp.path().join("light-mem-mixed");
        std::fs::create_dir_all(&mem_dir).expect("light mem dir");
        let md = format!(
            "---\nProtocol: L1GHT/1.0\nNode: LeaseLeadershipMixed\nState: verified\n\
             Created: {now_ms}\nSource-Agent: agent-mem\n---\n\n\
             # LeaseLeadershipMixed\n\n## LeaseLeadershipMixed\n\n\
             The lease leadership handoff must renew the registry lease before takeover.\n\n\
             [⍂ entity: lease enforcement leadership handoff]\n[𝔻 confidence: 0.9]\n"
        );
        std::fs::write(mem_dir.join("lease_leadership.light.md"), md).expect("write memory");
        super::dispatch_tool(
            &mut state,
            "ingest",
            &serde_json::json!({
                "agent_id": "agent-mem",
                "path": mem_dir.to_string_lossy(),
                "adapter": "light",
                "mode": "merge",
                "namespace": "light",
            }),
        )
        .expect("ingest light memory into mixed graph");

        // 3. north on a task matching the memory — the code corpus matches it too.
        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "northerner",
                "task": "lease enforcement leadership handoff",
            }),
        )
        .expect("north on the mixed graph");

        let memory = out["memory"].as_array().expect("memory array");
        let light: Vec<&serde_json::Value> = memory
            .iter()
            .filter(|e| e.get("kind").and_then(|k| k.as_str()) == Some("light"))
            .collect();
        assert!(
            !light.is_empty(),
            "north.memory must recall the memorized L1GHT claim even when a large code \
             corpus dominates ranking — got memory={memory:?}"
        );
        let hit = light
            .iter()
            .find(|e| e["source_agent"].as_str() == Some("agent-mem"))
            .expect("the memorized claim surfaces carrying its authoring agent");
        assert_eq!(
            hit["source_agent"], "agent-mem",
            "the recalled claim carries its authoring agent as source_agent"
        );
        assert!(
            hit["age_ms"].as_u64().is_some(),
            "the light hit carries an authored age lifted from its provenance"
        );

        // And the honest gap line must NOT claim there is no memory — that was the
        // exact live lie ("No durable memory yet" while the note existed).
        let gaps = out["honest_gaps"].as_array().cloned().unwrap_or_default();
        let claims_no_memory = gaps.iter().any(|g| {
            g.as_str()
                .map(|s| s.contains("No durable memory yet"))
                .unwrap_or(false)
        });
        assert!(
            !claims_no_memory,
            "honest_gaps must not claim 'No durable memory yet' when a memorized claim was \
             recalled — gaps={gaps:?}"
        );
    }

    /// The marker-fragment discriminator: `::tag::` id segment (structural, the
    /// only signal the l1ght_adapter mints on marker nodes) OR a leading marker
    /// glyph (fallback when only the label is at hand). Real claim/section/code
    /// ids and plain labels must NOT be flagged. Field-triage batch A / L28.
    #[test]
    fn is_marker_fragment_flags_markers_not_claims() {
        // Marker nodes — every real marker id carries `::tag::`.
        assert!(super::is_marker_fragment(
            "light::light::tag::note::15::entity-x",
            "⍂ entity: x"
        ));
        assert!(super::is_marker_fragment(
            "light::ns::tag::f::3::confidence-0-90",
            "𝔻 confidence: 0.9"
        ));
        // Glyph fallback when the id is unavailable (empty).
        assert!(super::is_marker_fragment("", "𝔻 evidence: src/foo.rs"));
        assert!(super::is_marker_fragment("", "⟁ depends_on: bar"));
        assert!(super::is_marker_fragment("", "  ⍐ state: leading ws"));
        // Real content — NOT markers.
        assert!(!super::is_marker_fragment(
            "light::light::section::note::topic-1",
            "The doctrine claim that actually matters"
        ));
        assert!(!super::is_marker_fragment(
            "file::m1nd-mcp/src/server.rs::fn::handle_north",
            "handle_north"
        ));
        assert!(!super::is_marker_fragment("", "confidence in the design"));
        assert!(!super::is_marker_fragment("", ""));
    }

    /// RED→GREEN for field-triage batch A (inbox L28): a memorized note plants
    /// `[𝔻 confidence: …]` + `[𝔻 evidence: …]` marker nodes. Pre-fix, north's
    /// memory slice and PageRank anchors surfaced those marker nodes as standalone
    /// rows (2/5 memory + 4/4 anchor slots in the live founder hook). This asserts
    /// the fix end-to-end: (a) the real claim/section still recalls, and (b) NO
    /// memory row and NO anchor row is a marker fragment.
    #[test]
    fn north_excludes_marker_fragments_from_memory_and_anchors() {
        let (temp, mut state) = build_state_populated(false);

        // Plant one memorized note carrying the full marker set (confidence +
        // evidence + a declaration marker), merged via the exact agent-memory path.
        let now_ms = super::now_ms();
        let mem_dir = temp.path().join("light-mem-markerfilter");
        std::fs::create_dir_all(&mem_dir).expect("light mem dir");
        let md = format!(
            "---\nProtocol: L1GHT/1.0\nNode: MarkerFilterCanary\nState: verified\n\
             Created: {now_ms}\nSource-Agent: marker-scout\n---\n\n\
             # MarkerFilterCanary\n\n## MarkerFilterCanary\n\n\
             The marker-filter canary: north memory and anchor slots carry only real \
             claim and section rows, never annotation fragments.\n\n\
             [⍐ state: marker filter canary]\n[𝔻 confidence: 0.9]\n\
             [𝔻 evidence: m1nd-mcp/src/server.rs]\n"
        );
        std::fs::write(mem_dir.join("marker_filter.light.md"), md).expect("write memory");
        super::dispatch_tool(
            &mut state,
            "ingest",
            &serde_json::json!({
                "agent_id": "marker-scout",
                "path": mem_dir.to_string_lossy(),
                "adapter": "light",
                "mode": "merge",
                "namespace": "light",
            }),
        )
        .expect("ingest light memory");

        let out = super::dispatch_tool(
            &mut state,
            "north",
            &serde_json::json!({
                "agent_id": "northerner",
                "task": "marker filter canary claim section rows annotation",
            }),
        )
        .expect("north");

        // A row is a marker fragment iff its node_id/label satisfies the discriminator.
        // INDEPENDENT of the production helper on purpose: the test must judge the
        // OUTPUT for itself (a `::tag::` id or a leading marker glyph), so it fails
        // RED when production filtering is off and passes GREEN when it is on.
        let row_is_marker = |e: &serde_json::Value| -> bool {
            let nid = e
                .get("node_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let label = e
                .get("claim")
                .or_else(|| e.get("label"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            nid.contains("::tag::") || label.trim_start().starts_with(['𝔻', '⟁', '⍂', '⍐', '⍌'])
        };

        // (a) The real claim/section still recalls (no over-filtering).
        let memory = out["memory"].as_array().expect("memory array");
        let light: Vec<&serde_json::Value> = memory
            .iter()
            .filter(|e| e.get("kind").and_then(|k| k.as_str()) == Some("light"))
            .collect();
        assert!(
            !light.is_empty(),
            "the real memorized claim must still recall after marker filtering — memory={memory:?}"
        );

        // (b) NO memory row is a marker fragment.
        let mem_markers: Vec<&serde_json::Value> =
            memory.iter().filter(|e| row_is_marker(e)).collect();
        assert!(
            mem_markers.is_empty(),
            "north.memory must carry NO L1GHT marker fragment rows — leaked={mem_markers:?}"
        );

        // (c) NO anchor row is a marker fragment.
        let anchors = out["context"]["anchors"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let anchor_markers: Vec<&serde_json::Value> =
            anchors.iter().filter(|e| row_is_marker(e)).collect();
        assert!(
            anchor_markers.is_empty(),
            "north.context.anchors must carry NO L1GHT marker fragment rows — leaked={anchor_markers:?}"
        );

        // (d) NO focus_nodes row is a marker fragment.
        let focus = out["context"]["focus_nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let focus_markers: Vec<&serde_json::Value> =
            focus.iter().filter(|e| row_is_marker(e)).collect();
        assert!(
            focus_markers.is_empty(),
            "north.context.focus_nodes must carry NO L1GHT marker fragment rows — leaked={focus_markers:?}"
        );
    }

    /// REAL PROBE: load the repo's actual graph_snapshot.json (~5540 nodes) and
    /// run `seek` for a broad query twice — once unbudgeted, once with a tight
    /// `token_budget` — printing both result counts, the `budget` block, and the
    /// kept labels so the context-budget packing can be SEEN keeping the
    /// top-signal hits and dropping the rest on real data.
    ///
    /// Run with: `cargo test -p m1nd-mcp seek_token_budget_real_snapshot_probe -- --nocapture`
    /// Skips gracefully (printing a note) if the snapshot is not present.
    #[test]
    fn seek_token_budget_real_snapshot_probe() {
        let snapshot = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("graph_snapshot.json"))
            .filter(|p| p.exists());
        let Some(snapshot_path) = snapshot else {
            eprintln!(
                "[seek_token_budget_real_snapshot_probe] graph_snapshot.json not found — skipping"
            );
            return;
        };

        let graph =
            m1nd_core::snapshot::load_graph(&snapshot_path).expect("load real graph_snapshot.json");
        let node_count = graph.nodes.count;

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            registry_dir: Some(runtime_dir.join("registry")),
            runtime_dir: Some(runtime_dir),
            read_only: true,
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(graph, &config, DomainConfig::code())
            .expect("init session from real snapshot");

        let query = "read only";
        let top_k = 40;

        // Baseline: no token_budget.
        let base = super::dispatch_tool(
            &mut state,
            "seek",
            &serde_json::json!({
                "agent_id": "probe",
                "query": query,
                "top_k": top_k,
            }),
        )
        .expect("baseline seek on real snapshot");
        let base_results = base["results"].as_array().cloned().unwrap_or_default();

        // Budgeted: tight ~300-token budget.
        let budget_tokens = 300u64;
        let budgeted = super::dispatch_tool(
            &mut state,
            "seek",
            &serde_json::json!({
                "agent_id": "probe",
                "query": query,
                "top_k": top_k,
                "token_budget": budget_tokens,
            }),
        )
        .expect("budgeted seek on real snapshot");
        let budgeted_results = budgeted["results"].as_array().cloned().unwrap_or_default();

        eprintln!(
            "\n=== seek(query=\"{}\") on REAL graph ({} nodes) ===",
            query, node_count
        );
        eprintln!("BASELINE (no token_budget): {} results", base_results.len());
        eprintln!(
            "  top labels: {:?}",
            base_results
                .iter()
                .take(8)
                .map(|r| r["label"].as_str().unwrap_or("?"))
                .collect::<Vec<_>>()
        );
        eprintln!(
            "BUDGETED (token_budget={}): {} results",
            budget_tokens,
            budgeted_results.len()
        );
        eprintln!("  budget block: {}", budgeted["budget"]);
        eprintln!("  kept labels (score / path):");
        for (i, r) in budgeted_results.iter().enumerate() {
            eprintln!(
                "    {:>2}. {:<48} score={:.4} path={}",
                i + 1,
                r["label"].as_str().unwrap_or("?"),
                r["score"].as_f64().unwrap_or(0.0),
                r["file_path"].as_str().unwrap_or("·"),
            );
        }
        eprintln!("=== end probe ===\n");

        // Baseline absent of a budget block; budgeted carries one.
        assert!(base.get("budget").is_none() || base["budget"].is_null());
        assert!(budgeted["budget"].is_object());

        // Packing must keep fewer than the (larger) baseline and keep the
        // top-ranked prefix.
        assert!(
            budgeted_results.len() <= base_results.len(),
            "budgeted seek must not return more than baseline"
        );
        if !base_results.is_empty() {
            assert!(!budgeted_results.is_empty(), "must keep at least one hit");
            assert_eq!(
                budgeted_results[0]["label"], base_results[0]["label"],
                "budgeted set must keep the same top-ranked hit"
            );
        }

        // budget accounting must be internally consistent.
        let b = &budgeted["budget"];
        let kept = b["kept"].as_u64().unwrap();
        let dropped = b["dropped"].as_u64().unwrap();
        assert_eq!(kept as usize, budgeted_results.len());
        assert_eq!(kept + dropped, base_results.len() as u64);
        assert_eq!(b["requested_tokens"].as_u64().unwrap(), budget_tokens);
    }

    // -----------------------------------------------------------------------
    // am_i_stale — agent-first on-disk staleness perception
    // -----------------------------------------------------------------------

    /// Ingest a temp repo with a single file and return (temp, state, abs_path).
    /// After ingest, `state.file_inventory` holds the recorded sha256 baseline.
    fn ingest_single_file(contents: &str) -> (tempfile::TempDir, SessionState, std::path::PathBuf) {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        let file = repo.join("src/target.py");
        std::fs::write(&file, contents).expect("write target file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "stale-agent".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
                project_root: None,
            },
        )
        .expect("ingest single file");

        // The ingest pipeline canonicalizes paths (on macOS /var → /private/var),
        // so resolve the test's path the same way to address the file the tool
        // will actually see. Mirrors what a caller passing a real on-disk path
        // gets, and what `am_i_stale`'s own canonicalization-aware match handles.
        let file = std::fs::canonicalize(&file).unwrap_or(file);

        // Inventory must record the file with a hash baseline.
        assert!(
            state
                .file_inventory
                .values()
                .any(|e| e.file_path == file.to_string_lossy() && e.sha256.is_some()),
            "ingest must record the target file with a sha256 baseline"
        );
        (temp, state, file)
    }

    #[test]
    fn am_i_stale_detects_changed_file() {
        let (_temp, mut state, file) = ingest_single_file("def target():\n    return 'original'\n");

        // Rewrite the file on disk with different content (the change m1nd can't see).
        std::fs::write(&file, "def target():\n    return 'MUTATED'\n").expect("rewrite file");

        let out = super::handle_am_i_stale(
            &mut state,
            &serde_json::json!({
                "agent_id": "stale-agent",
                "files": [file.to_string_lossy()],
            }),
        )
        .expect("am_i_stale must succeed");

        assert_eq!(out["source"], "explicit_files");
        assert_eq!(out["checked"], serde_json::json!(1));
        let stale = out["stale"].as_array().expect("stale array");
        assert_eq!(stale.len(), 1, "the rewritten file must be flagged stale");
        assert_eq!(stale[0]["path"], file.to_string_lossy().as_ref());
        assert_eq!(stale[0]["reason"], "changed");
        assert!(
            out["fresh"].as_array().unwrap().is_empty(),
            "no file should be fresh after the rewrite"
        );
    }

    #[test]
    fn am_i_stale_detects_missing_file() {
        let (_temp, mut state, file) = ingest_single_file("def target():\n    return 'original'\n");

        // Delete the file on disk after ingest.
        std::fs::remove_file(&file).expect("delete file");

        let out = super::handle_am_i_stale(
            &mut state,
            &serde_json::json!({
                "agent_id": "stale-agent",
                "files": [file.to_string_lossy()],
            }),
        )
        .expect("am_i_stale must succeed");

        let stale = out["stale"].as_array().expect("stale array");
        assert_eq!(stale.len(), 1, "the deleted file must be flagged stale");
        assert_eq!(stale[0]["reason"], "missing");
    }

    #[test]
    fn am_i_stale_reports_fresh() {
        let (_temp, mut state, file) =
            ingest_single_file("def target():\n    return 'untouched'\n");

        // Do NOT touch the file — it must come back fresh.
        let out = super::handle_am_i_stale(
            &mut state,
            &serde_json::json!({
                "agent_id": "stale-agent",
                "files": [file.to_string_lossy()],
            }),
        )
        .expect("am_i_stale must succeed");

        assert_eq!(out["checked"], serde_json::json!(1));
        assert!(
            out["stale"].as_array().unwrap().is_empty(),
            "an untouched file must not be stale"
        );
        let fresh = out["fresh"].as_array().expect("fresh array");
        assert_eq!(fresh.len(), 1, "the untouched file must be fresh");
        assert_eq!(fresh[0], file.to_string_lossy().as_ref());
    }

    #[test]
    fn am_i_stale_defaults_to_coverage_session() {
        let (_temp, mut state, file) = ingest_single_file("def target():\n    return 'original'\n");

        // Record a coverage session for the agent that has visited the file,
        // then mutate the file so the default working set should flag it.
        state.note_coverage(
            "stale-agent",
            "view",
            [file.to_string_lossy().to_string()],
            std::iter::empty::<String>(),
        );
        std::fs::write(&file, "def target():\n    return 'changed-under-agent'\n")
            .expect("rewrite file");

        // No `files`/`nodes` → must default to the coverage session's visited files.
        let out = super::handle_am_i_stale(
            &mut state,
            &serde_json::json!({ "agent_id": "stale-agent" }),
        )
        .expect("am_i_stale must succeed");

        assert_eq!(
            out["source"], "coverage_session",
            "with no explicit targets it must default to the coverage session"
        );
        assert_eq!(out["checked"], serde_json::json!(1));
        let stale = out["stale"].as_array().expect("stale array");
        assert_eq!(
            stale.len(),
            1,
            "the touched-then-changed file must be stale"
        );
        assert_eq!(stale[0]["reason"], "changed");
    }

    #[test]
    fn am_i_stale_empty_when_no_targets_and_no_session() {
        let (_temp, mut state, _file) = ingest_single_file("def target():\n    return 'x'\n");

        // A different agent with no coverage session and no explicit targets.
        let out = super::handle_am_i_stale(
            &mut state,
            &serde_json::json!({ "agent_id": "ghost-agent" }),
        )
        .expect("am_i_stale must succeed");

        assert_eq!(out["source"], "empty");
        assert_eq!(out["checked"], serde_json::json!(0));
        assert!(out["notes"].is_array(), "empty result must carry a note");
    }

    #[test]
    fn am_i_stale_is_not_read_only_denied() {
        use super::read_only_denied;
        assert!(
            !read_only_denied("am_i_stale", &serde_json::json!({})),
            "am_i_stale only reads disk + inventory — it must be allowed in read-only attach"
        );
        // The prefix-normalized forms must also be allowed.
        assert!(!read_only_denied("m1nd_am_i_stale", &serde_json::json!({})));
        assert!(!read_only_denied("m1nd.am_i_stale", &serde_json::json!({})));
    }

    /// REAL PROBE: ingest a SMALL real directory (`m1nd-core/src`), record the
    /// inventory, mutate ONE real file's content on disk, call `am_i_stale`, and
    /// print the stale/fresh classification so we can SEE it catch the real
    /// change. Restores the file afterward so the working tree stays clean.
    ///
    /// Run with: `cargo test -p m1nd-mcp am_i_stale_real_probe -- --nocapture`
    #[test]
    fn am_i_stale_real_probe() {
        // Locate m1nd-core/src relative to the crate dir (repo-root/m1nd-core/src).
        let real_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("m1nd-core").join("src"))
            .filter(|p| p.is_dir());
        let Some(real_dir) = real_dir else {
            eprintln!("[am_i_stale_real_probe] m1nd-core/src not found — skipping");
            return;
        };

        let (_temp, mut state) = build_state();
        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: real_dir.to_string_lossy().to_string(),
                agent_id: "real-probe".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
                project_root: None,
            },
        )
        .expect("ingest real m1nd-core/src");

        let inv_count = state.file_inventory.len();
        eprintln!(
            "\n=== am_i_stale REAL PROBE: ingested {} files from {} ===",
            inv_count,
            real_dir.display()
        );
        assert!(
            inv_count >= 3,
            "expected several real files in m1nd-core/src"
        );

        // Pick three real ingested files: one to mutate, two left untouched.
        let mut paths: Vec<String> = state
            .file_inventory
            .values()
            .map(|e| e.file_path.clone())
            .collect();
        paths.sort();
        let victim = paths[0].clone();
        let untouched_a = paths.get(1).cloned().expect("a second real file");
        let untouched_b = paths.get(2).cloned().expect("a third real file");

        // Snapshot the victim's real bytes, mutate, and ALWAYS restore.
        let original_bytes = std::fs::read(&victim).expect("read victim file");
        let mut mutated = original_bytes.clone();
        mutated.extend_from_slice(b"\n// am_i_stale real probe touch\n");
        std::fs::write(&victim, &mutated).expect("mutate victim file");

        // Run the probe inside a closure so we can restore on any path.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let out = super::handle_am_i_stale(
                &mut state,
                &serde_json::json!({
                    "agent_id": "real-probe",
                    "files": [victim.clone(), untouched_a.clone(), untouched_b.clone()],
                }),
            )
            .expect("am_i_stale on real files");

            eprintln!("source : {}", out["source"]);
            eprintln!("checked: {}", out["checked"]);
            eprintln!("summary: {}", out["summary"].as_str().unwrap_or(""));
            eprintln!("STALE:");
            for s in out["stale"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
                eprintln!(
                    "  - {} [{}]",
                    s["path"].as_str().unwrap_or(""),
                    s["reason"].as_str().unwrap_or("")
                );
            }
            eprintln!("FRESH:");
            for f in out["fresh"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
                eprintln!("  - {}", f.as_str().unwrap_or(""));
            }
            eprintln!("=== end real probe ===\n");

            // The mutated file MUST be flagged changed; the others MUST be fresh.
            let stale_paths: Vec<String> = out["stale"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|s| s.get("path").and_then(|p| p.as_str()).map(String::from))
                .collect();
            assert!(
                stale_paths.contains(&victim),
                "the mutated real file must be flagged stale"
            );
            let fresh_paths: Vec<String> = out["fresh"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|f| f.as_str().map(String::from))
                .collect();
            assert!(
                fresh_paths.contains(&untouched_a) && fresh_paths.contains(&untouched_b),
                "the untouched real files must be fresh, not flagged"
            );
        }));

        // ALWAYS restore the real file so the working tree is clean.
        std::fs::write(&victim, &original_bytes).expect("restore victim file");
        let restored = std::fs::read(&victim).expect("re-read restored file");
        assert_eq!(
            restored, original_bytes,
            "victim file must be restored byte-for-byte"
        );

        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    // ---- Broad L1GHT recall freshest-first ordering (spine-north #10) --------

    #[test]
    fn broad_light_recall_sorts_dated_claims_before_undated_freshest_first() {
        // Regression: the broad (non-task-scoped) memory fallback sorted
        // `Option<u64>` ages with a bare key, and Rust orders `None < Some(_)`, so
        // an UNDATED legacy claim outranked every dated one — oldest/unknown-first
        // where freshest-first is promised.
        //
        // Fixture mirrors seek hits by (label, authored_ms_ago); smaller age =
        // fresher. Expected freshest-first order: fresh(1h) → old(30d) → undated.
        let fresh_ms = 60 * 60 * 1000u64; // 1 hour old
        let old_ms = 30 * 24 * 60 * 60 * 1000u64; // 30 days old
        let mut hits: Vec<(&str, Option<u64>)> = vec![
            ("undated-legacy", None),
            ("old-dated", Some(old_ms)),
            ("fresh-dated", Some(fresh_ms)),
        ];

        hits.sort_by_key(|(_, age)| light_recall_freshness_key(*age));

        let order: Vec<&str> = hits.iter().map(|(label, _)| *label).collect();
        assert_eq!(
            order,
            vec!["fresh-dated", "old-dated", "undated-legacy"],
            "dated claims must lead (freshest first); undated must trail"
        );

        // Guard against the specific inversion: the undated claim must NOT be
        // first, and the bare-key order (which WOULD put it first) must differ.
        assert_ne!(order.first(), Some(&"undated-legacy"));
        let mut bare = [
            ("undated-legacy", None::<u64>),
            ("old-dated", Some(old_ms)),
            ("fresh-dated", Some(fresh_ms)),
        ];
        bare.sort_by_key(|(_, age)| *age); // the OLD, inverted key
        assert_eq!(
            bare.first().map(|(l, _)| *l),
            Some("undated-legacy"),
            "sanity: the bare key really does float the undated claim to the front"
        );
    }

    // -----------------------------------------------------------------------
    // P1 presences — the beat by traffic, its throttle, its fail-open, and the
    // north collision gap on BOTH colliding sessions' packets.
    // -----------------------------------------------------------------------

    fn presence_dir_of(state: &SessionState) -> std::path::PathBuf {
        state.instance.registry_root().join("presences")
    }

    fn read_presence_file(
        state: &SessionState,
        agent: &str,
        brain: &str,
    ) -> Option<crate::presence::PresenceRecord> {
        let path = presence_dir_of(state).join(format!(
            "{}.json",
            crate::presence::stable_presence_id(agent, brain)
        ));
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Registration by traffic + the throttle: the FIRST tracked call writes the
    /// sidecar; an immediate second call is THROTTLED (no second write); a
    /// changed signal (an observed mutation) clears the throttle so the next
    /// call carries it promptly.
    #[test]
    fn presence_beat_registers_by_traffic_and_throttles() {
        let (_temp, mut state) = build_state();
        let brain = "/wt/beat-brain";
        state.workspace_root = Some(brain.to_string());

        // 1. Registration by traffic — the seam call is track_agent.
        state.track_agent("beat-agent");
        let first = read_presence_file(&state, "beat-agent", brain)
            .expect("first tracked call writes the presence sidecar");
        assert_eq!(first.agent_id, "beat-agent");
        assert_eq!(first.brain, brain);
        assert_eq!(first.query_count, 1);

        // 2. Throttle — an immediate second call updates memory, not disk.
        state.track_agent("beat-agent");
        let after = read_presence_file(&state, "beat-agent", brain).expect("sidecar persists");
        assert_eq!(
            after.query_count, 1,
            "an immediate re-beat is throttled: the sidecar still carries the first write"
        );

        // 3. A changed signal clears the throttle: the observed mutation rides
        //    the very next tracked call.
        state.note_mutation_observed("beat-agent");
        state.track_agent("beat-agent");
        let mutated = read_presence_file(&state, "beat-agent", brain).expect("sidecar persists");
        assert!(
            mutated.mutation.observed_at_ms.is_some(),
            "the observed-mutation stamp must ride the next beat promptly"
        );
        assert_eq!(
            mutated.query_count, 3,
            "the forced beat carries fresh counters"
        );
    }

    /// FAIL-OPEN: a broken sidecar (the presences dir path occupied by a FILE)
    /// must never break the tool call — track_agent still succeeds and the
    /// in-memory session still advances.
    #[test]
    fn presence_beat_fails_open_when_sidecar_write_breaks() {
        let (_temp, mut state) = build_state();
        state.workspace_root = Some("/wt/failopen-brain".to_string());
        // Occupy the presences DIR path with a regular file so create_dir_all fails.
        let dir = presence_dir_of(&state);
        std::fs::create_dir_all(dir.parent().expect("registry root")).expect("mk registry");
        std::fs::write(&dir, b"not a directory").expect("plant the blocker");

        state.track_agent("unlucky-agent");

        let session = state
            .sessions
            .get("unlucky-agent")
            .expect("session tracked");
        assert_eq!(
            session.query_count, 1,
            "the tool call's tracking must survive a broken sidecar (fail-open)"
        );
    }

    /// The P1 gate's packet law: an arranged collision surfaces in the north
    /// honest_gaps of BOTH colliding sessions — and NOT in a bystander's packet.
    #[test]
    fn north_collision_gap_rides_both_colliding_packets() {
        let (_temp, mut state) = build_state();
        let brain = "/wt/north-brain";
        state.workspace_root = Some(brain.to_string());
        let registry = state.instance.registry_root();
        let now = crate::util::now_ms();

        let seed = |agent: &str, caller: &str| {
            let record = crate::presence::PresenceRecord {
                schema: crate::presence::PRESENCE_SCHEMA.to_string(),
                presence_id: crate::presence::stable_presence_id(agent, brain),
                agent_id: agent.to_string(),
                brain: brain.to_string(),
                caller_root: Some(caller.to_string()),
                kind: None,
                theme: None,
                worktree: None,
                working_set: Vec::new(),
                task_ref: None,
                mutation: crate::presence::MutationSignal {
                    observed_at_ms: Some(now),
                    declared_intent: None,
                },
                first_seen_ms: now,
                last_beat_ms: now,
                query_count: 1,
                ttl_ms: crate::presence::PRESENCE_TTL_MS,
            };
            crate::presence::write_presence(&registry, &record).expect("seed presence");
        };
        // Two mutating hands in ONE worktree; a third in its own worktree.
        seed("hand-a", "/wt/shared");
        seed("hand-b", "/wt/shared");
        seed("hand-c", "/wt/isolated");

        let gaps_of = |state: &mut SessionState, agent: &str| -> Vec<String> {
            let north = super::dispatch_tool(
                state,
                "north",
                &serde_json::json!({ "agent_id": agent, "task": "collision probe" }),
            )
            .expect("north packet");
            north["honest_gaps"]
                .as_array()
                .map(|gaps| {
                    gaps.iter()
                        .filter_map(|g| g.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };

        let a_gaps = gaps_of(&mut state, "hand-a");
        assert!(
            a_gaps
                .iter()
                .any(|g| g.starts_with("COLLISION:") && g.contains("hand-b")),
            "hand-a's packet must carry the collision gap naming hand-b: {a_gaps:?}"
        );
        let b_gaps = gaps_of(&mut state, "hand-b");
        assert!(
            b_gaps
                .iter()
                .any(|g| g.starts_with("COLLISION:") && g.contains("hand-a")),
            "hand-b's packet must carry the collision gap naming hand-a: {b_gaps:?}"
        );
        let c_gaps = gaps_of(&mut state, "hand-c");
        assert!(
            !c_gaps.iter().any(|g| g.starts_with("COLLISION:")),
            "the isolated-worktree hand must NOT carry a collision gap: {c_gaps:?}"
        );
    }
}
