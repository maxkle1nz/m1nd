/*
 * BuildMapView — the 'map' surface (HUMAN-VIEW-V2 F1/F3b/F0c/F11-c). Wires
 * useBuildMap and maps its status onto the screens: loading (§1.3), error + Retry
 * (§1.3), and the map or the honest empty screen (delegated to BuildMap). The
 * surface is read-only BY DEFAULT; every write it offers is owned HERE — the
 * reconcile gesture (F3b), the scan (F0c), and the F11 editor's gesture batches
 * (`candidate_edit`), the Name-with-runner route (`candidate_naming`) and the
 * ratify (all / selected). Each write is OCC-keyed on the store_version this
 * surface read; success and conflicts reload (never a silent merge).
 */
import { useCallback, useState } from 'react';
import { api } from '../../api/client';
import {
  brainRefFor,
  repoIdFromSkeletonId,
  runReconcile,
  runRatify,
  runScan,
  type ReconcileToast,
} from '../../lib/buildMap';
import {
  runCandidateEdit,
  runCandidateNaming,
  type EditOpInput,
} from '../../lib/candidateEdit';
import { composeCurationPacket, dispatchCuration } from '../../lib/curation';
import { sendDirectPacket } from '../../lib/missions';
import { useBuildMap } from '../../hooks/useBuildMap';
import { useRunnerdStatus } from '../../hooks/useRunnerdStatus';
import BuildMap from './BuildMap';
import ReviewRatify from './ReviewRatify';

export interface BuildMapViewProps {
  /** Open the Living Tree (kept one click away — PRD: the deterministic surface
   *  is never killed). */
  onOpenTree?: () => void;
  enabled?: boolean;
  /** §4A.9 — the brain this map reads. `null`/absent = the bound brain (F1
   *  behavior, byte-compatible); a hosted project root routes every read AND the
   *  writes through the `?brain=` selector, so a multi-brain owner shows the
   *  skeleton of the brain the human is actually viewing. */
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
  // F0c §5 — the scan gesture; F11-c — the editor. The write owner runs the
  // verbs, owns the honest toasts, and reloads.
  const [scanning, setScanning] = useState(false);
  const [scanToast, setScanToast] = useState<ReconcileToast | null>(null);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [ratifying, setRatifying] = useState(false);
  const [ratifyToast, setRatifyToast] = useState<ReconcileToast | null>(null);
  const [applying, setApplying] = useState(false);
  const [naming, setNaming] = useState(false);
  // §2b/F12: the runner-daemon liveness read. Polled while the editor is open (the
  // Name-with-runner button) OR while a candidate banner is shown (the curation
  // button offers the SPAWN path only when a daemon is announced).
  const candidatePresent =
    snapshot?.present === true && snapshot.store?.skeleton?.state === 'candidate';
  const runnerd = useRunnerdStatus(reviewOpen || (enabled && candidatePresent));

