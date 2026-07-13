/*
 * MissionCard — one mission's head, rendered as a tray card (HUMAN-VIEW-V2 F2.5 §3b,
 * §3c, §3f). Pure/presentational: every datum is a projection of the letter the
 * owner served; the card never posts (§3e). It shows the block name (click → the
 * block on the map), seat + capability + runner_id, the phase VERBATIM, the verdict
 * gist, the gate line, elapsed, and — on `landed` — the receipt anchor. A `merge_wait`
 * card flies the §1d landed-law ("gate green — receipt not landed") so the UI can
 * never render a green gate as a landing. A `failed` card carries the dismiss ✕
 * (§3c, presentation-only). A `landed` card expands to its provenance chain (§3f).
 *
 * §6-F2.5d — the human landing: a `merge_wait` card WITH a `receipt_candidate` offers
 * "Import this receipt" — one click opens a compact confirm that shows EXACTLY what
 * will be imported (block, type, the truncated artifact hash, the evidence refs) so the
 * gesture is explicit, never silent; confirming hands the head up to `onImportReceipt`
 * (the Live layer runs `landCandidate` + toasts + reloads). WITHOUT a candidate the card
 * flies the honest "gate green — no candidate attached" and offers no button.
 *
 * Copy law: no "done/proven/correct". Tokens are all sanctioned non-violet families.
 */
import { useState } from 'react';
import type { ArchiveBoundaryView, MissionHead, Phase } from '../../lib/missions';
import {
  blockLabelFromId,
  elapsedLabel,
  gateLine,
  mergeWaitStatusLine,
  phaseLabel,
  PHASE_META,
  receiptAnchorLabel,
} from '../../lib/missions';

export interface MissionCardProps {
  head: MissionHead;
  onOpenBlock: (blockId: string) => void;
  /** Present only for a `failed` card (§3c) — dismiss the pin (data never leaves the box). */
  onDismiss?: (missionId: string) => void;
  /** §3f — a `landed` card's provenance chain is showing. */
  provenanceOpen?: boolean;
  onToggleProvenance?: (missionId: string) => void;
  /** §6-F2.5d — the human-landing gesture. Present → a `merge_wait` card with a
   *  `receipt_candidate` offers "Import this receipt"; confirming calls this with the
   *  head. Absent (e.g. a pure SSR render, or a read-only owner) → no button. */
  onImportReceipt?: (head: MissionHead) => void;
  /** SSR/test seam: open the import confirm at mount (like PacketCompose's `initialMode`),
   *  so the confirm's honest detail is provable with a static render. */
  initialConfirmOpen?: boolean;
  /** F2.5e — the archive gesture. Present (with onArchivePreview) → a merge_wait+candidate
   *  card offers a discreet "Archive — superseded by newer boundary"; confirming calls this
   *  with the head (the Live layer runs `archiveHead`). Absent → no archive affordance. */
  onArchive?: (head: MissionHead) => void;
  /** F2.5e — fetch the FRESH two-boundary comparison for the archive confirm (the
   *  `landCandidate` step-1 read, done in the Live layer). Called when the human opens the
   *  confirm; its result fills "proved at vX — the block is at vY". */
  onArchivePreview?: (head: MissionHead) => Promise<ArchiveBoundaryView>;
  /** SSR/test seam: the archive confirm's boundary view, pre-supplied so the two-boundary
   *  copy is provable with a static render (like initialConfirmOpen for import). */
  initialArchiveView?: ArchiveBoundaryView;
  /** Injectable clock for a deterministic elapsed under test. */
  now?: number;
}

/** Truncate a long anchor hash for the confirm (`sha256:<12>…<6>`); short hashes pass
 *  through verbatim. Display-only — the full hash rides the receipt, never mangled. */
function truncateHash(hash: string): string {
  return hash.length > 24 ? `${hash.slice(0, 16)}…${hash.slice(-6)}` : hash;
}

/** Phase → accent (left rule + phase chip). All sanctioned non-violet tokens:
 *  in-progress = socket-blue; evidence-pending (gate/review/merge_wait) = amber
 *  reverify; landed = sage act; failed = clay failure; archived = muted hairline
 *  (terminal + deliberately quiet — a set-aside is never a loud state, F2.5e). */
