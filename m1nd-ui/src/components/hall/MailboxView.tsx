/*
 * MailboxView — the caixinha, one brain's field-report box (HUMAN-LAYER-PRD §4A.11).
 *
 * A drawer-class surface beside the Hall (ESC returns). Renders EXACTLY what the
 * `/api/mailbox?brain=…` endpoint served for the viewed brain — letters grouped in
 * day-chapters, each wearing a matte class chip + a class left-border + a FATE line
 * (the visible loop: ● aberta · ◍ em voo · ↳ respondida · ◌ externa). The medulla
 * box is the same view under its own labeled header. SOFT PROOF only: nothing glows,
 * nothing animates, violet stays quarantined (external wears stone, not iris).
 *
 * Honesty (INV-17/18): the read is scoped to THIS box (a wrong `served_brain` echo
 * is dropped with a notice, never rendered); every receipt link resolves in-box or
 * renders the honest breakage. There is NO compose box — the human reads only.
 *
 * Split: `MailboxBody` is the PURE presentational surface (no fetch — component-
 * testable from a captured fixture); `MailboxView` is the fetch shell that feeds it.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, ApiError } from '../../api/client';
import { Icon } from '../../lib/icons/registry';
import {
  type MailboxResponse,
  type MailboxLetter,
  classChip,
  CHIP_TONE_CLASS,
  CARD_BORDER_TONE_CLASS,
  CARD_FILL_TONE_CLASS,
  fateLine,
  type FateLine,
  dayChapters,
  boxIdSet,
  headerLine,
  mailboxEchoMatches,
} from '../../lib/mailbox';

export interface MailboxViewProps {
  /** The brain root to open the box for: an absolute project root, the literal
   *  `medulla`, or null for the bound brain's box. */
  brainRoot: string | null;
  /** The brain's display name for the header (seeded from the card; the echo
   *  confirms). `medulla` renders the labeled medulla header instead. */
  displayName: string | null;
  /** ESC / close returns to the Hall (the ladder grammar). */
  onClose: () => void;
}

function errorDetail(error: unknown, fallback: string): string {
  if (error instanceof ApiError) return error.detail;
  if (error instanceof Error) return error.message;
  return fallback;
}

const isMedulla = (root: string | null) => root != null && root.trim().toLowerCase() === 'medulla';

/** The fate-line glyph tone → an ink/soft/matte color class (never violet). */
const FATE_TONE_CLASS: Record<FateLine['tone'], string> = {
  ink: 'text-ink',
  sage: 'text-verdict-act',
  amber: 'text-verdict-reverify',
  stone: 'text-ink-soft',
};

