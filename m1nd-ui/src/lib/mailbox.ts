/*
 * mailbox — the caixinha's pure heart (HUMAN-LAYER-PRD §4A.11).
 *
 * Each brain's field-report box, rendered as chronological correspondence: letters
 * grouped into day-chapters, each wearing a matte class chip + a FATE line (the
 * visible loop). This module is the DOM-free logic the MailboxView renders and the
 * tests prove — it names NO violet (external wears stone, not iris — §6.2 holds).
 *
 * The wire shape is the §9.2 `GET /api/mailbox?brain=…` payload verbatim
 * (mailbox.rs `MailboxView` / `SweptLetter`): letters with a DERIVED `state`
 * (fate) + `answers[]`/`answered_by[]` (the reply graph) + a `counts` block. The
 * UI never re-derives a fate or re-folds the spool — it renders exactly what the
 * endpoint served for the viewed brain (INV-17).
 */
import { canonRootForCompare } from './viewedBrain';

/** A letter's derived fate (mailbox.rs `Fate`). Never re-derived here — the wire
 *  string is authoritative; an unknown value degrades to wet_ink honesty. */
export type Fate = 'wet_ink' | 'in_flight' | 'fired_clay' | 'external';

/** One letter as the `/api/mailbox` endpoint serves it (mailbox.rs `SweptLetter`). */
export interface MailboxLetter {
  /** Content-derived id (`sha256(line)[0..12]`) — stable across machines. */
  id: string;
  /** ISO-8601 timestamp string, verbatim. */
  ts: string;
  /** The authoring agent id. */
  agent: string;
  /** The letter class (`win | bug | honesty | friction | triage | …`). */
  class: string;
  /** Raw repo field (un-normalized). */
  repo: string;
  /** Explicit brain field (wins over repo). */
  brain: string;
  /** The tool the letter is about. */
  tool: string;
  /** The observation, verbatim. */
  what: string;
  /** Optional expected behavior. */
  expected: string;
  /** Optional evidence snippet. */
  snippet: string;
  /** The reply graph: ids this letter answers (a receipt carries these). */
  answers: string[];
  /** The receipt ids that answered THIS letter (derived server-side). */
  answered_by: string[];
  /** The derived fate — the wire string is the source of truth. */
  state: string;
}

/** The `counts` block (mailbox.rs `BoxCounts`) — fate tallies for the header. */
export interface MailboxCounts {
  wet_ink: number;
  in_flight: number;
  fired_clay: number;
  external: number;
  /** `wet_ink + in_flight` — the "abertas" total (external NEVER inflates it). */
  open: number;
}

/** The full `/api/mailbox` payload the client returns. */
export interface MailboxResponse {
  served_brain?: { project_root?: string | null; display_name?: string | null } | null;
  letters: MailboxLetter[];
  counts: MailboxCounts;
}

/**
 * The matte class chip palette (§4A.11 table + §6.1 tokens). Every hue comes from
 * an EXISTING non-violet token family, so the violet quarantine holds:
 *   - win     → sage   (verdict.act)      · a proven good
 *   - bug     → brick  (state.failure)    · a break / loss
 *   - honesty → âmbar  (verdict.reverify) · a calibration finding
 *   - friction→ stone  (state.unverified) · rough-but-not-broken
 *   - recibo  → slate  (ink-soft)         · a triage receipt
 * An unknown class degrades to stone (never violet, never an alarm hue).
 */
export interface ClassChip {
  /** The human label rendered in the chip. */
  label: string;
  /** The Tailwind text/border token family (never iris). */
  tone: 'sage' | 'brick' | 'amber' | 'stone' | 'slate';
}

const RECIBO_CLASSES = new Set(['triage', 'recibo', 'receipt']);

/** Map a letter class to its matte chip (label + non-violet tone). */
export function classChip(cls: string): ClassChip {
  const c = cls.trim().toLowerCase();
  if (c === 'win') return { label: 'win', tone: 'sage' };
  if (c === 'bug') return { label: 'bug', tone: 'brick' };
  if (c === 'honesty') return { label: 'honesty', tone: 'amber' };
  if (c === 'friction') return { label: 'friction', tone: 'stone' };
  if (RECIBO_CLASSES.has(c)) return { label: 'recibo', tone: 'slate' };
  // Any other class (e.g. memory_misdelivery) reads as itself, calm stone.
  return { label: c || 'nota', tone: 'stone' };
}

/** The matte token classes for a chip tone — text + border + a faint tint fill.
 *  Pure Tailwind class strings; NONE reference the iris/violet family (§6.2). */
