import React, { useCallback, useEffect, useRef, useState } from 'react';
import TopBar from './components/TopBar';
import GraphCanvas from './components/GraphCanvas';
import DetailPanel from './components/DetailPanel';
import CommandPalette from './components/CommandPalette';
import ActivationReplay from './components/ActivationReplay';
import ToastContainer from './components/ToastContainer';
import { useGraphStore } from './stores/graphStore';
import { useToastStore } from './stores/toastStore';
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts';
import { useSSE } from './hooks/useSSE';
import { useM1ndApi } from './hooks/useM1ndApi';
import type { ToolId, NodeAction, SseEvent, SseActivationData, SseIngestData, SseLearnData } from './types';

// App-level Error Boundary (FM-FE-056)
class AppErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { hasError: boolean; error: string }
> {
  state = { hasError: false, error: '' };
  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error: error.message };
  }
  componentDidCatch(error: Error) { console.error('[m1nd App error]', error); }
  render() {
    if (this.state.hasError) {
      return (
        <div className="w-screen h-screen flex items-center justify-center bg-m1nd-base text-slate-300">
          <div className="text-center space-y-4 max-w-md px-6">
            <div className="text-red-400 text-lg">Something went wrong.</div>
            <div className="text-xs text-slate-600 font-mono">{this.state.error}</div>
            <button
              onClick={() => window.location.reload()}
              className="px-4 py-2 text-sm bg-m1nd-elevated border border-m1nd-border-medium text-slate-300 rounded hover:border-m1nd-accent transition-colors"
            >
              Reload page
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

// Reconnection overlay (FM-FE-034, FM-FE-040)
type BackendStatus = 'ok' | 'degraded' | 'empty' | 'reconnecting' | 'down';

function useBackendHealth() {
  const [status, setStatus] = useState<BackendStatus>('reconnecting');
  const [retryCount, setRetryCount] = useState(0);
  const [nextRetryMs, setNextRetryMs] = useState(0);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const { fetchHealth } = useM1ndApi();

  const check = useCallback(async (attempt: number) => {
    try {
      const h = await fetchHealth();
      setStatus(h.status as BackendStatus);
      setRetryCount(0);
      setNextRetryMs(0);
      // Poll every 5s when healthy
      timerRef.current = setTimeout(() => check(0), 5000);
    } catch {
      const nextAttempt = attempt + 1;
      if (nextAttempt >= 6) {
        setStatus('down');
        setRetryCount(nextAttempt);
        return;
      }
      setStatus('reconnecting');
      setRetryCount(nextAttempt);
      const delay = Math.min(1000 * Math.pow(2, attempt), 30_000);
      setNextRetryMs(delay);
      timerRef.current = setTimeout(() => check(nextAttempt), delay);
    }
  }, [fetchHealth]);

  useEffect(() => {
    check(0);
    return () => { if (timerRef.current) clearTimeout(timerRef.current); };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const retry = useCallback(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    setStatus('reconnecting');
    check(0);
  }, [check]);

  return { status, retryCount, nextRetryMs, retry };
}

function ReconnectionOverlay({ status, retryCount, onRetry }: { status: BackendStatus; retryCount: number; onRetry: () => void }) {
  if (status === 'ok' || status === 'degraded' || status === 'empty') return null;

  return (
    <div className="fixed inset-0 z-50 bg-m1nd-base/95 flex items-center justify-center backdrop-blur-sm">
      <div className="text-center space-y-4 max-w-sm px-6">
        <div className="text-2xl text-slate-700">◈</div>
        {status === 'reconnecting' ? (
          <>
            <div className="text-slate-300 text-sm">Connecting to m1nd server...</div>
            <div className="text-xs text-slate-600">
              Attempt {retryCount + 1} · localhost:1337
            </div>
            <div className="flex justify-center">
              <div className="w-6 h-6 border-2 border-m1nd-accent border-t-transparent rounded-full animate-spin" />
            </div>
          </>
        ) : (
          <>
            <div className="text-red-400 text-sm">Connection lost to m1nd server.</div>
            <div className="text-xs text-slate-600">
              Make sure m1nd-mcp is running: <code className="font-mono">m1nd-mcp --serve</code>
            </div>
            <button
              onClick={onRetry}
              className="px-4 py-2 text-sm bg-m1nd-elevated border border-m1nd-border-medium text-slate-300 rounded hover:border-m1nd-accent transition-colors"
            >
              Click to retry
            </button>
          </>
        )}
      </div>
    </div>
  );
}

// Left sidebar — tool selector + query history + display toggles
function LeftSidebar({ onRunTool }: { onRunTool: (tool: ToolId, params: Record<string, unknown>) => void }) {
  const { activeTool, setActiveTool, queryHistory, colorMode, setColorMode, showGhostEdges, toggleGhostEdges, showMinimap, toggleMinimap, layout, setLayout } = useGraphStore();

  const EXPLORE_TOOLS: ToolId[] = ['activate', 'seek', 'missing', 'differential'];
  const ANALYZE_TOOLS: ToolId[] = ['impact', 'why', 'predict', 'counterfactual', 'fingerprint'];
  const NAV_TOOLS: ToolId[] = ['drift', 'timeline', 'warmup'];

  return (
    <div className="w-64 border-r border-m1nd-border-subtle bg-m1nd-surface flex flex-col text-xs shrink-0">
      {/* Tools */}
      <div className="p-3 border-b border-m1nd-border-subtle">
        <div className="text-[10px] text-slate-600 uppercase tracking-wide mb-2">Explore</div>
        {EXPLORE_TOOLS.map((t) => (
          <button
            key={t}
            className={`w-full text-left px-2 py-1.5 rounded mb-0.5 font-mono transition-colors ${
              activeTool === t ? 'bg-m1nd-elevated text-m1nd-accent' : 'text-slate-400 hover:bg-m1nd-elevated hover:text-slate-200'
            }`}
            onClick={() => setActiveTool(t)}
          >
            {t}
          </button>
        ))}
        <div className="text-[10px] text-slate-600 uppercase tracking-wide mb-2 mt-3">Analyze</div>
        {ANALYZE_TOOLS.map((t) => (
          <button
            key={t}
            className={`w-full text-left px-2 py-1.5 rounded mb-0.5 font-mono transition-colors ${
              activeTool === t ? 'bg-m1nd-elevated text-m1nd-accent' : 'text-slate-400 hover:bg-m1nd-elevated hover:text-slate-200'
            }`}
            onClick={() => setActiveTool(t)}
          >
            {t}
          </button>
        ))}
        <div className="text-[10px] text-slate-600 uppercase tracking-wide mb-2 mt-3">Navigate</div>
        {NAV_TOOLS.map((t) => (
          <button
            key={t}
            className={`w-full text-left px-2 py-1.5 rounded mb-0.5 font-mono transition-colors ${
              activeTool === t ? 'bg-m1nd-elevated text-m1nd-accent' : 'text-slate-400 hover:bg-m1nd-elevated hover:text-slate-200'
            }`}
            onClick={() => setActiveTool(t)}
          >
            {t}
          </button>
        ))}
      </div>

      {/* Display toggles */}
      <div className="p-3 border-b border-m1nd-border-subtle space-y-2">
        <div className="text-[10px] text-slate-600 uppercase tracking-wide mb-1">Display</div>
        <label className="flex items-center justify-between cursor-pointer">
          <span className="text-slate-400">Color by</span>
          <select
            value={colorMode}
            onChange={(e) => setColorMode(e.target.value as typeof colorMode)}
            className="bg-m1nd-elevated border border-m1nd-border-medium text-slate-300 rounded px-1.5 py-0.5 text-[11px]"
          >
            <option value="type">type</option>
            <option value="activation">activation</option>
            <option value="trust">trust</option>
            <option value="layer">layer</option>
          </select>
        </label>
        <label className="flex items-center justify-between cursor-pointer">
          <span className="text-slate-400">Layout</span>
          <select
            value={layout}
            onChange={(e) => setLayout(e.target.value as typeof layout)}
            className="bg-m1nd-elevated border border-m1nd-border-medium text-slate-300 rounded px-1.5 py-0.5 text-[11px]"
          >
            <option value="auto">auto</option>
            <option value="hierarchical">hierarchical</option>
            <option value="radial">radial</option>
          </select>
        </label>
        <label className="flex items-center justify-between cursor-pointer" onClick={toggleGhostEdges}>
          <span className="text-slate-400">Ghost edges</span>
          <div className={`w-8 h-4 rounded-full transition-colors ${showGhostEdges ? 'bg-m1nd-accent' : 'bg-m1nd-border-medium'}`}>
            <div className={`w-3 h-3 bg-white rounded-full mt-0.5 transition-transform ${showGhostEdges ? 'translate-x-4' : 'translate-x-0.5'}`} />
          </div>
        </label>
        <label className="flex items-center justify-between cursor-pointer" onClick={toggleMinimap}>
          <span className="text-slate-400">Minimap</span>
          <div className={`w-8 h-4 rounded-full transition-colors ${showMinimap ? 'bg-m1nd-accent' : 'bg-m1nd-border-medium'}`}>
            <div className={`w-3 h-3 bg-white rounded-full mt-0.5 transition-transform ${showMinimap ? 'translate-x-4' : 'translate-x-0.5'}`} />
          </div>
        </label>
      </div>

      {/* Query history */}
      {queryHistory.length > 0 && (
        <div className="flex-1 overflow-y-auto p-3">
          <div className="text-[10px] text-slate-600 uppercase tracking-wide mb-2">History</div>
          {queryHistory.slice(0, 10).map((h, i) => (
            <button
              key={i}
              className="w-full text-left px-2 py-1.5 rounded mb-0.5 hover:bg-m1nd-elevated transition-colors"
              onClick={() => onRunTool(h.tool, { query: h.query })}
            >
              <div className="text-slate-500 font-mono text-[10px]">{h.tool}</div>
              <div className="text-slate-400 truncate text-[11px]">{h.query}</div>
              <div className="text-slate-700 text-[10px]">{h.nodeCount} nodes</div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

// Ingest modal
function IngestModal({ isOpen, onClose, onComplete }: { isOpen: boolean; onClose: () => void; onComplete: () => void }) {
  const [path, setPath] = useState('');
  const [loading, setLoading] = useState(false);
  const { runQuery } = useM1ndApi();

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!path.trim()) return;
    setLoading(true);
    try {
      await runQuery('ingest', { path: path.trim(), agent_id: 'gui', incremental: false });
      onComplete();
      onClose();
    } finally {
      setLoading(false);
    }
  };

  return (
    <>
      <div className="fixed inset-0 bg-black/60 z-40" onClick={onClose} />
      <div className="fixed top-1/3 left-1/2 -translate-x-1/2 z-50 w-full max-w-md mx-4">
        <div className="bg-m1nd-surface border border-m1nd-border-strong rounded-lg shadow-2xl p-6">
          <h2 className="text-slate-200 font-semibold mb-1">Ingest Codebase</h2>
          <p className="text-xs text-slate-500 mb-4">Load a directory into the m1nd graph for querying.</p>
          <form onSubmit={handleSubmit} className="space-y-3">
            <input
              type="text"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="/path/to/your/project"
              className="w-full bg-m1nd-elevated border border-m1nd-border-medium text-slate-200 text-sm font-mono rounded px-3 py-2 outline-none focus:border-m1nd-accent placeholder-slate-700"
              autoFocus
            />
            <div className="flex gap-2 justify-end">
              <button type="button" onClick={onClose} className="px-4 py-2 text-sm text-slate-400 hover:text-slate-200 transition-colors">
                Cancel
              </button>
              <button
                type="submit"
                disabled={loading || !path.trim()}
                className="px-4 py-2 text-sm bg-m1nd-elevated border border-m1nd-accent text-m1nd-accent rounded hover:bg-m1nd-accent/10 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                {loading ? 'Ingesting...' : 'Ingest'}
              </button>
            </div>
          </form>
        </div>
      </div>
    </>
  );
}

export default function App() {
  const [cmdOpen, setCmdOpen] = useState(false);
  const [ingestOpen, setIngestOpen] = useState(false);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const { clearGraph, setError, applySSEEvent } = useGraphStore();
  const addToast = useToastStore((s) => s.addToast);
  const { runQuery, fetchHealth } = useM1ndApi();
  const { status, retryCount, retry } = useBackendHealth();

  // SSE — update graph when activations arrive + show toasts
  const handleSSEEvent = useCallback((event: SseEvent) => {
    // Apply graph updates for activation/learn events
    applySSEEvent(event);

    // Show toasts for user awareness
    switch (event.event_type) {
      case 'activation': {
        const d = event.data as SseActivationData;
        const count = d.activated?.length ?? 0;
        addToast(
          `Agent query: "${d.query}" \u2014 ${count} nodes activated`,
          `agent: ${d.agent_id}`,
          'info'
        );
        break;
      }
      case 'learn': {
        const d = event.data as SseLearnData;
        const nodeCount = d.node_ids?.length ?? 0;
        addToast(
          `Learn: ${d.feedback} \u2014 ${nodeCount} nodes`,
          `agent: ${d.agent_id}`,
          'learn'
        );
        break;
      }
      case 'ingest': {
        const d = event.data as SseIngestData;
        addToast(
          `Ingest complete: +${d.nodes_added} nodes, +${d.edges_added} edges`,
          d.path,
          'success'
        );
        break;
      }
      // persist events are silent
    }
  }, [applySSEEvent, addToast]);

  useSSE({
    onEvent: handleSSEEvent,
    enabled: status === 'ok' || status === 'degraded',
  });

  useKeyboardShortcuts({
    onCommandPalette: () => setCmdOpen(true),
    onEscape: () => { setCmdOpen(false); setIngestOpen(false); },
    onClearGraph: clearGraph,
    onIngest: () => setIngestOpen(true),
  });

  const handleExecute = useCallback((tool: ToolId, params: Record<string, unknown>) => {
    runQuery(tool, params);
  }, [runQuery]);

  const handleNodeAction = useCallback((action: NodeAction, nodeId: string) => {
    switch (action) {
      case 'activate_from':
        runQuery('activate', { query: nodeId, agent_id: 'gui', top_k: 30 });
        break;
      case 'impact':
        runQuery('impact', { node_id: nodeId, agent_id: 'gui' });
        break;
      case 'predict':
        runQuery('predict', { node_id: nodeId, agent_id: 'gui' });
        break;
      case 'counterfactual':
        runQuery('counterfactual', { node_id: nodeId, agent_id: 'gui' });
        break;
      default:
        break;
    }
  }, [runQuery]);

  const isBlocked = status === 'reconnecting' || status === 'down';

  return (
    <AppErrorBoundary>
      <div className="flex flex-col h-screen w-screen bg-m1nd-base text-slate-200 font-mono overflow-hidden">
        {/* TopBar */}
        <TopBar onIngestClick={() => setIngestOpen(true)} />

        {/* Main layout */}
        <div
          className="flex flex-1 overflow-hidden"
          style={{ pointerEvents: isBlocked ? 'none' : undefined }}
        >
          {/* Left sidebar */}
          <LeftSidebar onRunTool={handleExecute} />

          {/* Graph canvas (center) */}
          <div className="flex-1 relative overflow-hidden">
            <GraphCanvas
              onNodeSelect={setSelectedNodeId}
            />
          </div>

          {/* Right detail panel */}
          <DetailPanel onAction={handleNodeAction} />
        </div>

        {/* Overlays */}
        <CommandPalette
          isOpen={cmdOpen}
          onClose={() => setCmdOpen(false)}
          onExecute={handleExecute}
        />

        <IngestModal
          isOpen={ingestOpen}
          onClose={() => setIngestOpen(false)}
          onComplete={() => runQuery('health', { agent_id: 'gui' })}
        />

        {/* Activation replay (BUILD-R3 responsibility) */}
        <ActivationReplay />

        {/* SSE toast notifications */}
        <ToastContainer />

        {/* Reconnection overlay — MUST be last (highest z-index) */}
        <ReconnectionOverlay
          status={status}
          retryCount={retryCount}
          onRetry={retry}
        />
      </div>
    </AppErrorBoundary>
  );
}
