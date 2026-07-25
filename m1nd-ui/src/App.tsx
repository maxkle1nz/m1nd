/*
 * m1nd workspace — the served UI (HUMAN-LAYER-PRD §5).
 *
 * Slice 0: the Living Tree is the FRONT DOOR. The force-directed map is demoted
 * to a rung-2 drill-down and is not mounted at the landing screen (§1 decision 2);
 * the existing GraphCanvas / DetailPanel / CommandPalette survive in the tree for
 * later slices (map drill-down, pre-flight, change preview) and are intentionally
 * not rendered here. SOFT PROOF tokens + the violet quarantine land in this slice.
 */
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import LivingTree from './components/tree/LivingTree';
import HallView from './components/hall/HallView';
import BuildMapView from './components/map/BuildMapView';
import BrainChip from './components/hall/BrainChip';
import ManifestStatus from './components/ManifestStatus';
import ThresholdCard from './components/hall/ThresholdCard';
import BrainPalette from './components/hall/BrainPalette';
import MissionTrayLive from './components/tray/MissionTrayLive';
import UniverseView from './components/universe/UniverseView';
import { useToastStore } from './stores/toastStore';
import ToastContainer from './components/ToastContainer';
import { useSSE } from './hooks/useSSE';
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts';
import { useBuildMap } from './hooks/useBuildMap';
import { api } from './api/client';
import { useM1ndApi } from './hooks/useM1ndApi';
import { useUniverse } from './hooks/useUniverse';
import type { InstanceRegistryEntry, InstanceSelfResponse, SseEvent, SseIngestData } from './types';
import { restBrainSelectorSupported, brainDisplayName, brainProjectPath } from './lib/hallSemantics';
import { BOUND_VIEW, type ViewedBrain } from './lib/viewedBrain';
import {
  parseRoute,
  serializeRoute,
  resolveDeepLink,
  type Surface,
  type NavTarget,
  type ParsedRoute,
} from './lib/router';

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
  onOpenMap,
  onOpenUniverse,
}: {
  status: BackendStatus;
  self: InstanceSelfResponse | null;
  /** The brain the tree is viewing (§4A.9). Bound (root=null) → the chip reads the
   *  self envelope; a hosted brain → the chip flips to the ECHO's name/counts
   *  (render truth, never the request), so no graph pixel ever wears the wrong
   *  brain's name (§4A.5 law). */
  viewedBrain: ViewedBrain;
  onOpenHall: () => void;
  /** Open the Build Map for the viewed brain — the missing tree/hall→map door
   *  (the F1 gap became blocking once the map went brain-aware). */
  onOpenMap?: () => void;
  /** Return to the Universe L0 (the home panorama) — present only when the owner
   *  serves ≥1 world, so the wordmark becomes the home affordance (F30). */
  onOpenUniverse?: () => void;
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
        {onOpenUniverse ? (
          <button
            type="button"
            data-role="open-universe"
            onClick={onOpenUniverse}
            title="Back to the Universe"
            className="text-ink font-semibold text-base tracking-tight hover:text-ink-soft transition-colors"
          >
            m1nd
          </button>
        ) : (
          <span className="text-ink font-semibold text-base tracking-tight">m1nd</span>
        )}
        <span
          className="w-2 h-2 rounded-full inline-block"
          style={{ backgroundColor: dot }}
          title={`status: ${status}`}
        />
        <ManifestStatus brain={viewedBrain.root} />
        {onOpenMap && (
          <button
            type="button"
            data-role="open-map"
            onClick={onOpenMap}
            title="Open the Build Map for the viewed brain"
            className="ml-2 px-2 py-0.5 text-xs text-ink-soft border border-ink/10 rounded hover:shadow-contact transition-shadow"
          >
            Build Map
          </button>
        )}
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
 * Cross-root ingest modal — fail-honest P0. The internal owner bootstrap is not
 * a public mutation surface. Until its exact typed G2/G3 consumer exists, every
 * entry point renders the closed state and sends no request.
 */
function IngestModal({
  isOpen,
  onClose,
}: {
  isOpen: boolean;
  onClose: () => void;
}) {
  if (!isOpen) return null;

  return (
    <>
      <div className="fixed inset-0 bg-ink/30 z-40" onClick={onClose} />
      <div className="fixed top-1/3 left-1/2 -translate-x-1/2 z-50 w-full max-w-md mx-4">
        <div className="bg-porcelain border border-ink/15 rounded-lg shadow-card p-6">
          <h2 className="text-ink font-semibold mb-1">New project brain unavailable</h2>
          <p className="text-xs text-ink-soft mb-4">
            This owner has no authority-backed bootstrap consumer. No repository was changed.
          </p>
          <div
            data-role="bootstrap-unavailable"
            className="text-xs text-ink font-mono border border-ink/15 bg-bone/50 rounded px-3 py-3 space-y-2"
          >
            <p>brain_bootstrap_consumer_not_installed</p>
            <p className="text-ink-soft">Use “Re-read” only for a brain this owner already hosts.</p>
            <div className="flex justify-end pt-1">
              <button type="button" onClick={onClose} className="px-3 py-1 text-xs text-ink-soft hover:text-ink">
                Close
              </button>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

/* The surface the shell is showing (`Surface`, in ./lib/router). The Universe
 * ('universe') is the L0 landing — the per-world panorama that leads when the owner
 * serves ≥1 project brain (F30). Threshold is rung −∞; Hall is rung −1; tree is rung
 * 0; the Build Map ('map') is a world's room, one click from the Universe. Every
 * transition now flows through the single `navigate()` below, which syncs the URL
 * (hash router) — deep links and a real back/forward. */

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
  // The brain the tree is viewing (§4A.9). Bound by default (byte-compatible);
  // Open on a hosted card sets it and switches to the tree.
  const [viewedBrain, setViewedBrain] = useState<ViewedBrain>(BOUND_VIEW);
  // The block a mission-tray card asked to open on the map (F2.5 §3b). Seeds the
  // Build Map's selection so a tray click lands the human on the named block.
  const [mapTargetBlock, setMapTargetBlock] = useState<string | null>(null);
  // Honest doors: the Universe Landing's owner item routes to the Hall AND asks it to
  // open on the owner-alerts panel. True only when the Hall was entered via that item.
  const [hallOpenAlerts, setHallOpenAlerts] = useState(false);
  const status = useBackendStatus();
  const backendUp = status === 'ok' || status === 'degraded';
  // The Build Map snapshot does NOT depend on the graph: an 'empty' owner (no
  // graph yet) can still hold a ratified skeleton — the first-run case the
  // front door exists for. Only a DOWN backend blocks the read.
  const backendReachable = status !== 'down';
  const self = useSelf(backendUp);
  const brains = useBrains(backendUp);
  const restSelector = useRestBrainSelector(backendUp);
  // The Build Map read (HUMAN-VIEW-V2 F1) — decides the front door and feeds the
  // 'map' surface. Read-only; gated on the backend being up.
  const buildMap = useBuildMap(backendReachable);
  // The Universe panorama read (HUMAN-VIEW-V2 F30) — the L0 landing's data AND the
  // entry signal (≥1 world → land on the Universe). Read-only; a pre-F30 owner
  // degrades to an empty panorama, so the entry rule falls through untouched.
  const { universe, status: universeStatus, note: universeNote } = useUniverse(backendReachable);
  const worldCount = universe.worlds.length;
  const brainCount = brains?.length ?? null;
  const addToast = useToastStore((s) => s.addToast);

  // ── The hash router (deep links + a real back) ───────────────────────────────
  // One `navigate()` writes both the shell state AND the URL (R4); nothing else
  // touches history. Refs mirror the two brain-scoped axes so navigate() can keep
  // the "unchanged" ones and still serialize the full location without a dep churn.
  const viewedBrainRef = useRef(viewedBrain);
  const mapTargetBlockRef = useRef(mapTargetBlock);
  useEffect(() => {
    viewedBrainRef.current = viewedBrain;
  }, [viewedBrain]);
  useEffect(() => {
    mapTargetBlockRef.current = mapTargetBlock;
  }, [mapTargetBlock]);

  // The ONE transition writer. Moves surface + viewed brain + map target (+ the
  // transient hall-alerts flag) together, then serializes the URL. `history`:
  // 'push' (a user navigation, default) → a new entry; 'replace' (the landing
  // baseline + the deep-link seed) → no new entry; 'none' (popstate: the browser
  // already moved) → sync state WITHOUT writing. An omitted view/block keeps the
  // current value; `block: null` clears it.
  const navigate = useCallback(
    (next: NavTarget, opts?: { history?: 'push' | 'replace' | 'none' }) => {
      const history = opts?.history ?? 'push';
      const nextView = next.view ?? viewedBrainRef.current;
      const nextBlock = next.block === undefined ? mapTargetBlockRef.current : next.block;
      setSurface(next.surface);
      if (next.view !== undefined) setViewedBrain(next.view);
      if (next.block !== undefined) setMapTargetBlock(next.block);
      setHallOpenAlerts(next.hallAlerts ?? false);
      // Keep the refs hot so a same-tick serialize (or follow-up nav) is correct.
      viewedBrainRef.current = nextView;
      mapTargetBlockRef.current = nextBlock;
      if (history === 'none' || typeof window === 'undefined') return;
      const url = serializeRoute(next.surface, nextView, nextBlock);
      const current = window.location.hash || '#/';
      if (url === current) {
        // Re-selecting the current surface: never stack a duplicate history entry.
        if (history === 'replace') window.history.replaceState(null, '', url);
        return;
      }
      if (history === 'replace') window.history.replaceState(null, '', url);
      else window.history.pushState(null, '', url);
    },
    [],
  );

  // The deep link parsed ONCE at mount — the seed that beats the landing rule. A
  // null route (no / '#/' / unknown hash) means "no deep link, land normally".
  const initialRoute = useMemo<ParsedRoute | null>(
    () => (typeof window !== 'undefined' ? parseRoute(window.location.hash) : null),
    [],
  );
  const [deepLinkPending, setDeepLinkPending] = useState<boolean>(initialRoute != null);
  const pendingRouteRef = useRef<ParsedRoute | null>(initialRoute);

  // Seed the shell from the deep link BEFORE the landing rule (which gates on
  // `surface == null`). A bound route applies at once; a world route waits for the
  // worlds/brains reads to settle, then resolves — or gives up (evicted brain /
  // basename collision / pre-F30 owner) so the human lands normally instead of
  // stranding in an empty map. While pending, the landing effect stands down.
  useEffect(() => {
    if (!deepLinkPending) return;
    const settled = universeStatus !== 'loading' && brains !== null;
    const outcome = resolveDeepLink(pendingRouteRef.current, universe.worlds, brains, settled);
    if (outcome.kind === 'apply') {
      navigate(outcome.intent, { history: 'replace' });
      pendingRouteRef.current = null;
      setDeepLinkPending(false);
    } else if (outcome.kind === 'give-up') {
      pendingRouteRef.current = null;
      setDeepLinkPending(false); // surface stays null → the landing rule decides
    }
    // 'pending' → wait for the next worlds/brains tick.
  }, [deepLinkPending, universe, brains, universeStatus, navigate]);

  // Real back/forward: popstate re-parses the hash and syncs state THROUGH navigate
  // (history:'none' — the browser already moved the URL). A backed-to bare hash or an
  // evicted world key falls back honestly to the universe (or the bound tree), never
  // a blank map.
  useEffect(() => {
    const onPop = () => {
      const route = parseRoute(typeof window !== 'undefined' ? window.location.hash : '');
      const fallback = (): NavTarget => ({
        surface: worldCount >= 1 ? 'universe' : 'tree',
        view: BOUND_VIEW,
        block: null,
      });
      // The browser moved the URL on its own (history:'none'), so a FALLBACK — a
      // human who backed into a junk (`#/zzz`) or unresolvable (`#/world/ghost`) hash
      // — would otherwise leave that dead route stranded in the address bar while the
      // tree/universe shows. Canonicalize it to the surface we actually land on
      // (replaceState — no new history entry).
      const land = (target: NavTarget) => {
        navigate(target, { history: 'none' });
        if (typeof window !== 'undefined') {
          const canonical = serializeRoute(target.surface, target.view ?? BOUND_VIEW, target.block ?? null);
          if (window.location.hash !== canonical) window.history.replaceState(null, '', canonical);
        }
      };
      if (!route) {
        land(fallback());
        return;
      }
      const outcome = resolveDeepLink(route, universe.worlds, brains, true);
      // A resolved route: the browser already sits on its canonical URL — apply it
      // without a rewrite. Unresolvable → fall back AND canonicalize the junk away.
      if (outcome.kind === 'apply') navigate(outcome.intent, { history: 'none' });
      else land(fallback());
    };
    window.addEventListener('popstate', onPop);
    return () => window.removeEventListener('popstate', onPop);
  }, [navigate, universe, brains, worldCount]);

  // Open a brain IN THE TREE (§4A.9). The bound/self brain resets to BOUND_VIEW
  // (no selector — the tree's default graph); a hosted brain seeds the viewed
  // brain from its card (name + counts), which the served_brain echo then
  // confirms. Either way the shell shows the tree.
  const openBrainInTree = useCallback(
    (entry: InstanceRegistryEntry, isSelf: boolean) => {
      const view: ViewedBrain = isSelf
        ? BOUND_VIEW
        : {
            root: brainProjectPath(entry),
            displayName: brainDisplayName(entry),
            nodeCount: entry.node_count ?? null,
          };
      navigate({ surface: 'tree', view });
    },
    [navigate],
  );

  // A mission-tray card asked to open its block (F2.5 §3b): seed the map's selection
  // and switch to the map surface (the tray is fixed on every surface, so this works
  // from tree/hall/map alike).
  const onOpenBlock = useCallback(
    (blockId: string) => navigate({ surface: 'map', block: blockId }),
    [navigate],
  );

  // Zoom from a world into its ROOM (F30): the world's Build Map, viewing that
  // brain via the §4A.9 selector — the map is where its tray + ratify live. The
  // world root IS a valid `?brain=` selector; the name/counts seed the chip until
  // the served_brain echo confirms.
  const onOpenWorld = useCallback(
    (root: string) => {
      const w = universe.worlds.find((x) => x.root === root);
      navigate({
        surface: 'map',
        view: { root, displayName: w?.name ?? null, nodeCount: w?.node_count ?? null },
        block: null,
      });
    },
    [navigate, universe.worlds],
  );
  // An owner-scope Landing item (a daemon alert) opens the owner room — the Hall — and
  // lands ON its owner-alerts panel (the item finally has a destination).
  const onOpenOwner = useCallback(
    () => navigate({ surface: 'hall', hallAlerts: true }),
    [navigate],
  );
  // Every OTHER Hall entry (the brain chip) leaves the alerts panel closed.
  const onOpenHall = useCallback(() => {
    if (surface !== 'threshold') navigate({ surface: 'hall', hallAlerts: false });
  }, [navigate, surface]);
  // The wordmark is the home affordance ONLY when the owner holds ≥1 world.
  const onOpenUniverse = useCallback(() => navigate({ surface: 'universe' }), [navigate]);

  // Decide the landing ONCE the owner state is known (§4A.1 placement doctrine,
  // amended by F30). The UNIVERSE is the front door when the owner serves ≥1 project
  // brain (world) — we wait for its read to settle so we never flash then jump. With
  // ZERO worlds the first-run door is UNCHANGED (INV-12: a returning user never meets
  // the Threshold): a ratified skeleton leads to the Build Map, else — brains vs none —
  // the tree or the Threshold onboarding.
  useEffect(() => {
    if (deepLinkPending) return; // a deep link seeds the shell first (precedence)
    if (surface != null) return;
    // Still deciding — wait, don't flash. An 'error' (a first non-404 poll failure)
    // does NOT decide the landing either: a failed universe read is not proof of an
    // empty sky, so we never silence it into "zero worlds → tree". The ~5s retry
    // recovers a transient blip; a truly down owner is already gated to 'loading'.
    if (universeStatus === 'loading' || universeStatus === 'error') return;
    // Each landing is a `navigate(..., replace)` so the URL gets a baseline entry
    // (a real back works from the very first surface) without a spurious history push.
    if (worldCount >= 1) {
      navigate({ surface: 'universe', view: BOUND_VIEW, block: null }, { history: 'replace' });
      return;
    }
    // Zero worlds → the prior doctrine, byte-for-byte.
    if (buildMap.status === 'loading') return;
    if (buildMap.present) {
      // The skeleton alone decides the front door — even on an 'empty' owner
      // (no graph yet), where the brains fetch never resolves.
      navigate({ surface: 'map', view: BOUND_VIEW, block: null }, { history: 'replace' });
      return;
    }
    if (brainCount == null) return; // no skeleton: fall back to the prior doctrine
    navigate(
      { surface: brainCount <= 0 ? 'threshold' : 'tree', view: BOUND_VIEW, block: null },
      { history: 'replace' },
    );
  }, [
    deepLinkPending,
    surface,
    worldCount,
    universeStatus,
    brainCount,
    buildMap.status,
    buildMap.present,
    navigate,
  ]);

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
  const onWindowEsc = useCallback(
    (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (surface !== 'tree') return;
      const active = document.activeElement;
      const treeIsFocused =
        active instanceof HTMLElement &&
        (active.closest('[role="tree"]') != null ||
          active.closest('[data-role="tree-drawer"]') != null ||
          active.tagName === 'INPUT');
      if (!treeIsFocused) navigate({ surface: 'hall', hallAlerts: false });
    },
    [surface, navigate],
  );
  useEffect(() => {
    window.addEventListener('keydown', onWindowEsc);
    return () => window.removeEventListener('keydown', onWindowEsc);
  }, [onWindowEsc]);

  return (
    <AppErrorBoundary>
      <div className="flex flex-col h-screen w-screen bg-porcelain text-ink font-sans overflow-hidden">
        <TopBar
          status={status}
          self={self}
          viewedBrain={viewedBrain}
          onOpenHall={onOpenHall}
          onOpenMap={
            surface !== 'threshold' && surface !== 'map' ? () => navigate({ surface: 'map' }) : undefined
          }
          onOpenUniverse={
            worldCount >= 1 && surface !== 'universe' && surface !== 'threshold'
              ? onOpenUniverse
              : undefined
          }
        />
        <div className="flex flex-1 overflow-hidden">
          {surface === 'universe' ? (
            // The Universe L0 (HUMAN-VIEW-V2 F30) — the home panorama + the Landing.
            // Its own right column (the Landing) replaces the tray at L0.
            <UniverseView
              universe={universe}
              onOpenWorld={onOpenWorld}
              onOpenOwner={onOpenOwner}
              note={universeNote}
            />
          ) : surface === 'threshold' ? (
            <ThresholdCard />
          ) : surface === 'hall' ? (
            <HallView
              onExit={() => navigate({ surface: 'tree' })}
              onOpenBound={() => openBrainInTree({} as InstanceRegistryEntry, true)}
              onOpenBrain={openBrainInTree}
              onBootstrap={() => setIngestOpen(true)}
              restSelector={restSelector}
              viewedRoot={viewedBrain.root}
              autoOpenAlerts={hallOpenAlerts}
            />
          ) : surface === 'map' ? (
            // The Build Map front door (HUMAN-VIEW-V2 F1). Read-only; the Living
            // Tree is one click away (the deterministic surface is never killed).
            <BuildMapView
              onOpenTree={() => navigate({ surface: 'tree' })}
              enabled={backendReachable}
              brainRoot={viewedBrain.root}
              selectedBlockId={mapTargetBlock}
            />
          ) : (
            <LivingTree viewedBrain={viewedBrain} onIngest={() => setIngestOpen(true)} />
          )}

          {/* The MISSION TRAY (F2.5 §3) — on every working surface, outside the
              surface switch but INSIDE the shell row: an in-flow right column, so
              expanding it makes room instead of painting over the surface (the
              fixed-overlay tray click-blocked the block panel / tree drawer —
              field bug 2026-07-10). Not on the Threshold (no brain yet → no
              missions to track), nor on the Universe L0 (the Landing IS the tray's
              aggregate there — one queue, every world). */}
          {surface != null && surface !== 'threshold' && surface !== 'universe' && (
            <MissionTrayLive
              viewedBrain={viewedBrain}
              enabled={backendReachable}
              onOpenBlock={onOpenBlock}
            />
          )}
        </div>

        <IngestModal
          isOpen={ingestOpen}
          onClose={() => setIngestOpen(false)}
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