export const CHIP_TONE_CLASS: Record<ClassChip['tone'], string> = {
  sage: 'text-verdict-act border-verdict-act/40 bg-verdict-act-tint/40',
  brick: 'text-state-failure border-state-failure/40 bg-state-failure-tint/40',
  amber: 'text-verdict-reverify border-verdict-reverify/40 bg-verdict-reverify-tint/40',
  stone: 'text-ink-soft border-state-unverified/50 bg-state-unverified-tint/40',
  slate: 'text-ink-soft border-ink/20 bg-bone/60',
};

/** The 1 px LEFT-BORDER hue per tone (the class stripe down the letter card). */
export const CARD_BORDER_TONE_CLASS: Record<ClassChip['tone'], string> = {
  sage: 'border-l-verdict-act/60',
  brick: 'border-l-state-failure/60',
  amber: 'border-l-verdict-reverify/60',
  stone: 'border-l-state-unverified/70',
  slate: 'border-l-ink/30',
};

/**
 * The soft PASTEL card FILL per tone — a letter card wears a gentle wash of its
 * OWN class so the box reads at a glance (win = sage, bug = clay, honesty = honey,
 * friction = stone), replacing the near-invisible `bg-bone/50`. SOFT PROOF only:
 * every hue is an EXISTING §6.1 `-tint` token (never a new colour, never violet),
 * held at a low opacity so the wash stays calm and the `text-ink` body keeps its
 * contrast. A neutral receipt/other card wears a slightly stronger bone (the
 * calmest tone), never a coloured tint it did not earn.
 */
export const CARD_FILL_TONE_CLASS: Record<ClassChip['tone'], string> = {
  sage: 'bg-verdict-act-tint/50',
  brick: 'bg-state-failure-tint/50',
  amber: 'bg-verdict-reverify-tint/50',
  stone: 'bg-state-unverified-tint/50',
  slate: 'bg-bone/70',
};

/** Whether a fate string is `wet_ink`/`in_flight` — the two OPEN fates. Anything
 *  else (fired_clay, external, unknown) is NOT open. */
export function isOpenFate(state: string): boolean {
  return state === 'wet_ink' || state === 'in_flight';
}

/** The fate-line glyph + voice (§4A.11) — the soul of the view. `fired_clay`
 *  points UP at the receipt(s) that answered it; a receipt points DOWN at what it
 *  answered. `external` is stone `◌`, never counted, never violet. */
export interface FateLine {
  /** The leading glyph: ● wet · ◍ in-flight · ↳ answered · ◌ external. */
  glyph: string;
  /** The human voice, in English (the served UI's language). */
  text: string;
  /** For `fired_clay`, the receipt letter ids this scrolls/links to (may be
   *  empty → the honest breakage is rendered instead). */
  linkIds: string[];
  /** True when this fate carries a link that could not resolve in-box (INV-18
   *  breakage: "receipt not found"). */
  broken: boolean;
  /** A stable tone for the glyph (never violet). */
  tone: 'stone' | 'amber' | 'sage' | 'ink';
}

/**
 * Derive the fate-line for a letter, resolving its receipt link against the SAME
 * box (INV-18). `boxIds` is the set of letter ids present in this box; a
 * `fired_clay` letter whose `answered_by` ids are all absent from the box renders
 * the honest breakage, never a silent plain chip.
 *
 * A receipt letter (class in RECIBO_CLASSES) that carries `answers[]` renders the
 * DOWN-pointing "responde a carta N" line — the loop closes in both directions.
 */
export function fateLine(letter: MailboxLetter, boxIds: Set<string>): FateLine {
  const isReceipt = RECIBO_CLASSES.has(letter.class.trim().toLowerCase());

  // A receipt points DOWN at what it answered (regardless of its own fate).
  if (isReceipt && letter.answers.length > 0) {
    const inBox = letter.answers.filter((id) => boxIds.has(id));
    if (inBox.length === 0) {
      return {
        glyph: '↓',
        text: 'receipt not found',
        linkIds: [],
        broken: true,
        tone: 'amber',
      };
    }
    return {
      glyph: '↓',
      text: `answers ${plural(inBox.length, 'letter', 'letters')} ${inBox.map(shortId).join(' · ')}`,
      linkIds: inBox,
      broken: false,
      tone: 'ink',
    };
  }

  switch (letter.state) {
    case 'fired_clay': {
      const inBox = letter.answered_by.filter((id) => boxIds.has(id));
      if (inBox.length === 0) {
        // A `fired_clay` fate whose receipt is not in this box is the INV-18
        // breakage — the loop claims closed but the receipt cannot be shown.
        return {
          glyph: '↳',
          text: 'receipt not found',
          linkIds: [],
          broken: true,
          tone: 'amber',
        };
      }
      return {
        glyph: '↳',
        text: `answered by ${plural(inBox.length, 'letter', 'letters')} ${inBox.map(shortId).join(' · ')}`,
        linkIds: inBox,
        broken: false,
        tone: 'sage',
      };
    }
    case 'in_flight':
      return { glyph: '◍', text: 'in flight', linkIds: [], broken: false, tone: 'amber' };
    case 'external':
      return { glyph: '◌', text: 'external', linkIds: [], broken: false, tone: 'stone' };
    case 'wet_ink':
    default:
      // Unknown/absent fate degrades to the honest OPEN state (never a fake close).
      return { glyph: '●', text: 'open', linkIds: [], broken: false, tone: 'ink' };
  }
}

