/*
 * TreeRowView — one Living Tree row: the 2-second read (PRD §3.2).
 *   caret · name · trust dot · post-it chip ×N (+overflow) · tremor breath.
 * Density cap: at most 3 chips + "+N" (§3.3). Attention emphasis: unvisited rows
 * with post-its sit at full ink (dimming is of attention, not information §3.2).
 */
import type { TreeRow } from '../../lib/tree';
import type { TrustBand } from '../../lib/softProof';
import TrustDot from '../soft/TrustDot';
import PostItChip from '../soft/PostItChip';

const MAX_CHIPS = 3;

interface TreeRowViewProps {
  row: TreeRow;
  band: TrustBand;
  breathing: boolean;
  expanded: boolean;
  selected: boolean;
  onToggle: (row: TreeRow) => void;
  onSelect: (row: TreeRow) => void;
  onHover: (row: TreeRow | null) => void;
  onPostItClick: (row: TreeRow) => void;
}

function KindGlyph({ kind, nodeType }: { kind: TreeRow['kind']; nodeType?: number }) {
  if (kind === 'dir') return <span className="text-ink-soft" aria-hidden>▸</span>;
  if (kind === 'symbol') {
    // fn (2) / class (3) / struct (4) / enum (5) / type (6) / module (7)
    const g = nodeType === 2 ? 'ƒ' : nodeType === 7 ? '§' : '◇';
    return <span className="text-ink-soft/70 font-mono text-[11px]" aria-hidden>{g}</span>;
  }
  return <span className="text-ink-soft/60" aria-hidden>·</span>;
}

export default function TreeRowView({
  row,
  band,
  breathing,
  expanded,
  selected,
  onToggle,
  onSelect,
  onHover,
  onPostItClick,
}: TreeRowViewProps) {
  const hasChildren = row.children.length > 0;
  const isDir = row.kind === 'dir';
  const directChips = row.postIts.slice(0, MAX_CHIPS);
  const overflow = row.postIts.length - directChips.length;
  // Directory rows show an aggregate count instead of individual chips (§3.3).
  const dirCount = isDir ? row.subtreePostItCount : 0;

  // Attention emphasis (§3.2): rows carrying memory read at full ink.
  const hasMemory = row.subtreePostItCount > 0;
  const nameInk = hasMemory ? 'text-ink' : 'text-ink-soft';

  return (
    <div
      role="treeitem"
      aria-expanded={hasChildren ? expanded : undefined}
      aria-selected={selected}
      data-path={row.path}
      onMouseEnter={() => onHover(row)}
      onMouseLeave={() => onHover(null)}
      onClick={() => onSelect(row)}
      className={
        'group flex items-center gap-1.5 h-7 pr-2 rounded cursor-pointer select-none ' +
        (selected ? 'bg-bone' : 'hover:bg-bone/60')
      }
      style={{ paddingLeft: `${row.depth * 14 + 6}px` }}
    >
      {/* caret */}
      <button
        type="button"
        tabIndex={-1}
        aria-hidden={!hasChildren}
        onClick={(e) => {
          e.stopPropagation();
          if (hasChildren) onToggle(row);
        }}
        className={`w-3.5 shrink-0 text-ink-soft text-[10px] leading-none ${
          hasChildren ? 'visible' : 'invisible'
        }`}
      >
        {expanded ? '▾' : '▸'}
      </button>

      <KindGlyph kind={row.kind} nodeType={row.nodeType} />

      <span className={`text-[13px] truncate ${nameInk}`}>{row.name}</span>

      {/* trust dot — files/symbols only (dirs aggregate their children) */}
      {!isDir && (
        <TrustDot band={band} breathing={breathing} />
      )}

      {/* post-it chips (files) or aggregate count (dirs) */}
      <div className="flex items-center gap-1 ml-1 min-w-0">
        {!isDir &&
          directChips.map((p) => (
            <PostItChip
              key={p.nodeId}
              claim={p.claim}
              sourceAgent={p.sourceAgent}
              ageMs={p.ageMs}
              state={p.state}
              onClick={() => onPostItClick(row)}
            />
          ))}
        {!isDir && overflow > 0 && (
          <span className="text-[10px] text-ink-soft font-mono px-1 rounded bg-bone" title={`${overflow} more`}>
            +{overflow}
          </span>
        )}
        {isDir && dirCount > 0 && (
          <span
            className="text-[10px] text-ink-soft font-mono px-1 rounded bg-bone/70"
            title={`${dirCount} memories in this folder`}
          >
            ◷ {dirCount}
          </span>
        )}
      </div>
    </div>
  );
}
