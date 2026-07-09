/*
 * BlockCard — one SystemBlock as an ARTKIT card (HUMAN-VIEW-V2-SCREENS §0/§1.1).
 *
 * Anatomy: domain tag · block name · mono line (`N members · vN · ratified|
 * candidate`) · footer (`receipts M/N · <state>`) + proof ring (`◉ vN`). Fill is a
 * SOLID state tint (PRD §5 — never a color average; at 0 receipts there is no
 * proportion to draw). Border is SEPARATE from fill: wired = state-tinted, no
 * sockets = neutral hairline (holes are never hidden), candidate = dashed +
 * stale-lilac (never mistakable for a ratified block). Read-only — a click only
 * selects; nothing here mutates.
 */
import type { BlockRollup, BlockState, SystemBlock } from '../../lib/buildMap';
import { STATE_LABEL } from '../../lib/buildMap';

/** State → the (reused) SOFT PROOF token classes. Coabitação decision: the Build
 *  Map borrows the verdict/state families rather than minting new hues. */
const STATE_CLASSES: Record<BlockState, { border: string; wash: string; dot: string; text: string }> = {
  'evidence-backed': {
    border: 'border-verdict-act',
    wash: 'bg-verdict-act-tint/40',
    dot: 'bg-verdict-act',
    text: 'text-verdict-act',
  },
  'needs-evidence': {
    border: 'border-verdict-reverify',
    wash: 'bg-verdict-reverify-tint/40',
    dot: 'bg-verdict-reverify',
    text: 'text-verdict-reverify',
  },
  broken: {
    border: 'border-state-failure',
    wash: 'bg-state-failure-tint/40',
    dot: 'bg-state-failure',
    text: 'text-state-failure',
  },
  unknown: {
    border: 'border-state-unverified',
    wash: 'bg-state-unverified-tint/40',
    dot: 'bg-state-unverified',
    text: 'text-ink-soft',
  },
};

export interface BlockCardProps {
  block: SystemBlock;
  rollup: BlockRollup;
  domainTag: string;
  selected: boolean;
  onSelect: (blockId: string) => void;
  /** Absolute grid placement (F0-TECH §7); omitted in flow/test contexts. */
  style?: React.CSSProperties;
}

export default function BlockCard({ block, rollup, domainTag, selected, onSelect, style }: BlockCardProps) {
  const sc = STATE_CLASSES[rollup.state];
  const memberCount = block.membership.length;
  const borderStyle = rollup.candidate ? 'border-dashed' : 'border-solid';
  const borderColor = rollup.candidate
    ? 'border-stale-lilac'
    : rollup.wired
      ? sc.border
      : 'border-hairline';
  const ratifyWord = block.state === 'ratified' ? 'ratified' : 'candidate';

  return (
    <button
      type="button"
      data-role="block-card"
      data-block-card={block.block_id}
      data-state={rollup.state}
      data-wired={rollup.wired}
      data-candidate={rollup.candidate}
      onClick={() => onSelect(block.block_id)}
      style={style}
      className={[
        'absolute text-left flex flex-col rounded-lg bg-warm-paper border-2 p-3 transition-shadow',
        borderStyle,
        borderColor,
        sc.wash,
        selected ? 'shadow-card ring-1 ring-socket-blue' : 'shadow-contact hover:shadow-card',
      ].join(' ')}
    >
      {/* Domain tag — 12/600 caps (ARTKIT §0), with a state dot (colorblind
          redundancy: the state is a dot + a word, never hue alone). */}
      <div className="flex items-center gap-1.5">
        <span className={`w-1.5 h-1.5 rounded-full inline-block shrink-0 ${sc.dot}`} aria-hidden />
        <span className={`text-[11px] font-semibold tracking-wide uppercase ${sc.text}`}>{domainTag}</span>
      </div>

      {/* Block name — the human name (ratified, never auto-asserted). */}
      <div className="text-[13px] font-semibold text-ink leading-tight mt-0.5">{block.name}</div>

      {/* Mono metadata line. */}
      <div className="text-[11px] font-mono text-ink-soft mt-1">
        {memberCount} members · v{block.boundary_version} · {ratifyWord}
      </div>

      {/* Boundary-moved badge (F3b §C): the block was reconciled and carries evidence
          earned against an older boundary — discreet amber warn, never an alarm. */}
      {rollup.boundaryStale && (
        <div className="mt-1">
          <span
            data-role="boundary-badge"
            className="inline-flex items-center gap-1 text-[10px] font-mono text-verdict-reverify border border-verdict-reverify/40 bg-verdict-reverify-tint/40 rounded px-1.5 py-0.5"
            title="the boundary moved — some receipts predate the current membership"
          >
            ⚠ boundary v{block.boundary_version}
          </span>
        </div>
      )}

      <div className="flex-1" />

      {/* Footer: receipts M/N · state, and the proof ring (vN). */}
      <div className="flex items-end justify-between mt-2">
        <div className="text-[11px] font-mono">
          <span className="text-ink-soft">receipts </span>
          <span className="text-ink tabular-nums">
            {rollup.receiptsEarned}/{rollup.receiptsRequired}
          </span>
          <span className="text-ink-soft"> · </span>
          <span className={sc.text} data-role="block-state-label">
            {STATE_LABEL[rollup.state]}
          </span>
        </div>
        <div className="flex items-center gap-1" data-role="proof-ring" title="proof ring — freshness · contract version">
          <span className={`w-2.5 h-2.5 rounded-full border-2 ${sc.border} inline-block`} aria-hidden />
          <span className="text-[10px] font-mono text-ink-soft">v{block.contract_version}</span>
        </div>
      </div>
    </button>
  );
}
