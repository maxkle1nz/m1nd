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
  needs: string | null; // "needs_ingest" when cold
  binding: { trust_mode: string; [k: string]: unknown };
  context?: { focus_nodes?: unknown[]; anchors?: unknown[]; coverage?: unknown };
  memory: NorthMemoryEntry[];
  honest_gaps: string[];
  next_move: string | null;
  sufficiency?: { state?: string; captured?: number; top_score?: number; why?: string };
  recovery_playbook?: unknown;
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

// ── seek (SeekResultEntry) ────────────────────────────────────────────────────
export interface SeekResultEntry {
  node_id: string;
  label: string;
  type?: string;
  file_path?: string;
  line_start?: number;
  line_end?: number;
  source_agent?: string; // Option — absent → unknown
  authored_ms_ago?: number; // Option — absent → unknown
  excerpt?: string;
  score?: number;
}
export interface SeekOutput {
  query: string;
  results: SeekResultEntry[];
  trust_envelope?: unknown;
  sufficiency?: { state?: string };
}
