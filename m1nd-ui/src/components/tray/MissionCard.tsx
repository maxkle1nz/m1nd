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
 * Copy law: no "done/proven/correct". Tokens are all sanctioned non-violet families.
 */
import type { MissionHead, Phase } from '../../lib/missions';
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
  /** Injectable clock for a deterministic elapsed under test. */
  now?: number;
}

/** Phase → accent (left rule + phase chip). All sanctioned non-violet tokens:
 *  in-progress = socket-blue; evidence-pending (gate/review/merge_wait) = amber
 *  reverify; landed = sage act; failed = clay failure. */
const PHASE_ACCENT: Record<Phase, { rule: string; chip: string }> = {
  judging: { rule: 'border-l-socket-blue/60', chip: 'text-socket-blue border-socket-blue/40 bg-socket-blue/10' },
  executing: { rule: 'border-l-socket-blue/60', chip: 'text-socket-blue border-socket-blue/40 bg-socket-blue/10' },
  gate: { rule: 'border-l-verdict-reverify/70', chip: 'text-verdict-reverify border-verdict-reverify/40 bg-verdict-reverify-tint/40' },
  review: { rule: 'border-l-verdict-reverify/70', chip: 'text-verdict-reverify border-verdict-reverify/40 bg-verdict-reverify-tint/40' },
  merge_wait: { rule: 'border-l-verdict-reverify/70', chip: 'text-verdict-reverify border-verdict-reverify/40 bg-verdict-reverify-tint/40' },
  landed: { rule: 'border-l-verdict-act/70', chip: 'text-verdict-act border-verdict-act/40 bg-verdict-act-tint/40' },
  failed: { rule: 'border-l-state-failure/70', chip: 'text-state-failure border-state-failure/40 bg-state-failure-tint/40' },
};

export default function MissionCard({
  head,
  onOpenBlock,
  onDismiss,
  provenanceOpen = false,
  onToggleProvenance,
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
