/*
 * BrainPalette — the Cmd+K Brains group (HUMAN-LAYER-PRD §4A.5, research R4).
 *
 * The verified command-palette switcher pattern with ZERO new data: fuzzy jump
 * across every brain the owner holds — basename + liveness dot + last-seen —
 * recents-first FOR FREE because the registry sort IS recency
 * (instance_registry.rs:310-314), never re-sorted. Jumping to a live sibling
 * opens its entry_base_url; to the bound brain focuses the tree; hosted brains
 * obey the same honesty as their Open action (disabled + tooltip). SOFT PROOF;
 * no violet (not an abstain surface).
 *
 * The filter/selection logic is pure (`filterBrains`) so it is testable without a
 * DOM.
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import type { InstanceRegistryEntry } from '../../types';
import {
  livenessBand,
  LIVENESS_STYLE,
  brainDisplayName,
  lastSeenPhrase,
  entryBaseUrl,
} from '../../lib/hallSemantics';

export interface BrainRow {
  entry: InstanceRegistryEntry;
  isSelf: boolean;
  /** Can this brain be opened in place? (bound self, or a live sibling with a port) */
  openable: boolean;
}

/**
 * Filter brains by a fuzzy basename query, preserving the registry's recency
 * order (never re-sorted — R4/INV-10). Empty query returns all, recents-first.
 */
export function filterBrains(rows: BrainRow[], query: string): BrainRow[] {
  const q = query.trim().toLowerCase();
  if (!q) return rows;
  return rows.filter((r) => brainDisplayName(r.entry).toLowerCase().includes(q));
}

interface BrainPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  instances: InstanceRegistryEntry[];
  selfId: string | null;
  /** Focus the bound brain's tree. */
  onOpenBound: () => void;
}

export default function BrainPalette({ isOpen, onClose, instances, selfId, onOpenBound }: BrainPaletteProps) {
  const [query, setQuery] = useState('');
  const [selectedIdx, setSelectedIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const rows: BrainRow[] = useMemo(
    () =>
      instances.map((entry) => {
        const isSelf = entry.instance_id === selfId;
        return { entry, isSelf, openable: isSelf || entryBaseUrl(entry) != null };
      }),
    [instances, selfId],
  );
  const filtered = useMemo(() => filterBrains(rows, query), [rows, query]);

  useEffect(() => {
    if (isOpen) {
      setQuery('');
      setSelectedIdx(0);
      setTimeout(() => inputRef.current?.focus(), 30);
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const jump = (row: BrainRow) => {
    if (!row.openable) return;
    if (row.isSelf) onOpenBound();
    else {
      const url = entryBaseUrl(row.entry);
      if (url) window.open(url, '_blank', 'noopener,noreferrer');
    }
    onClose();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      onClose();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIdx((i) => Math.min(i + 1, filtered.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const row = filtered[selectedIdx];
      if (row) jump(row);
    }
  };

  return (
    <>
      <div className="fixed inset-0 bg-ink/30 z-40" onClick={onClose} aria-hidden />
      <div className="fixed top-1/4 left-1/2 -translate-x-1/2 z-50 w-full max-w-lg mx-4" data-role="brain-palette">
        <div className="bg-porcelain border border-ink/15 rounded-lg shadow-card overflow-hidden">
          <div className="flex items-center gap-3 px-4 py-3 border-b border-ink/10">
            <span className="text-ink-soft text-sm">⌘K</span>
            <input
              ref={inputRef}
              type="text"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setSelectedIdx(0);
              }}
              onKeyDown={onKeyDown}
              placeholder="jump to a brain…"
              className="flex-1 bg-transparent text-ink text-sm outline-none placeholder-ink-soft/50"
              spellCheck={false}
              autoComplete="off"
            />
            <kbd className="text-[10px] text-ink-soft border border-ink/15 rounded px-1">esc</kbd>
          </div>

          <div className="max-h-80 overflow-y-auto py-1" role="listbox" aria-label="brains">
            <div className="px-4 py-1 text-[10px] uppercase tracking-widest text-ink-soft/60">Brains</div>
            {filtered.map((row, i) => {
              const band = livenessBand(row.entry);
              const dot = LIVENESS_STYLE[band];
              const name = brainDisplayName(row.entry);
              return (
                <button
                  key={row.entry.instance_id}
                  role="option"
                  aria-selected={i === selectedIdx}
                  data-role="brain-jump"
                  data-openable={row.openable}
                  disabled={!row.openable}
                  onClick={() => jump(row)}
                  onMouseEnter={() => setSelectedIdx(i)}
                  title={
                    row.openable
                      ? row.isSelf
                        ? 'Open this brain (the tree)'
                        : 'Open this brain in its own tab'
                      : 'Opening a hosted project brain in place needs REST brain routing (not built yet)'
                  }
                  className={`w-full text-left px-4 py-2 flex items-center gap-2 transition-colors disabled:opacity-45 disabled:cursor-not-allowed ${
                    i === selectedIdx ? 'bg-bone' : 'hover:bg-bone/60'
                  }`}
                >
                  <span className="w-2 h-2 rounded-full inline-block shrink-0" style={{ backgroundColor: dot.color }} />
                  <span className="text-sm text-ink font-medium truncate">{name}</span>
                  {row.isSelf && <span className="text-[10px] text-ink-soft font-mono">this brain</span>}
                  <span className="ml-auto text-[10px] text-ink-soft font-mono shrink-0">
                    {lastSeenPhrase(row.entry.last_heartbeat_ms)}
                  </span>
                </button>
              );
            })}
            {filtered.length === 0 && (
              <div className="px-4 py-4 text-center text-xs text-ink-soft/70">No brains match.</div>
            )}
          </div>
        </div>
      </div>
    </>
  );
}