const PHASE_ACCENT: Record<Phase, { rule: string; chip: string }> = {
  judging: { rule: 'border-l-socket-blue/60', chip: 'text-socket-blue border-socket-blue/40 bg-socket-blue/10' },
  executing: { rule: 'border-l-socket-blue/60', chip: 'text-socket-blue border-socket-blue/40 bg-socket-blue/10' },
  gate: { rule: 'border-l-verdict-reverify/70', chip: 'text-verdict-reverify border-verdict-reverify/40 bg-verdict-reverify-tint/40' },
  review: { rule: 'border-l-verdict-reverify/70', chip: 'text-verdict-reverify border-verdict-reverify/40 bg-verdict-reverify-tint/40' },
  merge_wait: { rule: 'border-l-verdict-reverify/70', chip: 'text-verdict-reverify border-verdict-reverify/40 bg-verdict-reverify-tint/40' },
  landed: { rule: 'border-l-verdict-act/70', chip: 'text-verdict-act border-verdict-act/40 bg-verdict-act-tint/40' },
  failed: { rule: 'border-l-state-failure/70', chip: 'text-state-failure border-state-failure/40 bg-state-failure-tint/40' },
  archived: { rule: 'border-l-hairline', chip: 'text-ink-soft border-hairline bg-porcelain' },
};

