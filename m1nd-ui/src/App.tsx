/*
 * m1nd workspace — the served UI (HUMAN-LAYER-PRD §5).
 *
 * Slice 0: the Living Tree is the FRONT DOOR. The force-directed map is demoted
 * to a rung-2 drill-down and is not mounted at the landing screen (§1 decision 2);
 * the existing GraphCanvas / DetailPanel / CommandPalette survive in the tree for
 * later slices (map drill-down, pre-flight, change preview) and are intentionally
 * not rendered here. SOFT PROOF tokens + the violet quarantine land in this slice.
 */
import React, { useCallback, useEffect, useMemo, useState } from 'react';
import LivingTree from './components/tree/LivingTree';
import HallView from './components/hall/HallView';
import BrainChip from './components/hall/BrainChip';
import ThresholdCard from './components/hall/ThresholdCard';
import OrientationBeats from './components/hall/OrientationBeats';
import BrainPalette from './components/hall/BrainPalette';
import { useToastStore } from './stores/toastStore';
import ToastContainer from './components/ToastContainer';
import { useSSE } from './hooks/useSSE';
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts';
import { api } from './api/client';
import { useM1ndApi } from './hooks/useM1ndApi';
import type { NorthPacket } from './api/toolTypes';
import type { InstanceRegistryEntry, InstanceSelfResponse, SseEvent, SseIngestData } from './types';
import {
  ingestSupportsProjectRoot,
  mayOfferForeignIngest,
  bootstrapParams,
  orientationDismissed,
} from './lib/threshold';
import { restBrainSelectorSupported, brainDisplayName, brainProjectPath } from './lib/hallSemantics';
import { BOUND_VIEW, type ViewedBrain } from './lib/viewedBrain';

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
  viewedBrain,
  onOpenHall,
}: {
  status: BackendStatus;
  self: InstanceSelfResponse | null;
  /** The brain the tree is viewing (§4A.9). Bound (root=null) → the chip reads the
   *  self envelope; a hosted brain → the chip flips to the ECHO's name/counts
   *  (render truth, never the request), so no graph pixel ever wears the wrong
   *  brain's name (§4A.5 law). */
  viewedBrain: ViewedBrain;
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
  const hosted = viewedBrain.root != null;
  const chipName = hosted ? viewedBrain.displayName : self?.display_name ?? null;
  const chipPath = hosted ? viewedBrain.root : self?.project_root ?? null;
  const chipCount = hosted ? viewedBrain.nodeCount : self ? self.graph_state.node_count : null;
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
        displayName={chipName}
        projectPath={chipPath}
        nodeCount={chipCount}
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

/**
 * Ingest modal — SOFT PROOF skin + the clobber ban (§4A.4, INV-11).
 *
 * A bare foreign-path ingest REPLACES the bound graph for everyone (field-proven
 * on Cherry/almus). So on a NON-EMPTY owner without project_root routing, a
 * foreign path is never offered — the affordance is the isolated one-call
 * bootstrap (when advertised) or nothing. On an empty owner it is safe (nothing
 * to clobber). Feature-detected via GET /api/tools, never assumed.
 */
function IngestModal({
  isOpen,
  onClose,
  onComplete,
  ownerHasGraph,
}: {
  isOpen: boolean;
  onClose: () => void;
  onComplete: () => void;
  ownerHasGraph: boolean;
}) {
  const [path, setPath] = useState('');
  const [loading, setLoading] = useState(false);
  const [supportsProjectRoot, setSupportsProjectRoot] = useState(false);

  useEffect(() => {
    if (!isOpen) return;
    let mounted = true;
    api
      .tools()
      .then((r) => mounted && setSupportsProjectRoot(ingestSupportsProjectRoot(r.tools)))
      .catch(() => mounted && setSupportsProjectRoot(false));
    return () => {
      mounted = false;
    };
  }, [isOpen]);

  if (!isOpen) return null;

  const foreignAllowed = mayOfferForeignIngest({ ownerHasGraph, supportsProjectRoot });
  const isolated = ownerHasGraph && supportsProjectRoot; // the bootstrap isolates the new brain

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!path.trim() || !foreignAllowed) return;
    setLoading(true);
    try {
      // On a non-empty owner the only safe path is the isolated bootstrap
      // (project_root). On an empty owner a plain ingest is safe.
      await api.tool('ingest', bootstrapParams(path, isolated || supportsProjectRoot));
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
          <h2 className="text-ink font-semibold mb-1">{isolated ? 'Read a new repo' : 'Read a codebase'}</h2>
          <p className="text-xs text-ink-soft mb-4">
            {isolated
              ? 'Reads it into its own brain — your current map is untouched.'
              : 'Load a directory into the m1nd graph.'}
          </p>
          {!foreignAllowed ? (
            <div
              data-role="clobber-ban"
              className="text-xs text-ink font-mono border border-ink/15 bg-bone/50 rounded px-3 py-3 space-y-2"
            >
              <p className="text-ink-soft">
                Reading a different repo would replace this brain's map for everyone. This owner can't isolate a
                second brain yet.
              </p>
              <p className="text-ink-soft">
                To re-read <span className="text-ink">this</span> repo, use “Re-read” on its card in the Hall.
              </p>
              <div className="flex justify-end pt-1">
                <button type="button" onClick={onClose} className="px-3 py-1 text-xs text-ink-soft hover:text-ink">
                  Close
                </button>
              </div>
            </div>
          ) : (
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
          )}
        </div>
      </div>
    </>
  );
}

