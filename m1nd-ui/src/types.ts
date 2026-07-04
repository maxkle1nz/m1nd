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

export interface SubgraphResponse {
  nodes: GraphNode[];
  edges: GraphEdge[];
  meta: {
    total_nodes: number;
    rendered_nodes: number;
    query: string;
    elapsed_ms: number;
  };
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
   * The repo basename ("m1nd", "Cerrybubbles1"), NEVER the runtime dir ("claude")
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
}

export type SseEvent =
  | { event_type: 'activation'; data: SseActivationData }
  | { event_type: 'learn'; data: SseLearnData }
  | { event_type: 'ingest'; data: SseIngestData }
  | { event_type: 'persist'; data: SsePersistData }
  | { event_type: 'graph_changed'; data: SseGraphChangedData };

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
