// ---- Graph types ----

export interface GraphNode {
  id: string;
  label: string;
  node_type: number;      // 0=file, 1=class, 2=function, 3=generic
  activation: number;
  tags: string[];
  source_path?: string;
  pagerank?: number;
  layer?: number;
  trust?: number;
}

export interface GraphEdge {
  source: string;
  target: string;
  weight: number;
  relation: string;       // "import", "call", "contains", "ghost"
}

/**
 * The `served_brain` echo (HUMAN-LAYER-PRD §4A.9.4) attached to every
 * `/api/graph/*` response: WHICH brain actually answered. The client asserts this
 * against the brain it asked for and drops mismatches (INV-15) — render truth,
 * never the request. Absent on a pre-2H owner (feature-detected via the tools
 * stamp); when present, `project_root` is the canonical root that answered.
 */
export interface ServedBrain {
  project_root: string | null;
  display_name: string | null;
}

export interface SubgraphResponse {
  nodes: GraphNode[];
  edges: GraphEdge[];
  meta: {
    total_nodes: number;
    rendered_nodes: number;
    query: string;
    elapsed_ms: number;
  };
  /** §4A.9.4 — the brain that answered (absent on a pre-2H owner). */
  served_brain?: ServedBrain;
}

// ---- Health ----

export interface HealthResponse {
  status: 'ok' | 'degraded' | 'empty' | 'down';
  uptime_secs: number;
  node_count: number;
  edge_count: number;
  queries_processed: number;
  agent_sessions: AgentSession[];
  domain: string;
  graph_generation: number;
  plasticity_generation: number;
}

export interface InstanceRegistryEntry {
  instance_id: string;
  workspace_root: string;
  runtime_root: string;
  graph_source: string;
  plasticity_state: string;
  pid: number;
  bind?: string | null;
  port?: number | null;
  started_at_ms: number;
  last_heartbeat_ms: number;
  mode: string;
  status: string;
  owner_live?: boolean | null;
  stale: boolean;
  conflicts: string[];
  /**
   * Two-Tier Brain kind (instance_registry.rs:45). Absent (serde default) on the
   * ~54k legacy entries → the classic single bound/dev graph; "project" marks an
   * owner-hosted per-project brain. Rendered as the Hall's kind badge (§4A.3);
   * absence is honest, never faked.
   */
  brain_kind?: string | null;
  /**
   * The brain's PROJECT name — the Hall card's headline (HUMAN-LAYER-PRD §4A.3).
   * The repo basename ("m1nd", "project-b"), NEVER the runtime dir ("claude")
   * nor its `agent-memory` sidecar. Computed server-side (http_server.rs
   * `instances_listing`): bound brain → its primary code ingest root's basename;
   * project brain → its store manifest's project_root basename. Absent only on a
   * rootless brain; the card falls back to the workspace basename if so.
   */
  display_name?: string | null;
  /**
   * The repo this brain maps — the Hall card's path (§4A.3). The server-resolved
   * real project root, not the fingerprint store dir that leaked before.
   */
  project_root?: string | null;
  /**
   * Graph size for a hosted **project** brain (kind=project), server-enriched
   * from the warm brain (live) or its store manifest (dormant). A project brain
   * lives in-process and has no instance "running" state — the Hall shows THESE
   * counts, never "not running". Absent (null) only for a fresh store before its
   * first persist; never a fabricated 0. Bound/sibling brains leave these null
   * (their counts arrive via graph_state / a polled stat).
   */
  node_count?: number | null;
  edge_count?: number | null;
  /**
   * A project brain's freshness (manifest `updated_ms`/`created_ms`) — used for
   * its card's "last seen" instead of the instance heartbeat.
   */
  last_activity_ms?: number | null;
  /**
   * The brain's OWN attached-session count (ladder R14 / TWO-TIER §9.5.1) — the
   * distinct wire sessions bound to THIS brain, partitioned on
   * `session.bound_project_root`, NOT the owner-global total (which stays on the
   * owner's receipt, labeled owner-wide). Feeds the card's G4 aliveness line.
   * Absent (null) for a dormant brain with no live SessionState — never a faked 0.
   */
  attached_sessions?: number | null;
  /**
   * The brain's OWN query count (R14 / §9.5.1) — its `queries_processed`,
   * partitioned per brain. Absent (null) for a dormant brain; never a faked 0.
   */
  query_count?: number | null;
  /**
   * Whether THIS brain's `predict` calibration is armed (R14 / §9.5.1 card field
   * G2) — a measured τ exists for this repo. Absent (null) for a dormant brain.
   */
  calibration_armed?: boolean | null;
  /**
   * The D3 face count (HUMAN-LAYER-PRD §4A.11, MEDULLA-PRD §9.2 M7b): "N open" —
   * the repo-side box's `wet_ink + in_flight` (unresolved letters only; external
   * NEVER counted). Server-enriched from `<repo>/.m1nd/inbox.jsonl` and rendered
   * ONLY when the box file exists — absent (null) for a repo with no box, never a
   * fabricated zero (INV-10 discipline). Clicking it opens the Mailbox for this
   * brain (`GET /api/mailbox?brain=<root>`).
   */
  mailbox_open_count?: number | null;
}

