/*
 * SeekPanel — the `meaning` search results (HUMAN-LAYER-PRD §4A.10).
 *
 * Replaces the tree body while meaning-search is active (ESC returns — the ladder
 * grammar). The PRECIOUS part is the header: the engine's `sufficiency` (state +
 * why VERBATIM) rendered calm, and the `trust_envelope` verdict as the existing
 * VerdictChip — both fields the UI used to DISCARD, now rendered honestly.
 *
 * Each hit shows label · file:line · intent_summary · the engine's score VERBATIM
 * (no invented stars, no theatrical fuzzy meter). Truncation is honest
 * ("showing N of M that cleared relevance"); a trigram fallback is worn
 * ("matched by text, not meaning"); a foreign-brain hit is dropped (INV-16).
 * No sparkle — the honesty markers ARE the sufficiency line and the verdict chip.
 */
import type { SeekOutput, SeekResultEntry } from '../../api/toolTypes';
import { humanizeAge, authorLabel } from '../../lib/softProof';
import { textNotMeaningCaption, resultBelongsToBrain } from '../../lib/treeLenses';
import { Icon } from '../../lib/icons/registry';
import VerdictChip from '../soft/VerdictChip';
import { StatValue } from '../soft/StatCell';

interface SeekPanelProps {
  query: string;
  /** null while loading; the real SeekOutput once returned. */
  result: SeekOutput | null;
  loading: boolean;
  error: string | null;
  /** The source_paths the viewed brain's snapshot knows (INV-16 scope guard). */
  knownPaths: Set<string>;
  /** Jump into the tree at a hit's path (re-mount expanded + focused + drawer). */
  onOpenHit: (hit: SeekResultEntry) => void;
  onClose: () => void;
}

/** The verdict → action-language label (no epistemic vocabulary at the header). */
function verdictLabel(verdict: 'act' | 'reverify' | 'abstain'): string {
  switch (verdict) {
    case 'act':
      return 'good to go';
    case 'reverify':
      return 'worth a second look';
    case 'abstain':
    default:
      return "I won't guess this one";
  }
}

export default function SeekPanel({
  query,
  result,
  loading,
  error,
  knownPaths,
  onOpenHit,
  onClose,
}: SeekPanelProps) {
  // INV-16: render only hits that belong to the viewed brain; count the drops so
  // the world is never silently smaller.
  const allHits = result?.results ?? [];
  const hits = allHits.filter((r) => resultBelongsToBrain(r.file_path, knownPaths));
  const dropped = allHits.length - hits.length;
  const textFallback = textNotMeaningCaption(result?.embeddings_used);
  const clearingTotal = result?.relevance_clearing_total;
  const truncated = clearingTotal != null && clearingTotal > hits.length;

  return (
    <div
      data-role="seek-panel"
      className="flex-1 flex flex-col min-w-0 overflow-hidden bg-porcelain"
    >
      {/* Header — the precious part: sufficiency + verdict, calm */}
      <div className="px-4 py-3 border-b border-ink/10">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2 min-w-0">
            <Icon name="search" size={16} decorative className="text-ink-soft/80 shrink-0" />
            <span className="text-[11px] uppercase tracking-wide text-ink-soft">meaning</span>
            <span className="text-sm text-ink font-mono truncate" title={query}>
              {query}
            </span>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="close meaning search"
            className="text-ink-soft hover:text-ink text-sm font-mono shrink-0"
          >
            esc
          </button>
        </div>

        {loading && <div className="mt-2 text-[12px] text-ink-soft/80">searching by meaning…</div>}

        {error && (
          <div className="mt-2 text-[12px] text-ink font-mono border border-state-failure/25 bg-state-failure-tint/40 rounded px-2 py-1">
            {error}
          </div>
        )}

        {result && !loading && (
          <div className="mt-2 space-y-1.5" data-role="sufficiency">
            {/* The sufficiency line — state + why VERBATIM (never invented). */}
            <div className="flex items-start gap-2">
              <VerdictChip
                verdict={result.trust_envelope.verdict}
                label={verdictLabel(result.trust_envelope.verdict)}
              />
              <span
                data-role="sufficiency-state"
                className="text-[10px] font-mono uppercase tracking-wide text-ink-soft/80 mt-1"
              >
                {result.sufficiency.state}
              </span>
            </div>
            <div data-role="sufficiency-why" className="text-[11px] text-ink-soft leading-snug">
              {result.sufficiency.why}
            </div>
            {result.trust_envelope.calibrated === false && (
              <div className="text-[10px] text-ink-soft/70 italic">
                not measured on this repo yet — answers stay at "worth a second look" (calibrate in the Hall)
              </div>
            )}
            {textFallback && (
              <div data-role="text-fallback" className="flex items-center gap-1.5 text-[11px] text-ink-soft">
                <Icon name="search" size={14} decorative className="text-ink-soft/70" />
                {textFallback}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Results */}
      <div className="flex-1 overflow-y-auto">
        {result && !loading && hits.length === 0 && (
          <div className="px-4 py-8 text-center text-[13px] text-ink-soft/80" data-role="seek-empty">
            {result.filtering_reason ? (
              <>Nothing cleared: {result.filtering_reason}</>
            ) : (
              <>No matches by meaning{dropped > 0 ? ' in this brain.' : '.'}</>
            )}
          </div>
        )}

        {hits.map((hit) => (
          <button
            key={hit.node_id}
            type="button"
            data-role="seek-hit"
            onClick={() => onOpenHit(hit)}
            className="w-full text-left px-4 py-2.5 border-b border-ink/5 hover:bg-bone/50 transition-colors outline-none focus:bg-bone/60"
          >
            <div className="flex items-baseline justify-between gap-3">
              <span className="text-[13px] text-ink font-medium truncate">{hit.label}</span>
              {/* The engine's score, VERBATIM (Plex Mono, right-aligned). */}
              {hit.score != null && (
                <StatValue className="text-[11px] text-ink-soft shrink-0">{hit.score.toFixed(3)}</StatValue>
              )}
            </div>
            {hit.file_path && (
              <div className="text-[11px] text-ink-soft font-mono truncate">
                {hit.file_path}
                {hit.line_start != null ? `:${hit.line_start}` : ''}
              </div>
            )}
            {hit.intent_summary && (
              <div className="text-[11px] text-ink-soft/80 truncate mt-0.5">{hit.intent_summary}</div>
            )}
            {/* Provenance absent → honest unknown, never faked (INV-04). */}
            {(hit.source_agent != null || hit.authored_ms_ago != null) && (
              <div className="text-[10px] text-ink-soft/70 mt-0.5">
                {authorLabel(hit.source_agent ?? null)} · {humanizeAge(hit.authored_ms_ago ?? null)}
              </div>
            )}
          </button>
        ))}
      </div>

      {/* Footer — truncation + INV-16 drop honesty, never a silently smaller world */}
      {result && !loading && (hits.length > 0 || dropped > 0) && (
        <div className="h-6 px-4 flex items-center gap-3 border-t border-ink/10 text-[11px] text-ink-soft">
          {truncated && (
            <span data-role="seek-truncation">
              showing {hits.length} of {clearingTotal} that cleared relevance
            </span>
          )}
          {dropped > 0 && (
            <span data-role="seek-dropped" className="text-ink-soft/80">
              {dropped} hidden — not this brain
            </span>
          )}
        </div>
      )}
    </div>
  );
}
