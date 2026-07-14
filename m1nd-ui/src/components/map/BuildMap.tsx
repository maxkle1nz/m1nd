/*
 * BuildMap — the pure, presentational Build Map root (HUMAN-VIEW-V2-SCREENS §1.1,
 * §1.2, §1.3). Given a parsed `system_blocks_snapshot` + its rollup, it draws the
 * whole front door: the System Health rail, the deterministic card canvas with its
 * wires, and the selected block's panel — or, when no store is present, the honest
 * empty screen (§1.3). First-run (a candidate skeleton, §1.2) dashes every card
 * and flies a banner that can NEVER be mistaken for a ratified map.
 *
 * Read-only by construction: no engine write renders this. It manages only local
 * selection; every datum is a projection of the ratified skeleton.
 */
import { useEffect, useState } from 'react';
import {
  canvasSize,
  domainTag,
  gridLayout,
  repoIdFromSkeletonId,
  type MapRollup,
  type ReconcileToast,
  type SystemBlocksSnapshot,
} from '../../lib/buildMap';
import {
  formatElapsed,
  isScanInFlight,
  scanDisplayLabel,
  scanSlowNote,
  scanWaitCopy,
  type ScanPhaseName,
  type ScanServerPhase,
} from '../../lib/scanMachine';
import { Icon } from '../../lib/icons/registry';
import { useRunnerdStatus } from '../../hooks/useRunnerdStatus';
import BlockCard from './BlockCard';
import BlockPanel from './BlockPanel';
import PacketCompose from './PacketCompose';
import ShowCode from './ShowCode';
import SystemHealthSidebar from './SystemHealthSidebar';
import Wires from './Wires';

/** The open F2 surface (Show Code modal or Copy Packet panel), scoped to a block. */
type MapModal =
  | { kind: 'showcode'; blockId: string }
  | { kind: 'packet'; blockId: string; subPath: string | null }
  | null;

export interface BuildMapProps {
  snapshot: SystemBlocksSnapshot;
  /** The rollup (PRD §5). Required when the snapshot is present. */
  rollup: MapRollup | null;
  /** §4A.9 — the brain this map reads (null = bound). Rides into the Show Code
   *  viewer so file reads resolve against the SAME brain the map shows. */
  brainRoot?: string | null;
  /** Seed the selected block (used by the surface + by tests to open the panel). */
  initialSelectedId?: string | null;
  onOpenTree?: () => void;
  /** F3b §D — the reconcile gesture. When provided, the header shows the Reconcile
   *  button; the owner (BuildMapView) runs the write + owns the toast/reload. */
  onReconcile?: () => void;
  /** A reconcile is in flight — the button shows an honest "Reconciling…" and locks. */
  reconciling?: boolean;
  /** The last reconcile's honest toast (summary, conflict, read-only, or error). */
  reconcileToast?: ReconcileToast | null;
  /** Dismiss the toast. */
  onDismissToast?: () => void;
  /** F0c §5 — the scan gesture. When provided, the empty state's SECOND button
   *  ("Scan this repo") wires the `skeleton_candidate` verb; the owner (BuildMapView)
   *  runs the write, owns the honest scan toast, and reloads into candidate dress. */
  onScan?: () => void;
  /** A scan is in flight — the button shows "Scanning…" and locks. */
  scanning?: boolean;
  /** The last scan's honest toast (shown in the empty state, where the scan lives). */
  scanToast?: ReconcileToast | null;
  /** Dismiss the scan toast. */
  onDismissScanToast?: () => void;
  /** The scan loading state machine's view (scanMachine, docs/uml/scan-loading.md):
   *  while in flight the empty state shows the wait panel — real phase + a live
   *  elapsed clock + the honest "takes a while" note past the slow threshold. The
   *  NEVER-DEAD law: with this present, the wait always has visible movement. */
  scanPhase?: {
    phase: ScanPhaseName;
    elapsedMs: number;
    nodeCount: number | null;
    /** The owner-named phase (SSE slice 2). When present the panel shows it in
     *  place of the static client label; absent keeps this slice's prior behavior. */
    serverPhase?: ScanServerPhase | null;
  } | null;
  /** Stop WAITING (aborts the fetch; the owner may still finish — the honest
   *  canceled toast says so). Renders the panel's "Stop waiting" button. */
  onCancelScan?: () => void;
  /** F0c §5 — open the Review-&-ratify walk (the candidate banner's button). */
  onReview?: () => void;
  /** F11-c §3a — dispatch the curation mission (the heavy-case escape hatch):
   *  compose the curation packet + post the seq-1 letter through the EXISTING
   *  direct path. The owner (BuildMapView) runs the write and owns the result. */
  onSendCuration?: () => void;
  /** A curation dispatch is in flight — the button locks. */
  sendingCuration?: boolean;
  /** The last curation dispatch's honest outcome (shown under the banner). */
  curationResult?: { ok: boolean; message: string } | null;
  /** F12 — true when a runner daemon is announced: the button offers the propose-
   *  apply SPAWN (`curation_spawn`) instead of the DIRECT clipboard fallback. */
  runnerAvailable?: boolean;
  /** Test seam: force the Unmapped tray open (SSR has no click). */
  initialUnmappedExpanded?: boolean;
  /** Test seam: open the reconcile confirm at mount (SSR has no click) so the
   *  two-step confirm's honest copy is provable with a static render. */
  initialReconcileConfirmOpen?: boolean;
  /** Stale-while-revalidate (F1) — a re-read is in flight but the last-good map stays
   *  mounted. Shows a discreet header indicator; the canvas/selection/scroll are kept. */
  refreshing?: boolean;
}