export interface InstanceSelfResponse {
  instance: InstanceRegistryEntry;
  graph_state: {
    node_count: number;
    edge_count: number;
    finalized: boolean;
    graph_generation: number;
    plasticity_generation: number;
    cache_generation: number;
    ingest_root_count: number;
    ingest_roots: string[];
    workspace_root?: string | null;
    runtime_root: string;
  };
  active_agent_sessions: number;
  queries_processed: number;
  last_persist_secs_ago?: number | null;
  /**
   * The bound brain's PROJECT name/root (§4A.5) — the Brain Chip's name source,
   * so the chip reads "m1nd", never the `agent-memory` sidecar that
   * `graph_state.workspace_root` carries. Same server derivation as the Hall.
   */
  display_name?: string | null;
  project_root?: string | null;
}

export interface InstanceListResponse {
  instances: InstanceRegistryEntry[];
  error?: string;
}

export interface AgentSession {
  agent_id: string;
  first_seen_secs_ago: number;
  last_seen_secs_ago: number;
  query_count: number;
}

// ---- Presences (ORGANISM-INSIDE-PRD P1 · askGOD-verdict-P1 · P1-UI-CONTRACT) ----

/**
 * One agent's presence — activity that is VISIBLE TO m1nd. Projected from the
 * throttled beat inside `track_agent` (verdict binding change 1), stored in the
 * presence sidecar, and TTL-filtered at read: an entry present in the roster is
 * alive-within-TTL. Expired presences are ABSENT (GC'd at read/boot) — the strip
 * never renders a ghost (metric: "Ghost presences → TTL expiry proven"). The AGE
 * is always rendered from `last_seen_ms`; there is no binary "online" (verdict 1b).
 *
 * THE HONEST LIMITATION, written on the surface (verdict risk "the inverse TTL
 * lie"): presence is activity m1nd SAW. A busy executor mid-compile that has not
 * called in stays invisible until its next verb. The strip renders that caveat.
 */
export interface PresenceEntry {
  /** The agent's own id — its `agent_id` on every m1nd call / its charter. */
  agent_id: string;
  /** The brain/root this agent is bound to — the served brain root (the WHERE). */
  root: string;
  /**
   * The agent's resolved caller_root / worktree when it differs from `root` (a
   * burst executor in an isolated worktree). The MEASURABLE collision signal
   * (verdict binding change 2): two hands in ONE caller_root is the 2026-07-06
   * incident shape. Absent/null when the caller_root equals the brain root.
   */
  caller_root?: string | null;
  /** First time m1nd saw this agent (ms epoch). */
  first_seen_ms: number;
  /** Last time m1nd saw this agent (ms epoch) — the AGE source, always rendered. */
  last_seen_ms: number;
  /** How many verbs m1nd has seen from this agent in this presence. */
  query_count: number;
  /**
   * The mutation signal in the verdict's TWO honest levels (binding change 1c):
   *  - `observed_at_ms` — the agent dispatched a verb classified mutating
   *    (server.rs `read_only_denied`), timestamped. The strong signal.
   *  - `declared_intent` — a handshake-declared intent, rendered as DECLARED
   *    cloth (weaker: a claim, not an observation).
   * Both absent → a quiet read-only presence (no dot). m1nd does not see git;
   * nothing beyond these two levels is claimable.
   */
  mutation: PresenceMutation;
  /**
   * The task line measured from the agent's OWN `mission_start` charter (never a
   * free declaration) — the "on what". Absent when the agent opened no letter.
   */
  task_ref?: string | null;
}

