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
  componentDidCatch(error: Error) { console.error('[m1nd ClassNode render error]', error); }
  render() {
    if (this.state.hasError) {
      return (
        <div className="w-11 h-11 flex items-center justify-center bg-red-900 border border-red-500 rounded text-red-300 text-lg font-bold">!</div>
      );
    }
    return this.props.children;
  }
}

const ClassNodeInner = React.memo(function ClassNode({ data, selected }: NodeProps<Node<M1ndNodeData>>) {
  const color = nodeTypeColor(1); // class = indigo
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
          backgroundColor: '#0f0f1e',
          border: `1.5px solid ${selected ? '#a78bfa' : color}`,
          opacity,
          ...glowStyle,
        }}
      >
        <div className="flex items-center gap-1.5 mb-0.5">
          <span style={{ color }} className="text-[10px] font-bold uppercase tracking-wide">CLASS</span>
          {data.layer != null && (
            <span className="text-[9px] text-slate-500">L{data.layer}</span>
          )}
        </div>
        <div className="text-slate-200 font-mono truncate" title={data.label}>
          {data.label}
        </div>
        {data.tags.length > 0 && (
          <div className="flex flex-wrap gap-0.5 mt-1">
            {data.tags.slice(0, 2).map((t) => (
              <span key={t} className="text-[9px] px-1 bg-indigo-900/50 text-indigo-300 rounded">
                {t}
              </span>
            ))}
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

const ClassNode = (props: NodeProps<Node<M1ndNodeData>>) => (
  <NodeErrorBoundary>
    <ClassNodeInner {...props} />
  </NodeErrorBoundary>
);

export default ClassNode;
