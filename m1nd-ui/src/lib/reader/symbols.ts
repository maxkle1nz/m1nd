/*
 * reader/symbols — the file OUTLINE, derived 100% from the graph (donor dossier
 * §2 Fatia 1.2: "Symbol outline FROM THE GRAPH, not a new parser"). Zero parser in
 * the browser: we filter the already-served snapshot nodes to the open file's
 * symbol nodes (`provenance.source_path === path`, a code kind), sort by
 * `line_start`, and hand the component clickable anchors + fold ranges.
 *
 * Pure + deterministic → unit-tested against the captured wire shape. The owner's
 * Rust extractors already produced these nodes; the browser only projects them.
 */
import { NODE_TYPE, type GraphSnapshot, type SnapshotNode } from '../snapshot';

/** The code symbol kinds the outline shows (dossier: Function/Class/Struct/Enum/
 *  Type/Module). File/Directory/Reference/Concept are NOT outline symbols. */
export type SymbolKind = 'function' | 'class' | 'struct' | 'enum' | 'type' | 'module';

const KIND_BY_NODE_TYPE: Record<number, SymbolKind> = {
  [NODE_TYPE.Function]: 'function',
  [NODE_TYPE.Class]: 'class',
  [NODE_TYPE.Struct]: 'struct',
  [NODE_TYPE.Enum]: 'enum',
  [NODE_TYPE.Type]: 'type',
  [NODE_TYPE.Module]: 'module',
};

/** One outline entry — a stable per-symbol anchor (the node_id) + its line span. */
export interface OutlineSymbol {
  /** The node's stable external id (`file::path::kind::Name`) — a permalink anchor. */
  id: string;
  label: string;
  kind: SymbolKind;
  /** 1-indexed line the symbol starts on — the scroll target. */
  lineStart: number;
  /** 1-indexed line the symbol ends on (≥ lineStart); drives fold ranges. */
  lineEnd: number;
  /** The owning namespace (`Graph` for `Graph::insert`), when the node carries one. */
  namespace: string | null;
  /** The node's churn signal (an existing graph fact, never invented). */
  changeFrequency: number;
}

/** A memory (L1GHT) node is never a code outline symbol — it rides node_type Module
 *  too, so exclude anything tagged `light` or with a `light::` id (snapshot.ts law). */
function isMemoryNode(node: SnapshotNode): boolean {
  return node.tags.includes('light') || node.external_id.startsWith('light::');
}

/** Map a node to an OutlineSymbol iff it is a code symbol IN this file with a real
 *  line span; otherwise `null` (Files/Dirs/References/Concepts/memory are dropped). */
export function symbolOf(node: SnapshotNode, path: string): OutlineSymbol | null {
  if (isMemoryNode(node)) return null;
  const kind = KIND_BY_NODE_TYPE[node.node_type];
  if (!kind) return null;
  const p = node.provenance;
  if (!p || p.source_path !== path) return null;
  if (p.line_start == null) return null;
  const lineStart = p.line_start;
  const lineEnd = p.line_end != null && p.line_end >= lineStart ? p.line_end : lineStart;
  return {
    id: node.external_id,
    label: node.label,
    kind,
    lineStart,
    lineEnd,
    namespace: p.namespace,
    changeFrequency: node.change_frequency,
  };
}

/**
 * The open file's outline: every code symbol whose provenance points at `path`,
 * sorted by `line_start` (then label, then id — a stable total order). Returns `[]`
 * for a file the graph carries no symbols for — the component then says so honestly
 * ("no symbols from the graph for this file"), never a fabricated tree.
 */
export function fileSymbols(snapshot: GraphSnapshot | null | undefined, path: string | null | undefined): OutlineSymbol[] {
  if (!snapshot || !path) return [];
  const out: OutlineSymbol[] = [];
  for (const node of snapshot.nodes) {
    const s = symbolOf(node, path);
    if (s) out.push(s);
  }
  out.sort((a, b) => a.lineStart - b.lineStart || a.label.localeCompare(b.label) || a.id.localeCompare(b.id));
  return out;
}

/** A collapsible region from the graph — a symbol whose body spans >1 line
 *  (dossier Fatia 1.5 / Fatia 2: fold ranges come from the graph spans). */
export interface FoldRange {
  /** The symbol id this range folds (stable key). */
  id: string;
  /** The line kept visible (the symbol's first line). */
  startLine: number;
  /** The last line of the collapsible body (inclusive). */
  endLine: number;
  /** How many lines collapse when folded (endLine - startLine). */
  hiddenCount: number;
}

/**
 * Fold ranges straight from the symbols' `[line_start, line_end]` spans — no
 * parser, no heuristic (dossier: "fold ranges come from the graph"). A symbol that
 * is a single line yields no range. Nested/overlapping spans are kept as-is (each a
 * candidate); the component collapses each independently.
 */
export function foldRangesFromSymbols(symbols: OutlineSymbol[]): FoldRange[] {
  const ranges: FoldRange[] = [];
  for (const s of symbols) {
    if (s.lineEnd > s.lineStart) {
      ranges.push({ id: s.id, startLine: s.lineStart, endLine: s.lineEnd, hiddenCount: s.lineEnd - s.lineStart });
    }
  }
  return ranges;
}
