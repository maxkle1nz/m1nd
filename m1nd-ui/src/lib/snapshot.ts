/*
 * Types for `GET /api/graph/snapshot` — the single source of tree structure
 * (HUMAN-LAYER-PRD §3.1: never a second fs.readdir view).
 *
 * Shapes are transcribed byte-for-byte from the live endpoint
 * (m1nd-mcp/src/http_server.rs:1301, node_type_to_u8 at :1401). Captured
 * fixtures in src/__fixtures__/ are the ground truth these types must match.
 */

/** node_type integers as emitted by node_type_to_u8 (http_server.rs). */
export const NODE_TYPE = {
  File: 0,
  Directory: 1,
  Function: 2,
  Class: 3,
  Struct: 4,
  Enum: 5,
  Type: 6,
  Module: 7,
  Reference: 8,
  Concept: 9,
} as const;

export interface SnapshotProvenance {
  source_path: string | null;
  line_start: number | null;
  line_end: number | null;
  namespace: string | null;
  canonical: boolean;
}

export interface SnapshotNode {
  external_id: string;
  label: string;
  node_type: number;
  tags: string[];
  last_modified: number;
  change_frequency: number;
  provenance: SnapshotProvenance;
}

export interface SnapshotEdge {
  source_id: string;
  target_id: string;
  relation: string;
  weight: number;
  direction: number;
  inhibitory: boolean;
  causal_strength: number;
}

export interface GraphSnapshot {
  version: number;
  nodes: SnapshotNode[];
  edges: SnapshotEdge[];
}

/** A node is a L1GHT (memory) node iff it carries the bare `light` tag. */
export function isLightNode(node: SnapshotNode): boolean {
  return node.tags.includes('light');
}

/**
 * Post-it eligibility (PRD §3.3 + the client-side marker filter): post-its are
 * drawn at CLAIM / SECTION level only. Marker-fragment nodes (`::tag::` — the
 * 𝔻/⟁ epistemic annotation rows) and metadata nodes (`::meta::`) are NOT
 * post-its. Mirrors the server-side filter a sibling is adding; done client-side
 * too (cheap).
 */
export function isPostItNode(node: SnapshotNode): boolean {
  if (!isLightNode(node)) return false;
  const id = node.external_id;
  return id.includes('::file::') || id.includes('::section::');
}

/** Read the L1GHT doc slug shared across a memory's file/section/tag/meta nodes. */
export function lightDocSlug(externalId: string): string | null {
  // Pattern: light::<ns>::<kind>::<slug>[::...]
  const m = externalId.match(/^light::[^:]+::(?:file|section|meta|tag)::([^:]+)/);
  return m ? m[1] : null;
}

/** Extract `light:source_agent:<id>` from a node's tags (absent → null). */
export function readSourceAgent(node: SnapshotNode): string | null {
  const tag = node.tags.find((t) => t.startsWith('light:source_agent:'));
  return tag ? tag.slice('light:source_agent:'.length) : null;
}

/** Extract `light:created:<ms>` from a node's tags (absent → null). */
export function readCreatedMs(node: SnapshotNode): number | null {
  const tag = node.tags.find((t) => t.startsWith('light:created:'));
  if (!tag) return null;
  const raw = tag.slice('light:created:'.length);
  const n = Number(raw);
  return Number.isFinite(n) ? n : null;
}