export interface PresenceMutation {
  observed_at_ms?: number | null;
  declared_intent?: string | null;
}

/** Which arm of the collision predicate fired — the honest reason on the surface. */
export type PresenceCollisionReason = 'same_worktree' | 'declared_overlap';

/**
 * A collision DERIVED AT READ (verdict binding changes 1d + 2), never
 * materialized. The predicate: same brain AND (same caller_root/worktree OR
 * declared working-set overlap) AND BOTH agents carry a mutation signal.
 * Same-brain-alone NEVER warns — N executors in ISOLATED worktrees on one brain
 * is the normal burst shape (AGENTS.md), not a collision. The measurable arm is
 * `caller_root` equality (two hands in one worktree = the real 2026-07-06
 * incident); the `declared_overlap` arm is server-derived from declared working
 * sets the UI does not invent.
 */
export interface PresenceCollision {
  /** The shared brain root. */
  brain_root: string;
  /** The shared caller_root/worktree when THAT is the trigger (the strong arm). */
  caller_root?: string | null;
  /** The colliding agents (≥2 distinct ids). */
  agent_ids: string[];
  /** WHICH arm of the predicate fired. */
  reason: PresenceCollisionReason;
}

/**
 * `GET /api/presences?brain=<root>` (P1-UI-CONTRACT). Absent `brain` = the
 * OWNER-WIDE roster (every agent visible to this owner across its brains — the
 * Hall's control-room scope, LABELED per the verdict's scope note); a `brain`
 * scopes it to that brain. The roster is TTL-filtered at read (no ghosts).
 * `collisions` is server-derived at read (authoritative); when the field is
 * ABSENT (a pre-P1 owner) the client degrades to `deriveCollisions` over the
 * roster — the honest-degradation pattern (like `served_brain`). `served_brain`
 * echoes which brain answered (§4A.9.4).
 */
export interface PresenceResponse {
  presences: PresenceEntry[];
  collisions?: PresenceCollision[];
  served_brain?: ServedBrain;
}

// ---- Tool call ----

export interface ToolCallResult {
  result: unknown;
}

export interface ToolCallError {
  error: string;
  detail: string;
}

export interface ToolSchema {
  name: string;
  description: string;
  inputSchema: {
    type: string;
    properties: Record<string, unknown>;
    required: string[];
  };
}

/**
 * The GET /api/tools envelope. `rest_brain_selector` (HUMAN-LAYER-PRD §4A.9.5) is
 * the capability stamp the Hall feature-detects to enable per-brain Open — absent
 * on a pre-2H owner (the 0T disabled posture holds).
 */
export interface ToolsResponse {
  tools: ToolSchema[];
  rest_brain_selector?: boolean;
}

// ---- SSE events ----

export interface SseActivationData {
  agent_id: string;
  query: string;
  activated: Array<{ node_id: string; activation: number }>;
  top_k: number;
}

export interface SseLearnData {
  agent_id: string;
  feedback: string;
  node_ids: string[];
}

export interface SseIngestData {
  agent_id: string;
  path: string;
  nodes_added: number;
  edges_added: number;
}

export interface SsePersistData {
  generation: number;
}

/**
 * Emitted by the server (http_server.rs `browser_graph_changed_event`) when the
 * shared graph actually changed under the UI — an agent ran memorize / ingest /
 * edit_commit / apply / learn. The Living Tree listens for this to refresh in
 * place (PRD §5.3). `event` names WHICH mutation; the tree re-fetches the
 * snapshot rather than trusting a diff.
 */