  // The reconcile gesture (F3b §D): OCC-key on the store_version we read, run the
  // write, and reduce it to a toast + reload decision (the pure `runReconcile`).
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
  // reduce it to a toast + reload (the pure `runScan`).
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
      if (shouldReload) reload();
    } finally {
      setScanning(false);
    }
  }, [scanning, snapshot, reload, brainRoot]);

  const dismissScanToast = useCallback(() => setScanToast(null), []);

  // F11-c §4b — ONE gesture batch through `candidate_edit`, OCC-keyed on the
  // version this surface read. Success/conflict reload; a refusal informs.
  const handleApplyOps = useCallback(
    async (ops: EditOpInput[]) => {
      if (applying || ops.length === 0) return;
      const version = snapshot?.store?.store_version ?? snapshot?.store_version;
      if (version == null) return;
      setApplying(true);
      try {
        const { toast: t, shouldReload } = await runCandidateEdit(
          () => api.candidateEdit({ expectedStoreVersion: version, ops }, brainRoot),
          version,
        );
        setRatifyToast(t);
        if (shouldReload) reload();
      } finally {
        setApplying(false);
      }
    },
    [applying, snapshot, reload, brainRoot],
  );

  // F11-c §2b — "Name with runner": absent ids = every provisional block. The
  // route refuses honestly without a live naming-runner; partial is normal.
  const handleNameWithRunner = useCallback(
    async (blockIds?: string[]) => {
      if (naming) return;
      const version = snapshot?.store?.store_version ?? snapshot?.store_version;
      if (version == null) return;
      setNaming(true);
      try {
        const { toast: t, shouldReload } = await runCandidateNaming(
          () => api.candidateNaming({ expectedStoreVersion: version, blockIds }, brainRoot),
          version,
        );
        setRatifyToast(t);
        if (shouldReload) reload();
      } finally {
        setNaming(false);
      }
    },
    [naming, snapshot, reload, brainRoot],
  );

  // The blanket ratify (F0c §5 / F11): `system_blocks_ratify`, block_ids omitted.
  // The o6 provenance gate is server law — an untouched heuristic block refuses
  // honestly and the toast says so.
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
        if (t.kind === 'ok') setReviewOpen(false);
        reload();
      }
    } finally {
      setRatifying(false);
    }
  }, [ratifying, snapshot, reload, brainRoot]);

  // F11-c: "Ratify selected only" — the same verb, scoped to one block id.
  const handleRatifySelected = useCallback(
    async (blockId: string) => {
      if (ratifying) return;
      const version = snapshot?.store?.store_version ?? snapshot?.store_version;
      if (version == null) return;
      setRatifying(true);
      try {
        const { toast: t, shouldReload } = await runRatify(
          () =>
            api.systemBlocksRatify(
              { expectedStoreVersion: version, ratifier: 'gui', blockIds: [blockId] },
              brainRoot,
            ),
          version,
        );
        setRatifyToast(t);
        if (shouldReload) reload();
      } finally {
        setRatifying(false);
      }
    },
    [ratifying, snapshot, reload, brainRoot],
  );

  const dismissRatifyToast = useCallback(() => setRatifyToast(null), []);
  const openReview = useCallback(() => setReviewOpen(true), []);
  const closeReview = useCallback(() => setReviewOpen(false), []);

  // F12 / F11-c §3a — "Send to curation": when a runner daemon is announced the
  // button offers the propose-apply SPAWN (`curation_spawn`) — the owner composes
  // the block views, calls the daemon's `/curate`, applies the hand's proposal
  // (runner seat, o5 + OCC) and posts the summary; the human reviews the RESULT and
  // ratifies. With NO runner announced it falls back to the DIRECT path — a seq-1
  // `judging` letter + the packet on the clipboard for the human to paste. The hand
  // can NEVER ratify, either way; the letter/mission anchors to the skeleton id.
  const [sendingCuration, setSendingCuration] = useState(false);
  const [curationResult, setCurationResult] = useState<{ ok: boolean; message: string } | null>(
    null,
  );
  const handleSendCuration = useCallback(async () => {
    if (sendingCuration) return;
    const store = snapshot?.present ? snapshot.store ?? null : null;
    if (!store) return;
    setSendingCuration(true);
    setCurationResult(null);
    try {
      const result = await dispatchCuration(
        { runnerAvailable: runnerd.available, storeVersion: store.store_version },
        {
          // F12 §3 — the propose-apply spawn (the owner applies the hand's proposal).
          spawn: (expectedStoreVersion) => api.curationSpawn({ expectedStoreVersion }, brainRoot),
          // DIRECT fallback. §1f: brain_ref is the brain's DISPLAY NAME (the basename
          // of its project root — the identity the owner's brain guard compares),
          // never the skeleton's sanitized slug.
          direct: async () => {
            const brainRef = brainRefFor(
              brainRoot,
              repoIdFromSkeletonId(store.skeleton.skeleton_id),
            );
            const markdown = composeCurationPacket({ store, repoId: brainRef });
            const res = await sendDirectPacket(
              {
                markdown,
                blockId: store.skeleton.skeleton_id,
                brainRef,
                seat: 'oracle',
                capability: 'hand-runner',
              },
              {
                postMission: (letter) => api.missionPost(letter, brainRoot),
                writeClipboard:
                  typeof navigator !== 'undefined' && navigator.clipboard
                    ? (text) => navigator.clipboard.writeText(text)
                    : undefined,
              },
            );
            return {
              mission_id: res.outcome.mission_id,
              mission_seq: res.outcome.mission_seq,
              clipboardCopied: res.clipboardCopied,
            };
          },
        },
      );
      setCurationResult({ ok: result.ok, message: result.message });
      // Only a SPAWN that applied changed the store — reload to show the curated map.
      if (result.applied) reload();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setCurationResult({ ok: false, message });
    } finally {
      setSendingCuration(false);
    }
  }, [sendingCuration, snapshot, brainRoot, runnerd.available, reload]);

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
  // Edit-Names-&-Boundaries screen (F11-c) mounts as an overlay on a candidate
  // store (this owner posts the gesture batches + the ratify writes).
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
        onSendCuration={handleSendCuration}
        sendingCuration={sendingCuration}
        curationResult={curationResult}
        runnerAvailable={runnerd.available}
      />
      {reviewOpen && store && (
        <ReviewRatify
          store={store}
          repoId={repoIdFromSkeletonId(store.skeleton.skeleton_id)}
          onApplyOps={handleApplyOps}
          applying={applying}
          onNameWithRunner={handleNameWithRunner}
          naming={naming}
          runnerAvailable={runnerd.available}
          onRatifyAll={handleRatifyAll}
          onRatifySelected={handleRatifySelected}
          ratifying={ratifying}
          ratifyToast={ratifyToast}
          onDismissToast={dismissRatifyToast}
          onClose={closeReview}
        />
      )}
    </>
  );
}