/** One letter card — chip + class left-border + header + body + the fate-line. */
function LetterCard({
  letter,
  boxIds,
  expanded,
  focused,
  onToggle,
  onLinkTo,
  registerRef,
}: {
  letter: MailboxLetter;
  boxIds: Set<string>;
  expanded: boolean;
  focused: boolean;
  onToggle: () => void;
  onLinkTo: (id: string) => void;
  registerRef: (el: HTMLDivElement | null) => void;
}) {
  const chip = classChip(letter.class);
  const fate = fateLine(letter, boxIds);
  const [expectedOpen, setExpectedOpen] = useState(false);

  return (
    <div
      ref={registerRef}
      data-role="letter-card"
      data-letter-id={letter.id}
      data-fate={letter.state}
      data-class={chip.label}
      id={`letter-${letter.id}`}
      tabIndex={focused ? 0 : -1}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          onToggle();
        }
      }}
      className={`rounded-r-lg border border-l-2 px-3 py-2.5 outline-none transition-shadow border-ink/10 ${
        CARD_FILL_TONE_CLASS[chip.tone]
      } ${CARD_BORDER_TONE_CLASS[chip.tone]} ${
        focused ? 'shadow-card ring-1 ring-ink/15' : 'hover:shadow-contact'
      }`}
    >
      {/* Header: class chip · agent · ts (mono) · tool */}
      <div className="flex items-center gap-2 flex-wrap">
        <span
          data-role="class-chip"
          className={`text-[10px] font-mono px-1.5 py-0.5 rounded border ${CHIP_TONE_CLASS[chip.tone]}`}
        >
          {chip.label}
        </span>
        <span className="text-[11px] text-ink-soft font-mono">{letter.agent}</span>
        <span className="text-[11px] text-ink-soft/80 font-mono" data-role="letter-ts">
          {letter.ts}
        </span>
        {letter.tool && (
          <span className="text-[11px] text-ink-soft/70 font-mono">· {letter.tool}</span>
        )}
      </div>

      {/* Body: what (verbatim) */}
      <p className="mt-1.5 text-sm text-ink leading-snug" data-role="letter-what">
        {letter.what}
      </p>

      {/* Expected — folded under a quiet caret */}
      {letter.expected && (
        <button
          type="button"
          data-role="expected-toggle"
          onClick={() => setExpectedOpen((o) => !o)}
          className="mt-1 text-[11px] text-ink-soft hover:text-ink font-mono"
        >
          {expectedOpen ? '▾' : '▸'} esperado
        </button>
      )}
      {expectedOpen && letter.expected && (
        <p className="mt-0.5 text-[12px] text-ink-soft leading-snug pl-3 border-l border-ink/10">
          {letter.expected}
        </p>
      )}

      {/* Snippet — mono, one-line clamp, expand on click */}
      {letter.snippet && (
        <pre
          data-role="letter-snippet"
          onClick={onToggle}
          className={`mt-1.5 text-[11px] text-ink-soft font-mono bg-porcelain/70 rounded px-2 py-1 cursor-pointer ${
            expanded ? 'whitespace-pre-wrap' : 'truncate'
          }`}
          title={expanded ? undefined : 'expand'}
        >
          {letter.snippet}
        </pre>
      )}

      {/* The FATE line — the soul of the view (exactly one per letter) */}
      <div className="mt-1.5 flex items-center gap-1.5">
        <span
          data-role="fate-line"
          data-fate-glyph={fate.glyph}
          data-broken={fate.broken ? 'true' : 'false'}
          className={`text-[12px] font-mono ${
            fate.broken ? 'text-verdict-reverify' : FATE_TONE_CLASS[fate.tone]
          }`}
        >
          <span aria-hidden>{fate.glyph}</span>{' '}
          {fate.linkIds.length > 0 && !fate.broken ? (
            <>
              {/* The link text carries the answered/answering ids as anchors. */}
              {fate.text.replace(fate.linkIds.map((id) => id.slice(0, 6)).join(' · '), '')}
              {fate.linkIds.map((id, i) => (
                <a
                  key={id}
                  data-role="fate-link"
                  href={`#letter-${id}`}
                  data-target-id={id}
                  onClick={(e) => {
                    e.preventDefault();
                    onLinkTo(id);
                  }}
                  className="underline decoration-dotted hover:text-ink"
                >
                  {i > 0 ? ' · ' : ''}
                  {id.slice(0, 6)}
                </a>
              ))}
            </>
          ) : (
            fate.text
          )}
        </span>
      </div>
    </div>
  );
}

export interface MailboxBodyProps {
  brainRoot: string | null;
  displayName: string | null;
  onClose: () => void;
  /** The served payload (echo-matched), or null while loading / after a drop. */
  data: MailboxResponse | null;
  /** True when the response was dropped for a wrong served_brain echo (INV-17). */
  dropped?: boolean;
  /** A human-readable load error (rendered verbatim). */
  error?: string | null;
}

/**
 * The PURE mailbox surface — no fetch, no network. Given a captured `/api/mailbox`
 * payload it renders the whole caixinha (header + day-chapters + letters + the
 * fate-lines) exactly as the shell does. Component-testable in isolation.
 */
