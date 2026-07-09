/*
 * ReviewRatify — the Review-&-ratify walk (HUMAN-VIEW-V2 F0c §5, objection 10).
 *
 * The candidate map's gesture is REVIEW, not blanket ratify: the walk lists the
 * proposed blocks lowest-support first (the honest weak ones surface), each with its
 * COMPONENT confidence (never a single vibe score) and any multi-owner seam, and asks
 * the owner to ACCEPT each provisional name — the "owner touch" a heuristic label
 * needs before it can be ratified. Only when EVERY candidate block is owner-accepted
 * AND no seam is unresolved does the blanket "Ratify all → v1" appear (§5): a blanket
 * gesture over provisional labels is never offered.
 *
 * Controlled + honest: the accept set is owned above (BuildMapView) so the write owner
 * runs the real `system_blocks_ratify` and reloads. F0c-a ships no name-write verb, so
 * this walk does NOT fake a rename — accepting adopts the block's stored name, and
 * renaming / splitting / seam resolution are honestly deferred to Edit Names &
 * Boundaries (a later slice), shown as a disabled affordance, never a dead button.
 * The `review_limit` bounds only this queue, never the emitted seed (§7).
 */
import {
  candidateConfidence,
  canRatifyAll,
  domainTag as toDomainTag,
  ratifyAllGateReason,
  reviewQueue,
  unresolvedSeamCount,
  blockNeedsNaming,
  blockHasUnresolvedSeam,
  type SystemBlock,
  type SystemBlockStore,
  type WriteToast,
} from '../../lib/buildMap';
import { Icon } from '../../lib/icons/registry';

/** Toast tint by kind — the shared write-toast palette (all sanctioned non-violet). */
const TOAST_CLASSES: Record<WriteToast['kind'], string> = {
  ok: 'border-verdict-act/50 bg-verdict-act-tint/40 text-ink',
  conflict: 'border-verdict-reverify/50 bg-verdict-reverify-tint/40 text-ink',
  readonly: 'border-verdict-reverify/50 bg-verdict-reverify-tint/40 text-ink',
  error: 'border-state-failure/50 bg-state-failure-tint/40 text-ink',
};

export interface ReviewRatifyProps {
  store: SystemBlockStore;
  repoId: string | null;
  /** Bounds the review QUEUE only (default 16) — never the emitted seed (§7). */
  reviewLimit?: number;
  /** The owner-accepted block ids (owned above so the write owner runs ratify). */
  acceptedIds: ReadonlySet<string>;
  /** Toggle a block's owner-acceptance (the §5 owner touch). */
  onAccept: (blockId: string) => void;
  /** Run the blanket ratify (the real `system_blocks_ratify`, block_ids omitted). */
  onRatifyAll: () => void;
  ratifying?: boolean;
  ratifyToast?: WriteToast | null;
  onDismissToast?: () => void;
  onClose: () => void;
  /** SSR/test seam: render the whole queue past the review limit. */
  initialExpanded?: boolean;
}

/** One block's component confidence, rendered honestly (§3b): each present component
 *  labeled; a `null` cohesion reads "—", never a fabricated number. */
function Confidence({ block }: { block: SystemBlock }) {
  const meta = block.candidate_meta;
  if (!meta) return null;
  const conf = candidateConfidence(meta);
  return (
    <div className="text-[10px] font-mono text-ink-soft" data-role="confidence">
      <span className="text-ink tabular-nums">{conf.summaryPct}%</span>
      <span className="text-ink-soft"> · </span>
      {conf.components.map((c, i) => (
        <span key={c.key}>
          {i > 0 ? ' · ' : ''}
          {c.label} <span className="tabular-nums">{c.pct == null ? '—' : `${c.pct}%`}</span>
        </span>
      ))}
      <span className="text-ink-soft"> · named by {conf.namedBy}</span>
    </div>
  );
}

