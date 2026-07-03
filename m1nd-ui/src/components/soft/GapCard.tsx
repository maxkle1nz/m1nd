/*
 * GapCard — "what I don't know yet", rendered as NEXT-ACTION guidance
 * (HUMAN-LAYER-PRD §2 anxiety principle, §4.2, INV-03/INV-07).
 * Violet, dignified, still. On the violet-lint allow-list. Every gap carries
 * exactly one action affordance (or the generic "double-check" fallback) and
 * NEVER speaks raw epistemology at rung 0–1. Abstain-class: never animates.
 */
import { actionLanguageForGap } from '../../lib/softProof';

interface GapCardProps {
  /** Raw engine gap string (from honest_gaps[]). */
  gap: string;
  /** Optional handler for the single suggested action. */
  onAction?: (action: string) => void;
}

export default function GapCard({ gap, onAction }: GapCardProps) {
  const copy = actionLanguageForGap(gap);
  return (
    <div
      data-abstain="true"
      data-role="gap-card"
      className="rounded-md border border-iris/40 bg-iris-veil/60 px-3 py-2 flex items-center justify-between gap-3"
    >
      <span className="text-sm text-iris-deep leading-snug">{copy.text}</span>
      {copy.action && (
        <button
          type="button"
          data-role="gap-action"
          onClick={() => onAction?.(copy.action as string)}
          className="shrink-0 text-[11px] font-medium px-2 py-1 rounded bg-bone text-ink border border-ink/15 hover:shadow-contact transition-shadow"
        >
          {copy.action}
        </button>
      )}
    </div>
  );
}
