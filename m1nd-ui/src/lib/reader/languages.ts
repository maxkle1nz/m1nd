/*
 * reader/languages — the ONE source-of-truth map from a file extension to its
 * Shiki grammar AND its click-to-definition policy (donor dossier §4 Risk 3: keep
 * one `ext → { grammar, navPolicy }` map and degrade honestly).
 *
 * Two independent language sets meet here:
 *  1. Shiki TextMate grammars (the PAINT) — `shikiLang` is `null` for an unknown
 *     extension, and the reader renders plain text honestly (never a fake grammar).
 *  2. The graph's click-to-def coverage (the STRUCTURE) — method/CALL edges are
 *     Rust-rich today; TS/JS/Python/Go carry def/import/use edges but not reliable
 *     call edges (docs/PATHOS.md Known Problems: "Method-call edges exist for Rust
 *     but not TS/Java/Go/Python"). `callEdgesTrusted` encodes that gap as DATA so
 *     the resolver degrades per language instead of fabricating a jump.
 *
 * v1 minimum grammars (what m1nd indexes + the common doc formats): rust,
 * typescript/tsx, javascript, python, go, json, markdown, toml, bash.
 */

/** The Shiki grammar ids the reader lazy-loads (one chunk each, wasm-free). */
export type LangId =
  | 'rust'
  | 'typescript'
  | 'tsx'
  | 'javascript'
  | 'python'
  | 'go'
  | 'json'
  | 'markdown'
  | 'toml'
  | 'bash';

export interface LanguageProfile {
  /** The Shiki grammar id (a lazy chunk), or `null` → render plain text honestly. */
  shikiLang: LangId | null;
  /** A short human label for the language pill (honest even when `shikiLang` is null). */
  label: string;
  /**
   * Whether the graph carries reliable CALL / method-call edges for this language.
   * Rust: yes. Everything else: no (the honest PATHOS gap) — click-to-def then
   * leans on def/import/use edges only, and a call-only reference abstains rather
   * than fabricating a jump.
   */
  callEdgesTrusted: boolean;
}

/** Relations that navigate a DEFINITION regardless of language (the always-trusted
 *  base). Names are normalized to lowercase; synonyms are folded in `relationClass`. */
const DEF_RELATIONS = new Set(['def', 'define', 'defines', 'import', 'imports', 'use', 'uses', 'reference', 'references', 'refers_to']);

/** Relations that are CALL-class — trusted only when `callEdgesTrusted` (Rust). */
const CALL_RELATIONS = new Set(['call', 'calls', 'called_by', 'invoke', 'invokes', 'method_call', 'methodcall']);

const PLAIN: LanguageProfile = { shikiLang: null, label: 'plain text', callEdgesTrusted: false };

/** ext (lower, no dot) → profile. Unknown ext → plain text, honest degradation. */
const BY_EXT: Record<string, LanguageProfile> = {
  rs: { shikiLang: 'rust', label: 'Rust', callEdgesTrusted: true },
  ts: { shikiLang: 'typescript', label: 'TypeScript', callEdgesTrusted: false },
  mts: { shikiLang: 'typescript', label: 'TypeScript', callEdgesTrusted: false },
  cts: { shikiLang: 'typescript', label: 'TypeScript', callEdgesTrusted: false },
  tsx: { shikiLang: 'tsx', label: 'TSX', callEdgesTrusted: false },
  js: { shikiLang: 'javascript', label: 'JavaScript', callEdgesTrusted: false },
  mjs: { shikiLang: 'javascript', label: 'JavaScript', callEdgesTrusted: false },
  cjs: { shikiLang: 'javascript', label: 'JavaScript', callEdgesTrusted: false },
  jsx: { shikiLang: 'tsx', label: 'JSX', callEdgesTrusted: false },
  py: { shikiLang: 'python', label: 'Python', callEdgesTrusted: false },
  pyi: { shikiLang: 'python', label: 'Python', callEdgesTrusted: false },
  go: { shikiLang: 'go', label: 'Go', callEdgesTrusted: false },
  json: { shikiLang: 'json', label: 'JSON', callEdgesTrusted: false },
  jsonc: { shikiLang: 'json', label: 'JSON', callEdgesTrusted: false },
  md: { shikiLang: 'markdown', label: 'Markdown', callEdgesTrusted: false },
  markdown: { shikiLang: 'markdown', label: 'Markdown', callEdgesTrusted: false },
  toml: { shikiLang: 'toml', label: 'TOML', callEdgesTrusted: false },
  sh: { shikiLang: 'bash', label: 'Shell', callEdgesTrusted: false },
  bash: { shikiLang: 'bash', label: 'Shell', callEdgesTrusted: false },
  zsh: { shikiLang: 'bash', label: 'Shell', callEdgesTrusted: false },
};

/** The lowercase extension of a path (no dot), or `''` when there is none. */
export function extOf(path: string): string {
  const base = path.split(/[\\/]/).pop() ?? path;
  const dot = base.lastIndexOf('.');
  if (dot <= 0) return '';
  return base.slice(dot + 1).toLowerCase();
}

/** The language profile for a path — always defined (unknown → the honest PLAIN). */
export function languageForPath(path: string | null | undefined): LanguageProfile {
  if (!path) return PLAIN;
  return BY_EXT[extOf(path)] ?? PLAIN;
}

/** Classify an edge relation for click-to-def. `def` navigates in every language;
 *  `call` navigates only where the graph has reliable call edges; `other` never. */
export function relationClass(relation: string): 'def' | 'call' | 'other' {
  const r = relation.toLowerCase();
  if (DEF_RELATIONS.has(r)) return 'def';
  if (CALL_RELATIONS.has(r)) return 'call';
  return 'other';
}

/** Is this edge relation navigable for a click-to-def under the given language
 *  profile? def-class always; call-class only when the language's call edges are
 *  trusted (Rust). The honest per-language degradation lives here. */
export function relationNavigable(relation: string, profile: LanguageProfile): boolean {
  const cls = relationClass(relation);
  if (cls === 'def') return true;
  if (cls === 'call') return profile.callEdgesTrusted;
  return false;
}
