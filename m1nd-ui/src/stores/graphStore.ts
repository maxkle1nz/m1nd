import type React from 'react';
import { create } from 'zustand';
import type { Node, Edge } from '@xyflow/react';
import type { GraphNode, GraphEdge, ToolId, M1ndNodeData, SseEvent, SseActivationData } from '../types';
import { applyLayout } from '../lib/layout';
import { nodeTypeColor, activationColor } from '../lib/colors';

export type ColorMode = 'type' | 'trust' | 'activation' | 'layer';
export type LayoutMode = 'auto' | 'hierarchical' | 'radial';

export interface QueryHistoryEntry {
  tool: ToolId;
  query: string;
  timestamp: number;
  nodeCount: number;
}

export interface GraphStore {
  // React Flow state
  nodes: Node[];
  edges: Edge[];

  // Selection
  selectedNodeId: string | null;
  selectedNodeIds: string[];

  // Display
  colorMode: ColorMode;
  layout: LayoutMode;
  showGhostEdges: boolean;
  showStructuralHoles: boolean;
  showMinimap: boolean;

  // Data
  activationScores: Map<string, number>;
  queryHistory: QueryHistoryEntry[];
  rawNodes: GraphNode[];
  rawEdges: GraphEdge[];

  // Loading
  isLoading: boolean;
  error: string | null;

  // Active tool
  activeTool: ToolId;

  // Actions
  setNodes: (nodes: Node[]) => void;
  setEdges: (edges: Edge[]) => void;
  selectNode: (id: string | null) => void;
  selectNodes: (ids: string[]) => void;
  setColorMode: (mode: ColorMode) => void;
  setLayout: (mode: LayoutMode) => void;
  setActiveTool: (tool: ToolId) => void;
  toggleGhostEdges: () => void;
  toggleStructuralHoles: () => void;
  toggleMinimap: () => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;
  loadSubgraph: (rawNodes: GraphNode[], rawEdges: GraphEdge[], query: string, tool: ToolId) => void;
  addToHistory: (entry: QueryHistoryEntry) => void;
  clearGraph: () => void;
  applySSEEvent: (event: SseEvent) => void;
}

/** Transform raw GraphNode → React Flow Node with typed data. */
function transformNode(n: GraphNode, colorMode: ColorMode): Node<M1ndNodeData> {
  const nodeTypeToComponent: Record<number, string> = {
    0: 'file',
    1: 'class',
    2: 'function',
  };

  let borderColor: string;
  switch (colorMode) {
    case 'activation':
      borderColor = activationColor(n.activation);
      break;
    case 'trust':
      borderColor = n.trust != null ? activationColor(n.trust) : nodeTypeColor(n.node_type);
      break;
    case 'layer':
      borderColor = n.layer != null ? `hsl(${(n.layer * 47) % 360}, 70%, 55%)` : nodeTypeColor(n.node_type);
      break;
    default:
      borderColor = nodeTypeColor(n.node_type);
  }

  return {
    id: n.id,
    type: nodeTypeToComponent[n.node_type] ?? 'default',
    position: { x: 0, y: 0 },
    style: { '--node-color': borderColor } as React.CSSProperties,
    data: {
      label: n.label,
      nodeType: n.node_type,
      activation: n.activation,
      pagerank: n.pagerank,
      trust: n.trust,
      layer: n.layer,
      tags: n.tags,
      sourcePath: n.source_path,
      animationState: { phase: 'inactive' },
    },
  };
}

/** Transform raw GraphEdge → React Flow Edge. */
function transformEdge(e: GraphEdge, i: number): Edge {
  return {
    id: `e-${i}-${e.source}-${e.target}`,
    source: e.source,
    target: e.target,
    type: e.relation === 'ghost' ? 'ghost' : 'weighted',
    animated: e.relation === 'ghost',
    data: { weight: e.weight, relation: e.relation },
  };
}

