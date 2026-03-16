/**
 * exportMermaid.ts — Convert a visible subgraph into a Mermaid diagram string.
 *
 * Constraints (from spec):
 *   - Cap at 50 edges max
 *   - Truncate labels to 20 characters
 *   - Use node_type to set shape (file=cylinder, class=hexagon, function=stadium, generic=rect)
 */

import type { GraphNode, GraphEdge } from '../types';

const MAX_EDGES = 50;
const MAX_LABEL_LEN = 20;

/** Mermaid node shapes by node_type */
function mermaidShape(nodeType: number, id: string, label: string): string {
  const safeId = sanitizeId(id);
  const safeLabel = truncate(label, MAX_LABEL_LEN).replace(/"/g, "'");

  switch (nodeType) {
    case 0: // file → cylinder
      return `  ${safeId}[(${JSON.stringify(safeLabel)})]`;
    case 1: // class → hexagon
      return `  ${safeId}{{${JSON.stringify(safeLabel)}}}`;
    case 2: // function → stadium
      return `  ${safeId}([${JSON.stringify(safeLabel)}])`;
    default: // generic → rect
      return `  ${safeId}[${JSON.stringify(safeLabel)}]`;
  }
}

/** Mermaid arrow style by relation */
function mermaidArrow(relation: string, weight: number): string {
  const weightLabel = weight < 1 ? `|${weight.toFixed(2)}|` : '';
  switch (relation) {
    case 'import':   return `-->${weightLabel}`;
    case 'call':     return `-.->`;
    case 'contains': return `--o`;
    case 'ghost':    return `~~~`;
    default:         return `-->${weightLabel}`;
  }
}

/** Replace unsafe characters for Mermaid node IDs. */
function sanitizeId(id: string): string {
  // Mermaid IDs: alphanumeric + underscore only; hash the rest
  return id.replace(/[^a-zA-Z0-9_]/g, '_').replace(/^(\d)/, 'n_$1');
}

function truncate(s: string, maxLen: number): string {
  return s.length > maxLen ? `${s.slice(0, maxLen - 1)}…` : s;
}

/**
 * Convert a subgraph (nodes + edges) to a Mermaid flowchart diagram string.
 *
 * @param nodes  GraphNode array (visible subgraph)
 * @param edges  GraphEdge array (visible subgraph)
 * @param direction  LR | TD (default LR)
 */
export function exportMermaid(
  nodes: GraphNode[],
  edges: GraphEdge[],
  direction: 'LR' | 'TD' = 'LR',
): string {
  if (nodes.length === 0) return '';

  // Build id lookup for safety
  const nodeIds = new Set(nodes.map((n) => n.id));

  // Cap edges
  const cappedEdges = edges
    .filter((e) => nodeIds.has(e.source) && nodeIds.has(e.target))
    .filter((e) => e.relation !== 'ghost') // skip ghost edges by default
    .sort((a, b) => b.weight - a.weight)    // highest weight first
    .slice(0, MAX_EDGES);

  // Build node declarations
  const nodeMap = new Map(nodes.map((n) => [n.id, n]));
  const usedNodeIds = new Set<string>();
  cappedEdges.forEach((e) => {
    usedNodeIds.add(e.source);
    usedNodeIds.add(e.target);
  });
  // Also include isolated nodes (no edges)
  nodes.forEach((n) => usedNodeIds.add(n.id));

  const lines: string[] = [`flowchart ${direction}`];

  // Node declarations
  usedNodeIds.forEach((id) => {
    const n = nodeMap.get(id);
    if (n) {
      lines.push(mermaidShape(n.node_type, id, n.label));
    }
  });

  // Edge declarations
  cappedEdges.forEach((e) => {
    const srcId = sanitizeId(e.source);
    const tgtId = sanitizeId(e.target);
    const arrow = mermaidArrow(e.relation, e.weight);
    lines.push(`  ${srcId} ${arrow} ${tgtId}`);
  });

  // Style classes
  lines.push('');
  lines.push('  classDef file fill:#a78bfa,stroke:#7c3aed,color:#fff');
  lines.push('  classDef class fill:#6366f1,stroke:#4f46e5,color:#fff');
  lines.push('  classDef fn fill:#059669,stroke:#047857,color:#fff');
  lines.push('  classDef generic fill:#475569,stroke:#334155,color:#fff');

  // Apply styles
  const fileNodes = nodes.filter((n) => n.node_type === 0).map((n) => sanitizeId(n.id));
  const classNodes = nodes.filter((n) => n.node_type === 1).map((n) => sanitizeId(n.id));
  const fnNodes = nodes.filter((n) => n.node_type === 2).map((n) => sanitizeId(n.id));
  const genericNodes = nodes.filter((n) => n.node_type >= 3).map((n) => sanitizeId(n.id));

  if (fileNodes.length) lines.push(`  class ${fileNodes.join(',')} file`);
  if (classNodes.length) lines.push(`  class ${classNodes.join(',')} class`);
  if (fnNodes.length)    lines.push(`  class ${fnNodes.join(',')} fn`);
  if (genericNodes.length) lines.push(`  class ${genericNodes.join(',')} generic`);

  return lines.join('\n');
}

/**
 * Export only the edges between a selected set of node IDs.
 * Useful for exporting "neighborhood" diagrams from DetailPanel.
 */
export function exportMermaidSubset(
  allNodes: GraphNode[],
  allEdges: GraphEdge[],
  selectedIds: string[],
  direction: 'LR' | 'TD' = 'LR',
): string {
  const idSet = new Set(selectedIds);
  const filteredNodes = allNodes.filter((n) => idSet.has(n.id));
  const filteredEdges = allEdges.filter((e) => idSet.has(e.source) && idSet.has(e.target));
  return exportMermaid(filteredNodes, filteredEdges, direction);
}