/** Toast tint by kind (F3b §D) — all sanctioned non-violet tokens: success = sage,
 *  conflict/read-only = amber warn, error = clay, canceled = neutral bone (a
 *  user gesture, not a failure). */
const TOAST_CLASSES: Record<ReconcileToast['kind'], string> = {
  ok: 'border-verdict-act/50 bg-verdict-act-tint/40 text-ink',
  conflict: 'border-verdict-reverify/50 bg-verdict-reverify-tint/40 text-ink',
  readonly: 'border-verdict-reverify/50 bg-verdict-reverify-tint/40 text-ink',
  error: 'border-state-failure/50 bg-state-failure-tint/40 text-ink',
  canceled: 'border-ink/20 bg-bone text-ink',
};

/** §1.3 EMPTY — no skeleton bound for this repo. The honest backend copy, the
 *  primary "Scan this repo" gesture (F0c §5 — the engine proposes a candidate map
 *  the human ratifies), and the Import CTA that stays DISABLED with a note pointing
 *  at the write verb (importing an authored seed is a CLI/verb job — the map never
 *  fakes that mutation). The scan itself is a real write: the button runs it and the
 *  owner surfaces the honest toast (read-only / conflict / error) right here. */
function BuildMapEmpty({
  honest,
  onScan,
  scanning = false,
  scanToast = null,
  onDismissScanToast,
  scanPhase = null,
  onCancelScan,
}: {
  honest: string | null;
  onScan?: () => void;
  scanning?: boolean;
  scanToast?: ReconcileToast | null;
  onDismissScanToast?: () => void;
  scanPhase?: {
    phase: ScanPhaseName;
    elapsedMs: number;
    nodeCount: number | null;
    /** The owner-named phase (SSE slice 2). When present the panel shows it in
     *  place of the static client label; absent keeps this slice's prior behavior. */
    serverPhase?: ScanServerPhase | null;
  } | null;
  onCancelScan?: () => void;
}) {
  const waiting = scanPhase != null && isScanInFlight(scanPhase.phase);
  return (
    <div className="flex-1 flex items-center justify-center bg-porcelain" data-role="build-map-empty">
      <div className="max-w-sm text-center space-y-3 px-6">
        <div className="text-ink font-semibold">No skeleton yet for this repo.</div>
        <p className="text-xs text-ink-soft font-mono">
          {honest ?? 'no skeleton yet — import a seed or run a scan'}
        </p>

        {/* The primary gesture — scan the repo into a candidate map (F0c §5). */}
        {onScan && (
          <div>
            <button
              type="button"
              data-role="scan-repo"
              onClick={onScan}
              disabled={scanning}
              title="Scan this repo's graph into a proposed map you can review and ratify"
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-mono text-ink bg-bone border border-ink/15 rounded hover:shadow-contact transition-shadow disabled:opacity-60 disabled:cursor-progress"
            >
              <Icon name="search" size={14} decorative />
              {scanning ? 'Scanning…' : 'Scan this repo'}
            </button>
            <p className="text-[10px] text-ink-soft mt-1">
              the engine proposes blocks; you ratify them — auto-clustering only ever produces a candidate
            </p>
          </div>
        )}

        {/* The wait panel (scanMachine, NEVER-DEAD law): while the scan request is
            out, the screen shows the REAL phase + a live elapsed clock — never a
            fabricated percentage (the owner emits no progress; we never invent
            one). Past the slow threshold the panel SAYS the wait is long and keeps
            counting. Calm paper-and-ink: a soft pulsing dot, tabular digits. */}
        {waiting && scanPhase && (
          <div
            data-role="scan-wait"
            data-scan-phase={scanPhase.phase}
            data-scan-server-phase={scanPhase.serverPhase?.phase ?? undefined}
            className="rounded-lg border border-ink/15 bg-bone px-3 py-2.5 text-left space-y-1.5"
          >
            <div className="flex items-center gap-2 text-xs text-ink font-mono">
              <span className="w-1.5 h-1.5 rounded-full bg-ink/50 animate-pulse shrink-0" aria-hidden />
              <span data-role="scan-phase-label">
                {scanDisplayLabel(scanPhase.phase, scanPhase.serverPhase ?? null)}
              </span>
              <span data-role="scan-elapsed" className="ml-auto tabular-nums text-ink-soft">
                {formatElapsed(scanPhase.elapsedMs)}
              </span>
            </div>
            <p className="text-[10px] text-ink-soft">{scanWaitCopy(scanPhase.nodeCount)}</p>
            {scanPhase.phase === 'slow' && (
              <p data-role="scan-slow-note" className="text-[10px] text-ink-soft">
                {scanSlowNote(scanPhase.nodeCount)}
              </p>
            )}
            {onCancelScan && (
              <button
                type="button"
                data-role="scan-cancel"
                onClick={onCancelScan}
                title="closes this request — the owner may still finish and write the candidate"
                className="text-[10px] font-mono text-ink-soft underline decoration-ink/30 hover:text-ink"
              >
                Stop waiting
              </button>
            )}
          </div>
        )}

        {/* The scan's honest outcome (read-only / conflict / error), shown in place. */}
        {scanToast && (
          <div
            data-role="scan-toast"
            data-toast-kind={scanToast.kind}
            className={`flex items-start justify-between gap-3 rounded-lg border px-3 py-2 text-xs text-left ${TOAST_CLASSES[scanToast.kind]}`}
          >
            <span className="font-mono">{scanToast.text}</span>
            {onDismissScanToast && (
              <button
                type="button"
                data-role="scan-toast-dismiss"
                onClick={onDismissScanToast}
                title="dismiss"
                className="text-ink-soft hover:text-ink shrink-0 leading-none"
              >
                ×
              </button>
            )}
          </div>
        )}

        <div className="pt-1 border-t border-ink/10" />
        <button
          type="button"
          data-role="import-seed"
          disabled
          title="Import is a write verb — run it from the CLI/verb (F1 is read-only)"
          className="px-3 py-1.5 text-xs bg-bone text-ink-soft border border-ink/15 rounded opacity-60 cursor-not-allowed"
        >
          Import seed
        </button>
        <p className="text-[10px] text-ink-soft">
          or import an authored seed via the <span className="font-mono">system_blocks_seed_import</span> verb
        </p>
      </div>
    </div>
  );
}

