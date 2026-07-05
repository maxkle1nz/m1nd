/*
 * Hall semantics — the projects-area honesty rules as code (HUMAN-LAYER-PRD §4A).
 *
 * The Hall renders the brains the owner reports (§4A.3) and offers the calm
 * two-step delete (§4A.4). This module is the pure, DOM-free heart of both:
 * liveness → a calm matte band, dormant-aware freshness, the exact-name match
 * that is the floor of a destructive call, and count-honesty (absent, never
 * zero). Like softProof.ts it returns band/enum values — it names NO violet
 * (the Hall carries no abstain surface; violet-lint keeps it clean).
 *
 * INV-10: every value here traces to an owner-reported field; absence is
 * absence, never an invented number.
 */
import type { InstanceRegistryEntry, InstanceSelfResponse } from '../types';

// ── Liveness band (PRD §4A.3 card anatomy: the liveness dot) ──────────────────
// sage = live · unfired grey = dormant · ochre = stale heartbeat · brick = hard
// failure. Matte, never an alarm. Reuses the SOFT PROOF verdict/state tokens.

export type LivenessBand = 'live' | 'dormant' | 'stale' | 'failure';

export interface LivenessStyle {
  /** CSS color (var + fallback) for the dot. No violet — the Hall is not abstain. */
  color: string;
  /** Plain, calm label for the tooltip. */
  label: string;
}

export const LIVENESS_STYLE: Record<LivenessBand, LivenessStyle> = {
  live: { color: 'var(--verdict-act, #6fa287)', label: 'live' },
  dormant: { color: 'var(--state-unverified, #b8b2a8)', label: 'dormant — not running' },
  stale: { color: 'var(--verdict-reverify, #c89b3c)', label: 'stale heartbeat' },
  failure: { color: 'var(--state-failure, #b0563b)', label: 'stopped with an error' },
};

/**
 * Resolve a registry entry's liveness band from its owner-reported fields.
 * Precedence: a hard failure status wins; then a live owner; then a stale
 * heartbeat; otherwise the honest "dormant" (unfired grey — not running).
 */
export function livenessBand(entry: {
  owner_live?: boolean | null;
  stale?: boolean;
  status?: string;
  brain_kind?: string | null;
}): LivenessBand {
  // A project brain has no process status of its own (it lives in the owner):
  // it is simply present in the owner or not — never "stale"/"failed"/"crashed",
  // which are OWNER-process states. Present → a calm live dot.
  if (isProjectBrain(entry)) return 'live';
  const status = (entry.status ?? '').toLowerCase();
  if (status === 'failed' || status === 'error' || status === 'crashed') return 'failure';
  if (entry.owner_live === true) return entry.stale ? 'stale' : 'live';
  if (entry.stale || status === 'stale') return 'stale';
  return 'dormant';
}

// ── Freshness — dormant-aware, never faked (PRD §4A.3) ────────────────────────

/**
 * "last seen 3 h ago" / "just now" from a heartbeat ms. A live entry reads
 * "seen just now"; a dormant one reads how long since its last heartbeat. Never
 * emits "now" for a real age gap (INV-04 discipline carried to the Hall).
 */
