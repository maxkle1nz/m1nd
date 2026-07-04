/*
 * m1nd workspace — the served UI (HUMAN-LAYER-PRD §5).
 *
 * Slice 0: the Living Tree is the FRONT DOOR. The force-directed map is demoted
 * to a rung-2 drill-down and is not mounted at the landing screen (§1 decision 2);
 * the existing GraphCanvas / DetailPanel / CommandPalette survive in the tree for
 * later slices (map drill-down, pre-flight, change preview) and are intentionally
 * not rendered here. SOFT PROOF tokens + the violet quarantine land in this slice.
 */
import React, { useCallback, useEffect, useState } from 'react';
import LivingTree from './components/tree/LivingTree';
import HallView from './components/hall/HallView';
import BrainChip from './components/hall/BrainChip';
import { useToastStore } from './stores/toastStore';
import ToastContainer from './components/ToastContainer';
import { useSSE } from './hooks/useSSE';
import { api } from './api/client';
import { useM1ndApi } from './hooks/useM1ndApi';
import type { InstanceSelfResponse, SseEvent, SseIngestData } from './types';

// App-level error boundary.
class AppErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { hasError: boolean; error: string }
> {
  state = { hasError: false, error: '' };
  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error: error.message };
  }
  componentDidCatch(error: Error) {
    console.error('[m1nd App error]', error);
  }
  render() {
    if (this.state.hasError) {
      return (
        <div className="w-screen h-screen flex items-center justify-center bg-porcelain text-ink">
          <div className="text-center space-y-4 max-w-md px-6">
            <div className="text-state-failure text-lg">Something went wrong.</div>
            <div className="text-xs text-ink-soft font-mono">{this.state.error}</div>
            <button
              onClick={() => window.location.reload()}
              className="px-4 py-2 text-sm bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow"
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

type BackendStatus = 'ok' | 'degraded' | 'empty' | 'down';

function useBackendStatus() {
  const [status, setStatus] = useState<BackendStatus>('down');
  const { fetchHealth } = useM1ndApi();
  useEffect(() => {
    let mounted = true;
    const poll = () =>
      fetchHealth()
        .then((h) => {
          if (mounted) setStatus(h.status as BackendStatus);
        })
        .catch(() => {
          if (mounted) setStatus('down');
        });
    poll();
    const id = setInterval(poll, 5000);
    return () => {
      mounted = false;
      clearInterval(id);
    };
  }, [fetchHealth]);
  return status;
}

/**
 * Minimal SOFT PROOF top bar — no violet chrome (that's quarantined to abstain).
 * Carries the Brain Chip (§4A.5): no graph pixel without the owning brain's name
 * in view. The chip is on EVERY surface, sourced from the same self envelope.
 */
function TopBar({
  status,
  self,
  onOpenHall,
}: {
  status: BackendStatus;
  self: InstanceSelfResponse | null;
  onOpenHall: () => void;
}) {
  const dot =
    status === 'ok'
      ? 'var(--verdict-act, #6fa287)'
      : status === 'degraded'
        ? 'var(--verdict-reverify, #c89b3c)'
        : status === 'empty'
          ? 'var(--state-unverified, #b8b2a8)'
          : 'var(--state-failure, #b0563b)';
  return (
    <div className="h-12 flex items-center justify-between px-4 border-b border-ink/10 bg-porcelain shrink-0">
      <div className="flex items-center gap-2">
        <span className="text-ink font-semibold text-base tracking-tight">m1nd</span>
        <span
          className="w-2 h-2 rounded-full inline-block"
          style={{ backgroundColor: dot }}
          title={`status: ${status}`}
        />
      </div>
      <BrainChip
        workspaceRoot={self ? self.graph_state.workspace_root ?? self.instance.workspace_root : null}
        nodeCount={self ? self.graph_state.node_count : null}
        healthy={status === 'ok'}
        onClick={onOpenHall}
      />
    </div>
  );
}

/** Poll the bound-brain self envelope — the Brain Chip's data source (§4A.5). */
function useSelf(enabled: boolean) {
  const [self, setSelf] = useState<InstanceSelfResponse | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let mounted = true;
    const poll = () =>
      api
        .instanceSelf()
        .then((s) => mounted && setSelf(s))
        .catch(() => mounted && setSelf(null));
    poll();
    const id = setInterval(poll, 5000);
    return () => {
      mounted = false;
      clearInterval(id);
    };
  }, [enabled]);
  return self;
}

/** Ingest modal (unchanged mechanics, SOFT PROOF skin). */
function IngestModal({
  isOpen,
  onClose,
  onComplete,
}: {
  isOpen: boolean;
  onClose: () => void;
  onComplete: () => void;
}) {
  const [path, setPath] = useState('');
  const [loading, setLoading] = useState(false);
  const { runQuery } = useM1ndApi();
  if (!isOpen) return null;
  const submit = async (e: React.FormEvent) => {
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
      <div className="fixed inset-0 bg-ink/30 z-40" onClick={onClose} />
      <div className="fixed top-1/3 left-1/2 -translate-x-1/2 z-50 w-full max-w-md mx-4">
        <div className="bg-porcelain border border-ink/15 rounded-lg shadow-card p-6">
          <h2 className="text-ink font-semibold mb-1">Read a codebase</h2>
          <p className="text-xs text-ink-soft mb-4">Load a directory into the m1nd graph.</p>
          <form onSubmit={submit} className="space-y-3">
            <input
              type="text"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="/path/to/your/project"
              className="w-full bg-bone/60 border border-ink/15 text-ink text-sm font-mono rounded px-3 py-2 outline-none focus:border-ink/30 placeholder-ink-soft/60"
              autoFocus
            />
            <div className="flex gap-2 justify-end">
              <button type="button" onClick={onClose} className="px-4 py-2 text-sm text-ink-soft hover:text-ink">
                Cancel
              </button>
              <button
                type="submit"
                disabled={loading || !path.trim()}
                className="px-4 py-2 text-sm bg-bone text-ink border border-ink/25 rounded hover:shadow-contact disabled:opacity-50 transition-shadow"
              >
                {loading ? 'Reading…' : 'Read it'}
              </button>
            </div>
          </form>
        </div>
      </div>
    </>
  );
}

/** The surface the shell is showing. The Hall is rung −1; the tree is rung 0. */
type Surface = 'tree' | 'hall';

export default function App() {
  const [ingestOpen, setIngestOpen] = useState(false);
  const [surface, setSurface] = useState<Surface>('tree');
  const status = useBackendStatus();
  const self = useSelf(status === 'ok' || status === 'degraded');
  const addToast = useToastStore((s) => s.addToast);
  const { runQuery } = useM1ndApi();

  const handleSSE = useCallback(
    (event: SseEvent) => {
      if (event.event_type === 'ingest') {
        const d = event.data as SseIngestData;
        addToast(`Read complete: +${d.nodes_added} nodes`, d.path, 'success');
      }
    },
    [addToast],
  );

  useSSE({ onEvent: handleSSE, enabled: status === 'ok' || status === 'degraded' });

  // The ESC ladder (§3.4 / §4A.1): ESC at the tree ROOT ascends to the Hall
  // (rung −1). The tree owns ESC while a row/drawer is focused; only when nothing
  // is selected does ESC bubble to window and ascend. The Hall owns its own ESC.
  const onWindowEsc = useCallback(
    (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (surface !== 'tree') return; // the Hall handles its own ESC (ascends out)
      // Only ascend if the tree isn't holding focus on a row/drawer/input.
      const active = document.activeElement;
      const treeIsFocused =
        active instanceof HTMLElement &&
        (active.closest('[role="tree"]') != null ||
          active.closest('[data-role="tree-drawer"]') != null ||
          active.tagName === 'INPUT');
      if (!treeIsFocused) setSurface('hall');
    },
    [surface],
  );
  useEffect(() => {
    window.addEventListener('keydown', onWindowEsc);
    return () => window.removeEventListener('keydown', onWindowEsc);
  }, [onWindowEsc]);

  return (
    <AppErrorBoundary>
      <div className="flex flex-col h-screen w-screen bg-porcelain text-ink font-sans overflow-hidden">
        <TopBar status={status} self={self} onOpenHall={() => setSurface('hall')} />
        <div className="flex flex-1 overflow-hidden">
          {surface === 'hall' ? (
            <HallView
              onExit={() => setSurface('tree')}
              onOpenBound={() => setSurface('tree')}
              onBootstrap={() => setIngestOpen(true)}
            />
          ) : (
            <LivingTree onIngest={() => setIngestOpen(true)} />
          )}
        </div>
        <IngestModal
          isOpen={ingestOpen}
          onClose={() => setIngestOpen(false)}
          onComplete={() => runQuery('health', { agent_id: 'gui' })}
        />
        <ToastContainer />
      </div>
    </AppErrorBoundary>
  );
}