export interface SseGraphChangedData {
  event: string;
  agent_id?: string;
  source?: string;
  batch_id?: string;
  timestamp_ms?: number;
  /**
   * WHICH brain mutated (HUMAN-LAYER-PRD §4A.9.6) — stamped when the mutating call
   * named a brain via `?brain=`. Additive: absent on a bound/legacy mutation and
   * on a pre-2H owner. A viewer refetches when this names ITS brain OR is absent
   * (the honest over-refetch on old owners).
   */
  brain_root?: string;
}

/**
 * Emitted by the owner (http_server.rs `scan_progress_sink`) as `skeleton_candidate`
 * crosses its real pipeline boundaries (docs/uml/scan-loading.md slice 2). Facts of
 * the process — a named phase + the counts the owner computed at that boundary —
 * never a fabricated percentage. The Build Map's scan wait panel consumes it while
 * a scan is in flight to replace the static phase label with the owner's real one;
 * absent events (older owner) degrade to the client-only wait exactly as before.
 */
export interface SseScanProgressData {
  /** file_list | clustering | naming | persisting | done | failed. */
  phase: string;
  file_count?: number;
  node_count?: number;
  edge_count?: number;
  block_count?: number;
  /** The naming budget's wave estimate (a fact, not progress). */
  naming_waves?: number;
  /** The honest error string on the terminal `failed` phase. */
  error?: string;
  tool?: string;
  source?: string;
  agent_id?: string;
  timestamp_ms?: number;
}

export type SseEvent =
  | { event_type: 'activation'; data: SseActivationData }
  | { event_type: 'learn'; data: SseLearnData }
  | { event_type: 'ingest'; data: SseIngestData }
  | { event_type: 'persist'; data: SsePersistData }
  | { event_type: 'graph_changed'; data: SseGraphChangedData }
  | { event_type: 'scan_progress'; data: SseScanProgressData };

// ---- Tool IDs ----

export type ToolId =
  | 'activate' | 'seek' | 'scan' | 'missing' | 'differential'
  | 'impact' | 'why' | 'counterfactual' | 'predict' | 'hypothesize'
  | 'validate_plan' | 'fingerprint' | 'resonate' | 'trace'
  | 'perspective.start' | 'drift' | 'timeline' | 'diverge' | 'warmup' | 'federate'
  | 'trail.list' | 'lock.create' | 'health'
  | 'ingest' | 'learn';

export type ToolCategory = 'EXPLORE' | 'ANALYZE' | 'NAVIGATE' | 'MEMORY';

export const TOOL_CATEGORIES: Record<ToolCategory, ToolId[]> = {
  EXPLORE: ['activate', 'seek', 'differential', 'scan', 'missing'],
  ANALYZE: ['impact', 'why', 'counterfactual', 'predict', 'hypothesize', 'validate_plan', 'fingerprint', 'resonate', 'trace'],
  NAVIGATE: ['perspective.start', 'drift', 'timeline', 'diverge', 'warmup', 'federate'],
  MEMORY: ['trail.list', 'lock.create', 'health'],
};

// ---- Custom React Flow node data ----

export interface M1ndNodeData extends Record<string, unknown> {
  label: string;
  nodeType: number;
  activation: number;
  pagerank?: number;
  trust?: number;
  layer?: number;
  tags: string[];
  sourcePath?: string;
  animationState?: NodeAnimationState;
}

export type NodeAnimationState =
  | { phase: 'inactive' }
  | { phase: 'firing'; intensity: number }
  | { phase: 'propagating'; intensity: number }
  | { phase: 'settled'; score: number }
  | { phase: 'decaying' };

export type NodeAction =
  | 'activate_from'
  | 'impact'
  | 'why_from'
  | 'predict'
  | 'hypothesize'
  | 'counterfactual'
  | 'timeline'
  | 'open_perspective'
  | 'branch_perspective';

export interface Trail {
  id: string;
  name: string;
  description: string;
  node_count: number;
  created_at: string;
}
