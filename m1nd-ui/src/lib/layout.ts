import type { Node, Edge } from '@xyflow/react';
import type { LayoutMode } from '../stores/graphStore';

// Dagre-based layout engine for React Flow nodes.
// Falls back to grid if dagre is unavailable.

type DagreNode = { width: number; height: number };

let dagreLib: typeof import('dagre') | null = null;
async function getDagre() {
  if (dagreLib) return dagreLib;
  try {
    dagreLib = await import('dagre');
    return dagreLib;
  } catch {
    return null;
  }
}

const NODE_W = 160;
const NODE_H = 48;

function gridFallback(nodes: Node[]): Node[] {
  const cols = Math.ceil(Math.sqrt(nodes.length));
  return nodes.map((n, i) => ({
    ...n,
    position: {
      x: (i % cols) * (NODE_W + 60),
      y: Math.floor(i / cols) * (NODE_H + 40),
    },
  }));
}

export async function applyLayout(
  nodes: Node[],
  edges: Edge[],
  mode: LayoutMode = 'auto',
): Promise<{ nodes: Node[]; edges: Edge[] }> {
  const dagre = await getDagre();

  if (!dagre || nodes.length === 0) {
    return { nodes: gridFallback(nodes), edges };
  }

  const g = new dagre.graphlib.Graph();

  const rankdir =
    mode === 'hierarchical' ? 'TB' : mode === 'radial' ? 'LR' : 'LR';

  g.setGraph({
    rankdir,
    ranksep: mode === 'radial' ? 80 : 60,
    nodesep: 40,
    edgesep: 20,
    marginx: 40,
    marginy: 40,
  });
  g.setDefaultEdgeLabel(() => ({}));

  nodes.forEach((n) => {
    g.setNode(n.id, { width: NODE_W, height: NODE_H } as DagreNode);
  });
  edges.forEach((e) => {
    g.setEdge(e.source, e.target);
  });

  dagre.layout(g);

  const positionedNodes = nodes.map((n) => {
    const pos = g.node(n.id);
    if (!pos) return n;
    return {
      ...n,
      position: {
        x: pos.x - NODE_W / 2,
        y: pos.y - NODE_H / 2,
      },
    };
  });

  return { nodes: positionedNodes, edges };
}
