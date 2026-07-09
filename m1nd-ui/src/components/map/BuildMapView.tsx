/*
 * BuildMapView — the 'map' surface (HUMAN-VIEW-V2 F1/F3b). Wires useBuildMap and
 * maps its status onto the screens: loading (§1.3), error + Retry (§1.3), and the
 * map or the honest empty screen (delegated to BuildMap). The surface is read-only
 * BY DEFAULT; the ONE write it offers is the reconcile gesture (F3b §D) — it owns
 * the write call, the honest toast, and the reload that re-renders the new truth.
 */
import { useCallback, useState } from 'react';
import { api } from '../../api/client';
import {
  repoIdFromSkeletonId,
  runReconcile,
  runRatify,
  runScan,
  type ReconcileToast,
} from '../../lib/buildMap';
import { useBuildMap } from '../../hooks/useBuildMap';
import BuildMap from './BuildMap';
import ReviewRatify from './ReviewRatify';

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
  // F0c §5 — the scan gesture (empty state) and the Review-&-ratify walk. The write
  // owner runs `skeleton_candidate` / `system_blocks_ratify`, owns the honest toasts,
  // and reloads. The accept set is owned here so the blanket ratify is a real write
  // gated on the owner having reviewed each provisional name.
  const [scanning, setScanning] = useState(false);
  const [scanToast, setScanToast] = useState<ReconcileToast | null>(null);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [acceptedIds, setAcceptedIds] = useState<ReadonlySet<string>>(new Set());
  const [ratifying, setRatifying] = useState(false);
  const [ratifyToast, setRatifyToast] = useState<ReconcileToast | null>(null);

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

  // The scan gesture (F0c §5): OCC-key on the store_version we read (null on the
  // first scan — no store yet), run `skeleton_candidate` with naming:"auto", and
  // reduce it to a toast + reload (the pure `runScan`). Success reloads into the
  // candidate dress; a fresh candidate supersedes any prior acceptances.
  const handleScan = useCallback(async () => {
    if (scanning) return;
    const version = snapshot?.store?.store_version ?? snapshot?.store_version ?? null;
    setScanning(true);
    try {
      const { toast: t, shouldReload } = await runScan(
        () => api.skeletonCandidate({ expectedStoreVersion: version, naming: 'auto' }, brainRoot),
        version,
      );
      setScanToast(t);
      if (shouldReload) {
        setAcceptedIds(new Set());
        reload();
      }
    } finally {
      setScanning(false);
    }
  }, [scanning, snapshot, reload, brainRoot]);

  const dismissScanToast = useCallback(() => setScanToast(null), []);

  // Toggle a block's owner-acceptance (the §5 owner touch — local review state).
  const handleAccept = useCallback((blockId: string) => {
    setAcceptedIds((prev) => {
      const next = new Set(prev);
      if (next.has(blockId)) next.delete(blockId);
      else next.add(blockId);
      return next;
    });
  }, []);

  // The blanket ratify (F0c §5): OCC-key on the store_version we read, run
  // `system_blocks_ratify` (block_ids omitted = every block), and reduce it to a
  // toast + reload. Success reloads the ratified map, closes the walk, and clears the
  // accept set; a conflict reloads (the store moved); a read-only/error keeps the walk.
  const handleRatifyAll = useCallback(async () => {
    if (ratifying) return;
    const version = snapshot?.store?.store_version ?? snapshot?.store_version;
    if (version == null) return;
    setRatifying(true);
    try {
      const { toast: t, shouldReload } = await runRatify(
        () => api.systemBlocksRatify({ expectedStoreVersion: version, ratifier: 'gui' }, brainRoot),
        version,
      );
      setRatifyToast(t);
      if (shouldReload) {
        if (t.kind === 'ok') {
          setReviewOpen(false);
          setAcceptedIds(new Set());
        }
        reload();
      }
    } finally {
      setRatifying(false);
    }
  }, [ratifying, snapshot, reload, brainRoot]);

  const dismissRatifyToast = useCallback(() => setRatifyToast(null), []);
  const openReview = useCallback(() => setReviewOpen(true), []);
  const closeReview = useCallback(() => setReviewOpen(false), []);

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

  // ready | empty — BuildMap renders the canvas or the honest empty screen; the
  // Review-&-ratify walk (F0c §5) mounts as an overlay when the human opens it on a
  // candidate store (the owner holds the accept set + runs the ratify write).
  const store = snapshot?.present ? snapshot.store ?? null : null;
  return (
    <>
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
        onScan={handleScan}
        scanning={scanning}
        scanToast={scanToast}
        onDismissScanToast={dismissScanToast}
        onReview={openReview}
      />
      {reviewOpen && store && (
        <ReviewRatify
          store={store}
          repoId={repoIdFromSkeletonId(store.skeleton.skeleton_id)}
          acceptedIds={acceptedIds}
          onAccept={handleAccept}
          onRatifyAll={handleRatifyAll}
          ratifying={ratifying}
          ratifyToast={ratifyToast}
          onDismissToast={dismissRatifyToast}
          onClose={closeReview}
        />
      )}
    </>
  );
}
