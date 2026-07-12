/*
 * reader/highlighter — the ONE donor (Shiki), wired the wasm-free way (donor
 * dossier §2 Fatia 1.1). Fine-grained bundle only: `shiki/core` +
 * `shiki/engine/javascript` (the JavaScript regex engine — NO wasm, honors the
 * air-gap/no-CDN law) + the custom paper/ink theme. NEVER import the monolithic
 * `shiki` bundle (6.4 MB). Grammars load LAZILY, one chunk per language, so the
 * initial reader payload carries no grammar it does not need.
 *
 * Line-based output: `codeToTokens` gives a 2D tokens-per-line array, which the
 * component renders as its own gutter + line rows (line numbers, scroll-to-symbol,
 * fold ranges) — capabilities the graph drives, over paint the donor provides.
 *
 * Browser-only by construction: the singleton is built on first call inside a
 * browser effect; nothing runs during SSR (`renderToStaticMarkup`), so the reader's
 * plain-text fallback renders deterministically with no network.
 */
import { createHighlighterCore, type HighlighterCore } from 'shiki/core';
import { createJavaScriptRegexEngine } from 'shiki/engine/javascript';
import { paperInkTheme, PAPER_INK_THEME_NAME } from './paperInkTheme';
import type { LangId } from './languages';

/** One highlighted token — the donor's paint, flattened to what the DOM needs. */
export interface ThemedTok {
  content: string;
  color?: string;
  bold: boolean;
  italic: boolean;
  underline: boolean;
}
export type ThemedLine = ThemedTok[];

/** Literal per-language dynamic imports so the bundler code-splits one chunk each
 *  (a record of `() => import('@shikijs/langs/<lang>')`; only the viewed language's
 *  chunk is ever fetched). */
const LANG_LOADERS: Record<LangId, () => Promise<unknown>> = {
  rust: () => import('@shikijs/langs/rust'),
  typescript: () => import('@shikijs/langs/typescript'),
  tsx: () => import('@shikijs/langs/tsx'),
  javascript: () => import('@shikijs/langs/javascript'),
  python: () => import('@shikijs/langs/python'),
  go: () => import('@shikijs/langs/go'),
  json: () => import('@shikijs/langs/json'),
  markdown: () => import('@shikijs/langs/markdown'),
  toml: () => import('@shikijs/langs/toml'),
  bash: () => import('@shikijs/langs/bash'),
};

let corePromise: Promise<HighlighterCore> | null = null;
const langPromises = new Map<LangId, Promise<void>>();

/** The lazily-built core highlighter: JS engine (wasm-free) + the paper/ink theme,
 *  NO grammar preloaded. Built once, reused for every file. */
function getCore(): Promise<HighlighterCore> {
  if (!corePromise) {
    corePromise = createHighlighterCore({
      themes: [paperInkTheme],
      langs: [],
      engine: createJavaScriptRegexEngine(),
    });
  }
  return corePromise;
}

/** Ensure a grammar is loaded (deduped per language). Each call fetches at most one
 *  chunk; a second view of the same language reuses the resolved promise. */
async function ensureLanguage(lang: LangId): Promise<void> {
  let p = langPromises.get(lang);
  if (!p) {
    p = (async () => {
      const core = await getCore();
      if (core.getLoadedLanguages().includes(lang)) return;
      const mod = await LANG_LOADERS[lang]();
      await core.loadLanguage(mod as never);
    })();
    langPromises.set(lang, p);
  }
  return p;
}

const FONT_ITALIC = 1;
const FONT_BOLD = 2;
const FONT_UNDERLINE = 4;

/**
 * Tokenize `code` in `lang` under the paper/ink theme → line rows. Returns `null`
 * on any failure (grammar load / tokenize) so the caller renders plain text
 * honestly — the donor never blocks the read. `null` lang is not handled here (the
 * caller decides plain vs highlight from the language profile).
 */
export async function highlightToLines(code: string, lang: LangId): Promise<ThemedLine[] | null> {
  try {
    await ensureLanguage(lang);
    const core = await getCore();
    const { tokens } = core.codeToTokens(code, { lang, theme: PAPER_INK_THEME_NAME });
    return tokens.map((line) =>
      line.map((t) => {
        const fs = typeof t.fontStyle === 'number' ? t.fontStyle : 0;
        return {
          content: t.content,
          color: t.color,
          bold: (fs & FONT_BOLD) !== 0,
          italic: (fs & FONT_ITALIC) !== 0,
          underline: (fs & FONT_UNDERLINE) !== 0,
        };
      }),
    );
  } catch {
    return null;
  }
}

/** Test/measurement hook — reset the singletons (so a bundle probe or a test can
 *  build a fresh highlighter). Not used by the app. */
export function __resetHighlighterForTest(): void {
  corePromise = null;
  langPromises.clear();
}