export function MailboxBody({ brainRoot, displayName, onClose, data, dropped = false, error = null }: MailboxBodyProps) {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [focusIdx, setFocusIdx] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const letterRefs = useRef<Map<string, HTMLDivElement>>(new Map());

  const medulla = isMedulla(brainRoot);
  const chapters = useMemo(() => (data ? dayChapters(data.letters) : []), [data]);
  const flatLetters = useMemo(() => chapters.flatMap((c) => c.letters), [chapters]);
  const boxIds = useMemo(() => (data ? boxIdSet(data.letters) : new Set<string>()), [data]);

  const scrollToLetter = useCallback((id: string) => {
    const el = letterRefs.current.get(id);
    if (el) {
      el.scrollIntoView({ block: 'center' });
      el.focus();
    }
  }, []);

  const onLinkTo = useCallback(
    (id: string) => {
      const idx = flatLetters.findIndex((l) => l.id === id);
      if (idx >= 0) setFocusIdx(idx);
      scrollToLetter(id);
    },
    [flatLetters, scrollToLetter],
  );

  // Keyboard: ↑/↓ move between letters, Enter expands the focused one, ESC returns.
  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
        return;
      }
      if (flatLetters.length === 0) return;
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setFocusIdx((i) => {
          const next = Math.min(i + 1, flatLetters.length - 1);
          scrollToLetter(flatLetters[next].id);
          return next;
        });
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setFocusIdx((i) => {
          const prev = Math.max(i - 1, 0);
          scrollToLetter(flatLetters[prev].id);
          return prev;
        });
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const cur = flatLetters[focusIdx];
        if (cur) setExpandedId((x) => (x === cur.id ? null : cur.id));
      }
    },
    [flatLetters, focusIdx, onClose, scrollToLetter],
  );

  const focusedId = flatLetters[focusIdx]?.id ?? null;

  return (
    <div
      className="w-full max-w-lg h-full flex flex-col bg-porcelain border-l border-ink/10 shadow-card"
      data-surface="mailbox"
      data-brain={medulla ? 'medulla' : brainRoot ?? 'bound'}
    >
      {/* Header */}
      <div className="px-5 py-4 border-b border-ink/10 flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Icon name="inbox" size={16} decorative className="text-ink-soft" />
            <h2 className="text-base text-ink font-semibold truncate" data-role="mailbox-title">
              {medulla ? 'Medulla — relatos transversais' : `Caixinha — ${displayName ?? brainRoot ?? 'este cérebro'}`}
            </h2>
          </div>
          {medulla && (
            <p className="text-[11px] text-ink-soft mt-0.5" data-role="medulla-note">
              Cartas que não pertencem a nenhum projeto — ferramentas transversais, runtime do dono.
            </p>
          )}
          {data && (
            <p className="text-[11px] text-ink-soft font-mono mt-1" data-role="mailbox-counts">
              {headerLine(data.counts)}
            </p>
          )}
        </div>
        <button
          type="button"
          data-role="mailbox-close"
          onClick={onClose}
          className="text-xs text-ink-soft hover:text-ink px-2 py-1"
          title="ESC — voltar ao Hall"
        >
          ✕
        </button>
      </div>

      {/* Body */}
      <div
        ref={listRef}
        role="list"
        aria-label="letters"
        tabIndex={0}
        onKeyDown={onKeyDown}
        className="flex-1 overflow-y-auto p-4 outline-none focus:ring-1 focus:ring-ink/10 space-y-5"
      >
        {error && (
          <div className="rounded-lg border border-state-failure/25 bg-state-failure-tint/40 px-3 py-2 text-xs text-ink font-mono">
            {error}
          </div>
        )}
        {dropped && (
          <div
            data-role="echo-drop-notice"
            className="rounded-lg border border-verdict-reverify/30 bg-verdict-reverify-tint/40 px-3 py-2 text-xs text-ink"
          >
            Esta caixa respondeu por outro cérebro — descartada para não misturar as cartas.
          </div>
        )}
        {data && flatLetters.length === 0 && !dropped && (
          <div className="rounded-xl border border-ink/10 bg-bone/40 px-5 py-8 text-center text-sm text-ink-soft">
            Nenhuma carta nesta caixa ainda.
          </div>
        )}
        {chapters.map((chapter) => (
          <section key={chapter.key} data-role="day-chapter" data-day={chapter.key}>
            <h3 className="text-[11px] text-ink-soft font-mono mb-2 sticky top-0 bg-porcelain/95 py-1">
              {chapter.key}
              {chapter.weekday ? ` — ${chapter.weekday}` : ''}
            </h3>
            <div className="space-y-2">
              {chapter.letters.map((letter) => (
                <LetterCard
                  key={letter.id}
                  letter={letter}
                  boxIds={boxIds}
                  expanded={expandedId === letter.id}
                  focused={focusedId === letter.id}
                  onToggle={() => setExpandedId((x) => (x === letter.id ? null : letter.id))}
                  onLinkTo={onLinkTo}
                  registerRef={(el) => {
                    if (el) letterRefs.current.set(letter.id, el);
                    else letterRefs.current.delete(letter.id);
                  }}
                />
              ))}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}

export default function MailboxView({ brainRoot, displayName, onClose }: MailboxViewProps) {
  const [data, setData] = useState<MailboxResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dropped, setDropped] = useState(false);

  useEffect(() => {
    let mounted = true;
    setData(null);
    setError(null);
    setDropped(false);
    api
      .mailbox(brainRoot)
      .then((resp) => {
        if (!mounted) return;
        // INV-17: a response whose served_brain echo names a DIFFERENT brain than
        // the one we asked for is DROPPED, never rendered under this header.
        if (!mailboxEchoMatches(brainRoot, resp)) {
          setDropped(true);
          setData(null);
          return;
        }
        setData(resp);
      })
      .catch((err) => mounted && setError(errorDetail(err, 'Failed to open the mailbox')));
    return () => {
      mounted = false;
    };
  }, [brainRoot]);

  return (
    <MailboxBody
      brainRoot={brainRoot}
      displayName={displayName}
      onClose={onClose}
      data={data}
      dropped={dropped}
      error={error}
    />
  );
}