export function lastSeenPhrase(lastHeartbeatMs: number | null | undefined, nowMs = Date.now()): string {
  if (lastHeartbeatMs == null) return 'last seen unknown';
  const delta = Math.max(0, nowMs - lastHeartbeatMs);
  const s = Math.floor(delta / 1000);
  if (s < 5) return 'seen just now';
  if (s < 60) return `last seen ${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `last seen ${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `last seen ${h}h ago`;
  const d = Math.floor(h / 24);
  return `last seen ${d}d ago`;
}

/** "persisted 2 m ago" from a seconds-ago count (self only). null → honest absence. */
export function persistedPhrase(secsAgo: number | null | undefined): string | null {
  if (secsAgo == null) return null;
  const s = Math.max(0, Math.floor(secsAgo));
  if (s < 60) return `persisted ${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `persisted ${m}m ago`;
  const h = Math.floor(m / 60);
  return `persisted ${h}h ago`;
}

/** "born 3 days ago" from a started-at ms. null → honest absence. */
export function bornPhrase(startedAtMs: number | null | undefined, nowMs = Date.now()): string | null {
  if (startedAtMs == null) return null;
  const delta = Math.max(0, nowMs - startedAtMs);
  const d = Math.floor(delta / (24 * 60 * 60 * 1000));
  if (d >= 1) return `born ${d} ${d === 1 ? 'day' : 'days'} ago`;
  const h = Math.floor(delta / (60 * 60 * 1000));
  if (h >= 1) return `born ${h} ${h === 1 ? 'hour' : 'hours'} ago`;
  const m = Math.floor(delta / (60 * 1000));
  return `born ${m} ${m === 1 ? 'minute' : 'minutes'} ago`;
}

// ── Repo basename + short path (the card name; the type-the-name target) ──────

/** The repo basename — the card's name and the delete flow's confirm target. */
export function repoBasename(workspaceRoot: string): string {
  const parts = workspaceRoot.split('/').filter(Boolean);
  return parts.length > 0 ? parts[parts.length - 1] : workspaceRoot;
}

/**
 * The brain's PROJECT name (HUMAN-LAYER-PRD §4A.3, Brain Chip law). The server
 * now resolves `display_name` = the repo basename ("m1nd", "project-b"),
 * never the runtime dir ("claude") nor its `agent-memory` sidecar. This is the
 * ONE name source for cards, the receipt, the chip, and the delete confirm
 * target — so they can never disagree. Falls back to `repoBasename(workspace)`
 * ONLY for a legacy entry the server did not enrich (pre-#261 owners); the
 * naming-guard test forbids that fallback from ever being a plumbing name while
 * a project_root exists.
 */
export function brainDisplayName(entry: {
  display_name?: string | null;
  workspace_root: string;
}): string {
  const name = entry.display_name?.trim();
  return name && name.length > 0 ? name : repoBasename(entry.workspace_root);
}

/** The brain's project path — the card's path line. Prefers the server-resolved
 *  `project_root` (the real repo), falling back to `workspace_root` for a legacy
 *  unenriched entry. */
export function brainProjectPath(entry: {
  project_root?: string | null;
  workspace_root: string;
}): string {
  const root = entry.project_root?.trim();
  return root && root.length > 0 ? root : entry.workspace_root;
}

/** ".../a/b/c" — the full-path-on-hover short form. */
export function shortPath(path: string): string {
  const parts = path.split('/').filter(Boolean);
  return parts.length <= 3 ? path : `.../${parts.slice(-3).join('/')}`;
}

// ── entry_base_url (PRD §4A.4 Open a live sibling; instance_registry.rs:645) ──

/** Loopback base URL for a live sibling that serves its own UI; null if not bound. */
export function entryBaseUrl(entry: {
  bind?: string | null;
  port?: number | null;
}): string | null {
  if (!entry.bind || !entry.port) return null;
  const host = entry.bind === '0.0.0.0' ? '127.0.0.1' : entry.bind;
  return `http://${host}:${entry.port}`;
}

// ── The REST brain selector capability (PRD §4A.9.5; the Open feature-detect) ──
// Open enables ONLY when the owner advertises `rest_brain_selector` on GET
// /api/tools — the 0T posture: never assumed, never version-sniffed. An old owner
// without the stamp keeps the honest disabled-with-tooltip Open (INV-11).

/** True iff the owner's `/api/tools` envelope stamps `rest_brain_selector: true`. */
export function restBrainSelectorSupported(
  tools: { rest_brain_selector?: boolean } | null | undefined,
): boolean {
  return tools?.rest_brain_selector === true;
}

/**
 * Can THIS card's brain be opened IN THE TREE (same tab)?
 * - the bound/self brain: always (it IS the tree's default graph);
 * - a hosted project brain: only when the owner advertises the REST selector
 *   (§4A.9.5) — then `?brain=<root>` routes the tree to it;
 * - a live sibling with its own port opens in a NEW tab, not in place — false here
 *   (HallView still offers that via `entryBaseUrl`).
 * `restSelector` is the feature-detected stamp; absent → hosted Open stays off.
 */
export function canOpenBrainInPlace(
  entry: { brain_kind?: string | null },
  isSelf: boolean,
  restSelector: boolean,
): boolean {
  if (isSelf) return true;
  if (isProjectBrain(entry)) return restSelector;
  return false;
}

// ── Count honesty (PRD §4A.3 Nodes·edges; INV-10) ─────────────────────────────
// A live brain (self, or a live sibling we polled) reports real counts. A
// dormant/hosted brain at rest does NOT — the last-known registry count fields
// are [needs-backend] (§9.5.1). Absence renders absent, NEVER zero.

export interface BrainCounts {
  nodeCount: number | null;
  edgeCount: number | null;
}

/**
 * Resolve the node/edge counts a card may show. Only when the counts are
 * genuinely known (self graph_state, or a live sibling's polled stats) do we
 * return numbers; otherwise both are null and the card must say
 * "counts unknown — not running" rather than print 0.
 */
export function brainCounts(known: {
  nodeCount?: number | null;
  edgeCount?: number | null;
}): BrainCounts {
  const n = typeof known.nodeCount === 'number' ? known.nodeCount : null;
  const e = typeof known.edgeCount === 'number' ? known.edgeCount : null;
  return { nodeCount: n, edgeCount: e };
}

// ── The delete floor: exact-name match (PRD §4A.4 step 2; INV-09) ─────────────

/**
 * The type-the-name gate. The confirm button is unreachable until this is true:
 * the typed value must equal the repo basename EXACTLY (trimmed, case-sensitive
 * — the GitHub pattern). Empty never matches.
 */
export function nameMatches(typed: string, basename: string): boolean {
  return typed.trim().length > 0 && typed.trim() === basename;
}

// ── Project-brain semantics (PRD §4A.3; brain_kind) ───────────────────────────
// A project brain lives IN-PROCESS inside the owner (Two-Tier interim), warm-
// booted lazily. It has NO instance "running"/"stale" status and NO lock of its
// own — those belong to owner processes. The Hall must render its recorded
// graph size + freshness, never a process state or a lock badge.

/** True for an owner-hosted per-project brain (kind=project). */
export function isProjectBrain(entry: { brain_kind?: string | null }): boolean {
  return entry.brain_kind === 'project';
}

/**
 * The counts a card should show. A project brain carries its OWN counts on the
 * entry (server-enriched from the warm brain or the store manifest); everything
 * else uses the caller-supplied known counts (self graph_state / a live
 * sibling). Absence stays absent (INV-10), never a fabricated 0.
 */
export function resolvedBrainCounts(
  entry: { brain_kind?: string | null; node_count?: number | null; edge_count?: number | null },
  known: { nodeCount?: number | null; edgeCount?: number | null },
): BrainCounts {
  if (isProjectBrain(entry)) {
    return brainCounts({ nodeCount: entry.node_count, edgeCount: entry.edge_count });
  }
  return brainCounts(known);
}

/** The freshness timestamp for a card: a project brain's last-activity (manifest
 *  updated/created), else the instance heartbeat. */
export function brainFreshnessMs(entry: {
  brain_kind?: string | null;
  last_activity_ms?: number | null;
  last_heartbeat_ms?: number;
}): number | null {
  if (isProjectBrain(entry)) return entry.last_activity_ms ?? null;
  return entry.last_heartbeat_ms ?? null;
}

/**
 * The conflict chips a card should show. Lock/instance conflicts (`stale_lock`,
 * `duplicate_lock`, `shared_runtime`) are OWNER-process concepts and never apply
 * to an in-process project brain — filtered out for kind=project so a hosted
 * brain never wears a "stale lock" it cannot own.
 */
export function visibleConflicts(entry: { brain_kind?: string | null; conflicts: string[] }): string[] {
  if (!isProjectBrain(entry)) return entry.conflicts;
  return entry.conflicts.filter((c) => !/lock|runtime/i.test(c));
}

// ── Implementation class → the RECEIPT line (PRD §4A.8, INV-14) ───────────────
// The kind badge is RETIRED from card faces (INV-14): "this brain" / "project" /
// "sibling" / "bound" / "hosted" leak plumbing taxonomy onto the owner's front
// door. Every brain IS a project; the class is an implementation residue on a
// timeline (Slices 2/3 dissolve it). The class stays REAL and stays visible — in
// the receipt drawer's `binding:` line, where the orchestrator reads it as what
// it is. The one distinction a human needs at the Hall is the VIEWING state, not
// taxonomy (see the viewing chip on the card).

export type BrainImplClass = 'bound' | 'project' | 'sibling';

/**
 * Classify a brain by implementation class — for the RECEIPT ONLY (never a card
 * face). `self` is the process-bound dev graph; a `brain_kind:"project"` entry is
 * owner-hosted; everything else is a sibling owner. Absent brain_kind on a
 * non-self entry is honestly a sibling, never guessed as "project".
 */
export function brainImplClass(entry: { brain_kind?: string | null }, isSelf: boolean): BrainImplClass {
  if (isSelf) return 'bound';
  if (entry.brain_kind === 'project') return 'project';
  return 'sibling';
}

/**
 * The receipt's `binding:` value — the human wording of the implementation class
 * (§4A.8): process-bound | owner-hosted | sibling owner (own port). This is the
 * one place the class-word survives; when per-brain REST routing + process-per-
 * repo land, "bound vs hosted" stops being observable and this line becomes the
 * ONLY place the word lives.
 */
export function bindingClassLabel(entry: { brain_kind?: string | null }, isSelf: boolean): string {
  switch (brainImplClass(entry, isSelf)) {
    case 'bound':
      return 'process-bound';
    case 'project':
      return 'owner-hosted';
    case 'sibling':
    default:
      return 'sibling owner (own port)';
  }
}

// ── Consequence-card copy (PRD §4A.4 step 1) ──────────────────────────────────
// Categories, never a filename list — copy survives backend/allow-list drift
// (§9 risk 10). "Dies" = the rebuildable runtime; "Survives" = committed memory.

export const DELETE_DIES = 'the map, calibration, and caches';
export const DELETE_SURVIVES =
  'your memories in agent-memory/ and brain.json — they live on disk and in git; the map rebuilds on the next read';

// ── Owner state → landing (PRD §4A.1 placement doctrine; INV-12) ──────────────

export type OwnerLanding = 'threshold' | 'tree' | 'hall';

/**
 * Decide the landing surface from owner state (§4A.1 table). Zero brains → the
 * Threshold (empty-state-as-onboarding). Brains + a remembered last-visited
 * brain → straight to the tree (experts land in their work). Brains, no local
 * history → the Hall (the choice is real). A degraded binding is handled by the
 * tree's own banner, so it is NOT this function's concern — it only routes the
 * healthy first paint.
 */
export function ownerLanding(input: {
  brainCount: number;
  hasLocalHistory: boolean;
}): OwnerLanding {
  if (input.brainCount <= 0) return 'threshold';
  if (input.hasLocalHistory) return 'tree';
  return 'hall';
}
