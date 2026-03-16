import React, { useCallback, useRef } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  type NodeTypes,
  type EdgeTypes,
  type NodeMouseHandler,
  BackgroundVariant,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { useGraphStore } from '../stores/graphStore';
import FileNode from './nodes/FileNode';
import ClassNode from './nodes/ClassNode';
import FunctionNode from './nodes/FunctionNode';
import StructuralHoleNode from './nodes/StructuralHoleNode';
import WeightedEdge from './edges/WeightedEdge';
import GhostEdge from './edges/GhostEdge';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const nodeTypes: NodeTypes = {
  file: FileNode as any,
  class: ClassNode as any,
  function: FunctionNode as any,
  structuralHole: StructuralHoleNode as any,
};

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const edgeTypes: EdgeTypes = {
  weighted: WeightedEdge as any,
  ghost: GhostEdge as any,
};

// Canvas-level Error Boundary (FM-FE-056)
class CanvasErrorBoundary extends React.Component<
  {
    children: React.ReactNode;
    onRetry: () => void;
    onClear: () => void;
  },
  { hasError: boolean }
> {
  state = { hasError: false };
  static getDerivedStateFromError() { return { hasError: true }; }
  componentDidCatch(error: Error) { console.error('[m1nd GraphCanvas error]', error); }
  render() {
    if (this.state.hasError) {
      return (
        <div className="w-full h-full flex items-center justify-center bg-m1nd-base">
          <div className="text-center space-y-3">
            <div className="text-red-400 text-sm">Graph rendering error.</div>
            <div className="flex gap-2 justify-center">
              <button
                onClick={() => { this.setState({ hasError: false }); this.props.onRetry(); }}
                className="px-3 py-1.5 text-xs bg-m1nd-elevated border border-m1nd-border-medium text-slate-300 rounded hover:border-m1nd-accent transition-colors"
              >
                Re-run query
              </button>
              <button
                onClick={() => { this.setState({ hasError: false }); this.props.onClear(); }}
                className="px-3 py-1.5 text-xs bg-m1nd-elevated border border-m1nd-border-medium text-slate-300 rounded hover:border-red-500 transition-colors"
              >
                Clear graph
              </button>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

interface GraphCanvasProps {
  onNodeSelect: (nodeId: string | null) => void;
  onNodeContextMenu?: (nodeId: string, position: { x: number; y: number }) => void;
  onEdgeContextMenu?: (edgeId: string, position: { x: number; y: number }) => void;
}

export default function GraphCanvas({ onNodeSelect, onNodeContextMenu, onEdgeContextMenu }: GraphCanvasProps) {
  const { nodes, edges, showMinimap, setNodes, setEdges, selectNode, clearGraph } = useGraphStore();
  const lastQueryRef = useRef<{ tool: string; query: string } | null>(null);

  const onNodeClick: NodeMouseHandler = useCallback((_event, node) => {
    selectNode(node.id);
    onNodeSelect(node.id);
  }, [selectNode, onNodeSelect]);

  const onPaneClick = useCallback(() => {
    selectNode(null);
    onNodeSelect(null);
  }, [selectNode, onNodeSelect]);

  const onNodeContextMenuHandler: NodeMouseHandler = useCallback((event, node) => {
    event.preventDefault();
    onNodeContextMenu?.(node.id, { x: event.clientX, y: event.clientY });
  }, [onNodeContextMenu]);

  const handleRetry = useCallback(() => {
    // Re-run last query — hook consumers handle this
  }, []);

  const isEmpty = nodes.length === 0;

  return (
    <CanvasErrorBoundary onRetry={handleRetry} onClear={clearGraph}>
      <div className="w-full h-full relative">
        {isEmpty && (
          <div className="absolute inset-0 flex items-center justify-center text-slate-600 text-sm pointer-events-none z-10">
            <div className="text-center space-y-2">
              <div className="text-2xl text-slate-700">◈</div>
              <div>Press <kbd className="px-1.5 py-0.5 bg-m1nd-elevated border border-m1nd-border-medium rounded text-[11px] text-slate-400">⌘K</kbd> to query the graph</div>
              <div className="text-xs text-slate-700">or click Ingest to load a codebase</div>
            </div>
          </div>
        )}

        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          onNodesChange={(changes) => {
            // Apply position changes only (dragging)
            setNodes(nodes.map((n) => {
              const change = changes.find((c) => c.type === 'position' && c.id === n.id);
              if (change && change.type === 'position' && change.position) {
                return { ...n, position: change.position };
              }
              return n;
            }));
          }}
          onEdgesChange={() => {}}
          onNodeClick={onNodeClick}
          onPaneClick={onPaneClick}
          onNodeContextMenu={onNodeContextMenuHandler}
          fitView
          fitViewOptions={{ padding: 0.2 }}
          minZoom={0.1}
          maxZoom={3}
          proOptions={{ hideAttribution: true }}
          style={{ background: '#09090b' }}
        >
          <Background
            variant={BackgroundVariant.Dots}
            gap={20}
            size={1}
            color="#1e1e2e"
          />
          <Controls
            className="!bg-m1nd-elevated !border-m1nd-border-medium"
            showInteractive={false}
          />
          {showMinimap && (
            <MiniMap
              nodeColor={(n) => {
                const type = n.type ?? 'default';
                const colors: Record<string, string> = {
                  file: '#a78bfa',
                  class: '#6366f1',
                  function: '#059669',
                  structuralHole: '#ef4444',
                };
                return colors[type] ?? '#64748b';
              }}
              maskColor="#09090b99"
              style={{ backgroundColor: '#0c0c10', border: '1px solid #1e1e2e' }}
            />
          )}
        </ReactFlow>
      </div>
    </CanvasErrorBoundary>
  );
}
