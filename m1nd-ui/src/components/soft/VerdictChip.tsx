/*
 * VerdictChip — the trust receipt on an answer surface (HUMAN-LAYER-PRD §4.3).
 * act = sage, reverify = ochre, abstain = IRIS violet. On the violet-lint
 * allow-list. Each verdict carries its glyph before the text (§4A.7 CONCEPT→ICON:
 * act=Check, reverify=RotateCcw, abstain=CircleDashed). The abstain glyph
 * SUPERSEDES the old plain iris dot — same size, iris ink, and it still NEVER
 * animates (INV-02); the chip carries NO numeral.
 */
import { Icon, type IconName } from '../../lib/icons/registry';
import type { TrustVerdict } from '../../api/toolTypes';

/** The chip's verdict IS the wire's four-state trust ladder (protocol/layers.rs). */
export type Verdict = TrustVerdict;

interface VerdictChipProps {
  verdict: Verdict;
  /** Rung-0 action-language label (no epistemic vocabulary). */
  label?: string;
}

const VERDICT_META: Record<
  Verdict,
  { text: string; bg: string; fg: string; icon: IconName; abstain: boolean }
> = {
  act: { text: 'good to go', bg: 'bg-verdict-act-tint', fg: 'text-ink', icon: 'verdictAct', abstain: false },
  reverify: {
    text: 'worth a second look',
    bg: 'bg-verdict-reverify-tint',
    fg: 'text-ink',
    icon: 'verdictReverify',
    abstain: false,
  },
  // IRIS — the only violet chip. The CircleDashed glyph wears iris ink (inherited
  // from the chip's text-iris-deep) and never animates.
  abstain: { text: "I won't guess this one", bg: 'bg-iris-veil', fg: 'text-iris-deep', icon: 'verdictAbstain', abstain: true },
  // The fourth rung of the ladder: nothing exists to check the claim against.
  // Unfired grey (the honest no-evidence tone), never animates, no numeral.
  unprovable: {
    text: 'nothing to check this against',
    bg: 'bg-state-unverified-tint',
    fg: 'text-ink',
    icon: 'verdictUnprovable',
    abstain: false,
  },
};

export default function VerdictChip({ verdict, label }: VerdictChipProps) {
  const m = VERDICT_META[verdict];
  return (
    <span
      data-verdict={verdict}
      data-abstain={m.abstain ? 'true' : undefined}
      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium ${m.bg} ${m.fg}`}
    >
      <Icon name={m.icon} size={14} decorative className="shrink-0" />
      {label ?? m.text}
    </span>
  );
}