function ReviewRow({
  block,
  repoId,
  accepted,
  onAccept,
}: {
  block: SystemBlock;
  repoId: string | null;
  accepted: boolean;
  onAccept: (blockId: string) => void;
}) {
  const needsNaming = blockNeedsNaming(block);
  const seam = blockHasUnresolvedSeam(block);
  const tag = toDomainTag(block.block_id, repoId);
  return (
    <li
      data-role="review-block"
      data-block-id={block.block_id}
      data-accepted={accepted}
      data-needs-naming={needsNaming}
      data-seam={seam}
      className="rounded-lg border border-dashed border-stale-lilac/60 bg-warm-paper px-3 py-2"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-[10px] font-semibold uppercase tracking-wide text-ink-soft">{tag}</div>
          {needsNaming ? (
            <div className="text-[13px] font-semibold text-stale-lilac leading-tight" data-role="provisional-name">
              {block.name} <span className="font-normal italic">— unnamed, needs you</span>
            </div>
          ) : (
            <div className="text-[13px] font-semibold text-ink leading-tight" data-role="block-name">
              {block.name}
            </div>
          )}
          <div className="text-[11px] font-mono text-ink-soft mt-0.5">{block.membership.length} members</div>
          <Confidence block={block} />
          {seam && (
            <div
              data-role="seam-warning"
              className="mt-1 inline-flex items-center gap-1 text-[10px] font-mono text-verdict-reverify border border-verdict-reverify/40 bg-verdict-reverify-tint/40 rounded px-1.5 py-0.5"
              title="a member is claimed by more than one block — resolve it in Edit Names & Boundaries (a later slice)"
            >
              ⚠ seam · {block.candidate_meta?.shared_member_count} shared
            </div>
          )}
        </div>
        <button
          type="button"
          data-role="accept-block"
          onClick={() => onAccept(block.block_id)}
          title={accepted ? 'this name is owner-accepted — click to undo' : 'accept this name (the owner touch a candidate needs before ratify)'}
          className={[
            'shrink-0 flex items-center gap-1 px-2 py-1 text-[11px] font-mono rounded border transition-shadow',
            accepted
              ? 'border-verdict-act/60 bg-verdict-act-tint/40 text-verdict-act'
              : 'border-ink/15 bg-bone text-ink hover:shadow-contact',
          ].join(' ')}
        >
          {accepted ? (
            <>
              <Icon name="verdictAct" size={14} decorative />
              accepted
            </>
          ) : (
            'Accept name'
          )}
        </button>
      </div>
    </li>
  );
}

