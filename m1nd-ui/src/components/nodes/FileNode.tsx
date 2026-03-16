import React from 'react';
import { Handle, Position, type NodeProps, type Node } from '@xyflow/react';
import type { M1ndNodeData } from '../../types';
import { nodeTypeColor } from '../../lib/colors';

// Per-node Error Boundary (MANDATORY per FM-FE-056)
class NodeErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { hasError: boolean }
> {
  state = { hasError: false };
  static getDerivedStateFromError() { return { hasError: true }; }
  componentDidCatch(error: Error) { console.error('[m1nd FileNode render error]', error); }
  render() {
    if (this.state.hasError) {
      return (
        <div className="w-11 h-11 flex items-center justify-center bg-red-900 border border-red-500 rounded text-red-300 text-lg font-bold">!</div>
      );
    }
    return this.props.children;
  }
}

const FileNodeInner = React.memo(function FileNode({ data, selected }: NodeProps<Node<M1ndNodeData>>) {
  const color = nodeTypeColor(0); // file = purple
  const activation = data.activation ?? 0;
  const opacity = 0.4 + activation * 0.6;
  const glowStyle = activation > 0.5
    ? { boxShadow: `0 0 ${Math.round(activation * 12)}px ${color}88` }
    : {};

  return (
    <>
      <Handle type="target" position={Position.Left} className="!bg-m1nd-border-strong !w-2 !h-2" />
      <div
        className="px-3 py-2 rounded text-xs min-w-[120px] max-w-[160px] select-none transition-shadow"
        style={{
          backgroundColor: '#1a1a2e',
          border: `1.5px solid ${selected ? '#a78bfa' : color}`,
          opacity,
          ...glowStyle,
        }}
      >
        <div className="flex items-center gap-1.5 mb-0.5">
          <span style={{ color }} className="text-[10px] font-bold uppercase tracking-wide">FILE</span>
          {activation > 0.7 && (
            <span className="text-[9px] text-orange-400">●</span>
          )}
        </div>
        <div className="text-slate-200 font-mono truncate" title={data.label}>
          {data.label}
        </div>
        {data.pagerank != null && (
          <div className="text-[10px] text-slate-500 mt-0.5">
            pr: {data.pagerank.toFixed(3)}
          </div>
        )}
      </div>
      <Handle type="source" position={Position.Right} className="!bg-m1nd-border-strong !w-2 !h-2" />
    </>
  );
}, (prev, next) =>
  prev.data.label === next.data.label &&
  prev.data.activation === next.data.activation &&
  prev.data.animationState?.phase === next.data.animationState?.phase &&
  prev.selected === next.selected
);

const FileNode = (props: NodeProps<Node<M1ndNodeData>>) => (
  <NodeErrorBoundary>
    <FileNodeInner {...props} />
  </NodeErrorBoundary>
);

export default FileNode;
