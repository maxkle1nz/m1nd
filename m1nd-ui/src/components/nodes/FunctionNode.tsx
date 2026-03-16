import React from 'react';
import { Handle, Position, type NodeProps, type Node } from '@xyflow/react';
import type { M1ndNodeData } from '../../types';
import { nodeTypeColor } from '../../lib/colors';

class NodeErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { hasError: boolean }
> {
  state = { hasError: false };
  static getDerivedStateFromError() { return { hasError: true }; }
  componentDidCatch(error: Error) { console.error('[m1nd FunctionNode render error]', error); }
  render() {
    if (this.state.hasError) {
      return (
        <div className="w-11 h-11 flex items-center justify-center bg-red-900 border border-red-500 rounded text-red-300 text-lg font-bold">!</div>
      );
    }
    return this.props.children;
  }
}

const FunctionNodeInner = React.memo(function FunctionNode({ data, selected }: NodeProps<Node<M1ndNodeData>>) {
  const color = nodeTypeColor(2); // function = emerald
  const activation = data.activation ?? 0;
  const opacity = 0.4 + activation * 0.6;
  const glowStyle = activation > 0.5
    ? { boxShadow: `0 0 ${Math.round(activation * 10)}px ${color}88` }
    : {};

  // Size node by PageRank
  const scale = data.pagerank != null ? Math.max(0.85, Math.min(1.2, 0.85 + data.pagerank * 5)) : 1;

  return (
    <>
      <Handle type="target" position={Position.Left} className="!bg-m1nd-border-strong !w-2 !h-2" />
      <div
        className="px-2.5 py-1.5 rounded text-xs min-w-[100px] max-w-[150px] select-none transition-shadow"
        style={{
          backgroundColor: '#0a1a12',
          border: `1px solid ${selected ? '#a78bfa' : color}`,
          opacity,
          transform: `scale(${scale})`,
          ...glowStyle,
        }}
      >
        <div className="flex items-center gap-1 mb-0.5">
          <span style={{ color }} className="text-[9px] font-bold tracking-wide">fn</span>
          {activation > 0.8 && (
            <span className="text-[9px] text-yellow-400">⚡</span>
          )}
        </div>
        <div className="text-slate-200 font-mono truncate text-[11px]" title={data.label}>
          {data.label}
        </div>
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

const FunctionNode = (props: NodeProps<Node<M1ndNodeData>>) => (
  <NodeErrorBoundary>
    <FunctionNodeInner {...props} />
  </NodeErrorBoundary>
);

export default FunctionNode;
