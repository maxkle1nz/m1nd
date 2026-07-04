/*
 * GroupHeader — a lens group header row (HUMAN-LAYER-PRD §4A.10).
 * Behaves like a directory: caret + lens icon + name + count (tabular,
 * right-aligned, INV-13). Rows inside stay §3.2 rows. Used by the kind + layer
 * lenses; the directory lens uses the native tree rows.
 */
import { Icon, type IconName } from '../../lib/icons/registry';
import { StatValue } from '../soft/StatCell';

interface GroupHeaderProps {
  label: string;
  count: number;
  icon: IconName;
  expanded: boolean;
  height: number;
  onToggle: () => void;
}

export default function GroupHeader({ label, count, icon, expanded, height, onToggle }: GroupHeaderProps) {
  return (
    <div
      role="treeitem"
      aria-expanded={expanded}
      data-role="group-header"
      onClick={onToggle}
      style={{ height }}
      className="flex items-center gap-1.5 px-3 cursor-pointer hover:bg-bone/40 border-b border-ink/5"
    >
      <span className="text-ink-soft/70 text-[10px] w-3 shrink-0">{expanded ? '▾' : '▸'}</span>
      <Icon name={icon} size={14} decorative className="text-ink-soft/80 shrink-0" />
      <span className="text-[12px] text-ink font-medium truncate">{label}</span>
      <StatValue className="ml-auto text-[11px] text-ink-soft">{count.toLocaleString()}</StatValue>
    </div>
  );
}
