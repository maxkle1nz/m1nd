/*
 * VerdictChip — the trust receipt on an answer surface (HUMAN-LAYER-PRD §4.3).
 * act = sage, reverify = ochre, abstain = IRIS violet. On the violet-lint
 * allow-list. The abstain chip NEVER animates (INV-02) and carries NO numeral.
 */

export type Verdict = 'act' | 'reverify' | 'abstain';

interface VerdictChipProps {
  verdict: Verdict;
  /** Rung-0 action-language label (no epistemic vocabulary). */
  label?: string;
}

const VERDICT_META: Record<
  Verdict,
  { text: string; bg: string; fg: string; abstain: boolean }
> = {
  act: { text: 'good to go', bg: 'bg-verdict-act-tint', fg: 'text-ink', abstain: false },
  reverify: {
    text: 'worth a second look',
    bg: 'bg-verdict-reverify-tint',
    fg: 'text-ink',
    abstain: false,
  },
  // IRIS — the only violet chip.
  abstain: { text: "I won't guess this one", bg: 'bg-iris-veil', fg: 'text-iris-deep', abstain: true },
};

export default function VerdictChip({ verdict, label }: VerdictChipProps) {
  const m = VERDICT_META[verdict];
  return (
    <span
      data-verdict={verdict}
      data-abstain={m.abstain ? 'true' : undefined}
      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium ${m.bg} ${m.fg}`}
    >
      {m.abstain && (
        <span
          className="w-1.5 h-1.5 rounded-full bg-iris shrink-0"
          aria-hidden
          data-abstain-dot="true"
        />
      )}
      {label ?? m.text}
    </span>
  );
}
