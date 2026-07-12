/*
 * CodeReader — the advanced, graph-driven read of a member file (donor dossier
 * "Fatia 1": the legible, navigable, honest file). It replaces Show Code's plain
 * `<pre>` with:
 *   1. real syntax highlight (Shiki, JS engine, paper/ink theme) rendered as OWN
 *      line rows (gutter + line numbers) — the one thing the graph cannot give;
 *   2. a symbol OUTLINE derived 100% from the graph (no browser parser); click a
 *      symbol → scroll to its line;
 *   3. click-to-DEFINITION from the graph EDGES, degrading honestly per language
 *      (Rust call-edges; TS/Py/Go def/import; ambiguous → candidates; ungrounded →
 *      abstain, never a fabricated jump) — with a breadcrumb + back;
 *   4. a per-symbol FRESHNESS dot (the block's receipt state — an existing fact);
 *   5. fold ranges straight from the symbols' line spans.
 *
 * Everything structural is the GRAPH; the donor (Shiki) only paints. Read-only.
 * SSR-safe: preserves the viewer-idle / viewer-glob / viewer / viewer-truncated
 * roles and never fetches in a static render (the F1 posture).
 */
import { Fragment, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { useFileView } from '../../hooks/useFileView';
import { useGraphSnapshot } from '../../hooks/useGraphSnapshot';
import { fileSymbols, foldRangesFromSymbols, type OutlineSymbol, type SymbolKind } from '../../lib/reader/symbols';
import { resolveDefinition, type DefResolution } from '../../lib/reader/definition';
import { languageForPath } from '../../lib/reader/languages';
import type { ThemedLine, ThemedTok } from '../../lib/reader/highlighter';
import { Icon, type IconName } from '../../lib/icons/registry';
import type { GraphSnapshot } from '../../lib/snapshot';
import { STATE_LABEL, type BlockRollup, type BlockState } from '../../lib/buildMap';

export interface CodeReaderProps {
  /** The file chosen from the Files-by-role rail (null = nothing selected → idle). */
  path: string | null;
  /** §4A.9 — the brain whose repo the reader reads (null = bound). */
  brainRoot?: string | null;
  /** The block's rollup — its state is the per-symbol freshness dot (an existing
   *  per-block fact; the reader never invents a per-symbol receipt). */
  rollup: BlockRollup;
  /** SSR/tests: inject the snapshot so the outline renders with no network. */
  snapshotOverride?: GraphSnapshot | null;
}

/** Above this the donor's tokenized DOM gets heavy (dossier Risk 5) — the owner
 *  already caps the file at ~256 KB; past this many chars we render plain text and
 *  say so, never a frozen paint. */
const MAX_HIGHLIGHT_CHARS = 200_000;

/** A membership entry declared as a glob is honestly deferred (the viewer reads
 *  exact files) — mirrors Show Code's original guard. */
function isGlobPath(path: string): boolean {
  return /[*?[\]]/.test(path);
}

/** The block-state → house dot color (the per-symbol freshness signal; NEVER
 *  violet — that is reserved for abstain). */
const FRESH_DOT: Record<BlockState, string> = {
  'evidence-backed': 'bg-verdict-act',
  'needs-evidence': 'bg-verdict-reverify',
  broken: 'bg-state-failure',
  unknown: 'bg-state-unverified',
};

function symbolIconName(kind: SymbolKind): IconName {
  switch (kind) {
    case 'function':
      return 'kindFunction';
    case 'struct':
    case 'class':
      return 'kindStruct';
    case 'enum':
      return 'kindEnum';
    case 'type':
      return 'kindType';
    case 'module':
      return 'kindModule';
  }
}

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/** Browser-only highlight: tokenize `content` in `lang` off the main render; SSR
 *  returns null (plain fallback renders). Re-runs when the file/lang changes. */
function useHighlightedLines(content: string | null, lang: ReturnType<typeof languageForPath>['shikiLang'], enabled: boolean): ThemedLine[] | null {
  const [lines, setLines] = useState<ThemedLine[] | null>(null);
  useEffect(() => {
    if (!enabled || content == null || lang == null) {
      setLines(null);
      return;
    }
    let mounted = true;
    setLines(null);
    // Dynamic import: the donor (Shiki core + JS engine) is a LAZY chunk, loaded
    // only when a file is actually painted — the initial app bundle carries none of
    // it (the air-gap/lean-bundle law, dossier Risk 1).
    void import('../../lib/reader/highlighter')
      .then(({ highlightToLines }) => highlightToLines(content, lang))
      .then((l) => {
        if (mounted) setLines(l);
      });
    return () => {
      mounted = false;
    };
  }, [content, lang, enabled]);
  return lines;
}

interface NavFrame {
  path: string;
  line: number | null;
}

const tokenStyle = (t: ThemedTok): CSSProperties => {
  const style: CSSProperties = {};
  if (t.color) style.color = t.color;
  if (t.bold) style.fontWeight = 600;
  if (t.italic) style.fontStyle = 'italic';
  if (t.underline) style.textDecoration = 'underline';
  return style;
};

export default function CodeReader({ path, brainRoot = null, rollup, snapshotOverride }: CodeReaderProps) {
  // Navigation stack: seeded by the rail selection; click-to-def pushes; back pops.
  const [nav, setNav] = useState<NavFrame[]>(path ? [{ path, line: null }] : []);
  const [folded, setFolded] = useState<Set<string>>(new Set());
  const [candidatesOpen, setCandidatesOpen] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  // The rail changed the base file → reset the whole stack to it (F2 posture).
  useEffect(() => {
    setNav(path ? [{ path, line: null }] : []);
    setFolded(new Set());
    setCandidatesOpen(null);
  }, [path]);

  const current = nav.length > 0 ? nav[nav.length - 1] : null;
  const currentPath = current?.path ?? null;
  const currentLine = current?.line ?? null;
  const glob = currentPath != null && isGlobPath(currentPath);

  const { snapshot } = useGraphSnapshot(brainRoot, path != null, snapshotOverride);
  const file = useFileView(glob ? null : currentPath, brainRoot);
  const profile = languageForPath(currentPath);

  const symbols = useMemo(() => fileSymbols(snapshot, currentPath), [snapshot, currentPath]);
  const foldRanges = useMemo(() => foldRangesFromSymbols(symbols), [symbols]);
  const defBySymbol = useMemo(() => {
    const m = new Map<string, DefResolution>();
    for (const s of symbols) m.set(s.id, resolveDefinition(snapshot, s.id, profile));
    return m;
  }, [symbols, snapshot, profile]);

  const contentReady = file.status === 'ready';
  const tooBig = contentReady && file.content.length > MAX_HIGHLIGHT_CHARS;
  const highlighted = useHighlightedLines(contentReady ? file.content : null, profile.shikiLang, contentReady && !tooBig);

  const rawLines = useMemo(() => (contentReady ? file.content.split('\n') : []), [contentReady, file.content]);

  // Fold bookkeeping: which lines are hidden, and which lines start a foldable span.
  const foldStartByLine = useMemo(() => {
    const m = new Map<number, (typeof foldRanges)[number]>();
    for (const r of foldRanges) {
      const prev = m.get(r.startLine);
      if (!prev || r.hiddenCount > prev.hiddenCount) m.set(r.startLine, r);
    }
    return m;
  }, [foldRanges]);
  const hiddenLines = useMemo(() => {
    const h = new Set<number>();
    for (const r of foldRanges) {
      if (!folded.has(r.id)) continue;
      for (let ln = r.startLine + 1; ln <= r.endLine; ln += 1) h.add(ln);
    }
    return h;
  }, [foldRanges, folded]);

  // Scroll the current line into view when it changes (after the lines render).
  useEffect(() => {
    if (currentLine == null || !scrollRef.current) return;
    const el = scrollRef.current.querySelector(`[data-line="${currentLine}"]`);
    if (el && 'scrollIntoView' in el) (el as HTMLElement).scrollIntoView({ block: 'center' });
  }, [currentLine, currentPath, highlighted, rawLines.length]);

  const goToLine = (line: number) =>
    setNav((prev) => {
      if (prev.length === 0) return prev;
      const f = [...prev];
      f[f.length - 1] = { ...f[f.length - 1], line };
      return f;
    });

  const navigateTo = (targetPath: string, line: number) => {
    setNav((prev) => [...prev, { path: targetPath, line }]);
    setCandidatesOpen(null);
    setFolded(new Set());
  };

  const back = () => setNav((prev) => (prev.length > 1 ? prev.slice(0, -1) : prev));
  const toggleFold = (id: string) =>
    setFolded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  // ── honest empty / glob states (unchanged roles) ───────────────────────────
  if (!currentPath) {
    return (
      <div className="flex-1 flex items-center justify-center text-xs text-ink-soft" data-role="viewer-idle">
        Select a file to view its contents.
      </div>
    );
  }
  if (glob) {
    return (
      <div className="flex-1 flex items-center justify-center px-4 text-center text-xs text-ink-soft" data-role="viewer-glob">
        <span>
          <span className="font-mono text-ink">{currentPath}</span> is a glob pattern — per-file view arrives when
          globs are resolved (a later slice).
        </span>
      </div>
    );
  }

  const dotTitle = `block is ${STATE_LABEL[rollup.state]} — the receipt state of the block this file belongs to`;

  return (
    <div className="flex-1 min-h-0 flex flex-col" data-role="viewer" data-reader-lang={profile.shikiLang ?? 'plain'}>
      {/* Header: breadcrumb + path + language pill + truncation honesty */}
      <div className="px-2 py-1.5 border-b border-ink/10 text-[11px] font-mono text-ink flex items-center gap-2" data-role="reader-header">
        {nav.length > 1 && (
          <div className="flex items-center gap-1" data-role="reader-breadcrumb">
            <button
              type="button"
              data-role="reader-back"
              onClick={back}
              className="px-1.5 py-0.5 rounded border border-ink/15 bg-bone text-ink-soft hover:text-ink"
              title="back to the previous file"
            >
              ← back
            </button>
            <span className="text-ink-soft">{nav.slice(0, -1).map((f) => baseName(f.path)).join(' › ')} ›</span>
          </div>
        )}
        <Icon name="kindFile" size={14} decorative />
        <span className="truncate" data-role="reader-path">
          {currentPath}
        </span>
        <span className="text-[10px] uppercase tracking-wide text-ink-soft border border-ink/10 rounded px-1" data-role="reader-langpill">
          {profile.label}
        </span>
        {file.truncated && (
          <span className="ml-auto text-[10px] text-verdict-reverify" data-role="viewer-truncated">
            showing {file.content.length} of {file.bytes} bytes
          </span>
        )}
      </div>

      <div className="flex-1 min-h-0 flex">
        {/* Outline rail — 100% from the graph */}
        <aside className="w-48 shrink-0 border-r border-ink/10 overflow-y-auto p-2" data-role="symbol-outline">
          <div className="text-[10px] uppercase tracking-wide text-ink-soft mb-1 flex items-center gap-1">
            <Icon name="blocks" size={14} decorative />
            Outline
            <span className="tabular-nums">{symbols.length}</span>
          </div>
          {symbols.length === 0 ? (
            <div className="text-[10px] text-ink-soft mt-1" data-role="outline-empty">
              No symbols from the graph for this file{profile.shikiLang == null ? ' — shown as plain text' : ''}.
            </div>
          ) : (
            <ul className="space-y-0.5">
              {symbols.map((s) => (
                <OutlineRow
                  key={s.id}
                  symbol={s}
                  def={defBySymbol.get(s.id)}
                  active={currentLine === s.lineStart}
                  state={rollup.state}
                  dotClass={FRESH_DOT[rollup.state]}
                  dotTitle={dotTitle}
                  candidatesOpen={candidatesOpen === s.id}
                  onGoToLine={goToLine}
                  onNavigate={navigateTo}
                  onToggleCandidates={() => setCandidatesOpen((c) => (c === s.id ? null : s.id))}
                />
              ))}
            </ul>
          )}
        </aside>

        {/* Code column — Shiki paint over graph-driven line rows */}
        <div className="flex-1 min-h-0 overflow-auto bg-warm-paper" ref={scrollRef} data-role="reader-code">
          {file.status === 'loading' && <div className="p-2 text-xs text-ink-soft">Loading {currentPath}…</div>}
          {file.status === 'error' && (
            <div className="p-2 text-xs text-state-failure font-mono break-words">{file.error}</div>
          )}
          {file.status === 'ready' && (
            <>
              {tooBig && (
                <div className="px-2 py-1 text-[10px] text-ink-soft border-b border-ink/10" data-role="reader-nohighlight">
                  syntax paint paused for a large file — reading as plain text.
                </div>
              )}
              <div className="text-[11px] font-mono leading-[1.5]">
                {rawLines.map((lineText, i) => {
                  const lineNo = i + 1;
                  if (hiddenLines.has(lineNo)) return null;
                  const foldable = foldStartByLine.get(lineNo);
                  const isFolded = foldable ? folded.has(foldable.id) : false;
                  const toks = highlighted && highlighted.length === rawLines.length ? highlighted[i] : null;
                  return (
                    <Fragment key={lineNo}>
                      <div
                        data-role="code-line"
                        data-line={lineNo}
                        data-current={currentLine === lineNo ? 'true' : undefined}
                        className={`flex ${currentLine === lineNo ? 'bg-bone' : ''}`}
                      >
                        <span className="select-none w-14 shrink-0 pr-2 text-right text-ink-soft tabular-nums flex items-center justify-end gap-1">
                          {foldable && (
                            <button
                              type="button"
                              data-role="code-fold"
                              data-fold={foldable.id}
                              data-folded={isFolded ? 'true' : undefined}
                              onClick={() => toggleFold(foldable.id)}
                              className="text-ink-soft hover:text-ink leading-none"
                              title={isFolded ? `unfold ${foldable.hiddenCount} lines` : `fold ${foldable.hiddenCount} lines`}
                              aria-label={isFolded ? 'unfold' : 'fold'}
                            >
                              {isFolded ? '▸' : '▾'}
                            </button>
                          )}
                          {lineNo}
                        </span>
                        <code className="whitespace-pre px-2 text-ink flex-1">
                          {toks ? toks.map((t, k) => <span key={k} style={tokenStyle(t)}>{t.content}</span>) : lineText || ' '}
                        </code>
                      </div>
                      {isFolded && foldable && (
                        <div
                          data-role="code-fold-placeholder"
                          className="pl-14 text-[10px] text-ink-soft italic cursor-pointer hover:text-ink"
                          onClick={() => toggleFold(foldable.id)}
                        >
                          ⋯ {foldable.hiddenCount} lines
                        </div>
                      )}
                    </Fragment>
                  );
                })}
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

/** One outline entry: freshness dot + kind icon + label (scroll) + a click-to-def
 *  affordance that degrades honestly (jump / candidates / abstain). */
function OutlineRow({
  symbol,
  def,
  active,
  state,
  dotClass,
  dotTitle,
  candidatesOpen,
  onGoToLine,
  onNavigate,
  onToggleCandidates,
}: {
  symbol: OutlineSymbol;
  def: DefResolution | undefined;
  active: boolean;
  state: BlockState;
  dotClass: string;
  dotTitle: string;
  candidatesOpen: boolean;
  onGoToLine: (line: number) => void;
  onNavigate: (path: string, line: number) => void;
  onToggleCandidates: () => void;
}) {
  return (
    <li data-role="outline-entry" data-symbol-id={symbol.id} data-symbol-line={symbol.lineStart}>
      <div className={`flex items-center gap-1 rounded px-1 py-0.5 ${active ? 'bg-bone' : 'hover:bg-bone/60'}`}>
        <span
          data-role="freshness-dot"
          data-state={state}
          title={dotTitle}
          className={`inline-block w-1.5 h-1.5 rounded-full shrink-0 ${dotClass}`}
        />
        <Icon name={symbolIconName(symbol.kind)} size={14} decorative />
        <button
          type="button"
          data-role="outline-goto"
          onClick={() => onGoToLine(symbol.lineStart)}
          className="text-[11px] font-mono text-ink-soft hover:text-ink truncate text-left flex-1"
          title={`${symbol.kind}${symbol.namespace ? ` · ${symbol.namespace}` : ''} · line ${symbol.lineStart}`}
        >
          {symbol.label}
        </button>
        <span className="text-[10px] text-ink-soft tabular-nums shrink-0">{symbol.lineStart}</span>
        {def?.kind === 'target' && (
          <button
            type="button"
            data-role="def-jump"
            data-target-path={def.target.path}
            data-target-line={def.target.line}
            onClick={() => onNavigate(def.target.path, def.target.line)}
            className="text-socket-blue hover:text-ink text-[11px] leading-none shrink-0"
            title={`go to definition · ${def.target.path}:${def.target.line}`}
            aria-label="go to definition"
          >
            →
          </button>
        )}
        {def?.kind === 'candidates' && (
          <button
            type="button"
            data-role="def-candidates-toggle"
            onClick={onToggleCandidates}
            className="text-socket-blue hover:text-ink text-[10px] leading-none shrink-0 tabular-nums"
            title={`${def.targets.length} possible definitions — ambiguous, pick one`}
            aria-label="show definition candidates"
          >
            →{def.targets.length}
          </button>
        )}
        {def?.kind === 'abstain' && def.reason !== 'none' && (
          <span data-role="def-abstain" className="text-[10px] text-ink-soft italic shrink-0" title={def.message}>
            no target
          </span>
        )}
      </div>
      {def?.kind === 'candidates' && candidatesOpen && (
        <ul className="ml-5 mt-0.5 border-l border-ink/10 pl-2 space-y-0.5" data-role="def-candidate-list">
          {def.targets.map((t) => (
            <li key={t.id}>
              <button
                type="button"
                data-role="def-candidate"
                data-target-path={t.path}
                data-target-line={t.line}
                onClick={() => onNavigate(t.path, t.line)}
                className="text-[10px] font-mono text-ink-soft hover:text-ink truncate text-left w-full"
                title={`${t.kind} · ${t.path}:${t.line}`}
              >
                {t.namespace ? `${t.namespace}::` : ''}
                {t.label} <span className="text-ink-soft">· {baseName(t.path)}:{t.line}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}
