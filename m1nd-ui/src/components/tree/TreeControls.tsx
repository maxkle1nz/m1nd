/*
 * TreeControls — the Reading-the-Tree instruments bar (HUMAN-LAYER-PRD §4A.10).
 *
 * A quiet mode picker (directory | kind | layer), the search field with a
 * two-value text toggle (name | meaning — NO sparkle, §4A.7), the matte filter
 * chips (each a real field, AND-combined), and the density toggle. Every icon
 * comes from the registry; counts are tabular. Calm defaults, depth on demand.
 */
import { forwardRef } from 'react';
import { Icon, type IconName } from '../../lib/icons/registry';
import type { Lens, FilterKey } from '../../lib/treeLenses';

export type SearchMode = 'name' | 'meaning';
export type Density = 'compact' | 'comfortable';

const LENSES: Array<{ key: Lens; label: string; icon: IconName }> = [
  { key: 'directory', label: 'directory', icon: 'groupDir' },
  { key: 'kind', label: 'kind', icon: 'groupKind' },
  { key: 'layer', label: 'layer', icon: 'layer' },
];

const CHIPS: Array<{ key: FilterKey; label: string }> = [
  { key: 'kind', label: 'kind' },
  { key: 'language', label: 'language' },
  { key: 'trust', label: 'trust' },
  { key: 'hasMemory', label: 'has memory' },
  { key: 'changed', label: 'changed since read' },
  { key: 'churning', label: 'churning now' },
];

interface TreeControlsProps {
  lens: Lens;
  onLens: (l: Lens) => void;
  searchMode: SearchMode;
  onSearchMode: (m: SearchMode) => void;
  query: string;
  onQuery: (q: string) => void;
  onSubmitMeaning: () => void;
  activeFilters: Set<FilterKey>;
  onToggleFilter: (k: FilterKey) => void;
  density: Density;
  onDensity: (d: Density) => void;
}

const TreeControls = forwardRef<HTMLInputElement, TreeControlsProps>(function TreeControls(
  {
    lens,
    onLens,
    searchMode,
    onSearchMode,
    query,
    onQuery,
    onSubmitMeaning,
    activeFilters,
    onToggleFilter,
    density,
    onDensity,
  },
  searchRef,
) {
  return (
    <div className="border-b border-ink/10">
      {/* Row 1: lens picker + search field + mode toggle + density */}
      <div className="px-3 py-2 flex items-center gap-2">
        {/* Lens mode picker */}
        <div role="tablist" aria-label="group by" className="flex items-center gap-0.5 shrink-0" data-role="lens-picker">
          {LENSES.map((l) => (
            <button
              key={l.key}
              type="button"
              role="tab"
              aria-selected={lens === l.key}
              data-lens={l.key}
              data-active={lens === l.key ? 'true' : undefined}
              onClick={() => onLens(l.key)}
              title={`group by ${l.label}`}
              className={`inline-flex items-center gap-1 px-2 py-1 rounded text-[11px] transition-colors ${
                lens === l.key ? 'bg-bone text-ink border border-ink/15' : 'text-ink-soft hover:text-ink border border-transparent'
              }`}
            >
              <Icon name={l.icon} size={14} decorative />
              {l.label}
            </button>
          ))}
        </div>

        {/* Search field */}
        <div className="flex-1 flex items-center gap-1.5 bg-bone/60 border border-ink/10 rounded px-2 focus-within:border-ink/25">
          <Icon name="search" size={14} decorative className="text-ink-soft/70 shrink-0" />
          <input
            ref={searchRef}
            value={query}
            onChange={(e) => onQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && searchMode === 'meaning') {
                e.preventDefault();
                onSubmitMeaning();
              }
            }}
            placeholder={searchMode === 'name' ? 'filter by name…' : 'search by meaning, then Enter…'}
            aria-label={`search by ${searchMode}`}
            className="flex-1 bg-transparent py-1 text-[13px] text-ink placeholder-ink-soft/60 outline-none"
          />
        </div>

        {/* name | meaning toggle — a labeled TEXT toggle, no sparkle (§4A.7) */}
        <div role="tablist" aria-label="search mode" className="flex items-center rounded border border-ink/12 overflow-hidden shrink-0" data-role="search-mode">
          {(['name', 'meaning'] as SearchMode[]).map((m) => (
            <button
              key={m}
              type="button"
              role="tab"
              aria-selected={searchMode === m}
              data-mode={m}
              data-active={searchMode === m ? 'true' : undefined}
              onClick={() => onSearchMode(m)}
              className={`px-2 py-1 text-[11px] transition-colors ${
                searchMode === m ? 'bg-bone text-ink' : 'text-ink-soft hover:text-ink'
              }`}
            >
              {m}
            </button>
          ))}
        </div>

        {/* Density toggle (a preference, not a mode) */}
        <button
          type="button"
          data-role="density-toggle"
          onClick={() => onDensity(density === 'compact' ? 'comfortable' : 'compact')}
          title={`row density: ${density} (click for ${density === 'compact' ? 'comfortable' : 'compact'})`}
          aria-label={`row density ${density}`}
          className="shrink-0 px-2 py-1 text-[11px] text-ink-soft hover:text-ink border border-ink/12 rounded"
        >
          {density === 'compact' ? '↕ compact' : '↕ comfortable'}
        </button>
      </div>

      {/* Row 2: filter chips (only in name/browse mode — meaning has its own panel) */}
      {searchMode === 'name' && (
        <div className="px-3 pb-2 flex items-center gap-1.5 flex-wrap" data-role="filter-bar">
          <Icon name="filter" size={14} decorative className="text-ink-soft/60" />
          {CHIPS.map((chip) => {
            const on = activeFilters.has(chip.key);
            return (
              <button
                key={chip.key}
                type="button"
                data-filter={chip.key}
                data-active={on ? 'true' : undefined}
                aria-pressed={on}
                onClick={() => onToggleFilter(chip.key)}
                className={`px-2 py-0.5 rounded-full text-[10px] border transition-colors ${
                  on ? 'bg-bone text-ink border-ink/25' : 'text-ink-soft border-ink/12 hover:border-ink/20'
                }`}
              >
                {chip.label}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
});

export default TreeControls;
