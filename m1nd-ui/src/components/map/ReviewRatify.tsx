/*
 * ReviewRatify — evolved into Edit Names & Boundaries (HUMAN-VIEW-V2 F11-c; the
 * screen book §3 is the visual spec — implementation invents nothing).
 *
 * The two-column ratification screen over a CANDIDATE store: the block list
 * (inline-editable names, member counts, component confidence, seam flags,
 * expandable members with certainty dots) and the selected panel (editable
 * purpose, the honest boundary view, seam-resolution radios, Split/Merge/Reset).
 * The unmapped tray assigns residue; the footer ratifies (all / selected / later).
 *
 * The laws it keeps:
 * - §4b: EVERY gesture compiles to `candidate_edit` ops, batched per gesture — a
 *   rename commit is one op, a seam radio is one op, an assign is one op, a merge
 *   is one op, a split is one op with EXPLICIT path groups from the member
 *   selection. "Reset proposal" reverts UNAPPLIED drafts only (applied edits are
 *   history — undone by further edits, never by store rollback).
 * - §0/§4c (the friction law): provisional blocks surface first ("— needs you");
 *   runner-named blocks render calm (no badge, no acceptance step — 0b); the
 *   header carries the zero-touch line when every block is named.
 * - §2b: "Name with runner" (all provisional, or the selected block) rides the
 *   `candidate_naming` route; a disabled button always says why (§4d).
 * - o4: a LIVE advisory curating lease renders a non-blocking banner; expired
 *   renders nothing.
 * - The owner above (BuildMapView) is the write owner: it posts the batches under
 *   the OCC key it read, owns the honest toasts, and reloads; a conflict reloads,
 *   never a silent merge.
 */
import { useState } from 'react';
import {
  candidateConfidence,
  domainTag as toDomainTag,
  type SystemBlock,
  type SystemBlockStore,
  type WriteToast,
} from '../../lib/buildMap';
import {
  acceptNameOp,
  assignOp,
  blockSeams,
  curatingBanner,
  mergeOp,
  provisionalFirstQueue,
  purposeOp,
  ratifyGateReasonV2,
  renameOp,
  seamOp,
  splitOpFromSelection,
  zeroTouchLine,
  type EditOpInput,
} from '../../lib/candidateEdit';
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
  /** Bounds the LIST page only (default 16) — never the emitted seed (§7). */
  reviewLimit?: number;
  /** Post ONE gesture batch through `candidate_edit` (the write owner holds the
   *  OCC key it read; success/conflict reload). */
  onApplyOps: (ops: EditOpInput[]) => void;
  applying?: boolean;
  /** §2b "Name with runner": absent ids = every provisional block. */
  onNameWithRunner?: (blockIds?: string[]) => void;
  naming?: boolean;
  /** Whether a runner daemon is live (the status read). A disabled naming button
   *  always says why (§4d). */
  runnerAvailable?: boolean;
  runnerUnavailableReason?: string;
  /** The blanket ratify (`system_blocks_ratify`, block_ids omitted). */
  onRatifyAll: () => void;
  /** Ratify ONLY the selected block (`block_ids:[selected]`). */
  onRatifySelected?: (blockId: string) => void;
  ratifying?: boolean;
  ratifyToast?: WriteToast | null;
  onDismissToast?: () => void;
  onClose: () => void;
  /** SSR/test seam: render the whole queue past the review limit. */
  initialExpanded?: boolean;
  /** Deterministic clock for the o4 curating banner (tests inject it). */
  nowIso?: string;
  /** SSR/test seam: the initially selected block (default: the queue's first). */
  initialSelectedId?: string | null;
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

/** One block row in the left column: the inline-editable name (a commit compiles to
 *  ONE rename op), the honest chips, and the expandable member list with certainty
 *  dots + seam claims (the drawn ▸/▾ anatomy). */