/** The surface the shell is showing. Threshold is rung −∞; Hall is rung −1; tree is rung 0. */
type Surface = 'tree' | 'hall' | 'threshold';

/**
 * The brains the owner holds — the landing signal (§4A.1, INV-12) AND the Cmd+K
 * Brains group source (§4A.5). Registry order IS recency; never re-sorted.
 * Returns null until the first fetch resolves (deciding the landing).
 */
function useBrains(enabled: boolean): InstanceRegistryEntry[] | null {
  const [brains, setBrains] = useState<InstanceRegistryEntry[] | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let mounted = true;
    const poll = () =>
      api
        .instances()
        .then((r) => mounted && setBrains(r.instances))
        .catch(() => mounted && setBrains(null));
    poll();
    const id = setInterval(poll, 5000);
    return () => {
      mounted = false;
      clearInterval(id);
    };
  }, [enabled]);
  return brains;
}

/**
 * Feature-detect the §4A.9 REST brain selector (GET /api/tools stamp). Open enables
 * for hosted brains only when true — the 0T posture (never assumed). Polled once
 * the backend is up; false against a pre-2H owner and while unknown.
 */
function useRestBrainSelector(enabled: boolean): boolean {
  const [supported, setSupported] = useState(false);
  useEffect(() => {
    if (!enabled) return;
    let mounted = true;
    api
      .tools()
      .then((r) => mounted && setSupported(restBrainSelectorSupported(r)))
      .catch(() => mounted && setSupported(false));
    return () => {
      mounted = false;
    };
  }, [enabled]);
  return supported;
}

