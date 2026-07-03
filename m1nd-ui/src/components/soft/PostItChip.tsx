/*
 * PostItChip — a memory stuck to the code it cites (HUMAN-LAYER-PRD §3.3, §6.4).
 * Bone paper tag, tie-hole dot, matte contact shadow only. Four states:
 *   fresh (flat) / aging (curled corner) / stale (flipped face-down) / unknown (violet-outlined).
 * On the violet-lint allow-list: the unknown state is IRIS-outlined.
 * Read-only — no inline editing (humans read, agents write). INV-04: provenance
 * absent renders "unknown", never faked.
 */
import { authorLabel, humanizeAge, type PostItState } from '../../lib/softProof';

export interface PostItChipProps {
  claim: string;
  sourceAgent: string | null;
  ageMs: number | null;
  state: PostItState;
  onClick?: () => void;
  /** Compact chip (tree row) vs full card (drawer). */
  variant?: 'chip' | 'card';
}

const STATE_LABEL: Record<PostItState, string> = {
  fresh: 'memory',
  aging: 'memory (aging)',
  stale: 'the code this cites changed',
  unknown: 'memory — origin unknown',
};

export default function PostItChip({
  claim,
  sourceAgent,
  ageMs,
  state,
  onClick,
  variant = 'chip',
}: PostItChipProps) {
  const isUnknown = state === 'unknown';
  const isStale = state === 'stale';
  const isAging = state === 'aging';

  const base =
    'relative text-left bg-bone text-ink border rounded-sm shadow-contact select-none ' +
    (isUnknown ? 'border-iris/60 ' : 'border-ink/15 ') +
    (onClick ? 'cursor-pointer hover:shadow-card transition-shadow ' : '');

  const rotation = variant === 'chip' ? 'rotate-[-0.8deg]' : '';

  if (variant === 'chip') {
    return (
      <button
        type="button"
        onClick={onClick}
        data-postit-state={state}
        data-abstain={isUnknown ? 'true' : undefined}
        title={`${claim} — ${authorLabel(sourceAgent)} · ${humanizeAge(ageMs)}`}
        className={`${base} ${rotation} inline-flex items-center gap-1 px-1.5 py-0.5 max-w-[13rem]`}
        aria-label={`post-it: ${STATE_LABEL[state]}`}
      >
        {/* tie-hole dot (specimen-tag motif) */}
        <span className="w-1 h-1 rounded-full bg-ink/30 shrink-0" aria-hidden />
        <span className={`text-[11px] truncate ${isStale ? 'italic text-ink-soft' : ''}`}>
          {isStale ? STATE_LABEL.stale : claim}
        </span>
      </button>
    );
  }

  // Full card (drawer / Project Brain).
  return (
    <button
      type="button"
      onClick={onClick}
      data-postit-state={state}
      data-abstain={isUnknown ? 'true' : undefined}
      className={`${base} block w-full p-3 space-y-1.5`}
      aria-label={`post-it: ${STATE_LABEL[state]}`}
    >
      <div className="flex items-start gap-1.5">
        <span className="w-1.5 h-1.5 rounded-full bg-ink/30 shrink-0 mt-1" aria-hidden />
        <span className={`text-sm leading-snug ${isStale ? 'italic text-ink-soft' : 'text-ink'}`}>
          {claim}
        </span>
      </div>
      <div className="flex items-center gap-2 font-mono text-[11px] text-ink-soft pl-3">
        <span data-role="author">{authorLabel(sourceAgent)}</span>
        <span aria-hidden>·</span>
        <span data-role="age">{humanizeAge(ageMs)}</span>
        {isUnknown && (
          <span className="text-iris" data-role="unknown-hedge">
            origin unknown
          </span>
        )}
        {isAging && <span data-role="aging">aging</span>}
        {isStale && <span data-role="stale">code changed</span>}
      </div>
    </button>
  );
}