function BlockRow({
  block,
  store,
  repoId,
  selected,
  expanded,
  draftKey,
  splitSelection,
  onSelect,
  onToggleExpand,
  onApplyOps,
  onToggleMember,
  onNameWithRunner,
  runnerAvailable,
  runnerUnavailableReason,
}: {
  block: SystemBlock;
  store: SystemBlockStore;
  repoId: string | null;
  selected: boolean;
  expanded: boolean;
  draftKey: number;
  splitSelection: ReadonlySet<string>;
  onSelect: (blockId: string) => void;
  onToggleExpand: (blockId: string) => void;
  onApplyOps: (ops: EditOpInput[]) => void;
  onToggleMember: (path: string) => void;
  onNameWithRunner?: (blockIds?: string[]) => void;
  runnerAvailable: boolean;
  runnerUnavailableReason: string;
}) {
  const needsNaming = block.candidate_meta?.needs_owner_naming === true;
  const seams = blockSeams(store, block.block_id);
  const seamPaths = new Set(seams.map((s) => s.path));
  const tag = toDomainTag(block.block_id, repoId);

  const commitName = (value: string) => {
    const op = renameOp(block, value);
    if (op) onApplyOps([op]);
  };

  return (
    <li
      data-role="review-block"
      data-block-id={block.block_id}
      data-selected={selected}
      data-needs-naming={needsNaming}
      data-seam={seams.length > 0}
      className={[
        'rounded-lg border px-3 py-2 bg-warm-paper cursor-pointer',
        selected ? 'border-ink/40 shadow-contact' : 'border-dashed border-stale-lilac/60',
      ].join(' ')}
      onClick={() => onSelect(block.block_id)}
    >
      <div className="flex items-start gap-2">
        <button
          type="button"
          data-role="expand-block"
          onClick={(e) => {
            e.stopPropagation();
            onToggleExpand(block.block_id);
          }}
          title={expanded ? 'collapse members' : 'expand members'}
          className="shrink-0 text-ink-soft hover:text-ink text-[11px] leading-6"
        >
          {expanded ? '▾' : '▸'}
        </button>
        <div className="min-w-0 flex-1">
          <div className="text-[10px] font-semibold uppercase tracking-wide text-ink-soft">{tag}</div>
          {/* The inline-editable name (screen §3: `[Auth_________]`). A commit
              (blur / Enter) compiles to ONE rename op; a no-op posts nothing. */}
          <div className="flex items-center gap-2">
            <input
              key={`name:${block.block_id}:${draftKey}`}
              data-role="name-input"
              defaultValue={block.name}
              onClick={(e) => e.stopPropagation()}
              onBlur={(e) => commitName(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') (e.currentTarget as HTMLInputElement).blur();
              }}
              title="rename — commits on blur/Enter as one candidate_edit op (an owner touch)"
              className={[
                'min-w-0 flex-1 bg-bone border border-ink/15 rounded px-1.5 py-0.5 text-[13px] font-semibold leading-tight',
                needsNaming ? 'text-stale-lilac' : 'text-ink',
              ].join(' ')}
            />
            {needsNaming && (
              <span
                data-role="provisional-name"
                className="shrink-0 text-[11px] italic text-stale-lilac"
              >
                — needs you
              </span>
            )}
          </div>
          <div className="text-[11px] font-mono text-ink-soft mt-0.5">
            {block.membership.length} members
          </div>
          <Confidence block={block} />
          {seams.length > 0 && (
            <div
              data-role="seam-warning"
              className="mt-1 inline-flex items-center gap-1 text-[10px] font-mono text-verdict-reverify border border-verdict-reverify/40 bg-verdict-reverify-tint/40 rounded px-1.5 py-0.5"
              title="a member is claimed by more than one block — resolve it in the panel"
            >
              ⚠ seam · {seams.length} member{seams.length === 1 ? '' : 's'}
            </div>
          )}
          {expanded && (
            <ul className="mt-1.5 space-y-0.5" data-role="member-list">
              {block.membership.map((m) => {
                const isSeam = seamPaths.has(m.path);
                const owners = seams.find((s) => s.path === m.path)?.owners ?? [];
                const others = owners.filter((o) => o !== block.block_id);
                return (
                  <li
                    key={m.path}
                    data-role="member-row"
                    data-member-seam={isSeam}
                    className="text-[11px] font-mono text-ink-soft flex items-start gap-1.5"
                  >
                    {selected && (
                      <input
                        type="checkbox"
                        data-role="member-select"
                        checked={splitSelection.has(m.path)}
                        onChange={() => onToggleMember(m.path)}
                        onClick={(e) => e.stopPropagation()}
                        title="select for Split block"
                        className="mt-0.5 shrink-0"
                      />
                    )}
                    <span className="min-w-0 break-all">
                      {m.path}{' '}
                      {isSeam ? (
                        <span className="text-verdict-reverify" data-role="member-certainty">
                          ◐ SEAM ⚠
                        </span>
                      ) : (
                        <span className="text-verdict-act" data-role="member-certainty">
                          ●
                        </span>
                      )}
                      {others.length > 0 && (
                        <span className="block text-[10px] text-ink-soft" data-role="also-claimed">
                          also claimed by: {others.join(', ')}
                        </span>
                      )}
                    </span>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
        {needsNaming && (
          <div className="shrink-0 flex flex-col items-end gap-1">
            <button
              type="button"
              data-role="accept-block"
              onClick={(e) => {
                e.stopPropagation();
                onApplyOps([acceptNameOp(block)]);
              }}
              title="accept this name — a REAL owner touch (one rename op; clears needs-naming)"
              className="px-2 py-1 text-[11px] font-mono rounded border border-ink/15 bg-bone text-ink hover:shadow-contact transition-shadow"
            >
              Accept name
            </button>
            {onNameWithRunner && (
              <button
                type="button"
                data-role="name-with-runner-block"
                disabled={!runnerAvailable}
                onClick={(e) => {
                  e.stopPropagation();
                  onNameWithRunner([block.block_id]);
                }}
                title={
                  runnerAvailable
                    ? 'name this block with the live naming-runner'
                    : runnerUnavailableReason
                }
                className="px-2 py-1 text-[10px] font-mono rounded border border-ink/15 bg-bone text-ink-soft disabled:opacity-60 disabled:cursor-not-allowed hover:shadow-contact transition-shadow"
              >
                Name with runner
              </button>
            )}
          </div>
        )}
      </div>
    </li>
  );
}

/** The right panel for the selected block (screen §3): editable purpose, the honest
 *  boundary view, seam radios, and the Split/Merge/Reset actions. */
function SelectedPanel({
  block,
  store,
  draftKey,
  splitSelection,
  applying,
  onApplyOps,
  onReset,
}: {
  block: SystemBlock;
  store: SystemBlockStore;
  draftKey: number;
  splitSelection: ReadonlySet<string>;
  applying: boolean;
  onApplyOps: (ops: EditOpInput[]) => void;
  onReset: () => void;
}) {
  const seams = blockSeams(store, block.block_id);
  const conf = block.candidate_meta ? candidateConfidence(block.candidate_meta) : null;
  const dominantDir = dominantTopDir(block);
  const outside = block.membership.filter((m) => topDir(m.path) !== dominantDir);
  const split = splitOpFromSelection(block, splitSelection);
  const others = store.blocks.filter(
    (b) => b.state === 'candidate' && b.block_id !== block.block_id,
  );

  return (
    <div data-role="selected-panel" className="min-w-0 flex-1 overflow-y-auto px-4 py-3 space-y-3">
      <div>
        <div className="text-[10px] font-semibold uppercase tracking-wide text-ink-soft">selected</div>
        <div className="text-sm font-semibold text-ink" data-role="selected-name">
          {block.name}
          {conf && (
            <span className="ml-2 text-[11px] font-mono text-ink-soft">conf {conf.summaryPct}%</span>
          )}
        </div>
      </div>

      {/* purpose (editable) — a blur commits ONE rename op carrying purpose only. */}
      <div>
        <div className="text-[11px] font-mono text-ink-soft mb-1">purpose (editable)</div>
        <textarea
          key={`purpose:${block.block_id}:${draftKey}`}
          data-role="purpose-input"
          defaultValue={block.purpose}
          rows={2}
          onBlur={(e) => {
            const op = purposeOp(block, e.currentTarget.value);
            if (op) onApplyOps([op]);
          }}
          title="the block's one-line purpose — commits on blur as one candidate_edit op"
          className="w-full bg-bone border border-ink/15 rounded px-2 py-1 text-[12px] text-ink"
        />
      </div>

      {/* The honest boundary view: members OUTSIDE the dominant directory are the
          graph's pull; the full directory diff needs the repo file list (reconcile),
          so this panel never fabricates the "pushes out" half. */}
      <div data-role="boundary-diff">
        <div className="text-[11px] font-mono text-ink-soft mb-1">
          BOUNDARY (proposal vs its directory)
        </div>
        {outside.length === 0 ? (
          <div className="text-[11px] font-mono text-ink-soft">
            every member sits under <span className="text-ink">{dominantDir}</span> — no pulls
          </div>
        ) : (
          <ul className="space-y-0.5">
            {outside.map((m) => (
              <li key={m.path} className="text-[11px] font-mono text-verdict-act break-all">
                + {m.path} <span className="text-ink-soft">(outside {dominantDir} — the graph pulls it in)</span>
              </li>
            ))}
          </ul>
        )}
      </div>

      {/* Seam resolution (screen §3): one radio group per seam member — keep in
          both, or name a primary among the REAL owners. One choice = one op. */}
      {seams.length > 0 && (
        <div data-role="seam-resolution" className="space-y-2">
          {seams.map((seam) => (
            <div key={seam.path} className="border border-verdict-reverify/40 bg-verdict-reverify-tint/20 rounded px-2 py-1.5">
              <div className="text-[11px] font-mono text-ink mb-1">
                SEAM: <span className="break-all">{seam.path}</span> belongs to{' '}
                {seam.owners.join(' and ')} (membership is many-to-many).
              </div>
              <div className="flex flex-wrap gap-x-3 gap-y-1">
                <label className="text-[11px] font-mono text-ink flex items-center gap-1">
                  <input
                    type="radio"
                    name={`seam:${block.block_id}:${seam.path}`}
                    data-role="seam-radio"
                    data-choice="both"
                    onChange={() => onApplyOps([seamOp(seam.path, 'both')])}
                  />
                  keep in both
                </label>
                {seam.owners.map((owner) => (
                  <label key={owner} className="text-[11px] font-mono text-ink flex items-center gap-1">
                    <input
                      type="radio"
                      name={`seam:${block.block_id}:${seam.path}`}
                      data-role="seam-radio"
                      data-choice={`primary:${owner}`}
                      onChange={() => onApplyOps([seamOp(seam.path, { primary: owner })])}
                    />
                    primary: {owner}
                  </label>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Actions (screen §3): Split from the member selection (o3 — explicit
          groups), Merge into a sibling, Reset the UNAPPLIED drafts. Disabled
          states always say why (§4d). */}
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          data-role="split-block"
          disabled={applying || 'reason' in split}
          onClick={() => {
            if ('op' in split) onApplyOps([split.op]);
          }}
          title={'op' in split ? 'split the selected members into a new block' : split.reason}
          className="px-2 py-1 text-[11px] font-mono rounded border border-ink/15 bg-bone text-ink disabled:opacity-60 disabled:cursor-not-allowed hover:shadow-contact transition-shadow"
        >
          Split block
        </button>
        <label className="text-[11px] font-mono text-ink-soft flex items-center gap-1">
          Merge into…
          <select
            key={`merge:${block.block_id}:${draftKey}`}
            data-role="merge-select"
            defaultValue=""
            disabled={applying || others.length === 0}
            onChange={(e) => {
              const into = e.currentTarget.value;
              if (into) onApplyOps([mergeOp(into, [block.block_id])]);
            }}
            title={
              others.length === 0
                ? 'no sibling candidate block to merge into'
                : 'absorb THIS block into the chosen one (sockets follow, meta recomputes)'
            }
            className="bg-bone border border-ink/15 rounded px-1 py-0.5 text-[11px] text-ink disabled:opacity-60"
          >
            <option value="">choose a block…</option>
            {others.map((b) => (
              <option key={b.block_id} value={b.block_id}>
                {b.name}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          data-role="reset-proposal"
          onClick={onReset}
          title="discard the UNAPPLIED drafts (typed names/purpose, member selection) — applied edits are history, undone by further edits"
          className="px-2 py-1 text-[11px] font-mono rounded border border-ink/15 bg-bone text-ink-soft hover:shadow-contact transition-shadow"
        >
          Reset proposal
        </button>
      </div>
    </div>
  );
}

function topDir(path: string): string {
  const i = path.indexOf('/');
  return i === -1 ? '.' : path.slice(0, i);
}

function dominantTopDir(block: SystemBlock): string {
  const counts = new Map<string, number>();
  for (const m of block.membership) {
    const d = topDir(m.path);
    counts.set(d, (counts.get(d) ?? 0) + 1);
  }
  let best = '.';
  let bestN = -1;
  for (const [d, n] of counts) {
    if (n > bestN || (n === bestN && d < best)) {
      best = d;
      bestN = n;
    }
  }
  return best;
}

export default function ReviewRatify({
  store,
  repoId,
  reviewLimit = 16,
  onApplyOps,
  applying = false,
  onNameWithRunner,
  naming = false,
  runnerAvailable = false,
  runnerUnavailableReason = 'no runner daemon connected — start m1nd-runnerd with a pinned naming-runner',
  onRatifyAll,
  onRatifySelected,
  ratifying = false,
  ratifyToast = null,
  onDismissToast,
  onClose,
  initialExpanded = false,
  nowIso,
  initialSelectedId = null,
}: ReviewRatifyProps) {
  const queue = provisionalFirstQueue(store);
  const total = queue.length;
  const limit = Math.max(1, reviewLimit);
  const shown = initialExpanded ? queue : queue.slice(0, limit);
  const overflow = total - shown.length;

  const [selectedId, setSelectedId] = useState<string | null>(initialSelectedId);
  const [expandedIds, setExpandedIds] = useState<ReadonlySet<string>>(new Set());
  const [splitSelection, setSplitSelection] = useState<ReadonlySet<string>>(new Set());
  // Bumping the nonce remounts the draft inputs → "Reset proposal" discards
  // UNAPPLIED text only (applied edits are history — §4b).
  const [draftKey, setDraftKey] = useState(0);

  const selected =
    queue.find((b) => b.block_id === (selectedId ?? shown[0]?.block_id)) ?? shown[0] ?? null;

  const gate = ratifyGateReasonV2(store);
  const zeroTouch = zeroTouchLine(store);
  const banner = curatingBanner(store, nowIso ?? new Date().toISOString());
  // The census counts DISTINCT seam members (the drawn "3 seams" is paths, not
  // blocks — one shared file claimed by two blocks is ONE seam).
  const seamCount = new Set(
    store.blocks
      .filter((b) => b.state === 'candidate')
      .flatMap((b) => blockSeams(store, b.block_id).map((s) => s.path)),
  ).size;
  const unmappedTotal = store.unmapped_total ?? 0;
  const unmappedFiles = store.unmapped_files ?? [];
  const provisionalIds = queue
    .filter((b) => b.candidate_meta?.needs_owner_naming === true)
    .map((b) => b.block_id);

  const selectBlock = (blockId: string) => {
    setSelectedId(blockId);
    setSplitSelection(new Set());
  };
  const toggleExpand = (blockId: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(blockId)) next.delete(blockId);
      else next.add(blockId);
      return next;
    });
  };
  const toggleMember = (path: string) => {
    setSplitSelection((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };
  const resetDrafts = () => {
    setSplitSelection(new Set());
    setDraftKey((n) => n + 1);
  };

  return (
    <div
      data-role="review-ratify"
      role="dialog"
      aria-label="Edit names and boundaries — ratify the candidate skeleton"
    >
      <div className="fixed inset-0 bg-ink/30 z-40" onClick={onClose} aria-hidden />
      <aside className="fixed top-[4%] left-1/2 -translate-x-1/2 z-50 w-full max-w-5xl mx-4 h-[90vh] flex flex-col rounded-lg border border-hairline bg-warm-paper shadow-card">
        {/* Header — the census + the zero-touch line + Name with runner (§4c/§2b). */}
        <div className="px-4 py-3 border-b border-ink/10 space-y-1.5">
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2 min-w-0">
              <Icon name="blocks" size={16} decorative />
              <div className="min-w-0">
                <div className="text-sm font-semibold text-ink">
                  Ratify skeleton — edit names &amp; boundaries
                </div>
                <div className="text-[11px] font-mono text-ink-soft" data-role="review-census">
                  candidate v{store.skeleton.version} · {total} block{total === 1 ? '' : 's'} ·{' '}
                  {seamCount} seam{seamCount === 1 ? '' : 's'} · {unmappedTotal} unmapped
                </div>
              </div>
            </div>
            <div className="flex items-center gap-2 shrink-0">
              {onNameWithRunner && (
                <button
                  type="button"
                  data-role="name-with-runner"
                  disabled={naming || !runnerAvailable || provisionalIds.length === 0}
                  onClick={() => onNameWithRunner(undefined)}
                  title={
                    !runnerAvailable
                      ? runnerUnavailableReason
                      : provisionalIds.length === 0
                        ? 'every block is named — nothing for the runner to do'
                        : `name the ${provisionalIds.length} provisional block${provisionalIds.length === 1 ? '' : 's'} with the live naming-runner`
                  }
                  className="px-2 py-1 text-[11px] font-mono rounded border border-ink/15 bg-bone text-ink disabled:opacity-60 disabled:cursor-not-allowed hover:shadow-contact transition-shadow"
                >
                  {naming ? 'Naming…' : 'Name with runner'}
                </button>
              )}
              <button
                type="button"
                data-role="review-close"
                onClick={onClose}
                title="close — nothing is ratified until you ratify"
                className="text-ink-soft hover:text-ink text-lg leading-none"
              >
                ×
              </button>
            </div>
          </div>
          {zeroTouch && (
            <div
              data-role="zero-touch"
              className="text-[11px] font-mono text-verdict-act border border-verdict-act/40 bg-verdict-act-tint/30 rounded px-2 py-1 inline-block"
            >
              {zeroTouch}
            </div>
          )}
          {banner && (
            <div
              data-role="curating-banner"
              className="text-[11px] font-mono text-ink border border-stale-lilac/50 bg-stale-lilac/10 rounded px-2 py-1 inline-block"
              title="an advisory lease — it never blocks you; your edits still apply"
            >
              a hand is curating candidate v{store.skeleton.version} ({store.curating_by}, until{' '}
              {store.curating_until}) — advisory, never blocking
            </div>
          )}
        </div>

        {/* The two columns (screen §3). */}
        <div className="flex-1 min-h-0 flex">
          {/* Left: the block list + the unmapped tray. */}
          <div
            data-role="block-list"
            className="w-[46%] min-w-0 border-r border-ink/10 overflow-y-auto px-4 py-3 space-y-2"
          >
            <div className="text-[10px] font-semibold uppercase tracking-wide text-ink-soft">
              blocks (edit name · merge · split)
            </div>
            <ul className="space-y-2" data-role="review-queue">
              {shown.map((block) => (
                <BlockRow
                  key={block.block_id}
                  block={block}
                  store={store}
                  repoId={repoId}
                  selected={selected?.block_id === block.block_id}
                  expanded={initialExpanded || expandedIds.has(block.block_id)}
                  draftKey={draftKey}
                  splitSelection={splitSelection}
                  onSelect={selectBlock}
                  onToggleExpand={toggleExpand}
                  onApplyOps={onApplyOps}
                  onToggleMember={toggleMember}
                  onNameWithRunner={onNameWithRunner}
                  runnerAvailable={runnerAvailable}
                  runnerUnavailableReason={runnerUnavailableReason}
                />
              ))}
              {total === 0 && (
                <li className="text-[11px] font-mono text-ink-soft" data-role="review-empty">
                  no candidate blocks to review — the map is ratified.
                </li>
              )}
              {overflow > 0 && (
                <li className="text-[11px] font-mono text-ink-soft pt-1" data-role="review-overflow">
                  showing {shown.length} of {total} — the review limit is {limit}; every block is in
                  the seed, the count is true.
                </li>
              )}
            </ul>

            {/* The unmapped tray (screen §3): assign residue, or honestly leave it. */}
            {(unmappedTotal > 0 || unmappedFiles.length > 0) && (
              <div data-role="unmapped-tray" className="pt-2 border-t border-ink/10 space-y-1">
                <div className="text-[10px] font-semibold uppercase tracking-wide text-ink-soft">
                  unmapped residue ({unmappedTotal} file{unmappedTotal === 1 ? '' : 's'})
                </div>
                <ul className="space-y-1">
                  {unmappedFiles.slice(0, 8).map((path) => (
                    <li
                      key={path}
                      data-role="unmapped-row"
                      className="flex items-center justify-between gap-2 text-[11px] font-mono text-ink-soft"
                    >
                      <span className="min-w-0 break-all">{path}</span>
                      <select
                        key={`assign:${path}:${draftKey}`}
                        data-role="assign-select"
                        defaultValue=""
                        onChange={(e) => {
                          const blockId = e.currentTarget.value;
                          if (blockId) onApplyOps([assignOp(path, blockId)]);
                        }}
                        title="assign this file to a block (one op) — or leave it unmapped, the honest default"
                        className="shrink-0 bg-bone border border-ink/15 rounded px-1 py-0.5 text-[10px] text-ink"
                      >
                        <option value="">leave unmapped</option>
                        {store.blocks
                          .filter((b) => b.state === 'candidate')
                          .map((b) => (
                            <option key={b.block_id} value={b.block_id}>
                              assign to {b.name}
                            </option>
                          ))}
                      </select>
                    </li>
                  ))}
                  {unmappedTotal > unmappedFiles.slice(0, 8).length && (
                    <li className="text-[10px] font-mono text-ink-soft" data-role="unmapped-overflow">
                      … {unmappedTotal - Math.min(8, unmappedFiles.length)} more (the count is true)
                    </li>
                  )}
                </ul>
              </div>
            )}
          </div>

          {/* Right: the selected panel. */}
          {selected ? (
            <SelectedPanel
              block={selected}
              store={store}
              draftKey={draftKey}
              splitSelection={splitSelection}
              applying={applying}
              onApplyOps={onApplyOps}
              onReset={resetDrafts}
            />
          ) : (
            <div className="flex-1 flex items-center justify-center text-[11px] font-mono text-ink-soft">
              select a block to edit it
            </div>
          )}
        </div>

        {/* Footer — the fine print + the honest gate + the three gestures (§3). */}
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
          <p className="text-[10px] text-ink-soft">
            Ratifying signs boundaries as v1 in this brain. Agents and scans will respect them;
            drift will reopen this screen — scoped to what drifted, never a silent re-cluster.
          </p>
          <div className="flex items-center justify-between gap-3">
            {gate == null ? (
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
            <div className="flex items-center gap-2 shrink-0">
              {onRatifySelected && selected && (
                <button
                  type="button"
                  data-role="ratify-selected"
                  onClick={() => onRatifySelected(selected.block_id)}
                  disabled={
                    ratifying || selected.candidate_meta?.needs_owner_naming === true
                  }
                  title={
                    selected.candidate_meta?.needs_owner_naming === true
                      ? 'this block still needs a name — name it (or accept the runner name) first'
                      : `ratify only ${selected.name} → v1`
                  }
                  className="px-3 py-1.5 text-xs font-mono text-ink border border-ink/15 rounded bg-bone hover:shadow-contact transition-shadow disabled:opacity-60 disabled:cursor-not-allowed"
                >
                  Ratify selected only
                </button>
              )}
              <button
                type="button"
                data-role="review-later"
                onClick={onClose}
                className="px-3 py-1.5 text-xs text-ink border border-ink/15 rounded bg-bone hover:shadow-contact transition-shadow"
              >
                Later
              </button>
            </div>
          </div>
        </div>
      </aside>
    </div>
  );
}