export default function App() {
  const [ingestOpen, setIngestOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [surface, setSurface] = useState<Surface | null>(null); // null = deciding the landing
  const [north, setNorth] = useState<NorthPacket | null>(null);
  const [orienting, setOrienting] = useState(false);
  // The brain the tree is viewing (§4A.9). Bound by default (byte-compatible);
  // Open on a hosted card sets it and switches to the tree.
  const [viewedBrain, setViewedBrain] = useState<ViewedBrain>(BOUND_VIEW);
  const status = useBackendStatus();
  const backendUp = status === 'ok' || status === 'degraded';
  const self = useSelf(backendUp);
  const brains = useBrains(backendUp);
  const restSelector = useRestBrainSelector(backendUp);
  const brainCount = brains?.length ?? null;
  const addToast = useToastStore((s) => s.addToast);
  const { runQuery } = useM1ndApi();
  const ownerHasGraph = (self?.graph_state.node_count ?? 0) > 0;

  // Open a brain IN THE TREE (§4A.9). The bound/self brain resets to BOUND_VIEW
  // (no selector — the tree's default graph); a hosted brain seeds the viewed
  // brain from its card (name + counts), which the served_brain echo then
  // confirms. Either way the shell shows the tree.
  const openBrainInTree = useCallback(
    (entry: InstanceRegistryEntry, isSelf: boolean) => {
      if (isSelf) {
        setViewedBrain(BOUND_VIEW);
      } else {
        setViewedBrain({
          root: brainProjectPath(entry),
          displayName: brainDisplayName(entry),
          nodeCount: entry.node_count ?? null,
        });
      }
      setSurface('tree');
    },
    [],
  );

  // Decide the landing ONCE the owner state is known (§4A.1 placement doctrine).
  // Zero brains → the Threshold (empty-state-as-onboarding). Otherwise the tree
  // (experts land in their work; the Hall is one ESC away). INV-12: a returning
  // user never meets the Threshold.
  useEffect(() => {
    if (surface != null || brainCount == null) return;
    setSurface(brainCount <= 0 ? 'threshold' : 'tree');
  }, [surface, brainCount]);

  // Cmd+K opens the Brains group (§4A.5). Not on the Threshold (no brains yet).
  useKeyboardShortcuts({
    onCommandPalette: () => {
      if (surface !== 'threshold') setPaletteOpen((o) => !o);
    },
  });

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
  // Orientation ESC is owned by OrientationBeats; the ladder stands down while it
  // is up so one ESC doesn't both dismiss orientation AND ascend.
  const onWindowEsc = useCallback(
    (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (surface !== 'tree' || orienting) return;
      const active = document.activeElement;
      const treeIsFocused =
        active instanceof HTMLElement &&
        (active.closest('[role="tree"]') != null ||
          active.closest('[data-role="tree-drawer"]') != null ||
          active.tagName === 'INPUT');
      if (!treeIsFocused) setSurface('hall');
    },
    [surface, orienting],
  );
  useEffect(() => {
    window.addEventListener('keydown', onWindowEsc);
    return () => window.removeEventListener('keydown', onWindowEsc);
  }, [onWindowEsc]);

  // After bootstrap: land on the tree and, unless the user already dismissed it
  // forever, run the 3-beat orientation seeded from the real north packet (§4A.2).
  const landAndOrient = useCallback(async () => {
    setSurface('tree');
    if (typeof window !== 'undefined' && orientationDismissed(window.localStorage)) return;
    try {
      const packet = await api.tool<NorthPacket>('north', { task: 'first look at this repo' });
      setNorth(packet);
      setOrienting(true);
    } catch {
      // No orientation is better than a fabricated one — land on the tree quietly.
    }
  }, []);

  // Orientation data from the real north packet (absent → the beat is not shown).
  const orientData = useMemo(() => {
    const fp = (north?.binding as { fingerprint?: { node_count?: number; edge_count?: number } } | undefined)
      ?.fingerprint;
    const anchors = ((north?.context?.anchors ?? []) as Array<{ label?: string }>)
      .map((a) => a.label ?? '')
      .filter(Boolean);
    const memoryCount = north ? north.memory.length : null;
    return {
      nodeCount: fp?.node_count ?? null,
      edgeCount: fp?.edge_count ?? null,
      anchorLabels: anchors,
      memoryCount,
      gaps: north?.honest_gaps ?? [],
    };
  }, [north]);

  return (
    <AppErrorBoundary>
      <div className="flex flex-col h-screen w-screen bg-porcelain text-ink font-sans overflow-hidden">
        <TopBar
          status={status}
          self={self}
          viewedBrain={viewedBrain}
          onOpenHall={() => surface !== 'threshold' && setSurface('hall')}
        />
        <div className="flex flex-1 overflow-hidden">
          {surface === 'threshold' ? (
            <ThresholdCard onBootstrapped={landAndOrient} />
          ) : surface === 'hall' ? (
            <HallView
              onExit={() => setSurface('tree')}
              onOpenBound={() => openBrainInTree({} as InstanceRegistryEntry, true)}
              onOpenBrain={openBrainInTree}
              onBootstrap={() => setIngestOpen(true)}
              restSelector={restSelector}
              viewedRoot={viewedBrain.root}
            />
          ) : (
            <LivingTree viewedBrain={viewedBrain} onIngest={() => setIngestOpen(true)} />
          )}
        </div>

        {/* The 3-beat orientation (§4A.2) — only after a fresh bootstrap, never returns. */}
        {orienting && surface === 'tree' && typeof window !== 'undefined' && (
          <OrientationBeats
            kv={window.localStorage}
            nodeCount={orientData.nodeCount}
            edgeCount={orientData.edgeCount}
            anchorLabels={orientData.anchorLabels}
            memoryCount={orientData.memoryCount}
            gaps={orientData.gaps}
            onSpent={() => setOrienting(false)}
          />
        )}

        <IngestModal
          isOpen={ingestOpen}
          onClose={() => setIngestOpen(false)}
          onComplete={() => runQuery('health', { agent_id: 'gui' })}
          ownerHasGraph={ownerHasGraph}
        />
        <BrainPalette
          isOpen={paletteOpen}
          onClose={() => setPaletteOpen(false)}
          instances={brains ?? []}
          selfId={self?.instance.instance_id ?? null}
          onOpenBound={() => openBrainInTree({} as InstanceRegistryEntry, true)}
          onOpenBrain={openBrainInTree}
          restSelector={restSelector}
        />
        <ToastContainer />
      </div>
    </AppErrorBoundary>
  );
}
