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
import { useCallback, useEffect, useState } from 'react';
import { api } from '../../api/client';
import type { SseEvent } from '../../types';
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
import { useLiveRefresh } from '../../hooks/useLiveRefresh';
import { useRunnerdStatus } from '../../hooks/useRunnerdStatus';
import { useScanMachine } from '../../hooks/useScanMachine';
import { useSSE } from '../../hooks/useSSE';
import { scanServerPhaseFromEvent } from '../../lib/scanMachine';
import BuildMap from './BuildMap';
import ReviewRatify from './ReviewRatify';
import { MapErrorScreen, MapLoadingScreen } from './MapStatusScreen';

/** After this much of a continuous cold load the loading screen promotes to its
 *  `slow` note + Retry (mirrors the scan wait's SCAN_SLOW_AFTER_MS): the map can
 *  never sit on a silent forever-spin when the engine is unreachable/hung. */
const MAP_LOADING_SLOW_AFTER_MS = 10_000;

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
  // §5.3 — the map breathes. An agent (or another viewer) mutating the SystemBlock
  // store / skeleton / X-RAY tags emits a `graph_changed` SSE (the block verbs the
  // relay now covers); useLiveRefresh debounces the burst (~500 ms) and bumps this
  // key, so useBuildMap re-reads. The re-read opens as 'refreshing' (stale-while-
  // revalidate, #372): the map stays mounted and the human's selection / scroll /
  // open panel are preserved — a live refresh never yanks them.
  const [liveRefreshKey, setLiveRefreshKey] = useState(0);
  const { status, snapshot, rollup, error, reload } = useBuildMap(enabled, brainRoot, liveRefreshKey);

  // Cold-load honesty (5-stati): a hung read (engine unreachable but the socket
  // stays open, so the fetch never rejects) used to leave 'loading' up forever with
  // no note and no way out. Past ~10s of continuous loading, promote to the `slow`
  // screen (honest note + Retry). `loadAttempt` bumps on Retry so a manual retry
  // restarts the grace window even when status stays 'loading' (loading→loading).
  const [loadingSlow, setLoadingSlow] = useState(false);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const retry = useCallback(() => {
    setLoadingSlow(false);
    setLoadAttempt((n) => n + 1);
    reload();
  }, [reload]);
  useEffect(() => {
    if (status !== 'loading') {
      setLoadingSlow(false);
      return;
    }
    setLoadingSlow(false);
    const id = setTimeout(() => setLoadingSlow(true), MAP_LOADING_SLOW_AFTER_MS);
    return () => clearTimeout(id);
  }, [status, loadAttempt]);
  // Subscribe ONLY this surface (never the App-level front-door read — that would
  // double-refetch). SCOPED to the brain in view (§4A.9.6): a mutation on another
  // brain leaves this map untouched. Gated off the cold 'loading' start (the first
  // read is already in flight); a live event thereafter rides the 'refreshing' path.
  useLiveRefresh({
    onRefresh: () => setLiveRefreshKey((k) => k + 1),
    enabled: enabled && status !== 'loading',
    viewedRoot: brainRoot,
  });
  const [reconciling, setReconciling] = useState(false);
  const [toast, setToast] = useState<ReconcileToast | null>(null);
  // F0c §5 — the scan gesture; F11-c — the editor. The write owner runs the
  // verbs, owns the honest toasts, and reloads. The scan's in-flight lifecycle is
  // the scanMachine (docs/uml/scan-loading.md): named phases + a live elapsed
  // clock + the honest slow note — the button never looks dead again.
  const scan = useScanMachine();
  // The REAL node count behind the wait copy ("clustering N nodes…"). Read once
  // per gesture from /api/graph/stats — never invented; null keeps the copy generic.
  const [scanNodeCount, setScanNodeCount] = useState<number | null>(null);
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
  // reduce it to a toast + reload (the pure `runScan`). The machine wraps the
  // whole wait: SCAN → SENT → ticking clustering/slow → RESOLVED (or the user's
  // ABORT — in which case the late settle is ignored; the reducer is total).
  const handleScan = useCallback(async () => {
    const controller = scan.begin();
    if (controller == null) return; // one gesture at a time — a click mid-flight is refused
    const version = snapshot?.store?.store_version ?? snapshot?.store_version ?? null;
    // Best-effort node count for the honest wait copy (a pure read; failure keeps
    // the generic copy — never a fabricated number).
    api
      .graphStats(brainRoot)
      .then((s) => setScanNodeCount(s.node_count))
      .catch(() => setScanNodeCount(null));
    const settled = runScan(
      () =>
        api.skeletonCandidate(
          { expectedStoreVersion: version, naming: 'auto' },
          brainRoot,
          controller.signal,
        ),
      version,
    );
    scan.sent(); // the POST left the browser — the wait is now the server's
    const { toast: t, shouldReload } = await settled;
    if (controller.signal.aborted) return; // the human stopped waiting — ABORTED already settled it
    scan.resolve(t, shouldReload);
    if (shouldReload) reload();
  }, [scan, snapshot, reload, brainRoot]);

  const scanning = scan.inFlight;
  const scanToast = scan.state.toast;
  const dismissScanToast = scan.dismissToast;

  // Slice 2 (docs/uml/scan-loading.md): while a scan is in flight, subscribe to the
  // owner's scan-phase narration on the EXISTING `/api/events` channel and feed each
  // phase to the machine (DISPLAY enrichment — the elapsed clock stays the client's
  // own). `enabled: scanning` opens the EventSource on SCAN and the useSSE cleanup
  // closes it on settle/unmount (honest teardown). An owner that emits nothing
  // leaves the static client label untouched — retrocompat honesta.
  const scanPhaseDispatch = scan.phase;
  const onScanSse = useCallback(
    (event: SseEvent) => {
      if (event.event_type !== 'scan_progress') return;
      const server = scanServerPhaseFromEvent(event.data);
      if (server) scanPhaseDispatch(server);
    },
    [scanPhaseDispatch],
  );
  useSSE({ onEvent: onScanSse, enabled: scanning });

  // The reload landed a store (candidate dress takes over the surface): settle
  // the machine back to idle so a later empty state starts clean.
  useEffect(() => {
    if (snapshot?.present === true && scan.state.phase === 'candidate_ready') scan.reset();
  }, [snapshot, scan]);

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
    return <MapLoadingScreen slow={loadingSlow} onRetry={retry} />;
  }

  if (status === 'error') {
    return <MapErrorScreen error={error} onRetry={retry} />;
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
        refreshing={status === 'refreshing'}
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
        scanPhase={{
          phase: scan.state.phase,
          elapsedMs: scan.state.elapsedMs,
          nodeCount: scanNodeCount,
          serverPhase: scan.state.serverPhase,
        }}
        onCancelScan={scan.abort}
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