export default function MissionCard({
  head,
  onOpenBlock,
  onDismiss,
  provenanceOpen = false,
  onToggleProvenance,
  onImportReceipt,
  initialConfirmOpen = false,
  onArchive,
  onArchivePreview,
  initialArchiveView,
  now,
}: MissionCardProps) {
  const letter = head.head;
  const accent = PHASE_ACCENT[letter.phase];
  const name = blockLabelFromId(letter.block_id, letter.brain_ref);
  const elapsed = elapsedLabel(letter.started_at, now ?? Date.now());
  const gate = gateLine(letter);
  const mergeLine = mergeWaitStatusLine(letter);
  const receipt = receiptAnchorLabel(letter);
  const isLanded = letter.phase === 'landed';
  const isFailed = letter.phase === 'failed';

  // §6-F2.5d — the landing affordance is live only on a `merge_wait` card that carries
  // a `receipt_candidate` AND has a handler wired (a pure SSR render offers no button).
  const candidate = letter.receipt_candidate;
  const canImport = letter.phase === 'merge_wait' && !!candidate && !!onImportReceipt;
  const [confirmOpen, setConfirmOpen] = useState(initialConfirmOpen);

  // F2.5e — the archive affordance is a discreet sibling of the import button, live on a
  // merge_wait card with a candidate AND an onArchive handler. The trigger needs
  // onArchivePreview (the fresh two-boundary read at click); the confirm renders whenever
  // a boundary view is present (incl. the SSR/test seam).
  const showArchive = letter.phase === 'merge_wait' && !!candidate && !!onArchive;
  const canArchiveTrigger = showArchive && !!onArchivePreview;
  const [archiveView, setArchiveView] = useState<ArchiveBoundaryView | null>(initialArchiveView ?? null);
  const [archiveLoading, setArchiveLoading] = useState(false);
  // Only one affordance cluster shows at a time — the two-boundary confirm doubles as the
  // ergonomic guard against a mis-click between "Import" and "Archive".
  const anyConfirm = confirmOpen || !!archiveView || archiveLoading;

  // The seat line: `hand · build-runner[ · runner-build-1]` (runner_id only when present).
  const seatLine = [letter.seat, letter.capability, letter.runner_id ?? undefined]
    .filter(Boolean)
    .join(' · ');

  return (
    <div
      data-role="mission-card"
      data-mission-id={head.mission_id}
      data-phase={letter.phase}
      className={`rounded border border-hairline bg-warm-paper border-l-2 ${accent.rule} px-2.5 py-2 text-xs shadow-contact`}
    >
      {/* Header: block name (→ map) + phase chip + dismiss (failed only). */}
      <div className="flex items-start gap-1.5">
        <button
          type="button"
          data-role="mission-open-block"
          data-block-id={letter.block_id}
          onClick={() => onOpenBlock(letter.block_id)}
          title={`open ${letter.block_id} on the map`}
          className="text-left font-semibold text-ink hover:text-socket-blue leading-tight min-w-0 break-words"
        >
          {name}
        </button>
        <span
          data-role="mission-phase"
          className={`ml-auto shrink-0 inline-flex items-center gap-1 rounded border px-1.5 py-0.5 text-[10px] font-mono ${accent.chip}`}
        >
          <span aria-hidden>{PHASE_META[letter.phase].glyph}</span>
          {phaseLabel(letter.phase)}
        </span>
        {isFailed && onDismiss && (
          <button
            type="button"
            data-role="mission-dismiss"
            onClick={() => onDismiss(head.mission_id)}
            aria-label="dismiss this failure from the tray"
            title="un-pin — the failure stays in the box, this only clears the pin"
            className="shrink-0 text-ink-soft hover:text-ink leading-none px-0.5"
          >
            ✕
          </button>
        )}
      </div>

      {/* Seat + capability + runner_id (§3b). */}
      <div data-role="mission-seat" className="mt-1 font-mono text-[10px] text-ink-soft break-words">
        {seatLine}
      </div>

      {/* The verdict gist, when the letter carries one (§3b). */}
      {letter.verdict && (
        <div data-role="mission-verdict" className="mt-1 text-ink-soft">
          <span className="font-mono text-[10px] text-ink">{letter.verdict.decision}</span> —{' '}
          {letter.verdict.gist}
        </div>
      )}

      {/* The gate line: `command · exit N` (§3b). */}
      {gate && (
        <div data-role="mission-gate" className="mt-1 font-mono text-[10px] text-ink-soft break-words">
          {gate}
        </div>
      )}

      {/* The §1d landed-law on a merge_wait: a green gate is NOT a landing. */}
      {mergeLine && (
        <div data-role="mission-landed-law" className="mt-1 text-[11px] text-verdict-reverify">
          {mergeLine}
        </div>
      )}

      {/* §6-F2.5d — the human landing. With a candidate: the one-click import + a
          compact confirm that shows EXACTLY what will be imported (the gesture is
          explicit, never silent). The confirm→import is still the human's gesture. */}
      {canImport && candidate && !anyConfirm && (
        <button
          type="button"
          data-role="import-receipt"
          onClick={() => setConfirmOpen(true)}
          className="mt-1.5 inline-flex items-center gap-1 rounded border border-verdict-act/40 bg-verdict-act-tint/40 px-2 py-1 text-[11px] text-verdict-act hover:bg-verdict-act-tint/60"
        >
          Import this receipt
        </button>
      )}
      {canImport && candidate && confirmOpen && (
        <div
          data-role="import-confirm"
          className="mt-1.5 rounded border border-hairline bg-porcelain px-2 py-1.5 space-y-1"
        >
          <div className="text-[10px] uppercase tracking-wide text-ink-soft">Import this receipt?</div>
          <dl className="font-mono text-[10px] text-ink-soft space-y-0.5">
            <div className="flex gap-1">
              <dt className="text-ink shrink-0">block</dt>
              <dd className="break-all">{blockLabelFromId(candidate.block_id, letter.brain_ref)}</dd>
            </div>
            <div className="flex gap-1">
              <dt className="text-ink shrink-0">type</dt>
              <dd data-role="import-type" className="break-all">{candidate.type}</dd>
            </div>
            <div className="flex gap-1">
              <dt className="text-ink shrink-0">artifact</dt>
              <dd data-role="import-artifact" className="break-all">
                {truncateHash(candidate.evidence.artifact_hash)}
              </dd>
            </div>
            <div className="flex gap-1">
              <dt className="text-ink shrink-0">evidence</dt>
              <dd data-role="import-evidence" className="break-all">
                {candidate.evidence.evidence_refs.join(', ') || '—'}
              </dd>
            </div>
          </dl>
          <div className="flex items-center gap-2 pt-0.5">
            <button
              type="button"
              data-role="import-confirm-go"
              onClick={() => {
                setConfirmOpen(false);
                onImportReceipt?.(head);
              }}
              className="rounded border border-verdict-act/40 bg-verdict-act-tint/40 px-2 py-0.5 text-[11px] text-verdict-act hover:bg-verdict-act-tint/60"
            >
              Import
            </button>
            <button
              type="button"
              data-role="import-cancel"
              onClick={() => setConfirmOpen(false)}
              className="text-[11px] text-ink-soft hover:text-ink"
            >
              cancel
            </button>
          </div>
          <p className="text-[10px] text-ink-soft leading-snug">
            the scope stays the candidate’s boundary — a moved boundary re-runs the gate, never a rewrite.
          </p>
        </div>
      )}

      {/* F2.5e — the archive gesture: a discreet sibling of "Import this receipt" on the
          SAME merge_wait+candidate card. Clicking it fetches a FRESH snapshot (the
          landCandidate step-1 read) and opens a confirm showing the live two-boundary
          comparison — the honesty AND the ergonomic guard against a mis-click. */}
      {canArchiveTrigger && candidate && !anyConfirm && (
        <button
          type="button"
          data-role="archive-superseded"
          onClick={() => {
            if (!onArchivePreview) return;
            setArchiveLoading(true);
            void onArchivePreview(head)
              .then((view) => setArchiveView(view))
              .finally(() => setArchiveLoading(false));
          }}
          className="mt-1 block text-left text-[10px] text-ink-soft hover:text-ink underline decoration-dotted underline-offset-2"
        >
          Archive — superseded by newer boundary
        </button>
      )}
      {showArchive && archiveLoading && (
        <div data-role="archive-loading" className="mt-1 text-[10px] text-ink-soft">
          reading the current boundary…
        </div>
      )}
      {showArchive && candidate && archiveView && (
        <div
          data-role="archive-confirm"
          className="mt-1.5 rounded border border-hairline bg-porcelain px-2 py-1.5 space-y-1"
        >
          <div className="text-[10px] uppercase tracking-wide text-ink-soft">
            Archive this superseded receipt?
          </div>
          <div data-role="archive-boundaries" className="font-mono text-[10px] text-ink-soft">
            proved at boundary v{archiveView.provedAt} — the block is at{' '}
            {archiveView.currentAt == null ? 'v— (gone)' : `v${archiveView.currentAt}`}
          </div>
          {archiveView.stillImportable && (
            <div data-role="archive-still-importable" className="text-[10px] text-verdict-reverify">
              still importable — archive anyway?
            </div>
          )}
          <div className="flex items-center gap-2 pt-0.5">
            <button
              type="button"
              data-role="archive-confirm-go"
              onClick={() => {
                setArchiveView(null);
                onArchive?.(head);
              }}
              className="rounded border border-hairline bg-porcelain px-2 py-0.5 text-[11px] text-ink-soft hover:text-ink"
            >
              Archive
            </button>
            <button
              type="button"
              data-role="archive-cancel"
              onClick={() => setArchiveView(null)}
              className="text-[11px] text-ink-soft hover:text-ink"
            >
              cancel
            </button>
          </div>
          <p className="text-[10px] text-ink-soft leading-snug">
            the superseded receipt stays in history — archiving only clears the bell, it lands nothing.
          </p>
        </div>
      )}

      {/* On `landed`: the receipt anchor — the only thing that lands (§1d). */}
      {isLanded && receipt && (
        <div data-role="mission-receipt" className="mt-1 text-[11px] text-verdict-act font-mono">
          {receipt}
        </div>
      )}

      {/* Footer: elapsed + (landed) the provenance toggle (§3f). */}
      <div className="mt-1.5 flex items-center gap-2">
        {elapsed && (
          <span data-role="mission-elapsed" className="font-mono text-[10px] text-ink-soft">
            {elapsed}
          </span>
        )}
        {isLanded && onToggleProvenance && (
          <button
            type="button"
            data-role="mission-provenance-toggle"
            aria-expanded={provenanceOpen}
            onClick={() => onToggleProvenance(head.mission_id)}
            className="ml-auto text-[10px] font-mono text-ink-soft hover:text-ink"
          >
            {provenanceOpen ? 'hide provenance' : 'provenance ▸'}
          </button>
        )}
      </div>

      {/* §3f — the provenance chain a `landed` card answers with:
          packet_ref → runner_id → gate artifact hash → receipt store_version. */}
      {isLanded && provenanceOpen && (
        <dl
          data-role="mission-provenance"
          className="mt-1.5 space-y-0.5 border-t border-hairline pt-1.5 font-mono text-[10px] text-ink-soft"
        >
          <div className="flex gap-1">
            <dt className="text-ink shrink-0">packet</dt>
            <dd className="break-all">{letter.packet_ref ?? '—'}</dd>
          </div>
          <div className="flex gap-1">
            <dt className="text-ink shrink-0">runner</dt>
            <dd className="break-all">{letter.runner_id ?? '— (no runner — direct)'}</dd>
          </div>
          <div className="flex gap-1">
            <dt className="text-ink shrink-0">gate</dt>
            <dd className="break-all">{letter.gate?.artifact_hash ?? '—'}</dd>
          </div>
          <div className="flex gap-1">
            <dt className="text-ink shrink-0">receipt</dt>
            <dd className="break-all">{receipt ?? '—'}</dd>
          </div>
        </dl>
      )}
    </div>
  );
}
