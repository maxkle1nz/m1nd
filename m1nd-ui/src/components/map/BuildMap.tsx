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
import { Icon } from '../../lib/icons/registry';
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
  /** Test seam: force the Unmapped tray open (SSR has no click). */
  initialUnmappedExpanded?: boolean;
}

/** Toast tint by kind (F3b §D) — all sanctioned non-violet tokens: success = sage,
 *  conflict/read-only = amber warn, error = clay. */
const TOAST_CLASSES: Record<ReconcileToast['kind'], string> = {
  ok: 'border-verdict-act/50 bg-verdict-act-tint/40 text-ink',
  conflict: 'border-verdict-reverify/50 bg-verdict-reverify-tint/40 text-ink',
  readonly: 'border-verdict-reverify/50 bg-verdict-reverify-tint/40 text-ink',
  error: 'border-state-failure/50 bg-state-failure-tint/40 text-ink',
};

/** §1.3 EMPTY — no skeleton bound for this repo. The honest backend copy, and an
 *  Import CTA that is DISABLED with a note pointing at the write verb (F1 is
 *  read-only; the map never fakes a mutation). */
function BuildMapEmpty({ honest }: { honest: string | null }) {
  return (
    <div className="flex-1 flex items-center justify-center bg-porcelain" data-role="build-map-empty">
      <div className="max-w-sm text-center space-y-3 px-6">
        <div className="text-ink font-semibold">No skeleton yet for this repo.</div>
        <p className="text-xs text-ink-soft font-mono">
          {honest ?? 'no skeleton yet — import a seed or run a scan'}
        </p>
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
          import a seed via the <span className="font-mono">system_blocks_seed_import</span> verb
        </p>
      </div>
    </div>
  );
}

/** §1.2 first-run banner — a candidate skeleton, nothing ratified yet. */
function CandidateBanner() {
  return (
    <div
      data-role="candidate-banner"
      className="mx-4 mt-4 rounded-lg border border-stale-lilac/50 bg-stale-lilac/10 px-4 py-2.5 text-xs text-ink"
    >
      <span className="font-semibold text-stale-lilac">Candidate skeleton ready</span> — nothing on this map is
      ratified yet. Names are guesses until you ratify them.
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
  initialUnmappedExpanded = false,
}: BuildMapProps) {
  const store = snapshot.present ? snapshot.store ?? null : null;
  const [selectedId, setSelectedId] = useState<string | null>(initialSelectedId);
  const [modal, setModal] = useState<MapModal>(null);

  // F2.5 §3b — when the mission tray asks to open a block while the map is already
  // mounted, `initialSelectedId` changes: follow it so the tray click lands the
  // selection. (No-op in SSR — effects don't run under renderToStaticMarkup.)
  useEffect(() => {
    if (initialSelectedId != null) setSelectedId(initialSelectedId);
  }, [initialSelectedId]);

  if (!store || !rollup) {
    return <BuildMapEmpty honest={snapshot.honest ?? null} />;
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
            </span>
          </div>
          <div className="flex items-center gap-2">
            {/* Reconcile (F3b §D) — the ONE write the map offers. Near the read-only
                tag: reconcile is itself a write, refused on a read-only owner (the
                toast says so, honestly; the button never vanishes). */}
            {onReconcile && (
              <button
                type="button"
                data-role="reconcile"
                onClick={onReconcile}
                disabled={reconciling}
                title="Resolve every block's membership against the real repo files"
                className="flex items-center gap-1.5 px-2 py-1 text-[11px] font-mono text-ink bg-bone border border-ink/15 rounded hover:shadow-contact transition-shadow disabled:opacity-60 disabled:cursor-progress"
              >
                <Icon name="ingest" size={14} decorative />
                {reconciling ? 'Reconciling…' : 'Reconcile'}
              </button>
            )}
            <span className="text-[10px] font-mono text-ink-soft" data-role="read-only-note">
              read-only
            </span>
          </div>
        </div>

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

        {rollup.candidate && <CandidateBanner />}

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
          onClose={() => setModal(null)}
        />
      )}
    </div>
  );
}
