import React, { useEffect, useState } from 'react';
import { api } from '../api/client';
import { useGraphStore } from '../stores/graphStore';

interface TopBarProps {
  onIngestClick: () => void;
}

export default function TopBar({ onIngestClick }: TopBarProps) {
  const { nodes, edges, isLoading } = useGraphStore();
  const [health, setHealth] = useState<{ status: string; node_count: number; edge_count: number } | null>(null);
  const [liveSync, setLiveSync] = useState(false);

  useEffect(() => {
    let mounted = true;
    function poll() {
      api.health()
        .then((h) => { if (mounted) setHealth({ status: h.status, node_count: h.node_count, edge_count: h.edge_count }); })
        .catch(() => { if (mounted) setHealth((prev) => prev ? { ...prev, status: 'down' } : { status: 'down', node_count: 0, edge_count: 0 }); });
    }
    poll();
    const id = setInterval(poll, 5000);
    return () => { mounted = false; clearInterval(id); };
  }, []);

  // Live sync polling
  useEffect(() => {
    if (!liveSync) return;
    const id = setInterval(() => {
      const q = useGraphStore.getState().query;
      if (!q) return;
      api.callTool('m1nd.activate', { agent_id: 'gui', query: q, top_k: 30 })
        .catch(() => {});
    }, 3000);
    return () => clearInterval(id);
  }, [liveSync]);

  const healthDot: Record<string, string> = {
    ok: '#059669', degraded: '#f59e0b', empty: '#6366f1', down: '#ef4444', reconnecting: '#f59e0b',
  };
  const dotColor = healthDot[health?.status ?? 'down'] ?? '#64748b';
  const displayNodes = nodes.length > 0 ? nodes.length : (health?.node_count ?? 0);
  const displayEdges = edges.length > 0 ? edges.length : (health?.edge_count ?? 0);

  return (
    <div className="h-12 flex items-center justify-between px-4 border-b border-m1nd-border-subtle bg-m1nd-surface shrink-0">
      <div className="flex items-center gap-4">
        <div className="flex items-center gap-2">
          <span className="text-m1nd-accent font-bold text-base tracking-tight">m1nd</span>
          <span className="w-2 h-2 rounded-full inline-block" style={{ backgroundColor: dotColor }}
            title={`Status: ${health?.status ?? 'connecting'}`} />
        </div>
        <div className="flex items-center gap-3 text-xs text-slate-500">
          <span><span className="text-slate-300 font-mono">{displayNodes.toLocaleString()}</span><span className="ml-1">nodes</span></span>
          <span className="text-slate-700">·</span>
          <span><span className="text-slate-300 font-mono">{displayEdges.toLocaleString()}</span><span className="ml-1">edges</span></span>
        </div>
        {isLoading && <span className="text-xs text-m1nd-accent animate-pulse">querying...</span>}
      </div>
      <div className="flex items-center gap-2">
        <span className="text-[10px] text-slate-600 hidden sm:inline">⌘K to query</span>
        <button onClick={() => setLiveSync(!liveSync)}
          className={`flex items-center gap-1.5 px-3 py-1.5 text-xs border rounded transition-colors ${
            liveSync ? 'bg-emerald-900/30 border-emerald-600 text-emerald-400' : 'bg-m1nd-elevated border-m1nd-border-medium text-slate-500'}`}
          title="Auto-refresh graph every 3s">
          <span className={liveSync ? 'animate-pulse' : ''}>⟳</span><span>Live Sync</span>
        </button>
        <button onClick={onIngestClick}
          className="flex items-center gap-1.5 px-3 py-1.5 text-xs bg-m1nd-elevated border border-m1nd-border-medium text-slate-300 rounded hover:border-m1nd-accent hover:text-m1nd-accent transition-colors"
          title="Ingest codebase (⌘I)">
          <span>⬆</span><span>Ingest</span>
        </button>
      </div>
    </div>
  );
}
