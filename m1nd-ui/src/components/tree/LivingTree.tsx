/*
 * LivingTree — the front door (HUMAN-LAYER-PRD §3, §4A.10).
 * The familiar filetree EVOLVED: directories/files/symbols with trust dots and
 * post-its, keyboard-navigable, calm. Honest cold states (§3.5). Hover whisper =
 * the blast-radius floor line (§3.4). Read-only.
 *
 * §4A.10 adds the READING instruments: three LENSES (directory | kind | layer),
 * six matte FILTER chips (each a real field, "N hidden by filters" residue), and
 * SEARCH in two modes — `name` (instant substring, shipped) and `meaning`
 * (`seek`, with the sufficiency line + verdict rendered honestly). A breadcrumb
 * of the focused node; a density toggle (a preference). No map here — the map is
 * a rung-2 drill-down only (§1 decision 2).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTreeData, bandFor } from '../../hooks/useTreeData';
import { useLiveRefresh } from '../../hooks/useLiveRefresh';
import { useToastStore } from '../../stores/toastStore';
import { flattenVisible, type TreeRow } from '../../lib/tree';
import { blastCountPhrase } from '../../lib/softProof';
import {
  groupByKind,
  groupByLayer,
  collectLeafRows,
  rowPasses,
  nameMatches,
  filterResidue,
  type Lens,
  type FilterKey,
  type GroupRow,
  type FilterContext,
} from '../../lib/treeLenses';
import { api } from '../../api/client';
import type { ImpactOutput, LayersOutput, SeekOutput, AmIStaleOutput } from '../../api/toolTypes';
import TreeRowView from './TreeRowView';
import TreeDrawer from './TreeDrawer';
import FreshnessBanner from '../soft/FreshnessBanner';
import TreeControls, { type SearchMode, type Density } from './TreeControls';
import GroupHeader from './GroupHeader';
import SeekPanel from './SeekPanel';
import { Icon, type IconName } from '../../lib/icons/registry';

const HOVER_DEBOUNCE_MS = 250;
const DENSITY_KEY = 'm1nd.tree.density';

interface LivingTreeProps {
  onIngest?: () => void;
}

/** The KIND-group icon per group id (§4A.7 KIND glyphs). */
function kindGroupIcon(id: string): IconName {
  if (id.startsWith('layer:')) return 'layer';
  if (id === 'kind:file') return 'kindFile';
  if (id === 'kind:function') return 'kindFunction';
  if (id === 'kind:struct') return 'kindStruct';
  if (id === 'kind:doc') return 'kindDoc';
  return 'graph';
}

