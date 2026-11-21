//! The doctrine behind the server instructions, served on pull.
//!
//! Every session pays for the MCP `instructions` before its first call, so they
//! carry only the operating card (`server::M1ND_INSTRUCTIONS`). The doctrine
//! they used to push in full lives here, one card per topic, and
//! `help {topic: "<id>"}` serves it when an agent asks: pull, never push. The
//! bodies are the former instruction paragraphs, moved without rewording.
//!
//! Pinned by `server::tests::every_help_topic_is_served_by_help` and
//! `server::tests::the_instructions_name_every_help_topic`.

/// One doctrine card `help {topic}` can open.
#[derive(Clone, Copy, Debug)]
pub struct HelpTopic {
    /// The id an agent passes as `topic`.
    pub id: &'static str,
    pub title: &'static str,
    pub body: &'static str,
}

/// Every doctrine card, in the order the instructions list them.
pub const HELP_TOPICS: &[HelpTopic] = &[
    HelpTopic {
        id: "reception",
        title: "FIRST CONTACT — heed reception",
        body: "\
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
(`m1nd init --birth <repo>`) is the only door.",
    },
    HelpTopic {
        id: "menu",
        title: "THE MENU IS A CORE, NOT THE SURFACE",
        body: "\
**THE MENU IS A CORE, NOT THE SURFACE.** By default `tools/list` advertises about fifteen verbs \
— the handful six weeks of real traffic showed agents actually reach for, plus the host-binding \
floor. That is the shop window, not the shop: additional verbs are registered but not advertised, \
and server dispatch accepts their names subject to authority checks. Some hosts require tools to be listed. **`help` is the door**: \
`help(intent)` or `help(stage)` routes you to the right verb for what you are doing, and \
`help(tool_name)` returns any verb's full schema — `help` catalogs the WHOLE registry, never just \
the menu, so it can name and explain a verb you cannot see. `health.tool_surface_contract` \
reports the live numbers (`advertised_tool_count`, `hidden_tool_count`, \
`full_registry_tool_count`). An operator who wants the whole list in the menu sets \
`M1ND_TOOL_TIER=full` on the server. Never conclude a capability is missing because it is not in \
the list — ask `help` first.",
    },
    HelpTopic {
        id: "authority",
        title: "ADVERTISED ≠ CALLABLE — the authority floors",
        body: "\
**ADVERTISED ≠ CALLABLE — the authority floors.** Bootstrap is not the only closed door. Every \
semantic action carries an M1ND-10 authority floor and generic MCP/REST dispatch admits only \
`ORDINARY`; a verb above it (`SCOPED_GRANT_A2` / `POSITIVE_SOVEREIGN` / `SERVICE_IDENTITY`) \
refuses with `generic_action_authority_required`. No payload shape, capability claim or retry \
lifts that — only an exact typed G2/G3 consumer (an authority lease), and none is installed for \
those actions yet. 40 of the registry's verbs are affected today, INCLUDING ones this \
document teaches: `learn`, `debrief`, `promote`, `calibrate_predict` / `calibrate_envelope`, \
`ghost_edges`, `runtime_overlay`, `apply` / `apply_batch` / `edit_commit`, `daemon_start` / \
`_stop` / `_tick`, `auto_ingest_start` / `_stop` / `_tick` (their `_status` reads stay open), \
the `xray_*` commit branch, `boot_memory` set/delete, \
`mission_close write_light_memory:true`, and every system-blocks writer. Every one of them \
carries the mark in its own description — `POLICY-DISABLED (authority floor …)` — in the menu and \
in `help` alike. READ THAT before planning \
around a verb, and do not spend turns retrying a refusal. Still ORDINARY and genuinely callable: \
every read, `memorize`, `delegate`, `trail_save`, the perspective family, plain `mission_start` / \
`mission_event` / `mission_verify` / `mission_handoff` / `mission_close`.",
    },
    HelpTopic {
        id: "north",
        title: "PRE-ORIENT — never start cold",
        body: "\
Call `north(task)` FIRST, before reading or editing anything. The default answer is lean, \
about 1 KB: `state`, one word, and `focus`, one `path:lines symbol` line per focus node in \
activation order (`lines` is `start-end` when the graph recorded the symbol's span, `start` \
when it recorded only its first line; a file node reads `(file)`). On a healthy graph the \
state is the sufficiency verdict: `sufficient` (the focus set holds the relevant context — stop \
gathering), `gathering` (relevant context did not fit — raise `top_k` or refine the task) or \
`saturated` (the task is weakly represented here). Otherwise it names the trouble: \
`mismatch`, `no_brain`, `needs_ingest` or `degraded`. Memory rows, the `landing_bell` and \
`graph_drift` ride the lean answer only when present.

`detail: true` returns the full packet: binding trust (`trust_mode`; the repair travels with it \
when degraded), task context (focus nodes + PageRank anchors), prior cross-session memory (each \
claim with its real age + author — absent, never faked, when unknown), a sufficiency signal, one \
`next_move`, `honest_gaps` (what m1nd does NOT yet know), `human_view` and `code`. An abnormal \
answer — `caller_root_mismatch`, no project brain, `needs_ingest`, a binding that is not full \
trust, `graph_drift`, or another mutating session on the same brain — returns the full packet \
without being asked, because then the diagnostics ARE the answer. If it returns \
`needs_ingest`, do not call generic \
`ingest` to REPLACE or MERGE — those stay policy-disabled. Read the packet's \
`next_move`, which names WHICH repair is live, and SAY IT rather than retrying a \
refusal. A repo with NO brain gets one from the HUMAN's one-time ceremony \
`m1nd init --birth <repo>`: OFFER that exact command and stop. It ingests the repo for \
real and reports the node and edge counts it produced; no agent can run it, because the \
origin stamp exists only inside that CLI ingress. For an existing brain, \
use the exact authority flow plus `external_mutation_service`; under \
`caller_root_mismatch`, creating or rebinding a brain from the wire remains unavailable. \
The ONE ingest you CAN run is \
`ingest` with mode=refresh — it re-scans a root this brain already declared, and only \
when your caller root is exactly that root; it refuses rather than shrink the graph. \
`north` \
composes trust_selftest + orient + boot_memory + focus — reach for the pieces directly \
only when you need just one.

**north hands back the CODE, not just the address.** With `detail: true` (or `code: true` on \
the lean answer) the packet carries `code`: the real source of its top focus nodes — always a \
symbol's own lines, never a file head — for up to 3 distinct files inside a ~2,000-char budget. \
`surgical_context` serves a symbol it finds at the node's own first line; otherwise `view` \
serves the lines the graph recorded for the node (a Go method, a document section). A file \
whose only focus node is the file itself gets no slice and counts in `files_omitted`. READ IT \
BEFORE YOU GO FETCH ANYTHING. Every slice states `file_path`, its line span, `total_lines` vs \
`lines_returned` and `truncated`, so you always know what you are missing: when a slice is \
truncated, `surgical_context(file_path, symbol)` or `view(file_path, offset, limit)` serves \
the rest. Widen the first call with `code_budget_chars` instead of paying a round-trip; pass \
`code: false` for the full packet without code. The payload is ABSENT — not empty — wherever \
the packet already says the graph cannot answer for your repo: `needs_ingest`, an empty graph, \
no project brain, and `caller_root_mismatch` (under a mismatch those file bytes would be \
ANOTHER repo's source, which is the most convincing form of context poisoning there is).",
    },
    HelpTopic {
        id: "recall",
        title: "Memory is PULL, never PUSH (the medulla law)",
        body: "\
**Memory is PULL, never PUSH (the medulla law).** Your default recall beat carries exactly \
TWO feeds: your own project brain's memory + the shared `medulla` (promoted/doctrine claims). \
Another repo's private claim NEVER appears in your beat — it can only reach you if it was \
promoted to the medulla. Every recall row is labeled with its `tier` (`project` | `medulla`) \
and `origin_brain` (WHICH brain it came from), so you always know a claim's provenance. \
Need to inspect across projects? Pass `tier` on `seek`/`north`/`boot_memory`: `project` (your \
store only), `medulla` (doctrine only), `project+medulla` (the default), or `all-brains` — the \
EXPLICIT cross-project fan-out that reads every hosted brain, each hit labeled by `origin_brain`. \
`all-brains` is one argument away and never ambient; don't reach for it unless you actually need \
another project's knowledge.",
    },
    HelpTopic {
        id: "verdicts",
        title: "ACT ON VERDICTS — respect qualification, don't override it",
        body: "\
Retrieval and prediction return advisory verdicts; obey their limits:
- **`act` / `reverify` / `abstain`** — `abstain` means unqualified or insufficient \
evidence: do NOT guess past it. Existing v1 calibration rows are \
`legacy_evidence_unqualified`: `predict` abstains, while the `seek` envelope stays \
`reverify` or `unprovable`, with its score preserved and no current calibration receipt. \
`calibrate_predict` and `calibrate_envelope` measure diagnostic aggregates only; \
they cannot arm `act`, and their existing authority restrictions still apply.
- **`why` answers carry a `closure` verdict** — `blocked` means the path rests on an \
unresolved (guessed/dropped) edge: verify that edge before you rely on the path.
- **`seek` carries a `trust_envelope` + a sufficiency stop-signal** — `sufficient` \
means stop gathering; `gathering`/`saturated` mean widen or refine.
- **`trust_band: insufficient_evidence` means NO evidence — not medium risk.** \
It is the honest cold-start answer, distinct from low/medium/high risk.",
    },
    HelpTopic {
        id: "capture",
        title: "POST-CAPTURE — leave the graph warmer than you found it",
        body: "\
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
`brain_bootstrap_consumer_not_installed`; it never fabricates an executable repair call.",
    },
    HelpTopic {
        id: "promotion",
        title: "PROMOTION — the audited crossing",
        body: "\
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
never destroy).",
    },
    HelpTopic {
        id: "telemetry",
        title: "FIELD TELEMETRY — report, never detour",
        body: "\
Then leave ONE field-telemetry signal and keep working (report, never detour): when a \
retrieval was right or wrong, `learn(correct|wrong)` (relevance only; ambiguous partial refuses); when m1nd ITSELF misbehaves \
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
`GET /api/mailbox?brain=<root>`. These are CLI/REST surfaces — NOT MCP tools (never in the loop).",
    },
    HelpTopic {
        id: "delegate",
        title: "DELEGATE — hand a grounded packet down, debrief the return",
        body: "\
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
surface, so skipping it wastes knowledge; do it.",
    },
    HelpTopic {
        id: "missions",
        title: "WORK RUNS INSIDE — the burst wears the wire",
        body: "\
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
`mission_close` closes a trail, it never colors a block).",
    },
    HelpTopic {
        id: "soul",
        title: "THE SOUL — trust the handoff by a receipt, not by faith",
        body: "\
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
pass `soul_check {verify_curator_report: <report>}` run by a DIFFERENT agent — grader ≠ author.",
    },
    HelpTopic {
        id: "write_surface",
        title: "THE WRITE SURFACE — the candidate map & missions (RATIFY IS HUMAN)",
        body: "\
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
block — only `receipt_import` does — and `landed` is reserved for a confirmed imported receipt.",
    },
    HelpTopic {
        id: "voice",
        title: "THE M1ND VOICE — rendering `human_view` (the card law)",
        body: "\
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
savings (G1).",
    },
    HelpTopic {
        id: "verbs",
        title: "SECONDARY VERBS (one line each)",
        body: "\
- `seek(query)` / `focus(task)` — budgeted retrieval; carry the trust + sufficiency signals above.
- `impact(node)` — directional blast radius before a change; `why(a,b)` — the load-bearing path between two nodes.
- `trust_selftest` / `recovery_playbook` — run when trust looks off or retrieval is blocked; `doctor` for deeper host-surface/graph diagnosis.
- `am_i_stale(claim)` — check BEFORE editing on the strength of remembered/cached knowledge.
- `soul_check` / `soul_read` — verify (freshness receipt) / pull the project's PATHOS handoff soul.
- `coverage_session` — surface the blind spots in what you have and haven't looked at this session.
- `ingest` — `replace`/`merge` are policy-disabled and must not be called. `ingest` with mode=refresh is the one exception: re-scan a root this brain already declared, from exactly that root.
- `external_mutation_service` — governed elevated graph ingest for an existing brain after the exact authority flow; it does not create project brains.",
    },
];

/// The topic `requested` names, accepting `-` or a space for `_` and any case.
pub fn find(requested: &str) -> Option<&'static HelpTopic> {
    let wanted = requested
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_");
    HELP_TOPICS.iter().find(|topic| topic.id == wanted)
}

/// Every topic id, in card order.
pub fn ids() -> Vec<&'static str> {
    HELP_TOPICS.iter().map(|topic| topic.id).collect()
}

/// One card as `help` renders it.
pub fn render(topic: &HelpTopic) -> String {
    format!("{}\n\n{}\n", topic.title, topic.body)
}

/// The list `help` answers with when a topic is missing or unknown.
pub fn render_index() -> String {
    let mut out = String::from("help topics: pass one as help {topic: \"<id>\"}\n");
    for topic in HELP_TOPICS {
        out.push_str(&format!("  - {}: {}\n", topic.id, topic.title));
    }
    out
}