/** §1.2 first-run banner — a candidate skeleton, nothing ratified yet. Its button
 *  is "Review & ratify" (F0c §5, objection 10): the walk reviews each provisional
 *  name before any ratify — a blanket gesture over guesses is never offered. */
function CandidateBanner({
  onReview,
  onSendCuration,
  sendingCuration = false,
  curationResult = null,
  runnerAvailable = false,
}: {
  onReview?: () => void;
  onSendCuration?: () => void;
  sendingCuration?: boolean;
  curationResult?: { ok: boolean; message: string } | null;
  runnerAvailable?: boolean;
}) {
  return (
    <div
      data-role="candidate-banner"
      className="mx-4 mt-4 rounded-lg border border-stale-lilac/50 bg-stale-lilac/10 px-4 py-2.5 text-xs text-ink space-y-1.5"
    >
      <div className="flex items-center justify-between gap-3">
        <div>
          <span className="font-semibold text-stale-lilac">Candidate skeleton ready</span> — nothing on this map is
          ratified yet. Names are guesses until you ratify them.
        </div>
        <div className="flex items-center gap-2 shrink-0">
          {/* §3a/F12 — the heavy-case escape hatch: the hand curates, the human
              reviews the RESULT. When a runner daemon is announced the button offers
              the propose-apply SPAWN (curation_spawn: the owner applies the hand's
              proposal under o5 + OCC); otherwise the DIRECT path (a letter + the
              packet to paste). The hand can NEVER ratify, either way. */}
          {onSendCuration && (
            <button
              type="button"
              data-role="send-curation"
              data-mode={runnerAvailable ? 'spawn' : 'direct'}
              onClick={onSendCuration}
              disabled={sendingCuration}
              title={
                runnerAvailable
                  ? 'send the candidate to the pinned hand-runner — it proposes candidate_edit ops, the owner sanitizes and applies them under OCC, and you review the result (the hand can never ratify)'
                  : 'compose the curation packet + post the mission letter (direct path — the packet is copied for you to paste into your agent; the hand can never ratify)'
              }
              className="flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-mono text-ink border border-ink/15 bg-bone rounded hover:shadow-contact transition-shadow disabled:opacity-60 disabled:cursor-progress"
            >
              {sendingCuration
                ? 'Dispatching…'
                : runnerAvailable
                  ? 'Send to the hand-runner'
                  : 'Send to an agent for curation'}
            </button>
          )}
          {onReview && (
            <button
              type="button"
              data-role="review-ratify-open"
              onClick={onReview}
              title="Open Edit Names & Boundaries — name, merge, split, resolve seams, then ratify"
              className="flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-mono text-stale-lilac border border-stale-lilac/50 bg-stale-lilac/10 rounded hover:shadow-contact transition-shadow"
            >
              <Icon name="blocks" size={14} decorative />
              Review &amp; ratify
            </button>
          )}
        </div>
      </div>
      {curationResult && (
        <div
          data-role="curation-result"
          data-ok={curationResult.ok}
          className={`text-[11px] font-mono ${curationResult.ok ? 'text-verdict-act' : 'text-state-failure'}`}
        >
          {curationResult.message}
        </div>
      )}
    </div>
  );
}

