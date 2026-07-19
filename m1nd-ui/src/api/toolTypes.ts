/*
 * Tool-output types — transcribed from REAL captured envelopes in src/__fixtures__/
 * (north_cold/north_warm, trust, tremor, impact, seek). Optional fields are
 * genuinely Option on the wire and their absence must render honestly (INV-04).
 */

// ── north (m1nd-north-packet-v0) ──────────────────────────────────────────────
export interface NorthMemoryEntry {
  kind: string; // "light" | "boot" ...
  claim: string;
  node_id: string;
  age_ms?: number; // absent → unknown age (never faked)
  source_agent?: string; // absent → author unknown
  stale?: boolean;
}

export interface NorthPacket {
  schema: string;
  task?: string; // the task the packet was composed for (verbatim on the wire)
  needs: string | null; // "needs_ingest" when cold
  binding: { trust_mode: string; [k: string]: unknown };
  context?: { focus_nodes?: unknown[]; anchors?: unknown[]; coverage?: unknown };
  memory: NorthMemoryEntry[];
  honest_gaps: string[];
  next_move: string | null;
  sufficiency?: { state?: string; captured?: number; top_score?: number; why?: string };
  recovery_playbook?: unknown;
  /** R0 honesty patch: the on-disk L1GHT store count — makes the empty-memory
   *  line truthful (recall miss vs. truly empty store). Absent on pre-R0 packets. */
  memory_exists?: number;
  /** JOINT-I: the reception match rides on north; the Card's binding header echoes
   *  it (§C1.4). `caller_root_mismatch` → the bound graph doesn't cover the caller. */
  reception?: { match?: string; honest?: string };
}

// ── trust (TrustResult) ───────────────────────────────────────────────────────
export interface TrustNodeOutput {
  node_id: string;
  label: string;
  trust_score: number;
  tier: string; // "LowRisk" | "MediumRisk" | "HighRisk" | "Unknown"
  defect_count: number;
  false_alarm_count: number;
  total_learn_events: number;
}

export interface TrustSummary {
  total_nodes_with_history: number;
  high_risk_count: number;
  medium_risk_count: number;
  low_risk_count: number;
  unknown_count: number;
  mean_trust: number | null;
}

export interface TrustResult {
  trust_scores: TrustNodeOutput[];
  summary: TrustSummary;
  scope: string;
  elapsed_ms: number;
}

// ── tremor (TremorResult) ─────────────────────────────────────────────────────
export interface TremorEntry {
  node_id?: string;
  label?: string;
  magnitude?: number;
  direction?: string;
  risk_level?: string;
}
export interface TremorResult {
  tremors: TremorEntry[];
  total_nodes_analyzed: number;
  nodes_with_sufficient_data: number;
  threshold: number;
  window: unknown;
  note?: string;
}

// ── impact (ImpactOutput) ─────────────────────────────────────────────────────
export interface ImpactBlastEntry {
  node_id?: string;
  label?: string;
  hop_distance?: number;
  signal_strength?: number;
}
export interface ImpactOutput {
  source: string;
  source_label?: string;
  total_blast_nodes: number;
  truncated: boolean;
  blast_radius: ImpactBlastEntry[];
  next_step_hint?: string;
}

// ── seek (SeekResultEntry / SeekOutput) ───────────────────────────────────────
// Transcribed from the REAL captured envelope (src/__fixtures__/seek_meaning.json,
// POST /api/tools/seek on the live :1338 owner). The §4A.10 meaning-search panel
// renders `sufficiency` + `trust_envelope` (which the UI currently discards) — so
// both are typed in full here, not as `unknown`.
export interface SeekResultEntry {
  node_id: string;
  label: string;
  type?: string;
  file_path?: string;
  line_start?: number;
  line_end?: number;
  source_agent?: string | null; // Option — absent/null → unknown (INV-04)
  authored_ms_ago?: number | null; // Option — absent/null → unknown (INV-04)
  excerpt?: string;
  intent_summary?: string; // the one-line "what this is" the panel shows
  score?: number; // the engine's score VERBATIM — no invented stars (§4A.10)
}

/** The sufficiency block — ALWAYS present on SeekOutput (layers.rs:103). */
export interface Sufficiency {
  state: string; // "sufficient" | "gathering" | "saturated"
  top_score?: number;
  captured?: number;
  why: string; // the engine's own explanation, rendered verbatim
}

/** One factor inside a TrustEnvelope. */
export interface TrustEnvelopeFactor {
  name: string;
  band: string;
  known: boolean;
  weight: number;
}

/** The four-state trust ladder — mirrors the Rust protocol union verbatim
 *  (protocol/layers.rs TrustEnvelope.verdict). `unprovable` = nothing exists
 *  to check the claim against; a REAL answer, never collapsed into abstain. */
export type TrustVerdict = 'act' | 'reverify' | 'abstain' | 'unprovable';

/** The trust envelope — ALWAYS present on SeekOutput (layers.rs:180-231). */
export interface TrustEnvelope {
  verdict: TrustVerdict;
  calibrated: boolean; // false → capped at reverify by engine law (G2 explains)
  score?: number;
  factors?: TrustEnvelopeFactor[];
  reasons?: string[];
  next_repair_call?: string;
}

export interface SeekGrounding {
  state: 'entity_grounded' | 'entity_absent' | 'not_applicable';
  salient_entities: string[];
  missing_entities: string[];
  reason: string;
}

export interface SeekOutput {
  query: string;
  result_granularity: 'node' | 'file_group';
  grounding: SeekGrounding;
  results: SeekResultEntry[];
  sufficiency: Sufficiency;
  trust_envelope: TrustEnvelope;
  embeddings_used?: boolean; // false → "matched by text, not meaning" (trigram fallback)
  relevance_clearing_total?: number; // engine-counted total that cleared relevance
  total_candidates_scanned?: number;
  filtering_reason?: string | null; // when results empty, the engine says WHY (verbatim)
}

// ── layers (LayersOutput) — the §4A.10 layer lens ─────────────────────────────
// From the REAL captured envelope (src/__fixtures__/layers.json, the `layers`
// verb). Layer names repeat across `level`s (e.g. two "entry_points"), so a
// group's stable key is its level, and its display combines name + level.
export interface LayerNode {
  node_id: string;
  label: string;
  type?: string;
  in_degree?: number;
  out_degree?: number;
  layer_confidence?: number;
}
export interface LayerGroup {
  name: string;
  level: number;
  node_count: number;
  description?: string;
  avg_out_degree?: number;
  avg_pagerank?: number;
  nodes: LayerNode[];
  nodes_returned?: number;
  nodes_truncated?: boolean;
}
export interface LayerUtilityNode {
  node_id: string;
  label: string;
  classification?: string;
  used_by_layers?: number[];
}
export interface LayersOutput {
  layers: LayerGroup[];
  summary?: {
    total_layers_detected?: number;
    total_nodes_classified?: number;
    total_utility_nodes?: number;
    total_violations?: number;
    has_cycles?: boolean;
    layer_separation_score?: number;
  };
  utility_nodes?: LayerUtilityNode[];
  truncated?: boolean;
}

// ── am_i_stale (AmIStaleOutput) — G1 freshness + the "changed since read" chip ─
// From the REAL captured envelope (src/__fixtures__/am_i_stale.json). The card
// passes the visible file set explicitly; the result partitions checked paths.
export interface AmIStaleOutput {
  checked: number;
  stale: Array<{ path: string; reason: string } | string>;
  fresh: string[];
  unknown?: string[];
  summary?: string;
  source?: string;
}