export default function ReviewRatify({
  store,
  repoId,
  reviewLimit = 16,
  acceptedIds,
  onAccept,
  onRatifyAll,
  ratifying = false,
  ratifyToast = null,
  onDismissToast,
  onClose,
  initialExpanded = false,
}: ReviewRatifyProps) {
  const { ordered, total, limit } = reviewQueue(store, reviewLimit);
  const shown = initialExpanded ? ordered : ordered.slice(0, limit);
  const overflow = total - shown.length;
  const gate = ratifyAllGateReason(store, acceptedIds);
  const ready = canRatifyAll(store, acceptedIds);
  const seamCount = unresolvedSeamCount(store);
  const unmappedTotal = store.unmapped_total ?? 0;

  return (
    <div data-role="review-ratify" role="dialog" aria-label="Review and ratify the candidate skeleton">
      <div className="fixed inset-0 bg-ink/30 z-40" onClick={onClose} aria-hidden />
      <aside
        className="fixed top-[6%] left-1/2 -translate-x-1/2 z-50 w-full max-w-2xl mx-4 h-[85vh] flex flex-col rounded-lg border border-hairline bg-warm-paper shadow-card"
      >
        {/* Header */}
        <div className="px-4 py-3 border-b border-ink/10 flex items-center justify-between gap-3">
          <div className="flex items-center gap-2 min-w-0">
            <Icon name="blocks" size={16} decorative />
            <div className="min-w-0">
              <div className="text-sm font-semibold text-ink">Review &amp; ratify</div>
              <div className="text-[11px] font-mono text-ink-soft" data-role="review-census">
                candidate v{store.skeleton.version} · {total} block{total === 1 ? '' : 's'} · {seamCount} seam
                {seamCount === 1 ? '' : 's'} · {unmappedTotal} unmapped
              </div>
            </div>
          </div>
          <button
            type="button"
            data-role="review-close"
            onClick={onClose}
            title="close — nothing is ratified until you ratify"
            className="text-ink-soft hover:text-ink text-lg leading-none shrink-0"
          >
            ×
          </button>
        </div>

        {/* Honest intro — accepting adopts the stored name; renaming/seams are deferred. */}
        <p className="px-4 pt-3 text-[11px] text-ink-soft">
          Names are guesses until you accept them. Ratifying signs these boundaries as v1 in this brain — agents
          and scans will respect them.
        </p>

        {/* The review queue — lowest support first (§3b). */}
        <ul className="flex-1 overflow-y-auto px-4 py-3 space-y-2" data-role="review-queue">
          {shown.map((block) => (
            <ReviewRow
              key={block.block_id}
              block={block}
              repoId={repoId}
              accepted={acceptedIds.has(block.block_id)}
              onAccept={onAccept}
            />
          ))}
          {total === 0 && (
            <li className="text-[11px] font-mono text-ink-soft" data-role="review-empty">
              no candidate blocks to review — the map is ratified.
            </li>
          )}
          {overflow > 0 && (
            <li className="text-[11px] font-mono text-ink-soft pt-1" data-role="review-overflow">
              showing {shown.length} of {total} — the review limit is {limit}; every block is in the seed, the count is true.
            </li>
          )}
        </ul>

        {/* Footer — the gate + the blanket ratify + the honest deferrals. */}
        <div className="px-4 py-3 border-t border-ink/10 space-y-2">
          {ratifyToast && (
            <div
              data-role="ratify-toast"
              data-toast-kind={ratifyToast.kind}
              className={`flex items-start justify-between gap-3 rounded-lg border px-3 py-2 text-xs ${TOAST_CLASSES[ratifyToast.kind]}`}
            >
              <span className="font-mono">{ratifyToast.text}</span>
              {onDismissToast && (
                <button
                  type="button"
                  data-role="ratify-toast-dismiss"
                  onClick={onDismissToast}
                  title="dismiss"
                  className="text-ink-soft hover:text-ink shrink-0 leading-none"
                >
                  ×
                </button>
              )}
            </div>
          )}

          {/* Renaming / splitting / seam resolution — honestly deferred (never faked). */}
          <div className="flex items-center justify-between gap-3">
            <p className="text-[10px] text-ink-soft" data-role="deferred-editor">
              Renaming, splitting/merging and seam resolution land with Edit Names &amp; Boundaries (a later slice).
            </p>
            <button
              type="button"
              data-role="edit-names"
              disabled
              title="Edit Names & Boundaries is a later slice — F0c ships the review + ratify walk"
              className="shrink-0 px-2 py-1 text-[11px] font-mono text-ink-soft bg-bone border border-ink/15 rounded opacity-60 cursor-not-allowed"
            >
              Edit names &amp; boundaries
            </button>
          </div>

          <div className="flex items-center justify-between gap-3">
            {ready ? (
              <button
                type="button"
                data-role="ratify-all"
                onClick={onRatifyAll}
                disabled={ratifying}
                title="Ratify every candidate block — signs the boundaries as v1"
                className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-mono text-verdict-act bg-verdict-act-tint/40 border border-verdict-act/60 rounded hover:shadow-contact transition-shadow disabled:opacity-60 disabled:cursor-progress"
              >
                <Icon name="verdictAct" size={14} decorative />
                {ratifying ? 'Ratifying…' : `Ratify all ${total} → v1`}
              </button>
            ) : (
              <p className="text-[11px] font-mono text-ink-soft" data-role="ratify-gate">
                {gate}
              </p>
            )}
            <button
              type="button"
              data-role="review-later"
              onClick={onClose}
              className="shrink-0 px-3 py-1.5 text-xs text-ink border border-ink/15 rounded bg-bone hover:shadow-contact transition-shadow"
            >
              Later
            </button>
          </div>
        </div>
      </aside>
    </div>
  );
}
