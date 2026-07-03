/*
 * LivingTree — the front door (HUMAN-LAYER-PRD §3, S1).
 * The familiar filetree EVOLVED: directories/files/symbols with trust dots and
 * post-its, keyboard-navigable, calm. Honest cold states (§3.5). Hover whisper =
 * the blast-radius floor line (§3.4). Read-only. No map here — the map is a
 * rung-2 drill-down only (killed as a front door, §1 decision 2).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTreeData, bandFor } from '../../hooks/useTreeData';
import { flattenVisible, type TreeRow } from '../../lib/tree';
import { blastCountPhrase } from '../../lib/softProof';
import { api } from '../../api/client';
import type { ImpactOutput } from '../../api/toolTypes';
import TreeRowView from './TreeRowView';
import TreeDrawer from './TreeDrawer';
import FreshnessBanner from '../soft/FreshnessBanner';

const HOVER_DEBOUNCE_MS = 250;

interface LivingTreeProps {
  onIngest?: () => void;
}

export default function LivingTree({ onIngest }: LivingTreeProps) {
  const { status, root, bands, breathingPaths, error, reload } = useTreeData();
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [filter, setFilter] = useState('');
  const [whisper, setWhisper] = useState<{ path: string; text: string } | null>(null);
  const hoverTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const impactCache = useRef<Map<string, string>>(new Map());
  const containerRef = useRef<HTMLDivElement>(null);

  // Auto-expand the first level once the tree loads, so it doesn't open empty.
  useEffect(() => {
    if (root && expanded.size === 0) {
      const top = new Set<string>();
      for (const c of root.children) if (c.kind === 'dir') top.add(c.path);
      setExpanded(top);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root]);

  const visible = useMemo(() => {
    if (!root) return [];
    let rows = flattenVisible(root, expanded);
    if (filter.trim()) {
      const q = filter.trim().toLowerCase();
      rows = rows.filter((r) => r.name.toLowerCase().includes(q) || r.path.toLowerCase().includes(q));
    }
    return rows;
  }, [root, expanded, filter]);

  const selectedRow = useMemo(
    () => visible.find((r) => r.path === selectedPath) ?? null,
    [visible, selectedPath],
  );

  const toggle = useCallback((row: TreeRow) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(row.path)) next.delete(row.path);
      else next.add(row.path);
      return next;
    });
  }, []);

  const onHover = useCallback(
    (row: TreeRow | null) => {
      if (hoverTimer.current) clearTimeout(hoverTimer.current);
      if (!row || !row.externalId) {
        setWhisper(null);
        return;
      }
      const cached = impactCache.current.get(row.externalId);
      if (cached) {
        setWhisper({ path: row.path, text: cached });
        return;
      }
      hoverTimer.current = setTimeout(async () => {
        try {
          const r = await api.tool<ImpactOutput>('impact', { node_id: row.externalId });
          const memN = row.postIts.length;
          const line =
            blastCountPhrase(r.total_blast_nodes, r.truncated) +
            (memN > 0 ? ` · ${memN} ${memN === 1 ? 'memory' : 'memories'} anchored` : '');
          impactCache.current.set(row.externalId as string, line);
          setWhisper({ path: row.path, text: line });
        } catch {
          setWhisper(null);
        }
      }, HOVER_DEBOUNCE_MS);
    },
    [],
  );

  // Keyboard navigation: up/down move, right expand, left collapse, enter select.
  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (visible.length === 0) return;
      const idx = visible.findIndex((r) => r.path === selectedPath);
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        const next = visible[Math.min(idx + 1, visible.length - 1)] ?? visible[0];
        setSelectedPath(next.path);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        const prev = visible[Math.max(idx - 1, 0)] ?? visible[0];
        setSelectedPath(prev.path);
      } else if (e.key === 'ArrowRight') {
        e.preventDefault();
        const row = visible[idx];
        if (row && row.children.length > 0 && !expanded.has(row.path)) toggle(row);
      } else if (e.key === 'ArrowLeft') {
        e.preventDefault();
        const row = visible[idx];
        if (row && row.children.length > 0 && expanded.has(row.path)) toggle(row);
      } else if (e.key === 'Escape') {
        setSelectedPath(null);
      }
    },
    [visible, selectedPath, expanded, toggle],
  );

  // ── Cold / degraded states (§3.5) ──────────────────────────────────────────
  if (status === 'loading') {
    return (
      <div className="flex-1 flex items-center justify-center text-ink-soft text-sm">
        Reading the map…
      </div>
    );
  }

  if (status === 'needs_ingest') {
    // The Empty Pedestal.
    return (
      <div className="flex-1 flex items-center justify-center bg-porcelain">
        <div className="max-w-sm text-center space-y-4">
          <div className="text-3xl text-ink-soft/40">🌱</div>
          <div className="text-ink text-lg">I haven't read this repo yet.</div>
          <button
            type="button"
            onClick={onIngest}
            className="px-4 py-2 text-sm bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow"
          >
            Read the repo
          </button>
        </div>
      </div>
    );
  }

  if (status === 'error') {
    return (
      <div className="flex-1 flex items-center justify-center p-6">
        <div className="max-w-md w-full">
          <FreshnessBanner
            tone="degraded"
            message={`Couldn't reach the graph — ${error ?? 'unknown error'}.`}
            action={{ label: 'Retry', onClick: reload }}
          />
        </div>
      </div>
    );
  }

  const noMemories = root != null && root.subtreePostItCount === 0;

  return (
    <div className="flex-1 flex overflow-hidden bg-porcelain">
      {/* Tree column */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Filter (type-to-filter) */}
        <div className="px-3 py-2 border-b border-ink/10">
          <input
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="filter files…"
            className="w-full bg-bone/60 border border-ink/10 rounded px-2 py-1 text-[13px] text-ink placeholder-ink-soft/60 outline-none focus:border-ink/25"
          />
        </div>

        {noMemories && (
          <div className="px-3 py-1.5 text-[11px] text-ink-soft border-b border-ink/10">
            No memories yet — agents leave notes here as they work.
          </div>
        )}

        {/* Rows */}
        <div
          ref={containerRef}
          role="tree"
          aria-label="Living Tree"
          tabIndex={0}
          onKeyDown={onKeyDown}
          className="flex-1 overflow-y-auto py-1 outline-none focus:ring-1 focus:ring-ink/15"
        >
          {visible.map((row) => (
            <TreeRowView
              key={row.id}
              row={row}
              band={bandFor(bands, row.externalId)}
              breathing={breathingPaths.has(row.path)}
              expanded={expanded.has(row.path)}
              selected={row.path === selectedPath}
              onToggle={toggle}
              onSelect={(r) => setSelectedPath(r.path)}
              onHover={onHover}
              onPostItClick={(r) => setSelectedPath(r.path)}
            />
          ))}
          {visible.length === 0 && (
            <div className="px-4 py-6 text-center text-ink-soft/70 text-sm">
              {filter ? 'No rows match the filter.' : 'The graph has no file nodes.'}
            </div>
          )}
        </div>

        {/* Hover whisper (blast-radius floor line) */}
        <div className="h-6 px-3 flex items-center border-t border-ink/10 text-[11px] text-ink-soft">
          {whisper ? (
            <span data-role="whisper">
              <span className="font-mono text-ink-soft/80">{whisper.path.split('/').pop()}</span> —{' '}
              {whisper.text}
            </span>
          ) : (
            <span className="text-ink-soft/50">hover a row to see what it touches</span>
          )}
        </div>
      </div>

      {/* Drawer (rung 1) */}
      <TreeDrawer
        row={selectedRow}
        band={bandFor(bands, selectedRow?.externalId)}
        onClose={() => setSelectedPath(null)}
      />
    </div>
  );
}
