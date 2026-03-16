import React from 'react';
import { Handle, Position, type NodeProps, type Node } from '@xyflow/react';
import type { M1ndNodeData } from '../../types';

class NodeErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { hasError: boolean }
> {
  state = { hasError: false };
  static getDerivedStateFromError() { return { hasError: true }; }
  componentDidCatch(error: Error) { console.error('[m1nd StructuralHoleNode render error]', error); }
  render() {
    if (this.state.hasError) {
      return (
        <div className="w-11 h-11 flex items-center justify-center bg-red-900 border border-red-500 rounded text-red-300 text-lg font-bold">!</div>
      );
    }
    return this.props.children;
  }
}

const StructuralHoleNodeInner = React.memo(function StructuralHoleNode({ data, selected }: NodeProps<Node<M1ndNodeData>>) {
  return (
    <>
      <Handle type="target" position={Position.Left} className="!bg-red-800 !w-2 !h-2" />
      <div
        className="px-3 py-2 rounded text-xs min-w-[120px] max-w-[160px] select-none"
        style={{
          backgroundColor: '#1a0a0a',
          border: `1.5px dashed ${selected ? '#f87171' : '#ef4444'}`,
          boxShadow: selected ? '0 0 8px #ef444488' : undefined,
        }}
      >
        <div className="flex items-center gap-1.5 mb-0.5">
          <span className="text-red-400 text-[10px] font-bold uppercase tracking-wide">HOLE</span>
          <span className="text-red-500 text-[10px]">◈</span>
        </div>
        <div className="text-slate-300 font-mono truncate" title={data.label}>
          {data.label}
        </div>
        <div className="text-[10px] text-red-500 mt-0.5">structural gap</div>
      </div>
      <Handle type="source" position={Position.Right} className="!bg-red-800 !w-2 !h-2" />
    </>
  );
}, (prev, next) =>
  prev.data.label === next.data.label &&
  prev.data.activation === next.data.activation &&
  prev.data.animationState?.phase === next.data.animationState?.phase &&
  prev.selected === next.selected
);

const StructuralHoleNode = (props: NodeProps<Node<M1ndNodeData>>) => (
  <NodeErrorBoundary>
    <StructuralHoleNodeInner {...props} />
  </NodeErrorBoundary>
);

export default StructuralHoleNode;
