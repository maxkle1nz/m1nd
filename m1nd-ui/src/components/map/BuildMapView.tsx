/*
 * BuildMapView — the 'map' surface (HUMAN-VIEW-V2 F1/F3b). Wires useBuildMap and
 * maps its status onto the screens: loading (§1.3), error + Retry (§1.3), and the
 * map or the honest empty screen (delegated to BuildMap). The surface is read-only
 * BY DEFAULT; the ONE write it offers is the reconcile gesture (F3b §D) — it owns
 * the write call, the honest toast, and the reload that re-renders the new truth.
 */
import { useCallback, useState } from 'react';
import { api } from '../../api/client';
import { runReconcile, type ReconcileToast } from '../../lib/buildMap';
import { useBuildMap } from '../../hooks/useBuildMap';
import BuildMap from './BuildMap';

export interface BuildMapViewProps {
  /** Open the Living Tree (kept one click away — PRD: the deterministic surface
   *  is never killed). */
  onOpenTree?: () => void;
  enabled?: boolean;
  /** §4A.9 — the brain this map reads. `null`/absent = the bound brain (F1
   *  behavior, byte-compatible); a hosted project root routes every read AND the
   *  reconcile write through the `?brain=` selector, so a multi-brain owner shows
   *  the skeleton of the brain the human is actually viewing. */
  brainRoot?: string | null;
  /** F2.5 §3b — the block a mission-tray card asked to open. Seeds (and re-seeds)
   *  the map's selection so the human lands on the named block. */
  selectedBlockId?: string | null;
}

export default function BuildMapView({
  onOpenTree,
  enabled = true,
  brainRoot = null,
  selectedBlockId = null,
}: BuildMapViewProps) {
  const { status, snapshot, rollup, error, reload } = useBuildMap(enabled, brainRoot);
  const [reconciling, setReconciling] = useState(false);
  const [toast, setToast] = useState<ReconcileToast | null>(null);

  // The reconcile gesture (F3b §D): OCC-key on the store_version we read, run the
  // write, and reduce it to a toast + reload decision (the pure `runReconcile`).
  // Success and conflict reload the snapshot (the map re-renders on the new truth);
  // a read-only/error refusal informs without a silent retry. Guarded against a
  // double-run while one is in flight.
  const handleReconcile = useCallback(async () => {
    if (reconciling) return;
    const version = snapshot?.store?.store_version ?? snapshot?.store_version;
    if (version == null) return;
    setReconciling(true);
    try {
      const { toast: t, shouldReload } = await runReconcile(
        (expected) => api.systemBlocksReconcile(expected, brainRoot),
        version,
      );
      setToast(t);
      if (shouldReload) reload();
    } finally {
      setReconciling(false);
    }
  }, [reconciling, snapshot, reload, brainRoot]);

  const dismissToast = useCallback(() => setToast(null), []);

  if (status === 'loading') {
    return (
      <div className="flex-1 flex items-center justify-center bg-porcelain" data-role="build-map-loading">
        <div className="text-sm text-ink-soft">Loading repository map…</div>
      </div>
    );
  }

  if (status === 'error') {
    return (
      <div className="flex-1 flex items-center justify-center bg-porcelain" data-role="build-map-error">
        <div className="text-center space-y-3 px-6">
          <div className="text-sm text-state-failure">Failed to load map</div>
          {error && <div className="text-xs text-ink-soft font-mono max-w-md break-words">{error}</div>}
          <button
            type="button"
            data-role="retry"
            onClick={reload}
            className="px-3 py-1.5 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  // ready | empty — BuildMap renders the canvas or the honest empty screen.
  return (
    <BuildMap
      snapshot={snapshot ?? { present: false }}
      rollup={rollup}
      brainRoot={brainRoot}
      initialSelectedId={selectedBlockId}
      onOpenTree={onOpenTree}
      onReconcile={handleReconcile}
      reconciling={reconciling}
      reconcileToast={toast}
      onDismissToast={dismissToast}
    />
  );
}