export default function BuildMap({
  snapshot,
  rollup,
  brainRoot = null,
  initialSelectedId = null,
  onOpenTree,
  onReconcile,
  reconciling = false,
  reconcileToast = null,
  onDismissToast,
  onScan,
  scanning = false,
  scanToast = null,
  onDismissScanToast,
  scanPhase = null,
  onCancelScan,
  onReview,
  onSendCuration,
  sendingCuration = false,
  curationResult = null,
  runnerAvailable = false,
  initialUnmappedExpanded = false,
  initialReconcileConfirmOpen = false,
  refreshing = false,
}: BuildMapProps) {
  const store = snapshot.present ? snapshot.store ?? null : null;
  const [selectedId, setSelectedId] = useState<string | null>(initialSelectedId);
  const [modal, setModal] = useState<MapModal>(null);
  // F3b §D — the reconcile is the ONE write the map offers, so it asks first: a
  // click opens a two-step confirm (mirroring the tray's import/archive confirm)
  // before `onReconcile` fires. Every human write on this surface confirms.
  const [reconcileConfirmOpen, setReconcileConfirmOpen] = useState(initialReconcileConfirmOpen);
  // F2.5c §4b — poll the runner-daemon liveness only while the compose panel is open,
  // so the spawn radio un-disables when a runner is registered.
  const runnerd = useRunnerdStatus(modal?.kind === 'packet');

  // F2.5 §3b — when the mission tray asks to open a block while the map is already
  // mounted, `initialSelectedId` changes: follow it so the tray click lands the
  // selection. (No-op in SSR — effects don't run under renderToStaticMarkup.)
  useEffect(() => {
    if (initialSelectedId != null) setSelectedId(initialSelectedId);
  }, [initialSelectedId]);

  if (!store || !rollup) {
    return (
      <BuildMapEmpty
        honest={snapshot.honest ?? null}
        onScan={onScan}
        scanning={scanning}
        scanToast={scanToast}
        onDismissScanToast={onDismissScanToast}
        scanPhase={scanPhase}
        onCancelScan={onCancelScan}
      />
    );
  }

  const repoId = repoIdFromSkeletonId(store.skeleton.skeleton_id);
  const positions = gridLayout(store.blocks.length);
  const size = canvasSize(store.blocks.length);
  const selectedBlock = store.blocks.find((b) => b.block_id === selectedId) ?? null;
  const selectedRollup = selectedBlock ? rollup.rollups.get(selectedBlock.block_id) ?? null : null;
  const modalBlock = modal ? store.blocks.find((b) => b.block_id === modal.blockId) ?? null : null;
  const modalRollup = modalBlock ? rollup.rollups.get(modalBlock.block_id) ?? null : null;

  return (
    <div className="flex-1 flex overflow-hidden bg-porcelain" data-surface="map">
      <SystemHealthSidebar
        counts={rollup.counts}
        unmapped={{ reconciled: rollup.reconciled, total: rollup.unmappedTotal, files: rollup.unmappedFiles }}
        onOpenTree={onOpenTree}
        initialUnmappedExpanded={initialUnmappedExpanded}
      />

      {/* Canvas column. */}
      <div className="flex-1 flex flex-col min-w-0">
        <div className="px-4 py-2.5 border-b border-ink/10 flex items-center justify-between gap-3">
          <div className="text-sm text-ink font-semibold flex items-center gap-2">
            Build Map
            <span className="text-[11px] font-mono text-ink-soft">
              {store.blocks.length} blocks · {rollup.candidate ? 'candidate' : 'ratified'}
              {refreshing && (
                <span data-role="map-refreshing" className="ml-1 text-ink-soft/70">
                  · refreshing…
                </span>
              )}
            </span>
          </div>
          <div className="flex items-center gap-2">
            {/* Reconcile (F3b §D) — the ONE write the map offers. A click opens the
                two-step confirm below (never fires the write directly); refused on a
                read-only owner (the toast says so, honestly; the button never vanishes). */}
            {onReconcile && (
              <button
                type="button"
                data-role="reconcile"
                onClick={() => setReconcileConfirmOpen(true)}
                disabled={reconciling}
                title="Re-resolve every block's membership against the repo — a write (asks first)"
                className="flex items-center gap-1.5 px-2 py-1 text-[11px] font-mono text-ink bg-bone border border-ink/15 rounded hover:shadow-contact transition-shadow disabled:opacity-60 disabled:cursor-progress"
              >
                <Icon name="ingest" size={14} decorative />
                {reconciling ? 'Reconciling…' : 'Reconcile'}
              </button>
            )}
            {/* A hairline sets the read-only SEAL apart from the write button — the seal
                describes the surface, the button is the one exception, so they never read
                as one thing. The legend names the split plainly. */}
            <span className="h-5 w-px bg-ink/10 shrink-0" aria-hidden />
            <span
              className="text-[10px] font-mono text-ink-soft leading-tight text-right"
              data-role="read-only-note"
              title="the map reads; the Reconcile button writes"
            >
              read-only
              {onReconcile && (
                <span className="block text-ink-soft/70">the map reads; this button writes</span>
              )}
            </span>
          </div>
        </div>

        {/* The reconcile confirm (F3b §D) — the two-step every human write earns: the
            honest cost stated before the write fires (a moved boundary bumps vN and
            stales that block's receipts). Mirrors the tray's import/archive confirm. */}
        {onReconcile && reconcileConfirmOpen && (
          <div
            data-role="reconcile-confirm"
            className="mx-4 mt-3 rounded-lg border border-verdict-reverify/50 bg-verdict-reverify-tint/40 px-3 py-2.5 text-xs text-ink space-y-1.5"
          >
            <div className="text-[10px] uppercase tracking-wide text-ink-soft">Reconcile this map?</div>
            <p className="leading-snug">
              This re-resolves every block's membership against the repo. A moved boundary bumps that
              block's version (vN) and stales its receipts — you re-earn them.
            </p>
            <div className="flex items-center gap-2 pt-0.5">
              <button
                type="button"
                data-role="reconcile-confirm-go"
                onClick={() => {
                  setReconcileConfirmOpen(false);
                  onReconcile();
                }}
                className="rounded border border-verdict-reverify/50 bg-bone px-2 py-0.5 text-[11px] font-mono text-ink hover:shadow-contact transition-shadow"
              >
                Reconcile
              </button>
              <button
                type="button"
                data-role="reconcile-confirm-cancel"
                onClick={() => setReconcileConfirmOpen(false)}
                className="text-[11px] text-ink-soft hover:text-ink"
              >
                cancel
              </button>
            </div>
          </div>
        )}

        {/* The reconcile toast (F3b §D) — the honest one-line outcome. */}
        {reconcileToast && (
          <div
            data-role="reconcile-toast"
            data-toast-kind={reconcileToast.kind}
            className={`mx-4 mt-3 flex items-start justify-between gap-3 rounded-lg border px-3 py-2 text-xs ${TOAST_CLASSES[reconcileToast.kind]}`}
          >
            <span className="font-mono">{reconcileToast.text}</span>
            {onDismissToast && (
              <button
                type="button"
                data-role="reconcile-toast-dismiss"
                onClick={onDismissToast}
                title="dismiss"
                className="text-ink-soft hover:text-ink shrink-0 leading-none"
              >
                ×
              </button>
            )}
          </div>
        )}

        {rollup.candidate && (
          <CandidateBanner
            onReview={onReview}
            onSendCuration={onSendCuration}
            sendingCuration={sendingCuration}
            curationResult={curationResult}
            runnerAvailable={runnerAvailable}
          />
        )}

        {/* Pan = overflow scroll (F1: no semantic zoom). Cards are absolutely
            placed on the deterministic grid; wires sit behind them. */}
        <div className="flex-1 overflow-auto p-1">
          <div className="relative" style={{ width: size.width, height: size.height }} data-role="map-canvas">
            <Wires store={store} positions={positions} width={size.width} height={size.height} />
            {store.blocks.map((block, i) => {
              const r = rollup.rollups.get(block.block_id);
              if (!r) return null;
              const pos = positions[i];
              return (
                <BlockCard
                  key={block.block_id}
                  block={block}
                  rollup={r}
                  domainTag={domainTag(block.block_id, repoId)}
                  selected={block.block_id === selectedId}
                  onSelect={setSelectedId}
                  style={{ left: pos.x, top: pos.y, width: 264, height: 138 }}
                />
              );
            })}
          </div>
        </div>
      </div>

      {/* The selected block's panel, or a calm hint. */}
      {selectedBlock && selectedRollup ? (
        <BlockPanel
          block={selectedBlock}
          rollup={selectedRollup}
          repoId={repoId}
          onShowCode={() => setModal({ kind: 'showcode', blockId: selectedBlock.block_id })}
          onAskAgent={() => setModal({ kind: 'packet', blockId: selectedBlock.block_id, subPath: null })}
        />
      ) : (
        <aside
          data-role="block-panel-empty"
          className="w-72 shrink-0 border-l border-ink/10 bg-porcelain p-4 text-xs text-ink-soft"
        >
          Select a block to see its receipts, membership and sockets.
        </aside>
      )}

      {/* F2 — Show Code modal / Copy Packet panel (read-only surfaces). */}
      {modal?.kind === 'showcode' && modalBlock && modalRollup && (
        <ShowCode
          block={modalBlock}
          rollup={modalRollup}
          repoId={repoId}
          brainRoot={brainRoot}
          onClose={() => setModal(null)}
          onAskAgent={(subPath) =>
            setModal({ kind: 'packet', blockId: modalBlock.block_id, subPath: subPath ?? null })
          }
        />
      )}
      {modal?.kind === 'packet' && modalBlock && modalRollup && (
        <PacketCompose
          block={modalBlock}
          rollup={modalRollup}
          repoId={repoId}
          subPath={modal.subPath}
          brainRoot={brainRoot}
          liveRunners={runnerd.runners}
          policy={{ runnerdAvailable: runnerd.available }}
          onClose={() => setModal(null)}
        />
      )}
    </div>
  );
}
