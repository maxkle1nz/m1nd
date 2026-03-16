import React, { useMemo } from 'react';
import { useGraphStore } from '../stores/graphStore';
import type { GraphNode, NodeAction } from '../types';
import { nodeTypeColor, nodeTypeLabel } from '../lib/colors';

interface DetailPanelProps {
  onAction?: (action: NodeAction, nodeId: string) => void;
}

const ACTION_LABELS: Record<NodeAction, string> = {
  activate_from: '⚡ Activate from here',
  impact: '💥 Blast radius',
  why_from: '→ Why connected',
  predict: '🔮 Predict co-changes',
  hypothesize: '💡 Hypothesize',
  counterfactual: '✕ Remove (counterfactual)',
  timeline: '⏱ Timeline',
  open_perspective: '◈ Open perspective',
  branch_perspective: '⎇ Branch perspective',
};

function ConnectionItem({ nodeId, relation, weight }: { nodeId: string; relation: string; weight: number }) {
  const { selectNode, rawNodes } = useGraphStore();
  const node = rawNodes.find((n) => n.id === nodeId);
  const label = node?.label ?? nodeId.split('/').pop() ?? nodeId;
  const color = nodeTypeColor(node?.node_type ?? 3);

  return (
    <button
      className="w-full text-left flex items-center justify-between px-2 py-1 rounded hover:bg-m1nd-elevated transition-colors group"
      onClick={() => selectNode(nodeId)}
    >
      <div className="flex items-center gap-1.5 min-w-0">
        <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ backgroundColor: color }} />
        <span className="text-slate-300 text-[11px] truncate group-hover:text-slate-100">{label}</span>
      </div>
      <div className="flex items-center gap-1.5 shrink-0 ml-2">
        <span className="text-[10px] text-slate-600">{relation}</span>
        <span className="text-[10px] text-slate-500 font-mono">{weight.toFixed(2)}</span>
      </div>
    </button>
  );
}

export default function DetailPanel({ onAction }: DetailPanelProps) {
  const { selectedNodeId, rawNodes, rawEdges } = useGraphStore();

  const node: GraphNode | null = useMemo(
    () => rawNodes.find((n) => n.id === selectedNodeId) ?? null,
    [selectedNodeId, rawNodes],
  );

  const connections = useMemo(() => {
    if (!selectedNodeId) return [];
    return rawEdges
      .filter((e) => e.source === selectedNodeId || e.target === selectedNodeId)
      .map((e) => ({
        nodeId: e.source === selectedNodeId ? e.target : e.source,
        relation: e.relation,
        weight: e.weight,
        direction: e.source === selectedNodeId ? 'out' : 'in',
      }))
      .sort((a, b) => b.weight - a.weight)
      .slice(0, 20);
  }, [selectedNodeId, rawEdges]);

  if (!node) {
    return (
      <div className="w-80 border-l border-m1nd-border-subtle bg-m1nd-surface flex flex-col items-center justify-center text-slate-700 text-xs shrink-0">
        <div className="text-center space-y-1">
          <div className="text-lg">◇</div>
          <div>Select a node</div>
        </div>
      </div>
    );
  }

  const color = nodeTypeColor(node.node_type);
  const typeLabel = nodeTypeLabel(node.node_type);
  const shortPath = node.source_path
    ? node.source_path.replace(/.*\/([^/]+\/[^/]+)$/, '$1')
    : null;

  return (
    <div className="w-80 border-l border-m1nd-border-subtle bg-m1nd-surface flex flex-col shrink-0 overflow-hidden">
      {/* Header */}
      <div className="px-4 py-3 border-b border-m1nd-border-subtle" style={{ borderLeftColor: color, borderLeftWidth: 3 }}>
        <div className="flex items-start justify-between">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2 mb-1">
              <span className="text-[10px] font-bold uppercase tracking-wide" style={{ color }}>
                {typeLabel}
              </span>
              {node.layer != null && (
                <span className="text-[10px] text-slate-600">L{node.layer}</span>
              )}
            </div>
            <div className="text-slate-100 text-sm font-mono font-semibold break-all leading-snug">
              {node.label}
            </div>
            {shortPath && (
              <div className="text-[11px] text-slate-500 mt-1 truncate" title={node.source_path}>
                {shortPath}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Scores */}
      <div className="px-4 py-3 border-b border-m1nd-border-subtle grid grid-cols-2 gap-2">
        <div>
          <div className="text-[10px] text-slate-600 mb-0.5">activation</div>
          <div className="flex items-center gap-1.5">
            <div className="flex-1 h-1.5 bg-m1nd-elevated rounded-full overflow-hidden">
              <div
                className="h-full rounded-full transition-all"
                style={{ width: `${Math.round(node.activation * 100)}%`, backgroundColor: color }}
              />
            </div>
            <span className="text-[11px] font-mono text-slate-300 w-10 text-right">
              {node.activation.toFixed(3)}
            </span>
          </div>
        </div>
        {node.pagerank != null && (
          <div>
            <div className="text-[10px] text-slate-600 mb-0.5">pagerank</div>
            <div className="text-sm font-mono text-slate-300">{node.pagerank.toFixed(4)}</div>
          </div>
        )}
        {node.trust != null && (
          <div>
            <div className="text-[10px] text-slate-600 mb-0.5">trust</div>
            <div className="text-sm font-mono text-slate-300">{node.trust.toFixed(3)}</div>
          </div>
        )}
      </div>

      {/* Tags */}
      {node.tags.length > 0 && (
        <div className="px-4 py-2 border-b border-m1nd-border-subtle flex flex-wrap gap-1">
          {node.tags.map((t) => (
            <span key={t} className="text-[10px] px-1.5 py-0.5 bg-m1nd-elevated border border-m1nd-border-medium text-slate-400 rounded">
              {t}
            </span>
          ))}
        </div>
      )}

      {/* Actions */}
      {onAction && (
        <div className="px-4 py-2 border-b border-m1nd-border-subtle">
          <div className="text-[10px] text-slate-600 mb-1.5 uppercase tracking-wide">Actions</div>
          <div className="space-y-0.5">
            {(['activate_from', 'impact', 'predict', 'counterfactual'] as NodeAction[]).map((action) => (
              <button
                key={action}
                onClick={() => onAction(action, node.id)}
                className="w-full text-left text-[11px] text-slate-400 px-2 py-1 rounded hover:bg-m1nd-elevated hover:text-slate-200 transition-colors"
              >
                {ACTION_LABELS[action]}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Connections */}
      <div className="flex-1 overflow-y-auto px-4 py-2">
        <div className="text-[10px] text-slate-600 mb-1.5 uppercase tracking-wide">
          Connections ({connections.length})
        </div>
        {connections.length === 0 && (
          <div className="text-xs text-slate-700 italic">No connections in current subgraph</div>
        )}
        <div className="space-y-0.5">
          {connections.map((c) => (
            <ConnectionItem
              key={`${c.nodeId}-${c.direction}`}
              nodeId={c.nodeId}
              relation={c.relation}
              weight={c.weight}
            />
          ))}
        </div>
      </div>
    </div>
  );
}