/** Short a 12-char content id for the fate-line link text (first 6). */
export function shortId(id: string): string {
  return id.length > 6 ? id.slice(0, 6) : id;
}

function plural(n: number, one: string, many: string): string {
  return n === 1 ? one : many;
}

/** The day-chapter key for a letter: the leading `YYYY-MM-DD` of its ts, verbatim
 *  (timezone-honest — the wire ts is authoritative, never re-zoned client-side).
 *  A ts that has no parseable date head groups under "no date". */
export function dayKey(ts: string): string {
  const head = ts.slice(0, 10);
  return /^\d{4}-\d{2}-\d{2}$/.test(head) ? head : 'no date';
}

/** A weekday word for a `YYYY-MM-DD` key, best-effort. "no date" → ''. */
export function weekdayLabel(key: string): string {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(key)) return '';
  const [y, m, d] = key.split('-').map((s) => parseInt(s, 10));
  // Zeller-free: use UTC Date on the pure date (no tz drift for the weekday name).
  const dt = new Date(Date.UTC(y, m - 1, d));
  const names = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
  return names[dt.getUTCDay()] ?? '';
}

/** One day-chapter: its date key + weekday + the letters filed under it. */
export interface DayChapter {
  key: string;
  weekday: string;
  letters: MailboxLetter[];
}

/**
 * Group letters into day-chapters, newest chapter FIRST, and within a chapter
 * newest letter first (the box reads as correspondence — §4A.11). Ordering is by
 * the full ts string (ISO-8601 sorts lexicographically); "no date" sinks last.
 */
export function dayChapters(letters: MailboxLetter[]): DayChapter[] {
  const byDay = new Map<string, MailboxLetter[]>();
  for (const l of letters) {
    const k = dayKey(l.ts);
    const arr = byDay.get(k);
    if (arr) arr.push(l);
    else byDay.set(k, [l]);
  }
  const keys = [...byDay.keys()].sort((a, b) => {
    if (a === 'no date') return 1;
    if (b === 'no date') return -1;
    return a < b ? 1 : a > b ? -1 : 0; // newest date first
  });
  return keys.map((key) => ({
    key,
    weekday: weekdayLabel(key),
    letters: (byDay.get(key) ?? []).slice().sort((a, b) => (a.ts < b.ts ? 1 : a.ts > b.ts ? -1 : 0)),
  }));
}

/** The set of letter ids in a box — the receipt-linkage resolver's domain. */
export function boxIdSet(letters: MailboxLetter[]): Set<string> {
  return new Set(letters.map((l) => l.id));
}

/**
 * The one-line header truth (§4A.11 §4): "12 letters · 3 open · 1 in flight · 1
 * external" — only the non-zero segments render, always leading with the total and
 * the "open" count (the honest headline). `open` = wet_ink + in_flight.
 */
export function headerLine(counts: MailboxCounts): string {
  const total = counts.wet_ink + counts.in_flight + counts.fired_clay + counts.external;
  const segs: string[] = [`${total} ${plural(total, 'letter', 'letters')}`, `${counts.open} open`];
  if (counts.in_flight > 0) segs.push(`${counts.in_flight} in flight`);
  if (counts.fired_clay > 0) segs.push(`${counts.fired_clay} answered`);
  if (counts.external > 0) segs.push(`${counts.external} external`);
  return segs.join(' · ');
}

/**
 * INV-17 guard: does this mailbox response belong to the brain we asked for?
 * Reuses the §4A.9 selector echo contract verbatim (servedBrainMatches). A
 * medulla view (`brainRoot === 'medulla'`) trusts the medulla echo; a hosted
 * view requires the echo to name the requested root. A mismatch is DROPPED, never
 * rendered (one box's letters never appear under another box's header).
 */
export function mailboxEchoMatches(brainRoot: string | null, resp: MailboxResponse): boolean {
  // The medulla box is addressed by the literal selector; its echo names it.
  if (brainRoot != null && brainRoot.trim().toLowerCase() === 'medulla') {
    const got = resp.served_brain?.project_root ?? resp.served_brain?.display_name ?? null;
    return got != null && String(got).trim().toLowerCase() === 'medulla';
  }
  const want = canonRootForCompare(brainRoot);
  if (want == null) return true; // bound view — the server resolves absent→bound
  const got = canonRootForCompare(resp.served_brain?.project_root);
  return got != null && got === want;
}
