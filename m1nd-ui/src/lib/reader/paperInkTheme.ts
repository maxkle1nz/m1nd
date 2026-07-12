/*
 * reader/paperInkTheme — the custom TextMate theme for the code reader (donor
 * dossier §2 Fatia 1.1: "author one TextMate theme in the house tokens; violet
 * stays quarantined; nothing glows"). It is NOT a stock Shiki theme — every color
 * is one of the existing SOFT PROOF / ARTKIT tokens (index.css / tailwind.config),
 * expressed as the literal hex those CSS vars hold, so the code reads as paper and
 * ink and passes violet-lint. No neon, no emission, no iris (the quarantined
 * violet family).
 *
 * Dependency-free (no Shiki import) so it is cheap to unit-test — the highlighter
 * passes this plain object to Shiki, which accepts a raw theme registration.
 */

// House tokens (mirror of index.css :root / tailwind.config), literal hex.
export const PAPER = '#fcfaf6'; //  --warm-paper : the code ground (viewer fill)
export const INK = '#2b2836'; //    --ink        : default text, symbol names
export const INK_SOFT = '#5b5566'; //--ink-soft   : comments, punctuation (muted)
export const SOCKET_BLUE = '#3d6f8e'; // --socket-blue : keywords + types (structure)
export const SAGE = '#6fa287'; //   --verdict-act    : strings (calm sage)
export const OCHRE = '#c89b3c'; //  --verdict-reverify: numbers / constants
export const BRICK = '#b0563b'; //  --state-failure  : invalid/broken tokens (honest)

/** The complete calm palette the theme is allowed to use — the theme test asserts
 *  every color the theme emits is in here (and, by construction, no violet/neon). */
export const PAPER_INK_PALETTE: readonly string[] = [PAPER, INK, INK_SOFT, SOCKET_BLUE, SAGE, OCHRE, BRICK];

export const PAPER_INK_THEME_NAME = 'm1nd-paper-ink';

interface TokenSetting {
  scope?: string[];
  settings: { foreground?: string; background?: string; fontStyle?: string };
}
export interface RawTheme {
  name: string;
  type: 'light' | 'dark';
  colors: Record<string, string>;
  bg: string;
  fg: string;
  settings: TokenSetting[];
}

/** The paper/ink theme. Quiet tones only: keywords + types in socket-blue, names in
 *  ink (weight, not hue), strings in sage, numbers in ochre, comments in muted ink,
 *  broken tokens in brick. Violet appears NOWHERE — it is reserved for abstain. */
export const paperInkTheme: RawTheme = {
  name: PAPER_INK_THEME_NAME,
  type: 'light',
  colors: { 'editor.background': PAPER, 'editor.foreground': INK },
  bg: PAPER,
  fg: INK,
  settings: [
    { settings: { background: PAPER, foreground: INK } },
    {
      scope: ['comment', 'punctuation.definition.comment', 'string.comment', 'comment.block.documentation'],
      settings: { foreground: INK_SOFT, fontStyle: 'italic' },
    },
    {
      scope: ['keyword', 'storage', 'storage.type', 'storage.modifier', 'keyword.control', 'keyword.other', 'variable.language', 'keyword.declaration'],
      settings: { foreground: SOCKET_BLUE },
    },
    {
      scope: ['entity.name.type', 'entity.name.class', 'entity.name.struct', 'entity.name.namespace', 'entity.name.enum', 'support.type', 'support.class', 'entity.other.inherited-class', 'meta.type.name'],
      settings: { foreground: SOCKET_BLUE, fontStyle: 'bold' },
    },
    {
      scope: ['entity.name.function', 'support.function', 'meta.function-call.generic', 'entity.name.tag', 'entity.name.function.macro'],
      settings: { foreground: INK, fontStyle: 'bold' },
    },
    {
      scope: ['string', 'string.quoted', 'string.template', 'markup.inline.raw', 'string.regexp'],
      settings: { foreground: SAGE },
    },
    {
      scope: ['constant.numeric', 'constant.language', 'constant.character', 'constant.other', 'keyword.other.unit', 'support.constant'],
      settings: { foreground: OCHRE },
    },
    {
      scope: ['variable', 'variable.parameter', 'meta.definition.variable', 'variable.other', 'meta.object-literal.key'],
      settings: { foreground: INK },
    },
    {
      scope: ['keyword.operator', 'punctuation', 'meta.brace', 'meta.delimiter', 'punctuation.separator', 'punctuation.terminator'],
      settings: { foreground: INK_SOFT },
    },
    { scope: ['invalid', 'invalid.illegal', 'invalid.deprecated'], settings: { foreground: BRICK } },
    { scope: ['markup.heading', 'markup.bold', 'entity.name.section'], settings: { foreground: INK, fontStyle: 'bold' } },
  ],
};

/** Every color literal the theme emits (bg/fg + colors map + token foreground/bg) —
 *  the test asserts each is in [`PAPER_INK_PALETTE`], so the theme can never drift
 *  into a neon or violet tone. */
export function collectThemeColors(theme: RawTheme = paperInkTheme): string[] {
  const colors = new Set<string>();
  colors.add(theme.bg);
  colors.add(theme.fg);
  for (const v of Object.values(theme.colors)) colors.add(v);
  for (const s of theme.settings) {
    if (s.settings.foreground) colors.add(s.settings.foreground);
    if (s.settings.background) colors.add(s.settings.background);
  }
  return [...colors];
}