export const useGraphStore = create<GraphStore>((set, get) => ({
  nodes: [],
  edges: [],
  selectedNodeId: null,
  selectedNodeIds: [],
  colorMode: 'type',
  layout: 'auto',
  showGhostEdges: true,
  showStructuralHoles: true,
  showMinimap: false,
  activationScores: new Map(),
  queryHistory: [],
  rawNodes: [],
  rawEdges: [],
  isLoading: false,
  error: null,
  activeTool: 'activate',

  setNodes: (nodes) => set({ nodes }),
  setEdges: (edges) => set({ edges }),
  selectNode: (id) => set({ selectedNodeId: id }),
  selectNodes: (ids) => set({ selectedNodeIds: ids }),
  setColorMode: (mode) => set({ colorMode: mode }),
  setLayout: (mode) => set({ layout: mode }),
  setActiveTool: (tool) => set({ activeTool: tool }),
  toggleGhostEdges: () => set((s) => ({ showGhostEdges: !s.showGhostEdges })),
  toggleStructuralHoles: () => set((s) => ({ showStructuralHoles: !s.showStructuralHoles })),
  toggleMinimap: () => set((s) => ({ showMinimap: !s.showMinimap })),
  setLoading: (loading) => set({ isLoading: loading }),
  setError: (error) => set({ error }),

  loadSubgraph: (rawNodes, rawEdges, query, tool) => {
    const { colorMode, layout, showGhostEdges } = get();

    // Filter ghost edges based on toggle
    const filteredEdges = showGhostEdges
      ? rawEdges
      : rawEdges.filter((e) => e.relation !== 'ghost');

    const rfNodes = rawNodes.map((n) => transformNode(n, colorMode));
    const rfEdges = filteredEdges.map((e, i) => transformEdge(e, i));

    // Build activation scores map
    const activationScores = new Map<string, number>();
    rawNodes.forEach((n) => activationScores.set(n.id, n.activation));

    const historyEntry: QueryHistoryEntry = {
      tool,
      query,
      timestamp: Date.now(),
      nodeCount: rawNodes.length,
    };

    // Run layout async then update
    applyLayout(rfNodes, rfEdges, layout).then(({ nodes, edges }) => {
      set({
        nodes,
        edges,
        rawNodes,
        rawEdges,
        activationScores,
        selectedNodeId: null,
        selectedNodeIds: [],
        isLoading: false,
        error: null,
        queryHistory: [historyEntry, ...get().queryHistory].slice(0, 50),
      });
    });
  },

  addToHistory: (entry) => set((s) => ({
    queryHistory: [entry, ...s.queryHistory].slice(0, 50),
  })),

  clearGraph: () => set({
    nodes: [],
    edges: [],
    rawNodes: [],
    rawEdges: [],
    selectedNodeId: null,
    selectedNodeIds: [],
    activationScores: new Map(),
  }),

  applySSEEvent: (event: SseEvent) => {
    if (event.event_type === 'activation') {
      const { nodes, activationScores } = get();
      const data = event.data as SseActivationData;
      if (!data.activated || nodes.length === 0) return;

      // Build lookup from SSE activation data
      const sseScores = new Map<string, number>();
      for (const item of data.activated) {
        sseScores.set(item.node_id, item.activation);
      }

      // Merge activation scores into existing map
      const newScores = new Map(activationScores);
      for (const [id, score] of sseScores) {
        newScores.set(id, score);
      }

      // Update existing nodes in-place with new activation + glow animation
      const updatedNodes = nodes.map((node) => {
        const score = sseScores.get(node.id);
        if (score == null) return node;
        const nodeData = node.data as M1ndNodeData;
        return {
          ...node,
          style: {
            ...node.style,
            '--node-color': activationColor(score),
            '--node-glow': score > 0.5 ? `0 0 10px ${activationColor(score)}88` : 'none',
          } as React.CSSProperties,
          data: {
            ...nodeData,
            activation: score,
            animationState: score > 0.3
              ? { phase: 'firing' as const, intensity: Math.min(1, score) }
              : { phase: 'settled' as const, score },
          },
        };
      });

      set({ nodes: updatedNodes, activationScores: newScores });
    } else if (event.event_type === 'learn') {
      const { nodes } = get();
      const data = event.data;
      if (!('node_ids' in data) || nodes.length === 0) return;
      const learnedIds = new Set((data as { node_ids: string[] }).node_ids);

      // Flash learned nodes by temporarily boosting their glow
      const updatedNodes = nodes.map((node) => {
        if (!learnedIds.has(node.id)) return node;
        const nodeData = node.data as M1ndNodeData;
        return {
          ...node,
          style: {
            ...node.style,
            '--node-color': '#a78bfa',
            '--node-glow': '0 0 12px #a78bfa88',
          } as React.CSSProperties,
          data: {
            ...nodeData,
            animationState: { phase: 'propagating' as const, intensity: 0.8 },
          },
        };
      });
      set({ nodes: updatedNodes });

      // Decay animation after 1.5s
      setTimeout(() => {
        const { nodes: currentNodes } = get();
        const decayedNodes = currentNodes.map((node) => {
          if (!learnedIds.has(node.id)) return node;
          const nodeData = node.data as M1ndNodeData;
          return {
            ...node,
            data: {
              ...nodeData,
              animationState: { phase: 'settled' as const, score: nodeData.activation },
            },
          };
        });
        set({ nodes: decayedNodes });
      }, 1500);
    }
    // ingest + persist events are handled via toast in App.tsx, not graph updates
  },
}));