export default function LivingTree({ onIngest }: LivingTreeProps) {
  const { status, root, bands, breathingPaths, error, reload } = useTreeData();
  const addToast = useToastStore((s) => s.addToast);

  const onLiveRefresh = useCallback(() => {
    reload();
    addToast('map updated', 'the graph changed under you', 'info');
  }, [reload, addToast]);
  useLiveRefresh({
    onRefresh: onLiveRefresh,
    enabled: status === 'ready' || status === 'error',
  });

  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [whisper, setWhisper] = useState<{ path: string; text: string } | null>(null);
  const hoverTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const impactCache = useRef<Map<string, string>>(new Map());
  const containerRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  // §4A.10 instrument state.
  const [lens, setLens] = useState<Lens>('directory');
  const [searchMode, setSearchMode] = useState<SearchMode>('name');
  const [query, setQuery] = useState('');
  const [activeFilters, setActiveFilters] = useState<Set<FilterKey>>(new Set());
  const [density, setDensity] = useState<Density>('comfortable');
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());

  // Lazily-fetched, cached-per-generation lens/filter data.
  const [layers, setLayers] = useState<LayersOutput | null>(null);
  const [stalePaths, setStalePaths] = useState<Set<string>>(new Set());

  // Meaning-search state.
  const [seekResult, setSeekResult] = useState<SeekOutput | null>(null);
  const [seekLoading, setSeekLoading] = useState(false);
  const [seekError, setSeekError] = useState<string | null>(null);

  // Density persists as a preference (localStorage), never a mode.
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const saved = window.localStorage.getItem(DENSITY_KEY);
    if (saved === 'compact' || saved === 'comfortable') setDensity(saved);
  }, []);
  const changeDensity = useCallback((d: Density) => {
    setDensity(d);
    if (typeof window !== 'undefined') window.localStorage.setItem(DENSITY_KEY, d);
  }, []);

  // Auto-expand the first level once the tree loads (directory lens).
  useEffect(() => {
    if (root && expanded.size === 0) {
      const top = new Set<string>();
      for (const c of root.children) if (c.kind === 'dir') top.add(c.path);
      setExpanded(top);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root]);

  // The lens/filter data is cached per graph generation: refetch on reload only.
  // Layers: one call when the layer lens is first shown, then cached.
  useEffect(() => {
    if (lens !== 'layer' || layers != null || !root) return;
    let mounted = true;
    api
      .tool<LayersOutput>('layers', {})
      .then((r) => mounted && setLayers(r))
      .catch(() => mounted && setLayers({ layers: [] }));
    return () => {
      mounted = false;
    };
  }, [lens, layers, root]);

  // am_i_stale: on demand only when the "changed since read" chip is turned on
  // (the §4A.3.1-G1 cost rule — never a background poll of every row).
  useEffect(() => {
    if (!activeFilters.has('changed') || !root) return;
    if (stalePaths.size > 0) return; // cached for this generation
    const paths = collectLeafRows(root)
      .filter((r) => r.kind === 'file')
      .map((r) => r.path);
    let mounted = true;
    api
      .tool<AmIStaleOutput>('am_i_stale', { paths })
      .then((r) => {
        if (!mounted) return;
        const s = new Set<string>();
        for (const entry of r.stale ?? []) s.add(typeof entry === 'string' ? entry : entry.path);
        setStalePaths(s);
      })
      .catch(() => {});
    return () => {
      mounted = false;
    };
  }, [activeFilters, root, stalePaths.size]);

  // A reload (new generation) invalidates the cached lens/filter data.
  useEffect(() => {
    setLayers(null);
    setStalePaths(new Set());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root]);

  // The paths the viewed brain knows — the INV-16 scope guard for meaning search.
  const knownPaths = useMemo(() => {
    const s = new Set<string>();
    if (root) for (const r of collectLeafRows(root)) if (r.kind === 'file') s.add(r.path);
    return s;
  }, [root]);

  const filterCtx: FilterContext = useMemo(() => {
    const kinds = new Set<number>();
    const languages = new Set<string>();
    // (kind/language/trust selections default to "all of the present values" when
    // the chip is toggled — the chip is a claim; the user can refine later. For
    // this slice a toggled chip with no explicit selection filters to the rows
    // that HAVE the field, matching the honest "showing the subset" posture.)
    return {
      bands,
      breathingPaths,
      stalePaths,
      kinds,
      languages,
      trustBands: new Set(),
      active: activeFilters,
    };
  }, [bands, breathingPaths, stalePaths, activeFilters]);

  const passes = useCallback(
    (row: TreeRow) => rowPasses(row, filterCtx, (id) => bandFor(bands, id)),
    [filterCtx, bands],
  );

  // The visible rows for the DIRECTORY lens (native tree + name filter + chips).
  const directoryRows = useMemo(() => {
    if (!root) return [];
    let rows = flattenVisible(root, expanded);
    if (searchMode === 'name' && query.trim()) rows = rows.filter((r) => nameMatches(r, query));
    // Filters apply to file/symbol rows; dirs stay as scaffolding.
    rows = rows.filter((r) => r.kind === 'dir' || passes(r));
    return rows;
  }, [root, expanded, query, searchMode, passes]);

  // The groups for the KIND / LAYER lenses.
  const groups: GroupRow[] = useMemo(() => {
    if (!root || lens === 'directory') return [];
    const raw = lens === 'kind' ? groupByKind(root) : groupByLayer(root, layers ?? { layers: [] });
    // Apply name filter + chips inside each group.
    return raw
      .map((g) => ({
        ...g,
        rows: g.rows.filter(
          (r) => (searchMode !== 'name' || nameMatches(r, query)) && passes(r),
        ),
      }))
      .filter((g) => g.rows.length > 0 || g.id.startsWith('layer:')); // keep empty layer groups (honest counts)
  }, [root, lens, layers, query, searchMode, passes]);

  // Filter residue (§4A.10): shown vs hidden over the full leaf set.
  const residue = useMemo(() => {
    if (!root) return { shown: 0, hidden: 0 };
    const total = collectLeafRows(root).length;
    const shown =
      lens === 'directory'
        ? directoryRows.filter((r) => r.kind !== 'dir').length
        : groups.reduce((n, g) => n + g.rows.length, 0);
    return filterResidue(shown, total);
  }, [root, lens, directoryRows, groups]);

  const filtersOrSearchActive = activeFilters.size > 0 || (searchMode === 'name' && query.trim().length > 0);

  const selectedRow = useMemo(() => {
    if (!root || !selectedPath) return null;
    const all = lens === 'directory' ? directoryRows : groups.flatMap((g) => g.rows);
    return all.find((r) => r.path === selectedPath) ?? null;
  }, [root, selectedPath, lens, directoryRows, groups]);

  // Breadcrumb of the focused node (§4A.10): clickable path segments.
  const breadcrumb = useMemo(() => {
    if (!selectedPath) return [];
    const bare = selectedPath.split('::')[0];
    const segs = bare.split('/').filter(Boolean);
    const acc: Array<{ label: string; path: string }> = [];
    let cur = '';
    for (const s of segs) {
      cur = cur ? `${cur}/${s}` : s;
      acc.push({ label: s, path: cur });
    }
    return acc;
  }, [selectedPath]);

  const toggle = useCallback((row: TreeRow) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(row.path)) next.delete(row.path);
      else next.add(row.path);
      return next;
    });
  }, []);

  const toggleGroup = useCallback((id: string) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
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

  // Run meaning search (Enter in meaning mode). ESC returns to the tree.
  const runMeaning = useCallback(async () => {
    const q = query.trim();
    if (!q) return;
    setSeekLoading(true);
    setSeekError(null);
    setSeekResult(null);
    try {
      const r = await api.tool<SeekOutput>('seek', { query: q, top_k: 20 });
      setSeekResult(r);
    } catch (e) {
      setSeekError(e instanceof Error ? e.message : 'meaning search failed');
    } finally {
      setSeekLoading(false);
    }
  }, [query]);

  // A meaning hit → jump the tree: directory lens, expand the path, select, drawer.
  const openHit = useCallback(
    (hit: { file_path?: string }) => {
      if (!hit.file_path) return;
      const path = hit.file_path;
      setLens('directory');
      setSearchMode('name');
      setQuery('');
      // Expand every ancestor directory of the path.
      setExpanded((prev) => {
        const next = new Set(prev);
        const segs = path.split('/').filter(Boolean);
        let cur = '';
        for (let i = 0; i < segs.length - 1; i++) {
          cur = cur ? `${cur}/${segs[i]}` : segs[i];
          next.add(cur);
        }
        return next;
      });
      setSelectedPath(path);
      setSeekResult(null);
    },
    [],
  );

  // Jump to a breadcrumb segment (expand + select the dir).
  const jumpBreadcrumb = useCallback((path: string) => {
    setExpanded((prev) => new Set(prev).add(path));
    setSelectedPath(path);
  }, []);

  // Keyboard: `/` focuses search; arrows navigate (directory lens); Enter selects.
  const flatNav = useMemo(
    () => (lens === 'directory' ? directoryRows : groups.flatMap((g) => g.rows)),
    [lens, directoryRows, groups],
  );
  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === '/' && document.activeElement !== searchRef.current) {
        e.preventDefault();
        searchRef.current?.focus();
        return;
      }
      if (flatNav.length === 0) return;
      const idx = flatNav.findIndex((r) => r.path === selectedPath);
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelectedPath((flatNav[Math.min(idx + 1, flatNav.length - 1)] ?? flatNav[0]).path);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelectedPath((flatNav[Math.max(idx - 1, 0)] ?? flatNav[0]).path);
      } else if (e.key === 'ArrowRight') {
        e.preventDefault();
        const row = flatNav[idx];
        if (row && row.children.length > 0 && !expanded.has(row.path)) toggle(row);
      } else if (e.key === 'ArrowLeft') {
        e.preventDefault();
        const row = flatNav[idx];
        if (row && row.children.length > 0 && expanded.has(row.path)) toggle(row);
      } else if (e.key === 'Escape') {
        setSelectedPath(null);
      }
    },
    [flatNav, selectedPath, expanded, toggle],
  );

  const toggleFilter = useCallback((k: FilterKey) => {
    setActiveFilters((prev) => {
      const next = new Set(prev);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });
  }, []);

  const clearFilters = useCallback(() => {
    setActiveFilters(new Set());
    setQuery('');
  }, []);

  // ── Cold / degraded states (§3.5) ──────────────────────────────────────────
  if (status === 'loading') {
    return (
      <div className="flex-1 flex items-center justify-center text-ink-soft text-sm">Reading the map…</div>
    );
  }

  if (status === 'needs_ingest') {
    return (
      <div className="flex-1 flex items-center justify-center bg-porcelain">
        <div className="max-w-sm text-center space-y-4">
          <div className="text-3xl text-ink-soft/40">🌱</div>
          <div className="text-ink text-lg">I haven't read this repo yet.</div>
          <button
            type="button"
            onClick={onIngest}
            className="inline-flex items-center gap-1.5 px-4 py-2 text-sm bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow"
          >
            <Icon name="ingest" size={16} decorative /> Read the repo
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

  const compact = density === 'compact';
  const inMeaning = searchMode === 'meaning' && (seekLoading || seekResult != null || seekError != null);

  return (
    <div className="flex-1 flex overflow-hidden bg-porcelain">
      <div className="flex-1 flex flex-col min-w-0">
        <TreeControls
          ref={searchRef}
          lens={lens}
          onLens={setLens}
          searchMode={searchMode}
          onSearchMode={(m) => {
            setSearchMode(m);
            if (m === 'name') {
              setSeekResult(null);
              setSeekError(null);
            }
          }}
          query={query}
          onQuery={setQuery}
          onSubmitMeaning={runMeaning}
          activeFilters={activeFilters}
          onToggleFilter={toggleFilter}
          density={density}
          onDensity={changeDensity}
        />

        {/* Breadcrumb of the focused node (§4A.10) */}
        {breadcrumb.length > 0 && !inMeaning && (
          <div className="px-3 py-1 flex items-center gap-1 border-b border-ink/5 text-[11px] font-mono text-ink-soft overflow-x-auto" data-role="breadcrumb">
            {breadcrumb.map((seg, i) => (
              <span key={seg.path} className="flex items-center gap-1 shrink-0">
                {i > 0 && <span className="text-ink-soft/40">/</span>}
                <button type="button" onClick={() => jumpBreadcrumb(seg.path)} className="hover:text-ink">
                  {seg.label}
                </button>
              </span>
            ))}
          </div>
        )}

        {inMeaning ? (
          <SeekPanel
            query={query}
            result={seekResult}
            loading={seekLoading}
            error={seekError}
            knownPaths={knownPaths}
            onOpenHit={openHit}
            onClose={() => {
              setSearchMode('name');
              setSeekResult(null);
              setSeekError(null);
            }}
          />
        ) : (
          <>
            {/* Rows */}
            <div
              ref={containerRef}
              role="tree"
              aria-label="Living Tree"
              tabIndex={0}
              onKeyDown={onKeyDown}
              className="flex-1 overflow-y-auto py-1 outline-none focus:ring-1 focus:ring-ink/15"
            >
              {lens === 'directory' &&
                directoryRows.map((row) => (
                  <TreeRowView
                    key={row.id}
                    row={row}
                    band={bandFor(bands, row.externalId)}
                    breathing={breathingPaths.has(row.path)}
                    expanded={expanded.has(row.path)}
                    selected={row.path === selectedPath}
                    compact={compact}
                    onToggle={toggle}
                    onSelect={(r) => setSelectedPath(r.path)}
                    onHover={onHover}
                    onPostItClick={(r) => setSelectedPath(r.path)}
                  />
                ))}

              {lens !== 'directory' &&
                groups.map((g) => {
                  const open = !collapsedGroups.has(g.id);
                  return (
                    <div key={g.id} data-group={g.id}>
                      <GroupHeader
                        label={g.label}
                        count={g.count}
                        icon={kindGroupIcon(g.id)}
                        expanded={open}
                        height={compact ? 24 : 28}
                        onToggle={() => toggleGroup(g.id)}
                      />
                      {open &&
                        g.rows.map((row) => (
                          <TreeRowView
                            key={row.id}
                            row={row}
                            band={bandFor(bands, row.externalId)}
                            breathing={breathingPaths.has(row.path)}
                            expanded={false}
                            selected={row.path === selectedPath}
                            compact={compact}
                            onToggle={toggle}
                            onSelect={(r) => setSelectedPath(r.path)}
                            onHover={onHover}
                            onPostItClick={(r) => setSelectedPath(r.path)}
                          />
                        ))}
                    </div>
                  );
                })}

              {((lens === 'directory' && directoryRows.length === 0) ||
                (lens !== 'directory' && groups.length === 0)) && (
                <div className="px-4 py-6 text-center text-ink-soft/70 text-sm">
                  {filtersOrSearchActive ? 'No rows match.' : 'The graph has no file nodes.'}
                </div>
              )}
            </div>

            {/* Filter residue (§4A.10): never a silently smaller world */}
            {residue.hidden > 0 && (
              <div className="px-3 h-6 flex items-center gap-2 border-t border-ink/10 text-[11px] text-ink-soft" data-role="filter-residue">
                <span>
                  <span className="font-mono tabular-nums">{residue.shown.toLocaleString()}</span> rows ·{' '}
                  <span className="font-mono tabular-nums">{residue.hidden.toLocaleString()}</span> hidden by filters
                </span>
                <button type="button" onClick={clearFilters} className="text-ink-soft/80 hover:text-ink underline">
                  clear
                </button>
              </div>
            )}

            {/* Hover whisper (blast-radius floor line) */}
            <div className="h-6 px-3 flex items-center border-t border-ink/10 text-[11px] text-ink-soft">
              {whisper ? (
                <span data-role="whisper">
                  <span className="font-mono text-ink-soft/80">{whisper.path.split('/').pop()}</span> — {whisper.text}
                </span>
              ) : (
                <span className="text-ink-soft/50">hover a row to see what it touches</span>
              )}
            </div>
          </>
        )}
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
